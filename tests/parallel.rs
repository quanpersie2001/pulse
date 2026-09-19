//! Parallel end-to-end (decision 0025, plan 0025 B6/B4/B5): two workers
//! share one checkout. The whole loop runs through the real `pulse` binary
//! against a temporary copy of a target fixture — never the fixture itself,
//! never this development repository (AGENTS.md).
//!
//! What is under test is the contract the host depends on when it spawns
//! more than one worker: disjoint scopes claim side by side; a colliding
//! claim is refused; each handoff ignores the other ticket's dirty files
//! because they are reserved by their own ticket; a lane reviews one scope
//! while the rest of the tree is dirty; a scoped close survives another
//! ticket's commit landing first; and a stray edit outside every `touches`
//! blocks handoff until it is reserved.

use std::fs;
use std::path::Path;

use serde_json::{json, Value};

#[allow(dead_code)]
#[path = "common/bin.rs"]
mod common_bin;
#[allow(dead_code)]
#[path = "common/git.rs"]
mod common_git;
#[allow(dead_code)]
#[path = "common/fixture_repo.rs"]
mod fixture_repo;

use fixture_repo::TestRepo;

const PULSE_MD: &str = "profiles:\n  cli-low: {lanes: [review-correctness]}\n";

/// A committed `api/` + `web/` baseline with a minimal profile, then
/// `pulse init` and a scaffold commit so the tree starts clean.
fn setup() -> TestRepo {
    let repo = TestRepo::from_fixture("minimal-service");
    fs::write(repo.path().join("PULSE.md"), PULSE_MD).unwrap();
    fs::create_dir_all(repo.path().join("api")).unwrap();
    fs::create_dir_all(repo.path().join("web")).unwrap();
    fs::write(repo.path().join("api/main.py"), "def main():\n    pass\n").unwrap();
    fs::write(repo.path().join("web/app.js"), "// app\n").unwrap();
    common_git::commit_all(repo.path());
    repo.pulse_ok(&["init", "--json"]);
    // `pulse init` wrote AGENTS.md, .gitignore entries and .pulse — commit
    // the scaffold so the test's own edits are the only dirt (golden-path
    // pattern: the fence must never read setup as someone's work).
    common_git::commit_all(repo.path());
    repo
}

fn new_ticket(
    repo: &TestRepo,
    harness: &Path,
    name: &str,
    title: &str,
    touches: &[&str],
) -> String {
    let payload = harness.join(format!("{name}.json"));
    fs::write(
        &payload,
        serde_json::to_string(&json!({
            "acceptance": [{"id": "AC-1", "when": "w", "then": "t"}],
            "touches": touches,
        }))
        .unwrap(),
    )
    .unwrap();
    // Payloads live in the harness dir, outside the repo: they must never
    // show up as unclaimed dirt at handoff time.
    let ticket = repo.pulse_ok(&[
        "work",
        "new",
        "ticket",
        title,
        "--risk",
        "low",
        "--surface",
        "cli",
        "--from",
        payload.to_str().unwrap(),
        "--actor",
        "human:quan",
        "--json",
    ]);
    let id = ticket["id"].as_str().unwrap().to_string();
    repo.pulse_ok(&["work", "ready", &id, "--actor", "human:quan", "--json"]);
    id
}

fn claim(repo: &TestRepo, id: &str, worker: &str) -> String {
    let ticket = repo.pulse_ok(&["claim", id, "--actor", worker, "--json"]);
    ticket["lease"]["run_id"].as_str().unwrap().to_string()
}

fn edit(repo: &TestRepo, relative: &str, body: &str) {
    fs::write(repo.path().join(relative), body).unwrap();
}

fn handoff(repo: &TestRepo, harness: &Path, name: &str, id: &str, worker: &str, run_id: &str) {
    let input = harness.join(format!("{name}-handoff.json"));
    fs::write(
        &input,
        serde_json::to_string(&json!({
            "run_id": run_id,
            "summary": "done",
            "acceptance": [{"id": "AC-1", "status": "done", "how": "ran it"}],
        }))
        .unwrap(),
    )
    .unwrap();
    repo.pulse_ok(&[
        "handoff",
        id,
        "--from",
        input.to_str().unwrap(),
        "--actor",
        worker,
        "--json",
    ]);
}

/// The host's dispatch: `lane input`, the lane writes its verdict file, the
/// lane's own actor seals it.
fn run_lane(repo: &TestRepo, id: &str) {
    repo.pulse_ok(&[
        "lane",
        "input",
        id,
        "review-correctness",
        "--actor",
        "human:quan",
        "--json",
    ]);
    let commit = common_git::git(repo.path(), &["rev-parse", "HEAD"]);
    let dir = repo.path().join(".pulse/evidence").join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("review-correctness.json"),
        serde_json::to_vec_pretty(&json!({
            "verdict": "pass",
            "acceptance": [{"id": "AC-1", "status": "pass", "how": "reviewed"}],
            "cases": [], "findings": [], "commands_run": [],
            "environment": {"commit": commit},
        }))
        .unwrap(),
    )
    .unwrap();
    repo.pulse_ok(&[
        "lane",
        "seal",
        id,
        "review-correctness",
        "--actor",
        "agent:review-correctness",
        "--json",
    ]);
}

fn pulse_error(repo: &TestRepo, args: &[&str]) -> Value {
    let output = repo.pulse(args);
    assert!(
        !output.status.success(),
        "pulse {args:?} unexpectedly succeeded:\n{}",
        String::from_utf8_lossy(&output.stdout),
    );
    serde_json::from_slice(&output.stderr).expect("JSON error on stderr")
}

fn ids<'a>(value: &'a Value, list: &str) -> Vec<&'a str> {
    value[list]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["id"].as_str().unwrap())
        .collect()
}

#[test]
fn two_workers_share_one_checkout_across_claim_handoff_lane_and_close() {
    let repo = setup();
    let harness = tempfile::tempdir().unwrap();
    let tk_a = new_ticket(&repo, harness.path(), "a", "api work", &["api/**"]);
    let tk_b = new_ticket(&repo, harness.path(), "b", "web work", &["web/**"]);
    let tk_c = new_ticket(
        &repo,
        harness.path(),
        "c",
        "api file work",
        &["api/main.py"],
    );

    // Frontier: two runnable, one waiting. Hash ids sort arbitrarily, so
    // WHICH of the api pair waits depends on the draw — but exactly one of
    // {A, C} waits on the other, and B (disjoint from both) always runs.
    let front = repo.pulse_ok(&["frontier", "--json"]);
    assert_eq!(ids(&front, "runnable").len(), 2, "{front}");
    let waiting = &front["waiting"][0];
    let (waited, blocker) = if waiting["id"] == tk_c.as_str() {
        (tk_c.as_str(), tk_a.as_str())
    } else {
        (tk_a.as_str(), tk_c.as_str())
    };
    assert_eq!(waiting["id"], waited, "{front}");
    assert_eq!(waiting["reason"], "frontier", "{front}");
    assert_eq!(waiting["blocked_on"], blocker, "{front}");
    // The pattern named is the blocker's — and either api ticket may be the
    // blocker, so the pattern follows whichever one won the greedy pass.
    let blocker_pattern = if blocker == tk_a.as_str() {
        "api/**"
    } else {
        "api/main.py"
    };
    assert_eq!(waiting["pattern"], blocker_pattern, "{front}");
    assert!(ids(&front, "runnable").contains(&tk_b.as_str()), "{front}");

    // Both claims succeed — parallel workers carry distinct actors — while
    // C's colliding claim is refused while A holds api/**.
    let run_a = claim(&repo, &tk_a, "agent:worker-1");
    let run_b = claim(&repo, &tk_b, "agent:worker-2");
    let error = pulse_error(
        &repo,
        &["claim", &tk_c, "--actor", "agent:worker-3", "--json"],
    );
    assert_eq!(error["code"], "claim_files_reserved", "{error}");

    // Each worker edits only its own scope; the other's dirt must not
    // matter at handoff time because it is reserved by its own ticket.
    edit(&repo, "api/main.py", "def main():\n    return 1\n");
    edit(&repo, "web/app.js", "// app, reworked\n");

    handoff(&repo, harness.path(), "a", &tk_a, "agent:worker-1", &run_a);
    handoff(&repo, harness.path(), "b", &tk_b, "agent:worker-2", &run_b);

    // A's lane runs and seals while B's web/ file is still dirty: the
    // scoped fence does not read foreign dirt as a lane mutation.
    run_lane(&repo, &tk_a);
    let closed = repo.pulse_ok(&["close", &tk_a, "--actor", "human:quan", "--json"]);
    assert_eq!(closed["status"], "done");

    // The host commits A's files right after close. HEAD moves, B's scope
    // does not — B's lane and close must not go stale (decision 0025: a
    // scoped ticket's fence is not HEAD).
    common_git::git(repo.path(), &["add", "api"]);
    common_git::git(repo.path(), &["commit", "-m", "land api"]);
    run_lane(&repo, &tk_b);
    let closed = repo.pulse_ok(&["close", &tk_b, "--actor", "human:quan", "--json"]);
    assert_eq!(closed["status"], "done");

    // With A done, api/main.py is free and C can finally claim.
    let run_c = claim(&repo, &tk_c, "agent:worker-3");
    assert!(!run_c.is_empty());
}

#[test]
fn a_stray_edit_blocks_handoff_until_it_is_reserved() {
    let repo = setup();
    let harness = tempfile::tempdir().unwrap();
    let tk_a = new_ticket(&repo, harness.path(), "a", "api work", &["api/**"]);
    let tk_b = new_ticket(&repo, harness.path(), "b", "web work", &["web/**"]);
    let run_a = claim(&repo, &tk_a, "agent:worker-1");
    let run_b = claim(&repo, &tk_b, "agent:worker-2");

    edit(&repo, "api/main.py", "def main():\n    return 1\n");
    edit(&repo, "web/app.js", "// app, reworked\n");
    // A's handoff sees only claimed dirt — A goes through clean.
    handoff(&repo, harness.path(), "a", &tk_a, "agent:worker-1", &run_a);

    // Now worker-2 touches a file outside every ticket's scope.
    edit(&repo, "NOTES.md", "whose note is this?\n");
    let b_input = harness.path().join("b-handoff.json");
    fs::write(
        &b_input,
        serde_json::to_string(&json!({
            "run_id": run_b,
            "summary": "done",
            "acceptance": [{"id": "AC-1", "status": "done", "how": "ran it"}],
        }))
        .unwrap(),
    )
    .unwrap();
    let error = pulse_error(
        &repo,
        &[
            "handoff",
            &tk_b,
            "--from",
            b_input.to_str().unwrap(),
            "--actor",
            "agent:worker-2",
            "--json",
        ],
    );
    assert_eq!(error["code"], "gate_failed", "{error}");
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("handoff_unreserved_changes"), "{message}");
    assert!(message.contains("NOTES.md"), "{message}");
    assert!(message.contains("pulse reserve"), "{message}");

    // The reserved-file route: widen B's claim, then the handoff passes.
    let ticket = repo.pulse_ok(&[
        "reserve",
        &tk_b,
        "NOTES.md",
        "--actor",
        "agent:worker-2",
        "--json",
    ]);
    assert_eq!(
        ticket["touches"],
        json!(["web/**", "NOTES.md"]),
        "reserve appends and dedups"
    );
    handoff(&repo, harness.path(), "b", &tk_b, "agent:worker-2", &run_b);
    let records = repo.pulse_ok(&["work", "show", &tk_b, "--json"]);
    assert_eq!(records["status"], "verifying");
}
