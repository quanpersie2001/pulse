//! `pulse::kernel::lane::{prepare, seal}` integration tests (plan 0022
//! §8.3-8.4).
//!
//! These replace the old `tests/run.rs`, which drove a runner that spawned
//! the lane itself. Pulse no longer dispatches anything: the host runs the
//! lane between `prepare` and `seal`, so what is left to test is the two
//! boundaries — what a lane is allowed to see, and what Pulse accepts back
//! as evidence. The tests therefore write the lane's output file themselves,
//! exactly as a dispatched lane agent would.

use std::fs;
use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};

use pulse::identity::actor::{ActorKind, ActorRef};
use pulse::kernel::completion::{close, handoff, HandoffAcceptance, HandoffInput};
use pulse::kernel::lane::{prepare, reconcile, reconcile_prepare, seal};
use pulse::kernel::reservation::acquire_lease;
use pulse::store::issues;

fn agent(id: &str) -> ActorRef {
    ActorRef {
        kind: ActorKind::Agent,
        id: id.to_string(),
    }
}

fn git_repo(profiles: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        assert!(std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(args)
            .status()
            .unwrap()
            .success());
    };
    fs::write(dir.path().join("PULSE.md"), profiles).unwrap();
    fs::write(dir.path().join("README.md"), "# fixture\n").unwrap();
    run(&["init", "-q"]);
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "user.name", "test"]);
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);
    dir
}

fn push_record(repo: &Path, record: Value) {
    issues::mutate(repo, |mut records| {
        records.push(record);
        Ok(records)
    })
    .unwrap();
}

fn ready_ticket(id: &str) -> Value {
    json!({
        "schema": 3, "id": id, "kind": "ticket", "title": "t",
        "status": "ready", "revision": 1,
        "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
        "role": "implementation", "risk": "low", "surface": "cli",
        "acceptance": [{"id": "AC-1", "when": "x", "then": "y"}],
    })
}

fn handoff_input() -> HandoffInput {
    HandoffInput {
        run_id: "run_1".to_string(),
        summary: "did the thing".to_string(),
        changed_files: Vec::new(),
        acceptance: vec![HandoffAcceptance {
            id: "AC-1".to_string(),
            status: "done".to_string(),
            how: "ran it".to_string(),
        }],
        verify_results: Vec::new(),
        docs_updated: Vec::new(),
        learnings_used: Vec::new(),
        friction: Vec::new(),
        open_risks: Vec::new(),
    }
}

fn human(id: &str) -> ActorRef {
    ActorRef {
        kind: ActorKind::Human,
        id: id.to_string(),
    }
}

/// A repo whose `TK-a3f9` is `verifying`, handed off by `worker`.
fn repo_with_handed_off_ticket_by(worker: &ActorRef) -> tempfile::TempDir {
    let repo = git_repo("profiles:\n  cli-low: {lanes: [review-correctness]}\n");
    push_record(repo.path(), ready_ticket("TK-a3f9"));
    acquire_lease(repo.path(), worker, "TK-a3f9", "worker", "run_1", 3600).unwrap();
    handoff(repo.path(), worker, "TK-a3f9", handoff_input()).unwrap();
    repo
}

/// The common case: handed off by `agent:worker`.
fn repo_with_handed_off_ticket() -> tempfile::TempDir {
    repo_with_handed_off_ticket_by(&agent("worker"))
}

fn head_commit(repo: &Path) -> String {
    String::from_utf8_lossy(
        &std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string()
}

fn write_output_at(repo: &Path, id: &str, stem: &str, mut output: Value) {
    let dir = repo.join(".pulse/evidence").join(id);
    fs::create_dir_all(&dir).unwrap();
    output["environment"] = json!({"commit": head_commit(repo)});
    fs::write(
        dir.join(format!("{stem}.json")),
        serde_json::to_vec_pretty(&output).unwrap(),
    )
    .unwrap();
}

fn write_lane_output(repo: &Path, id: &str, role: &str, output: Value) {
    write_output_at(repo, id, role, output);
}

fn passing_output() -> Value {
    json!({
        "verdict": "pass",
        "acceptance": [{"id": "AC-1", "status": "pass", "how": "reran it"}],
        "cases": [], "findings": [], "commands_run": [],
    })
}

fn events(repo: &Path) -> Vec<(String, Value)> {
    let dir = repo.join(".pulse/events");
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return out;
    };
    let mut files: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    files.sort();
    for path in files {
        for line in fs::read_to_string(&path).unwrap().lines() {
            if line.trim().is_empty() {
                continue;
            }
            let event: Value = serde_json::from_str(line).unwrap();
            out.push((
                event["event_type"].as_str().unwrap().to_string(),
                event["payload"].clone(),
            ));
        }
    }
    out
}

#[test]
fn a_ticket_that_has_not_handed_off_has_nothing_to_review() {
    let repo = git_repo("profiles:\n  cli-low: {lanes: [review-correctness]}\n");
    push_record(repo.path(), ready_ticket("TK-a3f9"));

    let err = prepare(
        repo.path(),
        &agent("review-correctness"),
        "TK-a3f9",
        "review-correctness",
        false,
        None,
    )
    .unwrap_err();
    assert_eq!(err.code(), "lane_not_verifying");
    assert!(err.hint().is_some());
}

#[test]
fn a_lane_outside_the_profile_needs_force() {
    let repo = repo_with_handed_off_ticket();

    let err = prepare(
        repo.path(),
        &agent("review-adversarial"),
        "TK-a3f9",
        "review-adversarial",
        false,
        None,
    )
    .unwrap_err();
    assert_eq!(err.code(), "lane_not_in_profile");

    prepare(
        repo.path(),
        &agent("review-adversarial"),
        "TK-a3f9",
        "review-adversarial",
        true,
        None,
    )
    .unwrap();
}

#[test]
fn a_prepared_lane_gets_the_claim_to_check_and_never_the_worker_narrative() {
    let repo = repo_with_handed_off_ticket();

    let input_path = prepare(
        repo.path(),
        &agent("review-correctness"),
        "TK-a3f9",
        "review-correctness",
        false,
        None,
    )
    .unwrap();
    let input: Value = serde_json::from_slice(&fs::read(&input_path).unwrap()).unwrap();

    assert_eq!(input["id"], "TK-a3f9");
    assert_eq!(input["acceptance"][0]["id"], "AC-1");
    assert!(!input["handoff_commit"].as_str().unwrap().is_empty());
    for narrative in ["summary", "verify_results", "checkpoints", "notes"] {
        assert!(
            input.get(narrative).is_none(),
            "lane input leaks {narrative}: {input}"
        );
    }
}

#[test]
fn story_scope_lane_without_surface_or_risk_is_profile_missing_even_with_force() {
    // A1: `--force` must not route a story-scope lane run around a Story
    // that never got surface/risk set — the ready gate does not require
    // either on a Story, so this is a real, unforceable data gap.
    let repo = git_repo("profiles: {}\n");
    push_record(
        repo.path(),
        json!({
            "schema": 3, "id": "ST-1111", "kind": "story", "title": "s",
            "status": "ready", "revision": 1,
            "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
            "outcome": "o",
        }),
    );

    let err = prepare(
        repo.path(),
        &agent("qa-cli"),
        "ST-1111",
        "qa-cli",
        true,
        None,
    )
    .unwrap_err();
    assert_eq!(err.code(), "profile_missing");
    assert!(err.hint().is_some());
}

#[test]
fn story_scope_lane_routes_through_its_cases_surfaces_without_force() {
    // Dogfood ST-1 F18: a story classified api-medium must still run qa-ui
    // for its ui-surface cases — the profile gate unions the case surfaces
    // instead of demanding --force.
    let repo = git_repo(
        "profiles:\n  api-medium: {lanes: [review-correctness, qa-api]}\n  ui-medium: {lanes: [review-correctness, qa-ui]}\n",
    );
    push_record(
        repo.path(),
        json!({
            "schema": 3, "id": "ST-1111", "kind": "story", "title": "s",
            "status": "ready", "revision": 1,
            "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
            "outcome": "o", "risk": "medium", "surface": "api",
            "qa_cases": [
                {"id": "QA-001", "surface": "api", "priority": "high", "steps": ["GET /"]},
                {"id": "QA-002", "surface": "ui", "priority": "high", "steps": ["http://127.0.0.1:3000/"]}
            ],
        }),
    );

    let input_path = prepare(
        repo.path(),
        &agent("qa-ui"),
        "ST-1111",
        "qa-ui",
        false,
        None,
    )
    .unwrap();
    let input: Value = serde_json::from_slice(&fs::read(&input_path).unwrap()).unwrap();
    let case_ids: Vec<&str> = input["qa_cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case["id"].as_str().unwrap())
        .collect();
    assert_eq!(case_ids, vec!["QA-001", "QA-002"]);
}

#[test]
fn sealing_a_lane_nobody_prepared_is_refused() {
    let repo = repo_with_handed_off_ticket();
    write_lane_output(
        repo.path(),
        "TK-a3f9",
        "review-correctness",
        passing_output(),
    );

    let err = seal(
        repo.path(),
        &agent("review-correctness"),
        "TK-a3f9",
        "review-correctness",
        None,
    )
    .unwrap_err();
    assert_eq!(err.code(), "lane_not_prepared");
    assert!(err.hint().is_some());
}

#[test]
fn the_actor_that_handed_off_cannot_also_seal_a_lane_on_it() {
    // The one-session trap: a human (or a host session running as one) works
    // the Ticket, hands it off, and then reviews its own work. The close gate
    // has always refused a lane receipt whose actor is the handoff actor;
    // with the lane's actor now being whoever sealed it — rather than a name
    // Pulse synthesized, which never matched anyone — that check can fail
    // fast here instead of at a close that rejects a receipt already on the
    // record.
    let reviewer = human("quan");
    let repo = repo_with_handed_off_ticket_by(&reviewer);
    prepare(
        repo.path(),
        &reviewer,
        "TK-a3f9",
        "review-correctness",
        false,
        None,
    )
    .unwrap();
    write_lane_output(
        repo.path(),
        "TK-a3f9",
        "review-correctness",
        passing_output(),
    );

    let err = seal(
        repo.path(),
        &reviewer,
        "TK-a3f9",
        "review-correctness",
        None,
    )
    .unwrap_err();
    assert_eq!(err.code(), "lane_actor_not_independent");
    assert!(err.hint().is_some());

    // A second human is evidence; the refusal is about identity, not kind.
    let ticket = seal(
        repo.path(),
        &human("someone-else"),
        "TK-a3f9",
        "review-correctness",
        None,
    )
    .unwrap();
    assert_eq!(ticket["verdicts"]["review-correctness"]["verdict"], "pass");
}

#[test]
fn a_worker_role_may_not_record_a_lane_receipt_at_all() {
    let repo = repo_with_handed_off_ticket();
    prepare(
        repo.path(),
        &agent("worker"),
        "TK-a3f9",
        "review-correctness",
        false,
        None,
    )
    .unwrap();
    write_lane_output(
        repo.path(),
        "TK-a3f9",
        "review-correctness",
        passing_output(),
    );

    // Even under a different name, an agent that is not a lane role is
    // refused before the independence check runs.
    let err = seal(
        repo.path(),
        &agent("worker-2"),
        "TK-a3f9",
        "review-correctness",
        None,
    )
    .unwrap_err();
    assert_eq!(err.code(), "role_forbidden");
}

#[test]
fn a_sealed_receipt_carries_the_sealing_actor_and_the_verdict() {
    let repo = repo_with_handed_off_ticket();
    prepare(
        repo.path(),
        &agent("review-correctness"),
        "TK-a3f9",
        "review-correctness",
        false,
        None,
    )
    .unwrap();
    write_lane_output(
        repo.path(),
        "TK-a3f9",
        "review-correctness",
        passing_output(),
    );

    let ticket = seal(
        repo.path(),
        &agent("review-correctness"),
        "TK-a3f9",
        "review-correctness",
        None,
    )
    .unwrap();
    assert_eq!(ticket["verdicts"]["review-correctness"]["verdict"], "pass");
    assert_eq!(ticket["status"], "verifying");

    let receipts = pulse::evidence::receipt::list_receipts(repo.path())
        .unwrap()
        .receipts;
    let lane_receipt = receipts
        .iter()
        .find(|receipt| receipt.kind == "lane")
        .expect("a lane receipt");
    assert_eq!(lane_receipt.actor, "agent:review-correctness");
    assert_eq!(lane_receipt.payload["verdict"], "pass");

    // The snapshot is consumed, so the same output cannot be sealed twice
    // without a fresh preparation.
    let err = seal(
        repo.path(),
        &agent("review-correctness"),
        "TK-a3f9",
        "review-correctness",
        None,
    )
    .unwrap_err();
    assert_eq!(err.code(), "lane_not_prepared");
}

#[test]
fn a_fail_verdict_returns_the_ticket_to_active_for_rework() {
    let repo = repo_with_handed_off_ticket();
    prepare(
        repo.path(),
        &agent("review-correctness"),
        "TK-a3f9",
        "review-correctness",
        false,
        None,
    )
    .unwrap();
    write_lane_output(
        repo.path(),
        "TK-a3f9",
        "review-correctness",
        json!({
            "verdict": "fail",
            "acceptance": [{"id": "AC-1", "status": "fail", "how": "reran it, it fails"}],
            "cases": [],
            "findings": [{"id": "F-1", "ref": "AC-1", "summary": "off by one",
                          "owner": "src/x.rs",
                          "check": {"argv": ["true"], "exit": 0},
                          "severity": "high", "status": "open"}],
            "commands_run": [],
        }),
    );

    let ticket = seal(
        repo.path(),
        &agent("review-correctness"),
        "TK-a3f9",
        "review-correctness",
        None,
    )
    .unwrap();
    assert_eq!(ticket["status"], "active");
    assert_eq!(ticket["verdicts"]["review-correctness"]["verdict"], "fail");
}

#[test]
fn a_rejected_output_can_be_fixed_and_resealed_against_the_same_snapshot() {
    let repo = repo_with_handed_off_ticket();
    prepare(
        repo.path(),
        &agent("review-correctness"),
        "TK-a3f9",
        "review-correctness",
        false,
        None,
    )
    .unwrap();
    // A closed schema: one extra key and the whole file is refused.
    write_lane_output(
        repo.path(),
        "TK-a3f9",
        "review-correctness",
        json!({
            "verdict": "pass", "acceptance": [], "cases": [], "findings": [],
            "commands_run": [], "worktree_dirty": false,
        }),
    );
    let err = seal(
        repo.path(),
        &agent("review-correctness"),
        "TK-a3f9",
        "review-correctness",
        None,
    )
    .unwrap_err();
    assert_eq!(err.code(), "lane_output_invalid");

    write_lane_output(
        repo.path(),
        "TK-a3f9",
        "review-correctness",
        passing_output(),
    );
    let ticket = seal(
        repo.path(),
        &agent("review-correctness"),
        "TK-a3f9",
        "review-correctness",
        None,
    )
    .unwrap();
    assert_eq!(ticket["verdicts"]["review-correctness"]["verdict"], "pass");
}

#[test]
fn preparing_and_sealing_emit_the_paired_run_events() {
    // Dogfood ST-1 F8: a lane that never seals must still be visible in
    // `events tail`.
    let repo = repo_with_handed_off_ticket();
    prepare(
        repo.path(),
        &agent("review-correctness"),
        "TK-a3f9",
        "review-correctness",
        false,
        None,
    )
    .unwrap();
    let after_prepare = events(repo.path());
    assert!(
        after_prepare
            .iter()
            .any(|(kind, payload)| kind == "run.started"
                && payload["role"] == "review-correctness"),
        "no run.started for the prepared lane: {after_prepare:?}"
    );
    assert!(
        !after_prepare
            .iter()
            .any(|(kind, payload)| kind == "run.completed"
                && payload["role"] == "review-correctness"),
        "a lane that has not sealed must not look completed: {after_prepare:?}"
    );

    write_lane_output(
        repo.path(),
        "TK-a3f9",
        "review-correctness",
        passing_output(),
    );
    seal(
        repo.path(),
        &agent("review-correctness"),
        "TK-a3f9",
        "review-correctness",
        None,
    )
    .unwrap();
    let after_seal = events(repo.path());
    assert!(
        after_seal
            .iter()
            .any(|(kind, payload)| kind == "run.completed"
                && payload["role"] == "review-correctness"
                && payload["outcome"] == "sealed"
                && payload["verdict"] == "pass"),
        "no sealed run.completed: {after_seal:?}"
    );
}

#[test]
fn a_lane_prepared_and_never_sealed_is_a_doctor_finding() {
    let repo = repo_with_handed_off_ticket();
    assert!(pulse::kernel::doctor::run(repo.path())
        .unwrap()
        .stale_lane_preparations
        .is_empty());

    prepare(
        repo.path(),
        &agent("review-correctness"),
        "TK-a3f9",
        "review-correctness",
        false,
        None,
    )
    .unwrap();

    let report = pulse::kernel::doctor::run(repo.path()).unwrap();
    let stale = &report.stale_lane_preparations;
    assert_eq!(stale.len(), 1, "{stale:?}");
    assert_eq!(stale[0].id, "TK-a3f9");
    assert_eq!(stale[0].role, "review-correctness");
    assert_eq!(
        stale[0].prepared_by.as_deref(),
        Some("agent:review-correctness")
    );
    assert!(report.warning_count() >= 1);
}

// ---------------------------------------------------------------------------
// Decision 0027 C2/C3: review panel seats and `pulse lane reconcile`
// ---------------------------------------------------------------------------

const PANEL_PROFILE: &str =
    "profiles:\n  cli-low:\n    lanes: [review-correctness]\n    panels:\n      review-correctness: {count: 3, quorum: 2}\n";

const ROLE: &str = "review-correctness";

/// A repo whose `TK-a3f9` is `verifying`, handed off by `agent:worker`, under
/// a profile that declares a 3-seat / quorum-2 panel on `review-correctness`.
fn repo_with_panel_ticket() -> tempfile::TempDir {
    let repo = git_repo(PANEL_PROFILE);
    push_record(repo.path(), ready_ticket("TK-a3f9"));
    let worker = agent("worker");
    acquire_lease(repo.path(), &worker, "TK-a3f9", "worker", "run_1", 3600).unwrap();
    handoff(repo.path(), &worker, "TK-a3f9", handoff_input()).unwrap();
    repo
}

fn seat_actor(seat: u32) -> ActorRef {
    agent(&format!("{ROLE}-{seat}"))
}

fn seat_output(verdict: &str, findings: Value) -> Value {
    json!({
        "verdict": verdict,
        "acceptance": [],
        "cases": [],
        "findings": findings,
        "commands_run": [],
    })
}

/// `pulse lane input --seat n` then `pulse lane seal --seat n`, exactly as the
/// host would drive one seat.
fn seal_seat(repo: &Path, seat: u32, output: Value) -> Result<Value, pulse::PulseError> {
    let actor = seat_actor(seat);
    prepare(repo, &actor, "TK-a3f9", ROLE, false, Some(seat)).unwrap();
    write_output_at(repo, "TK-a3f9", &format!("{ROLE}.{seat}"), output);
    seal(repo, &actor, "TK-a3f9", ROLE, Some(seat))
}

fn write_seat_votes(repo: &Path, seat: u32, votes: Value) {
    let dir = repo.join(".pulse/evidence/TK-a3f9");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join(format!("{ROLE}.reconcile.{seat}.json")),
        serde_json::to_vec_pretty(&json!({"votes": votes})).unwrap(),
    )
    .unwrap();
}

/// The one reconciled `lane` receipt this round sealed.
fn lane_payload(repo: &Path) -> Value {
    pulse::evidence::receipt::list_receipts(repo)
        .unwrap()
        .receipts
        .into_iter()
        .find(|receipt| receipt.kind == "lane")
        .expect("a reconciled lane receipt")
        .payload
}

fn high_finding_with_check(argv: &str, exit: i64) -> Value {
    json!([{
        "id": "F-1", "ref": "AC-1", "summary": "broken", "owner": "src/x.rs",
        "check": {"argv": [argv], "exit": exit},
        "severity": "high", "status": "open"
    }])
}

fn high_finding_without_check() -> Value {
    json!([{
        "id": "F-1", "ref": "AC-1", "summary": "broken", "owner": "src/x.rs",
        "severity": "high", "status": "open"
    }])
}

fn reconciler() -> ActorRef {
    human("reconciler")
}

fn seal_three_seats(repo: &Path, first: Value) {
    seal_seat(repo, 1, first).unwrap();
    seal_seat(repo, 2, seat_output("pass", json!([]))).unwrap();
    seal_seat(repo, 3, seat_output("pass", json!([]))).unwrap();
}

#[test]
fn seat_required_when_the_profile_declares_a_panel() {
    let repo = repo_with_panel_ticket();
    let err = prepare(repo.path(), &agent(ROLE), "TK-a3f9", ROLE, false, None).unwrap_err();
    assert_eq!(err.code(), "lane_seat_required");
    assert!(err.hint().is_some());
    assert!(err.to_string().contains('3'), "{err}");
}

#[test]
fn seat_refused_without_a_panel() {
    let repo = repo_with_handed_off_ticket();
    let err = prepare(repo.path(), &agent(ROLE), "TK-a3f9", ROLE, false, Some(1)).unwrap_err();
    assert_eq!(err.code(), "lane_seat_invalid");
    assert!(err.hint().is_some());
}

#[test]
fn seat_out_of_range_is_refused() {
    let repo = repo_with_panel_ticket();
    let err = prepare(repo.path(), &agent(ROLE), "TK-a3f9", ROLE, false, Some(4)).unwrap_err();
    assert_eq!(err.code(), "lane_seat_invalid");
}

#[test]
fn a_failing_seat_does_not_bounce_the_ticket() {
    // Decision 0027 C2: a seat's `fail` is a vote. `count` seats must not each
    // drag the Ticket back to `active` before reconciliation has run.
    let repo = repo_with_panel_ticket();
    let ticket = seal_seat(
        repo.path(),
        1,
        seat_output("fail", high_finding_with_check("true", 0)),
    )
    .unwrap();
    assert_eq!(ticket["status"], "verifying");
    assert!(
        ticket.get("verdicts").is_none(),
        "a seat must not write verdicts: {ticket}"
    );
    let receipts = pulse::evidence::receipt::list_receipts(repo.path())
        .unwrap()
        .receipts;
    assert_eq!(receipts.iter().filter(|r| r.kind == "lane_seat").count(), 1);
    assert_eq!(receipts.iter().filter(|r| r.kind == "lane").count(), 0);
}

#[test]
fn seat_input_carries_nothing_from_other_seats() {
    // Round 1 is blind (decision 0027 §3): seat 2's input is byte-for-byte
    // seat 1's, and never carries seat 1's output or actor.
    let repo = repo_with_panel_ticket();
    let first = prepare(repo.path(), &seat_actor(1), "TK-a3f9", ROLE, false, Some(1)).unwrap();
    seat_actor(1);
    write_output_at(
        repo.path(),
        "TK-a3f9",
        &format!("{ROLE}.1"),
        seat_output(
            "fail",
            json!([{"id": "F-1", "ref": "AC-1", "summary": "unique-marker-xyz",
                    "owner": "src/x.rs", "severity": "high", "status": "open"}]),
        ),
    );
    let second = prepare(repo.path(), &seat_actor(2), "TK-a3f9", ROLE, false, Some(2)).unwrap();

    let bytes = fs::read(&first).unwrap();
    assert_eq!(bytes, fs::read(&second).unwrap(), "round 1 must be blind");
    let text = String::from_utf8_lossy(&bytes);
    assert!(!text.contains("unique-marker-xyz"), "{text}");
    assert!(!text.contains("review-correctness-1"), "{text}");
}

#[test]
fn a_check_that_runs_beats_the_votes() {
    // (a) MACHINE WINS: the finding's `check.argv` fails, so it stands no
    // matter how many seats refute it.
    let repo = repo_with_panel_ticket();
    seal_three_seats(
        repo.path(),
        seat_output("fail", high_finding_with_check("false", 0)),
    );
    reconcile_prepare(repo.path(), &reconciler(), "TK-a3f9", ROLE).unwrap();
    for seat in 1..=3 {
        write_seat_votes(
            repo.path(),
            seat,
            json!([{"rid": "RF-1", "vote": "refuted", "how": "looks fine"}]),
        );
    }

    let ticket = reconcile(
        repo.path(),
        &reconciler(),
        "TK-a3f9",
        ROLE,
        Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(ticket["status"], "active");
    let payload = lane_payload(repo.path());
    assert_eq!(payload["verdict"], "fail");
    assert_eq!(payload["findings"][0]["status"], "open");
    assert_eq!(payload["findings"][0]["severity"], "high");
}

#[test]
fn a_lone_finding_without_confirmation_is_lowered_and_closes() {
    // (b) NOT ENOUGH QUORUM: nobody confirms, so the finding is downgraded and
    // the reconciled verdict passes; the Ticket can close.
    let repo = repo_with_panel_ticket();
    seal_three_seats(
        repo.path(),
        seat_output("fail", high_finding_without_check()),
    );
    reconcile_prepare(repo.path(), &reconciler(), "TK-a3f9", ROLE).unwrap();

    reconcile(
        repo.path(),
        &reconciler(),
        "TK-a3f9",
        ROLE,
        Duration::from_secs(30),
    )
    .unwrap();
    let payload = lane_payload(repo.path());
    assert_eq!(payload["verdict"], "pass");
    assert_eq!(payload["findings"][0]["status"], "unconfirmed");
    assert_eq!(payload["findings"][0]["severity"], "low");

    let ticket = close(repo.path(), &human("quan"), "TK-a3f9").unwrap();
    assert_eq!(ticket["status"], "done");
}

#[test]
fn a_finding_confirmed_by_quorum_stands_and_close_refuses() {
    // (c) QUORUM: one raiser plus one confirming seat keeps it open; with no
    // checkable finding the existing rule lowers `fail` to `inconclusive`,
    // which is exactly why close refuses.
    let repo = repo_with_panel_ticket();
    seal_three_seats(
        repo.path(),
        seat_output("fail", high_finding_without_check()),
    );
    reconcile_prepare(repo.path(), &reconciler(), "TK-a3f9", ROLE).unwrap();
    write_seat_votes(
        repo.path(),
        2,
        json!([{"rid": "RF-1", "vote": "confirmed", "how": "reproduced: src/x.rs:41"}]),
    );
    write_seat_votes(
        repo.path(),
        3,
        json!([{"rid": "RF-1", "vote": "refuted", "how": "could not reproduce"}]),
    );

    let ticket = reconcile(
        repo.path(),
        &reconciler(),
        "TK-a3f9",
        ROLE,
        Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(ticket["status"], "verifying");
    let payload = lane_payload(repo.path());
    assert_eq!(payload["verdict"], "inconclusive");
    assert_eq!(payload["findings"][0]["status"], "open");

    let err = close(repo.path(), &human("quan"), "TK-a3f9").unwrap_err();
    assert_eq!(err.code(), "gate_failed");
    assert!(
        err.to_string().contains("close_lane_not_satisfied"),
        "{err}"
    );
}

#[test]
fn a_check_that_passes_resolves_the_finding() {
    // (d) The worker fixed the bug: the declared check now matches, so the
    // finding resolves and the reconciled verdict passes.
    let repo = repo_with_panel_ticket();
    seal_three_seats(
        repo.path(),
        seat_output("fail", high_finding_with_check("true", 0)),
    );
    reconcile_prepare(repo.path(), &reconciler(), "TK-a3f9", ROLE).unwrap();

    reconcile(
        repo.path(),
        &reconciler(),
        "TK-a3f9",
        ROLE,
        Duration::from_secs(30),
    )
    .unwrap();
    let payload = lane_payload(repo.path());
    assert_eq!(payload["verdict"], "pass");
    assert_eq!(payload["findings"][0]["status"], "resolved");
}

#[test]
fn reconcile_prepare_names_the_missing_seats() {
    // (e) Two of three seats sealed: `--prepare` names the one that is not.
    let repo = repo_with_panel_ticket();
    seal_seat(repo.path(), 1, seat_output("pass", json!([]))).unwrap();
    seal_seat(repo.path(), 2, seat_output("pass", json!([]))).unwrap();
    let err = reconcile_prepare(repo.path(), &reconciler(), "TK-a3f9", ROLE).unwrap_err();
    assert_eq!(err.code(), "reconcile_seats_missing");
    assert!(err.to_string().contains('3'), "{err}");
}

#[test]
fn close_refuses_while_only_seat_receipts_exist() {
    // (f)
    let repo = repo_with_panel_ticket();
    seal_three_seats(repo.path(), seat_output("pass", json!([])));
    let err = close(repo.path(), &human("quan"), "TK-a3f9").unwrap_err();
    assert_eq!(err.code(), "gate_failed");
    assert!(err.to_string().contains("panel of 3"), "{err}");
    assert!(
        err.to_string().contains("close_lane_not_satisfied"),
        "a seat receipt must never satisfy close: {err}"
    );
}

#[test]
fn a_new_handoff_round_ignores_the_old_seats() {
    // (g) A re-handoff opens a new round: the old seats do not count, and
    // `--prepare` reports every seat missing.
    let repo = repo_with_panel_ticket();
    seal_three_seats(repo.path(), seat_output("pass", json!([])));
    issues::mutate(repo.path(), |mut records| {
        let ticket = records
            .iter_mut()
            .find(|record| record["id"] == "TK-a3f9")
            .unwrap();
        ticket["status"] = json!("active");
        Ok(records)
    })
    .unwrap();
    // The first handoff consumed the lease; a re-handoff needs a fresh one.
    acquire_lease(
        repo.path(),
        &agent("worker"),
        "TK-a3f9",
        "worker",
        "run_1",
        3600,
    )
    .unwrap();
    handoff(repo.path(), &agent("worker"), "TK-a3f9", handoff_input()).unwrap();

    let err = reconcile_prepare(repo.path(), &reconciler(), "TK-a3f9", ROLE).unwrap_err();
    assert_eq!(err.code(), "reconcile_seats_missing");
    assert!(err.to_string().contains("1, 2, 3"), "{err}");
}

#[test]
fn a_seat_that_files_no_votes_does_not_stop_reconciliation() {
    // (h) One reviewer dying weakens the outcome, it does not break the run.
    let repo = repo_with_panel_ticket();
    seal_three_seats(
        repo.path(),
        seat_output("fail", high_finding_without_check()),
    );
    reconcile_prepare(repo.path(), &reconciler(), "TK-a3f9", ROLE).unwrap();
    write_seat_votes(
        repo.path(),
        1,
        json!([{"rid": "RF-1", "vote": "confirmed", "how": "src/x.rs:41"}]),
    );
    write_seat_votes(
        repo.path(),
        2,
        json!([{"rid": "RF-1", "vote": "confirmed", "how": "same repro"}]),
    );
    // seat 3 files nothing.

    reconcile(
        repo.path(),
        &reconciler(),
        "TK-a3f9",
        ROLE,
        Duration::from_secs(30),
    )
    .unwrap();
    let payload = lane_payload(repo.path());
    assert_eq!(payload["votes_summary"]["missing"], json!([3]));
}

#[test]
fn duplicate_findings_merge_and_their_raisers_count() {
    // (i) RF-2 is declared a duplicate of RF-1 by quorum, so it merges, and
    // RF-1 ends with two raisers — enough to stand.
    let repo = repo_with_panel_ticket();
    let same = |id: &str| {
        json!([{"id": id, "ref": "AC-1", "summary": "same bug", "owner": "src/x.rs",
                "severity": "high", "status": "open"}])
    };
    seal_seat(repo.path(), 1, seat_output("fail", same("F-1"))).unwrap();
    seal_seat(repo.path(), 2, seat_output("fail", same("F-1"))).unwrap();
    seal_seat(repo.path(), 3, seat_output("pass", json!([]))).unwrap();
    reconcile_prepare(repo.path(), &reconciler(), "TK-a3f9", ROLE).unwrap();
    for seat in 1..=2 {
        write_seat_votes(
            repo.path(),
            seat,
            json!([{"rid": "RF-2", "vote": "duplicate", "of": "RF-1", "how": "same root cause"}]),
        );
    }

    reconcile(
        repo.path(),
        &reconciler(),
        "TK-a3f9",
        ROLE,
        Duration::from_secs(30),
    )
    .unwrap();
    let payload = lane_payload(repo.path());
    let findings = payload["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0]["id"], "RF-1");
    assert_eq!(findings[0]["status"], "open");
    assert_eq!(
        payload["votes_summary"]["by_finding"]["RF-1"]["support"], 2,
        "{payload}"
    );
    assert!(
        payload["votes_summary"]["by_finding"].get("RF-2").is_none(),
        "a merged finding must not survive: {payload}"
    );
}

#[test]
fn doctor_reports_seat_and_reconcile_preparations() {
    // 4e: `stale_lane_preparations` scans every `<stem>.snapshot.json`, so a
    // seat and a reconciliation both show up as prepared-but-unsealed.
    let repo = repo_with_panel_ticket();
    seal_three_seats(repo.path(), seat_output("pass", json!([])));
    reconcile_prepare(repo.path(), &reconciler(), "TK-a3f9", ROLE).unwrap();
    prepare(repo.path(), &seat_actor(1), "TK-a3f9", ROLE, false, Some(1)).unwrap();

    let report = pulse::kernel::doctor::run(repo.path()).unwrap();
    let roles: Vec<&str> = report
        .stale_lane_preparations
        .iter()
        .map(|stale| stale.role.as_str())
        .collect();
    assert!(roles.contains(&"review-correctness.1"), "{roles:?}");
    assert!(roles.contains(&"review-correctness.reconcile"), "{roles:?}");
}
