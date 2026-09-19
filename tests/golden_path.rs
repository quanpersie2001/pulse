//! Golden path v3 (plan 0022 §15, F2 of P1.12): every step drives the real
//! `pulse` binary as a subprocess against a fresh temporary git repository.
//! Nothing here calls a `pulse::kernel`/`pulse::store` function directly —
//! that is the point: this proves the CLI surface, not the library, carries
//! a Ticket from `init` through `close-story`.
//!
//! The test plays the host: it runs the commands a worker session would run
//! (`claim`, `checkpoint`, `verify`, `handoff`), writes each lane's output
//! the way a dispatched lane agent would, and seals it under that lane's own
//! actor. Nothing here fakes an agent: the only commands that execute are the
//! ones Pulse runs itself, and those are the `verify[]` argv the Ticket
//! declares (decision 0026) — `true`, so the run is portable. The only thing
//! under test is which of those steps Pulse accepts.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{json, Value};

#[path = "common/bin.rs"]
mod common_bin;
#[path = "common/git.rs"]
mod common_git;

const PULSE_MD: &str = "\
fence_ignore: []
profiles:
  cli-low: {lanes: [review-correctness, qa-cli]}
";

/// What a dispatched lane agent writes before sealing: its verdict file
/// under `.pulse/evidence/<id>/<role>.json`, with the commit it actually
/// ran against.
fn write_lane_output(repo: &Path, id: &str, role: &str, body: Value) {
    let dir = repo.join(".pulse/evidence").join(id);
    fs::create_dir_all(&dir).unwrap();
    let mut body = body;
    body["environment"] = json!({"commit": common_git::git(repo, &["rev-parse", "HEAD"])});
    fs::write(
        dir.join(format!("{role}.json")),
        serde_json::to_vec_pretty(&body).unwrap(),
    )
    .unwrap();
}

fn write_file(path: &Path, body: &str) -> PathBuf {
    fs::write(path, body).unwrap();
    path.to_path_buf()
}

fn pulse_output(repo: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(common_bin::bin());
    command
        .arg("--repo-root")
        .arg(repo)
        .args(args)
        .env_remove("PULSE_ACTOR")
        // `pulse init` registers into the user registry (Decision 0023);
        // tests must never touch the real ~/.pulse.
        .env(
            "PULSE_REGISTRY",
            std::env::temp_dir().join("pulse-test-registry.json"),
        );
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("spawn pulse")
}

fn pulse_ok(repo: &Path, args: &[&str]) -> Value {
    pulse_ok_env(repo, args, &[])
}

fn pulse_ok_env(repo: &Path, args: &[&str], env: &[(&str, &str)]) -> Value {
    let output = pulse_output(repo, args, env);
    assert!(
        output.status.success(),
        "pulse {args:?} failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "pulse {args:?} did not print JSON: {error}\nstdout={}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn pulse_err_env(repo: &Path, args: &[&str], env: &[(&str, &str)]) -> Value {
    let output = pulse_output(repo, args, env);
    assert!(
        !output.status.success(),
        "pulse {args:?} unexpectedly succeeded:\nstdout={}",
        String::from_utf8_lossy(&output.stdout),
    );
    serde_json::from_slice(&output.stderr).unwrap_or_else(|error| {
        panic!(
            "pulse {args:?} did not print a JSON error: {error}\nstderr={}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn receipts(repo: &Path) -> Vec<Value> {
    let dir = repo.join(".pulse/receipts");
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            let content = fs::read_to_string(&path).unwrap();
            for line in content.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                out.push(serde_json::from_str(line).unwrap());
            }
        }
    }
    out
}

fn has_receipt(repo: &Path, kind: &str, subject_id: &str) -> bool {
    receipts(repo)
        .iter()
        .any(|r| r["kind"] == kind && r["subject"]["id"] == subject_id)
}

fn assert_issues_list_parses(repo: &Path) {
    let list = pulse_ok(repo, &["work", "list", "--json"]);
    assert!(
        list.is_array(),
        "pulse work list --json must print an array: {list}"
    );
}

/// Plan 0025 E1, end to end through the CLI: a friction recorded with
/// `pulse note --friction` survives the ticket's close (which only reports
/// it), blocks `close-story`, and stops blocking once
/// `pulse learn dismiss <id> --all --reason …` classifies it. `pulse learn
/// friction` is the window on the whole state machine.
#[test]
fn friction_blocks_close_story_until_dismissed() {
    let repo = tempfile::tempdir().unwrap();
    let harness = tempfile::tempdir().unwrap();

    fs::write(repo.path().join("README.md"), "# Friction Gate\n").unwrap();
    common_git::commit_all(repo.path());
    pulse_ok(repo.path(), &["init", "--json"]);
    fs::write(
        repo.path().join("PULSE.md"),
        "profiles:\n  cli-low: {lanes: [review-correctness]}\n",
    )
    .unwrap();
    common_git::commit_all(repo.path());

    let story = pulse_ok(repo.path(), &["work", "new", "story", "S", "--json"]);
    let story_id = story["id"].as_str().unwrap().to_string();
    let story_json = write_file(
        &harness.path().join("story.json"),
        &serde_json::to_string(&json!({
            "outcome": "the thing is done",
            "rules": [{"id": "BR-1", "text": "it works"}],
            "docs_written": ["docs/friction-gate.md"],
        }))
        .unwrap(),
    );
    // Plan 0025 F4: the story's rules must live in docs/ by close-story, so
    // the fixture carries the doc the gate will demand.
    fs::write(
        repo.path().join("docs/friction-gate.md"),
        "# Friction Gate\n\nThe behavior holds (BR-1).\n",
    )
    .unwrap();
    common_git::commit_all(repo.path());
    pulse_ok(
        repo.path(),
        &[
            "work",
            "update",
            &story_id,
            "--from",
            story_json.to_str().unwrap(),
            "--json",
        ],
    );
    pulse_ok(repo.path(), &["work", "ready", &story_id, "--json"]);

    let ticket_json = write_file(
        &harness.path().join("t.json"),
        &serde_json::to_string(&json!({
            "objective": "do the thing",
            "context": {"anchors": ["README.md"]},
            "acceptance": [{"id": "AC-1", "when": "w", "then": "t"}],
            "verify": [{"name": "unit", "argv": ["true"]}],
        }))
        .unwrap(),
    );
    let ticket = pulse_ok(
        repo.path(),
        &[
            "work",
            "new",
            "ticket",
            "T",
            "--story",
            &story_id,
            "--risk",
            "low",
            "--surface",
            "cli",
            "--json",
        ],
    );
    let ticket_id = ticket["id"].as_str().unwrap().to_string();
    pulse_ok(
        repo.path(),
        &[
            "work",
            "update",
            &ticket_id,
            "--from",
            ticket_json.to_str().unwrap(),
            "--json",
        ],
    );
    pulse_ok(repo.path(), &["work", "ready", &ticket_id, "--json"]);

    // Worker flow: claim -> verify -> handoff.
    let claimed = pulse_ok(
        repo.path(),
        &["claim", &ticket_id, "--actor", "agent:worker", "--json"],
    );
    let run_id = claimed["lease"]["run_id"].as_str().unwrap().to_string();
    pulse_ok(
        repo.path(),
        &["verify", &ticket_id, "--actor", "agent:worker", "--json"],
    );
    let handoff_json = write_file(
        &harness.path().join("handoff.json"),
        &serde_json::to_string(&json!({
            "run_id": run_id,
            "summary": "done",
            "acceptance": [{"id": "AC-1", "status": "done", "how": "ran it"}],
        }))
        .unwrap(),
    );
    pulse_ok(
        repo.path(),
        &[
            "handoff",
            &ticket_id,
            "--from",
            handoff_json.to_str().unwrap(),
            "--actor",
            "agent:worker",
            "--json",
        ],
    );

    // The lane reviews, then the ticket closes — reporting the friction but
    // not blocking on it (the skill caps learnings at one per ticket).
    write_lane_output(
        repo.path(),
        &ticket_id,
        "review-correctness",
        json!({
            "verdict": "pass",
            "acceptance": [{"id": "AC-1", "status": "pass", "how": "ok"}],
            "cases": [], "findings": [], "commands_run": [],
        }),
    );
    let verified = pulse_ok(
        repo.path(),
        &[
            "verify",
            &ticket_id,
            "--actor",
            "agent:review-correctness",
            "--json",
        ],
    );
    assert_eq!(verified["passed"], true);
    pulse_ok(
        repo.path(),
        &["lane", "input", &ticket_id, "review-correctness", "--json"],
    );
    pulse_ok(
        repo.path(),
        &[
            "lane",
            "seal",
            &ticket_id,
            "review-correctness",
            "--actor",
            "agent:review-correctness",
            "--json",
        ],
    );

    // Friction lands after the work (the reviewer's report), before close.
    let friction_text = "packet anchors pointed at a renamed module twice";
    pulse_ok(
        repo.path(),
        &[
            "note",
            &ticket_id,
            friction_text,
            "--friction",
            "--from",
            "agent:review-correctness",
            "--json",
        ],
    );
    let closed = pulse_ok_env(
        repo.path(),
        &["close", &ticket_id, "--json"],
        &[("PULSE_ACTOR", "human:test")],
    );
    let unclassified = closed["unclassified_friction"].as_array().unwrap();
    assert_eq!(unclassified.len(), 1, "{closed}");
    assert_eq!(unclassified[0]["text"], friction_text);
    let friction_key = unclassified[0]["key"].as_str().unwrap().to_string();
    assert!(friction_key.starts_with("evt_"), "{friction_key}");

    // `pulse learn friction` is the window the loop reads.
    let listed = pulse_ok(repo.path(), &["learn", "friction", &ticket_id, "--json"]);
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["key"], json!(friction_key));
    assert_eq!(listed[0]["state"]["kind"], "unclassified");

    // Close-story is blocked while it stays unclassified.
    let error = pulse_err_env(
        repo.path(),
        &["close-story", &story_id, "--json"],
        &[("PULSE_ACTOR", "human:test")],
    );
    assert_eq!(error["code"], "gate_failed");
    let message = error["message"].as_str().unwrap();
    assert!(
        message.contains("close_story_friction_unclassified"),
        "{message}"
    );
    assert!(
        message.contains(&format!("{ticket_id}#{friction_key}")),
        "{message}"
    );

    // A learning citation classifies too; here the dismissal path: every
    // actor may dismiss, the reason is required.
    let empty_reason = pulse_err_env(
        repo.path(),
        &[
            "learn",
            "dismiss",
            &ticket_id,
            "--all",
            "--reason",
            " ",
            "--actor",
            "human:test",
            "--json",
        ],
        &[("PULSE_ACTOR", "human:test")],
    );
    assert_eq!(empty_reason["code"], "friction_reason_missing");
    let dismissed = pulse_ok(
        repo.path(),
        &[
            "learn",
            "dismiss",
            &ticket_id,
            "--all",
            "--reason",
            "ticket-specific: the rename was local to this story",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(dismissed["dismissed"].as_array().unwrap().len(), 1);
    // Idempotent: re-dismissing the same key reports it skipped, not
    // re-dismissed. `--all` now selects nothing at all (nothing unclassified
    // is left).
    let again = pulse_ok(
        repo.path(),
        &[
            "learn",
            "dismiss",
            &ticket_id,
            &friction_key.clone(),
            "--reason",
            "ticket-specific",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(again["skipped"].as_array().unwrap().len(), 1);
    assert_eq!(again["dismissed"].as_array().unwrap().len(), 0);
    let nothing_left = pulse_ok(
        repo.path(),
        &[
            "learn",
            "dismiss",
            &ticket_id,
            "--all",
            "--reason",
            "ticket-specific",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(nothing_left["dismissed"].as_array().unwrap().len(), 0);
    assert_eq!(nothing_left["skipped"].as_array().unwrap().len(), 0);

    let story = pulse_ok_env(
        repo.path(),
        &["close-story", &story_id, "--json"],
        &[("PULSE_ACTOR", "human:test")],
    );
    assert_eq!(story["status"], "done");
    assert_issues_list_parses(repo.path());
}

#[test]
fn golden_path_new_to_close_story_driven_by_the_host() {
    let repo = tempfile::tempdir().unwrap();
    let harness = tempfile::tempdir().unwrap();

    // --- Bootstrap a real git repo with one tracked file and one commit. ---
    fs::write(repo.path().join("README.md"), "# Golden Path\n").unwrap();
    common_git::commit_all(repo.path());

    // --- Step 1: init. ---
    pulse_ok(repo.path(), &["init", "--json"]);
    assert!(repo.path().join("PULSE.md").is_file());
    assert!(repo.path().join(".pulse/prompts/worker.md").is_file());
    assert!(
        !repo.path().join(".pulse/runners.json").exists(),
        "v3 seeds no dispatch table: the host dispatches, Pulse gates"
    );
    assert!(repo.path().join("docs/README.md").is_file());
    let gitignore = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
    assert!(gitignore.contains("**/.pulse/runtime/"));

    // A custom profile: cli-low needs both review-correctness and qa-cli.
    fs::write(repo.path().join("PULSE.md"), PULSE_MD).unwrap();

    // --- Fixtures live outside the repo so they never dirty the tree.
    // cp.json/handoff.json are written at the worker step instead: their
    // run_id must be the one the claim minted (decision 0025 B3). ---

    // Commit the scaffold (PULSE.md, docs/README.md, AGENTS.md, .gitignore,
    // the prompts, the empty .pulse/issues.jsonl) as a clean baseline.
    common_git::commit_all(repo.path());

    // --- Step 2: Story. ---
    let story_json = write_file(
        &harness.path().join("story.json"),
        &serde_json::to_string(&json!({
            "outcome": "A user can create and complete tasks from the CLI.",
            "surface": "cli",
            "risk": "low",
            "qa_cases": [
                {"id": "QA-001", "intent": "create then list a task via the CLI",
                 "surface": "cli", "priority": "high"},
            ],
        }))
        .unwrap(),
    );
    let story = pulse_ok(repo.path(), &["work", "new", "story", "S", "--json"]);
    let story_id = story["id"].as_str().unwrap().to_string();
    pulse_ok(
        repo.path(),
        &[
            "work",
            "update",
            &story_id,
            "--from",
            story_json.to_str().unwrap(),
            "--json",
        ],
    );
    let story = pulse_ok(repo.path(), &["work", "ready", &story_id, "--json"]);
    assert_eq!(story["status"], "ready");
    assert_issues_list_parses(repo.path());

    // --- Step 3: Ticket. ---
    let ticket_json = write_file(
        &harness.path().join("t.json"),
        &serde_json::to_string(&json!({
            "objective": "Add a `tasks add`/`tasks list` CLI path.",
            "context": {"anchors": ["README.md"]},
            "acceptance": [
                {"id": "AC-1", "when": "a task is added and listed", "then": "it appears in the list"},
            ],
            "verify": [{"name": "unit", "argv": ["true"]}],
            "qa_cases": ["QA-001"],
        }))
        .unwrap(),
    );
    let ticket = pulse_ok(
        repo.path(),
        &[
            "work",
            "new",
            "ticket",
            "T",
            "--story",
            &story_id,
            "--risk",
            "low",
            "--surface",
            "cli",
            "--json",
        ],
    );
    let ticket_id = ticket["id"].as_str().unwrap().to_string();
    pulse_ok(
        repo.path(),
        &[
            "work",
            "update",
            &ticket_id,
            "--from",
            ticket_json.to_str().unwrap(),
            "--json",
        ],
    );
    let ticket = pulse_ok(repo.path(), &["work", "ready", &ticket_id, "--json"]);
    assert_eq!(ticket["status"], "ready");
    assert_issues_list_parses(repo.path());

    // --- Step 5: the worker session: claim, checkpoint, hand off. ---
    let ticket = pulse_ok(
        repo.path(),
        &["claim", &ticket_id, "--actor", "agent:worker", "--json"],
    );
    assert_eq!(ticket["status"], "active");
    assert_eq!(ticket["lease"]["actor"], "agent:worker");

    // Decision 0025 B3: checkpoint and handoff carry the run_id the claim
    // minted — a real worker reads protocol.run_id from the packet, which
    // is the lease the claim wrote. A stale run_id is refused.
    let run_id = ticket["lease"]["run_id"].as_str().unwrap().to_string();
    let cp_json = write_file(
        &harness.path().join("cp.json"),
        &serde_json::to_string(&json!({
            "run_id": run_id,
            "done_ac": ["AC-1"],
            "in_progress": "",
            "next": [],
            "files": ["README.md"],
            "decisions": [],
            "gotchas": [],
            "commands_run": [{"argv": ["true"], "exit": 0}],
        }))
        .unwrap(),
    );
    let handoff_json = write_file(
        &harness.path().join("handoff.json"),
        &serde_json::to_string(&json!({
            "run_id": run_id,
            "summary": "Implemented the CLI task path.",
            "changed_files": [],
            "acceptance": [{"id": "AC-1", "status": "done", "how": "ran the CLI path"}],
            // Informational since decision 0026: the handoff gate reads the
            // receipt `pulse verify` seals below, never this claim.
            "verify_results": [{"name": "unit", "exit": 0}],
            "docs_updated": [],
            "learnings_used": [],
            "friction": [],
            "open_risks": [],
        }))
        .unwrap(),
    );
    pulse_ok(
        repo.path(),
        &[
            "checkpoint",
            &ticket_id,
            "--from",
            cp_json.to_str().unwrap(),
            "--actor",
            "agent:worker",
            "--json",
        ],
    );
    // Decision 0026: this Ticket declares `verify[]`, so what the handoff
    // gate reads is the receipt `pulse verify` seals — the worker's own
    // `verify_results` claim above no longer counts for anything. The
    // fixture's argv is `true`, which runs anywhere.
    let verified = pulse_ok(
        repo.path(),
        &["verify", &ticket_id, "--actor", "agent:worker", "--json"],
    );
    assert_eq!(verified["passed"], true, "{verified}");
    let ticket = pulse_ok(
        repo.path(),
        &[
            "handoff",
            &ticket_id,
            "--from",
            handoff_json.to_str().unwrap(),
            "--actor",
            "agent:worker",
            "--json",
        ],
    );
    assert_eq!(ticket["status"], "verifying");
    assert!(
        ticket["lease"].is_null(),
        "handoff releases the lease: {ticket}"
    );
    assert!(
        has_receipt(repo.path(), "checkpoint", &ticket_id),
        "expected a checkpoint receipt: {:?}",
        receipts(repo.path())
    );
    assert!(
        has_receipt(repo.path(), "verify", &ticket_id),
        "expected a verify receipt: {:?}",
        receipts(repo.path())
    );
    assert!(
        has_receipt(repo.path(), "handoff", &ticket_id),
        "expected a handoff receipt: {:?}",
        receipts(repo.path())
    );
    assert_issues_list_parses(repo.path());

    // --- Step 6: both lanes in the cli-low profile. For each: Pulse says
    // what the lane may see, the host runs it, Pulse decides whether the
    // output is evidence. ---
    for (role, output) in [
        (
            "review-correctness",
            json!({
                "verdict": "pass",
                "acceptance": [{"id": "AC-1", "status": "pass", "how": "looks correct"}],
                "cases": [], "findings": [], "commands_run": [],
            }),
        ),
        (
            "qa-cli",
            json!({
                "verdict": "pass",
                "acceptance": [],
                "cases": [{"id": "QA-001", "status": "pass",
                           "observation": "cli path returns the expected output",
                           "artifacts": []}],
                "findings": [], "commands_run": [],
            }),
        ),
    ] {
        let prepared = pulse_ok(repo.path(), &["lane", "input", &ticket_id, role, "--json"]);
        let input_path = prepared["input"].as_str().unwrap();
        assert!(
            repo.path().join(input_path).is_file(),
            "lane input was not written at {input_path}"
        );
        let lane_input: Value =
            serde_json::from_slice(&fs::read(repo.path().join(input_path)).unwrap()).unwrap();
        assert!(
            lane_input.get("summary").is_none(),
            "a lane must never see the worker's narrative: {lane_input}"
        );

        write_lane_output(repo.path(), &ticket_id, role, output);
        // Decision 0026 D2: a review lane that reports `pass` on a ticket
        // with a declared verify[] must have run the commands itself — its
        // own receipt, on this fence, is the evidence. Without it the seal
        // downgrades the verdict to `inconclusive`. qa-* lanes are not held
        // to this.
        if role.starts_with("review-") {
            let lane_actor = format!("agent:{role}");
            let verified = pulse_ok(
                repo.path(),
                &["verify", &ticket_id, "--actor", &lane_actor, "--json"],
            );
            assert_eq!(verified["passed"], true, "{verified}");
        }
        let actor = format!("agent:{role}");
        let ticket = pulse_ok(
            repo.path(),
            &[
                "lane", "seal", &ticket_id, role, "--actor", &actor, "--json",
            ],
        );
        assert_eq!(ticket["verdicts"][role]["verdict"], "pass");
    }
    assert_issues_list_parses(repo.path());

    // --- Step 7: a tracked edit outside .pulse/ staled the handoff. ---
    let original_readme = fs::read_to_string(repo.path().join("README.md")).unwrap();
    fs::write(
        repo.path().join("README.md"),
        format!("{original_readme}\nedited after handoff\n"),
    )
    .unwrap();
    let error = pulse_err_env(
        repo.path(),
        &["close", &ticket_id, "--json"],
        &[("PULSE_ACTOR", "human:test")],
    );
    assert_eq!(error["code"], "gate_failed");
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("close_source_stale"), "{message}");
    assert!(message.contains("README.md"), "{message}");
    fs::write(repo.path().join("README.md"), &original_readme).unwrap();

    // --- Step 8: close. ---
    let ticket = pulse_ok_env(
        repo.path(),
        &["close", &ticket_id, "--json"],
        &[("PULSE_ACTOR", "human:test")],
    );
    assert_eq!(ticket["status"], "done");
    assert!(has_receipt(repo.path(), "close", &ticket_id));
    assert_issues_list_parses(repo.path());

    // --- Step 9: story-scope qa-cli, then close-story. The Story's own
    // surface/risk (set above) resolve the cli-low profile without needing
    // --force (A1: --force is no longer required to route a story-scope
    // lane run past profile resolution).
    pulse_ok(
        repo.path(),
        &["lane", "input", &story_id, "qa-cli", "--json"],
    );
    write_lane_output(
        repo.path(),
        &story_id,
        "qa-cli",
        json!({
            "verdict": "pass",
            "acceptance": [],
            "cases": [{"id": "QA-001", "status": "pass",
                       "observation": "create then list works end to end",
                       "artifacts": []}],
            "findings": [], "commands_run": [],
        }),
    );
    pulse_ok(
        repo.path(),
        &[
            "lane",
            "seal",
            &story_id,
            "qa-cli",
            "--actor",
            "agent:qa-cli",
            "--json",
        ],
    );
    assert!(
        has_receipt(repo.path(), "lane", &story_id),
        "expected a story-scope lane receipt: {:?}",
        receipts(repo.path())
    );
    let story = pulse_ok(repo.path(), &["close-story", &story_id, "--json"]);
    assert_eq!(story["status"], "done");
    assert_issues_list_parses(repo.path());
}

/// Dogfood 0025, F8: `pulse packet` must print the packet itself, with or
/// without `--json` — two real workers in the 0025 dogfood each got only a
/// 19-byte `packet for TK-…` header and had to rebuild the packet from
/// `work show --json` + `learn applicable`.
#[test]
fn packet_prints_the_packet_itself_without_a_json_flag() {
    let repo = tempfile::tempdir().unwrap();
    fs::write(repo.path().join("README.md"), "# Golden Path\n").unwrap();
    common_git::commit_all(repo.path());
    pulse_ok(repo.path(), &["init", "--json"]);

    let ticket = pulse_ok(
        repo.path(),
        &[
            "work",
            "new",
            "ticket",
            "t",
            "--risk",
            "low",
            "--surface",
            "cli",
            "--json",
        ],
    );
    let id = ticket["id"].as_str().unwrap();

    // No --json: stdout is the packet document, not a header line.
    let packet = pulse_ok(repo.path(), &["packet", id]);
    assert_eq!(packet["issue"]["id"], id);
    assert!(packet.get("protocol").is_some(), "{packet}");

    // --json keeps working (accepted, same output shape).
    let with_flag = pulse_ok(repo.path(), &["packet", id, "--json"]);
    assert_eq!(with_flag["issue"]["id"], id);
}
