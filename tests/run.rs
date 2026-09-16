//! `pulse::kernel::run::run_worker` integration tests (plan 0022 §10.1).
//!
//! Drives the real worker loop (real subprocess spawns via a fake shell
//! "agent" script) against a temporary git repo, calling the library
//! function directly rather than the CLI. Needs `CARGO_BIN_EXE_pulse`
//! (only set for this crate, not for `cargo test --lib`), which is why
//! these live here instead of inline in `src/kernel/run.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use serde_json::{json, Value};

use pulse::identity::actor::{ActorKind, ActorRef};
use pulse::kernel::run::{run_lane, run_worker};
use pulse::store::issues;

const PULSE_BIN: &str = env!("CARGO_BIN_EXE_pulse");

fn agent(id: &str) -> ActorRef {
    ActorRef {
        kind: ActorKind::Agent,
        id: id.to_string(),
    }
}

fn git_repo_with_ready_ticket() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        assert!(StdCommand::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(args)
            .status()
            .unwrap()
            .success());
    };
    fs::write(dir.path().join("PULSE.md"), "profiles: {}\n").unwrap();
    run(&["init", "-q"]);
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "user.name", "test"]);
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);
    issues::mutate(dir.path(), |mut records| {
        records.push(json!({
            "schema": 3, "id": "TK-a3f9", "kind": "ticket", "title": "t",
            "status": "ready", "revision": 1,
            "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
            "role": "implementation", "risk": "low", "surface": "cli",
            "acceptance": [{"id": "AC-1", "when": "x", "then": "y"}],
        }));
        Ok(records)
    })
    .unwrap();
    dir
}

fn write_script(repo: &Path, name: &str, body: &str) -> PathBuf {
    let path = repo.join(name);
    fs::write(&path, body).unwrap();
    path
}

fn write_checkpoint_fixture(repo: &Path, run_id: &str) -> PathBuf {
    let path = repo.join("cp.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "run_id": run_id, "done_ac": [], "in_progress": "", "next": [],
            "files": [], "decisions": [], "gotchas": [], "commands_run": [],
        }))
        .unwrap(),
    )
    .unwrap();
    path
}

fn write_runners_json(repo: &Path, roles: &[(&str, String)]) {
    let mut map = serde_json::Map::new();
    for (role, command) in roles {
        map.insert(
            (*role).to_string(),
            json!({"command": command, "timeout_seconds": 30}),
        );
    }
    fs::write(
        repo.join(".pulse/runners.json"),
        serde_json::to_vec(&Value::Object(map)).unwrap(),
    )
    .unwrap();
}

#[test]
fn continue_spawns_fresh_process_with_latest_checkpoint() {
    let repo = git_repo_with_ready_ticket();
    let checkpoint_fixture = write_checkpoint_fixture(repo.path(), "checkpoint-marker-xyz");
    let worker_script = write_script(
        repo.path(),
        "worker.sh",
        &format!(
            "#!/bin/sh\n\"{PULSE_BIN}\" --repo-root \"$1\" checkpoint \"$2\" --from \"$3\" --actor agent:worker --json >/dev/null 2>&1\necho '{{\"status\":\"continue\"}}'\n"
        ),
    );
    let continue_script = write_script(
        repo.path(),
        "worker-continue.sh",
        "#!/bin/sh\nif grep -q \"checkpoint-marker-xyz\" \"$1\"; then\n  echo '{\"status\":\"blocked\",\"reason\":\"saw-latest-checkpoint\"}'\nelse\n  echo '{\"status\":\"blocked\",\"reason\":\"stale-checkpoint\"}'\nfi\n",
    );
    write_runners_json(
        repo.path(),
        &[
            (
                "worker",
                format!(
                    "sh {} {{repo}} {{ticket}} {}",
                    worker_script.display(),
                    checkpoint_fixture.display()
                ),
            ),
            (
                "worker-continue",
                format!("sh {} {{input}}", continue_script.display()),
            ),
        ],
    );

    let updated = run_worker(repo.path(), &agent("worker"), "TK-a3f9", 3600, 5).unwrap();
    assert_eq!(updated["status"], "blocked");
    let notes = updated["notes"].as_array().unwrap();
    assert_eq!(notes.last().unwrap()["text"], "saw-latest-checkpoint");
}

#[test]
fn continue_without_new_checkpoint_is_a_crash() {
    let repo = git_repo_with_ready_ticket();
    let worker_script = write_script(
        repo.path(),
        "worker.sh",
        "#!/bin/sh\necho '{\"status\":\"continue\"}'\n",
    );
    write_runners_json(
        repo.path(),
        &[("worker", format!("sh {}", worker_script.display()))],
    );

    let err = run_worker(repo.path(), &agent("worker"), "TK-a3f9", 3600, 5).unwrap_err();
    assert_eq!(err.code(), "run_continue_without_checkpoint");
}

#[test]
fn continue_limit_blocks_with_needs_split() {
    let repo = git_repo_with_ready_ticket();
    let checkpoint_fixture = write_checkpoint_fixture(repo.path(), "run-x");
    let script = write_script(
        repo.path(),
        "worker.sh",
        &format!(
            "#!/bin/sh\n\"{PULSE_BIN}\" --repo-root \"$1\" checkpoint \"$2\" --from \"$3\" --actor agent:worker --json >/dev/null 2>&1\necho '{{\"status\":\"continue\"}}'\n"
        ),
    );
    let command = format!(
        "sh {} {{repo}} {{ticket}} {}",
        script.display(),
        checkpoint_fixture.display()
    );
    write_runners_json(
        repo.path(),
        &[("worker", command.clone()), ("worker-continue", command)],
    );

    let updated = run_worker(repo.path(), &agent("worker"), "TK-a3f9", 3600, 1).unwrap();
    assert_eq!(updated["status"], "blocked");
    let notes = updated["notes"].as_array().unwrap();
    assert_eq!(notes.last().unwrap()["text"], "needs_split");
}

#[test]
fn handed_off_ends_the_loop_successfully() {
    let repo = git_repo_with_ready_ticket();
    let handoff_fixture = repo.path().join("handoff.json");
    fs::write(
        &handoff_fixture,
        serde_json::to_vec(&json!({
            "summary": "done", "changed_files": [],
            "acceptance": [{"id": "AC-1", "status": "done", "how": "ran it"}],
            "verify_results": [], "docs_updated": [], "learnings_used": [],
            "friction": [], "open_risks": [],
        }))
        .unwrap(),
    )
    .unwrap();
    let script = write_script(
        repo.path(),
        "worker.sh",
        &format!(
            "#!/bin/sh\n\"{PULSE_BIN}\" --repo-root \"$1\" handoff \"$2\" --from \"$3\" --actor agent:worker --json >/dev/null 2>&1\necho '{{\"status\":\"handed_off\"}}'\n"
        ),
    );
    write_runners_json(
        repo.path(),
        &[(
            "worker",
            format!(
                "sh {} {{repo}} {{ticket}} {}",
                script.display(),
                handoff_fixture.display()
            ),
        )],
    );

    let updated = run_worker(repo.path(), &agent("worker"), "TK-a3f9", 3600, 5).unwrap();
    assert_eq!(updated["status"], "verifying");
}

#[test]
fn story_scope_lane_without_surface_or_risk_is_profile_missing_even_with_force() {
    // A1: `--force` must not route a story-scope lane run around a Story
    // that never got surface/risk set — the ready gate does not require
    // either on a Story, so this is a real, unforceable data gap.
    let repo = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        assert!(StdCommand::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(args)
            .status()
            .unwrap()
            .success());
    };
    fs::write(repo.path().join("PULSE.md"), "profiles: {}\n").unwrap();
    run(&["init", "-q"]);
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "user.name", "test"]);
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);
    issues::mutate(repo.path(), |mut records| {
        records.push(json!({
            "schema": 3, "id": "ST-1111", "kind": "story", "title": "s",
            "status": "ready", "revision": 1,
            "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
            "outcome": "o",
        }));
        Ok(records)
    })
    .unwrap();

    let err = run_lane(repo.path(), &agent("qa-cli"), "ST-1111", "qa-cli", true).unwrap_err();
    assert_eq!(err.code(), "profile_missing");
    assert!(err.hint().is_some());
}

#[test]
fn story_scope_lane_routes_through_its_cases_surfaces_without_force() {
    // Dogfood ST-1 F18: a story classified api-medium must still run qa-ui
    // for its ui-surface cases — the profile gate unions the case surfaces
    // instead of demanding --force.
    let repo = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        assert!(StdCommand::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(args)
            .status()
            .unwrap()
            .success());
    };
    fs::write(
        repo.path().join("PULSE.md"),
        "profiles:\n  api-medium: {lanes: [review-correctness, qa-api]}\n  ui-medium: {lanes: [review-correctness, qa-ui]}\n",
    )
    .unwrap();
    run(&["init", "-q"]);
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "user.name", "test"]);
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);
    issues::mutate(repo.path(), |mut records| {
        records.push(json!({
            "schema": 3, "id": "ST-1111", "kind": "story", "title": "s",
            "status": "ready", "revision": 1,
            "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
            "outcome": "o", "risk": "medium", "surface": "api",
            "qa_cases": [
                {"id": "QA-001", "surface": "api", "priority": "high", "steps": ["GET /"]},
                {"id": "QA-002", "surface": "ui", "priority": "high", "steps": ["http://127.0.0.1:3000/"]}
            ],
        }));
        Ok(records)
    })
    .unwrap();
    write_runners_json(repo.path(), &[]);

    // The profile gate must let qa-ui through (the case surface routes it);
    // failing later at the missing runner entry proves the gate passed.
    let err = run_lane(repo.path(), &agent("qa-ui"), "ST-1111", "qa-ui", false).unwrap_err();
    assert_eq!(err.code(), "runner_role_missing");
}

#[test]
fn a_nonzero_exit_is_inconclusive_and_keeps_the_lease() {
    let repo = git_repo_with_ready_ticket();
    let script = write_script(repo.path(), "worker.sh", "#!/bin/sh\nexit 1\n");
    write_runners_json(
        repo.path(),
        &[("worker", format!("sh {}", script.display()))],
    );

    let err = run_worker(repo.path(), &agent("worker"), "TK-a3f9", 3600, 5).unwrap_err();
    assert_eq!(err.code(), "run_inconclusive");
    let records = issues::read_all(repo.path()).unwrap();
    let ticket = records.iter().find(|r| r["id"] == "TK-a3f9").unwrap();
    assert_eq!(ticket["status"], "active");
    assert!(!ticket["lease"].is_null());
}

fn flip_ticket_to_verifying(repo: &Path) {
    issues::mutate(repo, |mut records| {
        for record in records.iter_mut() {
            if record["id"] == "TK-a3f9" {
                record["status"] = "verifying".into();
            }
        }
        Ok(records)
    })
    .unwrap();
}

fn lane_events(repo: &Path) -> Vec<(String, Value)> {
    pulse::event::read_event_log(repo)
        .unwrap()
        .events
        .iter()
        .filter(|event| event.subject.id == "TK-a3f9")
        .map(|event| (event.event_type.clone(), event.payload.clone()))
        .collect()
}

#[test]
fn a_lane_that_dies_without_sealing_still_leaves_run_events() {
    // Dogfood ST-1 F8: a lane that never sealed used to be invisible in
    // `events tail` — only the CLI's own log knew. The start and the
    // inconclusive end must both be recorded.
    let repo = git_repo_with_ready_ticket();
    flip_ticket_to_verifying(repo.path());
    write_runners_json(repo.path(), &[("qa-cli", "false".to_string())]);

    let err = run_lane(repo.path(), &agent("qa-cli"), "TK-a3f9", "qa-cli", true).unwrap_err();
    assert_eq!(err.code(), "run_inconclusive");

    let events = lane_events(repo.path());
    let started = events
        .iter()
        .any(|(kind, payload)| kind == "run.started" && payload["role"] == "qa-cli");
    let completed = events.iter().any(|(kind, payload)| {
        kind == "run.completed"
            && payload["role"] == "qa-cli"
            && payload["outcome"] == "inconclusive"
    });
    assert!(started, "no run.started event: {events:?}");
    assert!(completed, "no inconclusive run.completed event: {events:?}");
}

#[test]
fn a_sealed_lane_leaves_run_events_with_its_verdict() {
    let repo = git_repo_with_ready_ticket();
    flip_ticket_to_verifying(repo.path());
    let script = write_script(
        repo.path(),
        "lane.sh",
        "#!/bin/sh\ncommit=$(git -C \"$1\" rev-parse HEAD)\nmkdir -p \"$1/.pulse/evidence/$2\"\n\
             printf '{\"verdict\":\"pass\",\"acceptance\":[],\"cases\":[],\"findings\":[],\"commands_run\":[],\"environment\":{\"commit\":\"%s\"}}' \"$commit\" \
             > \"$1/.pulse/evidence/$2/qa-cli.json\"\necho '{\"status\":\"done\"}'\n",
    );
    write_runners_json(
        repo.path(),
        &[(
            "qa-cli",
            format!("sh {} {{repo}} {{ticket}}", script.display()),
        )],
    );

    run_lane(repo.path(), &agent("qa-cli"), "TK-a3f9", "qa-cli", true).unwrap();

    let events = lane_events(repo.path());
    let sealed = events.iter().any(|(kind, payload)| {
        kind == "run.completed"
            && payload["role"] == "qa-cli"
            && payload["outcome"] == "sealed"
            && payload["verdict"] == "pass"
    });
    assert!(sealed, "no sealed run.completed event: {events:?}");
}

fn handoff_fixture(path: &Path) {
    fs::write(
        path,
        serde_json::to_vec(&json!({
            "summary": "done", "changed_files": [],
            "acceptance": [{"id": "AC-1", "status": "done", "how": "ran it"}],
            "verify_results": [], "docs_updated": [], "learnings_used": [],
            "friction": [], "open_risks": [],
        }))
        .unwrap(),
    )
    .unwrap();
}

fn handoff_receipts(repo: &Path) -> Vec<Value> {
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
                let receipt: Value = serde_json::from_str(line).unwrap();
                if receipt["kind"] == "handoff" && receipt["subject"]["id"] == "TK-a3f9" {
                    out.push(receipt);
                }
            }
        }
    }
    out
}

#[test]
fn a_verifying_ticket_can_be_reverified_with_a_fresh_handoff_snapshot() {
    // Dogfood ST-1 F10: after a post-handoff source change the ticket used
    // to be bricked in verifying — close_source_stale forever, because the
    // only verifying->active door was a review-lane fail. `pulse run worker`
    // on a verifying ticket now re-verifies: fresh lease, back to active,
    // and the next handoff records a source snapshot that includes the fix.
    let repo = git_repo_with_ready_ticket();
    let script = write_script(
        repo.path(),
        "worker.sh",
        &format!(
            "#!/bin/sh\n\"{PULSE_BIN}\" --repo-root \"$1\" handoff \"$2\" --from \"$3\" --actor agent:worker --json >/dev/null 2>&1\necho '{{\"status\":\"handed_off\"}}'\n"
        ),
    );

    // Run 1: ready -> verifying (handoff #1).
    handoff_fixture(&repo.path().join("handoff1.json"));
    write_runners_json(
        repo.path(),
        &[(
            "worker",
            format!(
                "sh {} {{repo}} {{ticket}} {}",
                script.display(),
                repo.path().join("handoff1.json").display()
            ),
        )],
    );
    run_worker(repo.path(), &agent("worker"), "TK-a3f9", 3600, 5).unwrap();
    let records = issues::read_all(repo.path()).unwrap();
    assert_eq!(
        records.iter().find(|r| r["id"] == "TK-a3f9").unwrap()["status"],
        "verifying"
    );

    // A tracked file to fix (the helper's repo only tracks PULSE.md, which
    // is fence-exempt harness config).
    std::fs::write(repo.path().join("README.md"), "v1\n").unwrap();
    assert!(StdCommand::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["add", "README.md"])
        .status()
        .unwrap()
        .success());
    assert!(StdCommand::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["commit", "-qm", "readme"])
        .status()
        .unwrap()
        .success());

    // Post-handoff source fix, committed: HEAD moves past handoff #1.
    std::fs::write(repo.path().join("README.md"), "fixed\n").unwrap();
    let commit = StdCommand::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["commit", "-am", "post-handoff fix"])
        .output()
        .unwrap();
    assert!(
        commit.status.success(),
        "git commit failed: stderr={} stdout={} status={:?}",
        String::from_utf8_lossy(&commit.stderr),
        String::from_utf8_lossy(&commit.stdout),
        StdCommand::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["status", "--porcelain"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default()
    );

    // Run 2: re-verify from verifying.
    handoff_fixture(&repo.path().join("handoff2.json"));
    write_runners_json(
        repo.path(),
        &[(
            "worker",
            format!(
                "sh {} {{repo}} {{ticket}} {}",
                script.display(),
                repo.path().join("handoff2.json").display()
            ),
        )],
    );
    run_worker(repo.path(), &agent("worker"), "TK-a3f9", 3600, 5).unwrap();

    let records = issues::read_all(repo.path()).unwrap();
    assert_eq!(
        records.iter().find(|r| r["id"] == "TK-a3f9").unwrap()["status"],
        "verifying"
    );
    let handoffs = handoff_receipts(repo.path());
    assert_eq!(handoffs.len(), 2, "expected two handoff receipts");
    // The latest handoff's snapshot is current: close_source_stale is gone.
    let now = pulse::source::snapshot(repo.path(), &[]).unwrap();
    let latest = handoffs
        .iter()
        .max_by(|a, b| a["id"].as_str().unwrap().cmp(b["id"].as_str().unwrap()))
        .unwrap();
    assert_eq!(latest["source"]["commit"], json!(now.commit));
    assert_eq!(latest["source"]["dirty_hash"], json!(now.dirty_hash));
}
