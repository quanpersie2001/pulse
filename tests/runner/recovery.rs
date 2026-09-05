//! Recovery, resume and release integration tests for `pulse run`.
//!
//! Covers the no-blind-retry contract: an interrupted run keeps its lease and
//! its committed packet; a re-run with an unchanged packet resumes under the
//! same lease; a re-run after contract drift is refused until the operator
//! acknowledges the drift, which releases the stale lease and starts fresh;
//! and `pulse work release` frees a stuck run and returns the Ticket to
//! ready.

use std::fs;

use serde_json::Value;

use crate::cli_run::{install_worker_script, node_status, setup_ready_ticket, ACTOR};
use crate::common_fixture_repo::TestRepo;
use crate::common_git::commit_all;

fn set_worker_command(repo: &TestRepo, command: &str) {
    let config = serde_json::json!({
        "worker": {"command": command, "timeout_seconds": 30},
        "reviewer": {"command": "echo '{\"ok\":true}'", "timeout_seconds": 60},
        "qa": {"command": "echo '{\"cases\":[]}'", "timeout_seconds": 60},
    });
    fs::write(
        repo.path().join(".pulse/config/runners.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .unwrap();
    commit_all(repo.path());
}

#[test]
fn interrupted_run_resumes_under_the_same_lease_when_unchanged() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);

    // The script simulates an agent that times out on its first attempt and
    // succeeds when re-run, without changing any repo file between runs.
    install_worker_script(
        &repo,
        r#"
RUN_DIR="$(dirname "$1")"
if [ -f "$RUN_DIR/resumed" ]; then
  . "$RUN_DIR/worker-env"
  "$PULSE" work handoff --lease "$LEASE_ID" --session "$SESSION_ID" \
    --source-commit "$SOURCE_COMMIT" --summary "resumed and finished" \
    --idempotency-key handoff-resume --json
  echo '{"status": "handed_off", "summary": "resumed"}'
else
  touch "$RUN_DIR/resumed"
  sleep 30
fi
"#,
    );
    set_worker_command(&repo, "sh scripts/fake-worker.sh {input}");

    // First run times out: the lease and ticket stay active.
    let config = serde_json::json!({
        "worker": {"command": "sh scripts/fake-worker.sh {input}", "timeout_seconds": 1},
    });
    fs::write(
        repo.path().join(".pulse/config/runners.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .unwrap();
    let first = repo.pulse_ok(&["run", "worker", "--ticket", &ticket_id, "--json"]);
    assert_eq!(first["status"], "inconclusive");
    let first_lease = first["lease_id"].as_str().unwrap().to_string();
    assert_eq!(node_status(&repo, &ticket_id), "active");

    // Second run: nothing drifted, so it resumes under the same lease.
    let second = repo.pulse_ok(&["run", "worker", "--ticket", &ticket_id, "--json"]);
    assert_eq!(second["status"], "handed_off");
    assert_eq!(
        second["lease_id"].as_str().unwrap(),
        first_lease.as_str(),
        "unchanged packets must resume the same lease"
    );
    assert_eq!(node_status(&repo, &ticket_id), "verifying");
}

#[test]
fn drifted_resume_is_refused_until_drift_is_acknowledged() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);

    install_worker_script(&repo, "sleep 30");
    let config = serde_json::json!({
        "worker": {"command": "sh scripts/fake-worker.sh {input}", "timeout_seconds": 1},
    });
    fs::write(
        repo.path().join(".pulse/config/runners.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .unwrap();
    let first = repo.pulse_ok(&["run", "worker", "--ticket", &ticket_id, "--json"]);
    let first_lease = first["lease_id"].as_str().unwrap().to_string();

    // Contract drift: edit ticket.md and sync.
    let brief = repo.path().join("works").join(&ticket_id).join("ticket.md");
    let mut markdown = fs::read_to_string(&brief).unwrap();
    markdown.push_str("\n<!-- contract amended -->\n");
    fs::write(&brief, markdown).unwrap();
    let shown = repo.pulse_ok(&["work", "show", &ticket_id, "--json"]);
    let revision = shown["node"]["revision"].as_u64().unwrap();
    repo.pulse_ok(&[
        "work",
        "sync",
        &ticket_id,
        "--expected-revision",
        &revision.to_string(),
        "--actor",
        ACTOR,
        "--json",
    ]);

    // The interrupted lease still binds the old contract revision: the
    // re-run refuses with run_resume_drift until the operator decides.
    let output = repo.pulse(&["run", "worker", "--ticket", &ticket_id, "--json"]);
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["code"], "run_resume_drift");

    // Release returns the ticket to ready.
    repo.pulse_ok(&[
        "work",
        "release",
        &ticket_id,
        "--actor",
        ACTOR,
        "--reason",
        "contract changed under the interrupted run",
        "--json",
    ]);
    assert_eq!(node_status(&repo, &ticket_id), "ready");

    // A fresh run now succeeds with a new lease.
    install_worker_script(
        &repo,
        r#"
RUN_DIR="$(dirname "$1")"
. "$RUN_DIR/worker-env"
"$PULSE" work handoff --lease "$LEASE_ID" --session "$SESSION_ID" \
  --source-commit "$SOURCE_COMMIT" --summary "fresh after drift" \
  --idempotency-key handoff-fresh --json
echo '{"status": "handed_off", "summary": "fresh"}'
"#,
    );
    set_worker_command(&repo, "sh scripts/fake-worker.sh {input}");
    let second = repo.pulse_ok(&["run", "worker", "--ticket", &ticket_id, "--json"]);
    assert_eq!(second["status"], "handed_off");
    let new_lease = second["lease_id"].as_str().unwrap();
    assert_ne!(new_lease, first_lease.as_str());
    assert_eq!(node_status(&repo, &ticket_id), "verifying");
}

#[test]
fn release_without_live_lease_fails_explicitly() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    let output = repo.pulse(&["work", "release", &ticket_id, "--actor", ACTOR, "--json"]);
    assert!(!output.status.success());
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["code"], "release_lease_missing");
}

#[test]
fn drifted_packet_needs_explicit_acknowledge_even_when_ready_again() {
    // Simulate an interrupted worker on a ticket that is still ready by
    // holding a Reserved (not activated) lease: reserve directly.
    let repo = TestRepo::from_fixture("minimal-service");
    let store = pulse::JsonGraphStore::new(repo.path());
    let ticket_id = setup_ready_ticket(&repo);
    let reserved = store
        .reserve_work(pulse::reservation::ReserveWorkArgs {
            ticket_id: ticket_id.clone(),
            actor: ACTOR.to_string(),
            assignee: ACTOR.to_string(),
            ttl_seconds: 3600,
            idempotency_key: "hold-reserved".to_string(),
        })
        .unwrap();

    // Contract drift while the lease is held.
    let brief = repo.path().join("works").join(&ticket_id).join("ticket.md");
    let mut markdown = fs::read_to_string(&brief).unwrap();
    markdown.push_str("\n<!-- drift -->\n");
    fs::write(&brief, markdown).unwrap();
    let shown = repo.pulse_ok(&["work", "show", &ticket_id, "--json"]);
    let revision = shown["node"]["revision"].as_u64().unwrap();
    repo.pulse_ok(&[
        "work",
        "sync",
        &ticket_id,
        "--expected-revision",
        &revision.to_string(),
        "--actor",
        ACTOR,
        "--json",
    ]);

    // The stored packet fingerprint no longer matches the fresh one: without
    // acknowledgment the resume is refused with run_resume_drift.
    install_worker_script(&repo, "echo '{\"status\": \"handed_off\"}'");
    set_worker_command(&repo, "sh scripts/fake-worker.sh {input}");
    let output = repo.pulse(&["run", "worker", "--ticket", &ticket_id, "--json"]);
    assert!(!output.status.success());
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["code"], "run_resume_drift");

    // With the flag the stale lease is released and a fresh one allocated.
    let out = repo.pulse_ok(&[
        "run",
        "worker",
        "--ticket",
        &ticket_id,
        "--acknowledge-drift",
        "--json",
    ]);
    // The script claims handed_off without proof: the fresh run proceeds but
    // classifies it unproven. What matters here is the fresh lease.
    assert_eq!(out["status"], "inconclusive");
    assert_eq!(out["inconclusive_reason"], "unproven_claim");
    assert_ne!(
        out["lease_id"].as_str().unwrap(),
        reserved.reservation.lease_id.as_str()
    );
}
