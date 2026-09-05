//! Isolation-rule integration tests for `pulse run`.
//!
//! Covers the PRODUCT §5.3 isolation rule (Decision 13.2): the checkout is
//! the default, a run is refused with `run_isolation_required` while another
//! Ticket holds a live lease, `--isolation worktree` is the only way to get a
//! Pulse-owned worktree, and worktree reclaim never touches worktrees Pulse
//! did not create.

use serde_json::Value;
use std::fs;

use crate::cli_run::{install_worker_script, node_status, run_outcome, setup_ready_ticket, ACTOR};
use crate::common_fixture_repo::TestRepo;
use crate::common_git::commit_all;

use pulse::reservation::{
    ActivateReservationArgs, AssignmentAcknowledgement, ReserveWorkArgs, RuntimeBinding,
};
use pulse::JsonGraphStore;
fn install_proving_worker(repo: &TestRepo) {
    // Worker records its working directory, edits the worktree source, then
    // hands off through the CLI against the canonical repo root (passed as
    // the {repo} placeholder argument).
    install_worker_script(
        repo,
        r#"
RUN_DIR="$(dirname "$1")"
REPO_ROOT="$2"
exec 2>"$RUN_DIR/artifacts/err.txt"
. "$RUN_DIR/worker-env"
pwd > "$RUN_DIR/artifacts/cwd.txt"
echo "// worker edit" >> src/token.mjs
"$PULSE" work handoff --repo-root "$REPO_ROOT" --lease "$LEASE_ID" --session "$SESSION_ID" \
  --source-commit "$SOURCE_COMMIT" --summary "did the work" \
  --idempotency-key handoff-1 --json
echo '{"status": "handed_off", "summary": "done"}'
"#,
    );
    // {repo} second argument carries the canonical checkout into the script.
    let config = serde_json::json!({
        "worker": {"command": "sh scripts/fake-worker.sh {input} {repo}", "timeout_seconds": 60},
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

fn hold_lease_for(store: &JsonGraphStore, ticket_id: &str) -> String {
    let reserved = store
        .reserve_work(ReserveWorkArgs {
            ticket_id: ticket_id.to_string(),
            actor: ACTOR.to_string(),
            assignee: format!("agent:worker-{ticket_id}"),
            ttl_seconds: 3600,
            idempotency_key: format!("hold:{ticket_id}"),
        })
        .unwrap();
    store
        .activate_reservation(ActivateReservationArgs {
            lease_id: reserved.reservation.lease_id.clone(),
            actor: ACTOR.to_string(),
            runtime_binding: RuntimeBinding {
                project_id: ticket_id.to_string(),
                workspace_id: "checkout".to_string(),
                session_id: "ses-holder".to_string(),
                provider_id: "runner:worker".to_string(),
            },
            acknowledgement: AssignmentAcknowledgement {
                acknowledgement_id: format!("ack:{}", ticket_id),
                delivery_id: format!("delivery:{}", ticket_id),
                session_id: "ses-holder".to_string(),
                packet_fingerprint: reserved.reservation.packet_fingerprint.clone(),
                acknowledged_at: "2026-01-01T00:00:00Z".to_string(),
            },
        })
        .unwrap();
    reserved.reservation.lease_id
}

fn worktree_path(repo: &TestRepo, ticket_id: &str) -> std::path::PathBuf {
    repo.path().join(".pulse/runtime/worktrees").join(ticket_id)
}

fn run_refused(repo: &TestRepo, ticket_id: &str) -> Value {
    let output = repo.pulse(&["run", "worker", "--ticket", ticket_id, "--json"]);
    assert!(!output.status.success());
    serde_json::from_slice(&output.stderr).unwrap()
}

#[test]
fn run_refused_with_run_isolation_required_when_another_ticket_is_active() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    let holder = setup_ready_ticket(&repo);
    let worker_ticket = setup_ready_ticket(&repo);
    install_proving_worker(&repo);

    // Another Ticket holds the live lease: the new run must be refused,
    // naming the holder and the escape hatch, without creating a worktree.
    let _lease = hold_lease_for(&store, &holder);
    let err = run_refused(&repo, &worker_ticket);
    assert_eq!(err["code"], "run_isolation_required");
    let message = err["message"].as_str().unwrap();
    assert!(message.contains(&holder), "message: {message}");
    assert!(
        message.contains("--isolation worktree"),
        "message: {message}"
    );
    assert!(!worktree_path(&repo, &worker_ticket).exists());
    assert_eq!(node_status(&repo, &worker_ticket), "ready");
}

#[test]
fn worktree_flag_forces_isolation_when_another_ticket_is_active() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    let holder = setup_ready_ticket(&repo);
    let worker_ticket = setup_ready_ticket(&repo);
    install_proving_worker(&repo);

    let _lease = hold_lease_for(&store, &holder);
    let out = repo.pulse_ok(&[
        "run",
        "worker",
        "--ticket",
        &worker_ticket,
        "--isolation",
        "worktree",
        "--json",
    ]);
    assert_eq!(out["status"], "handed_off");
    assert!(worktree_path(&repo, &worker_ticket).exists());
}

#[test]
fn terminal_holder_ticket_lets_next_run_use_the_checkout() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    let holder = setup_ready_ticket(&repo);
    let worker_ticket = setup_ready_ticket(&repo);
    install_proving_worker(&repo);

    // Hold, then release: with no foreign live lease left, the run proceeds
    // in the checkout and must not create a worktree.
    let lease = hold_lease_for(&store, &holder);
    store
        .release_reservation(&lease, ACTOR, "test teardown")
        .unwrap();
    let outcome = run_outcome(&repo, &worker_ticket);
    assert_eq!(outcome["status"], "handed_off");
    assert!(!worktree_path(&repo, &worker_ticket).exists());
}

#[test]
fn explicit_worktree_flag_still_runs_without_other_active_leases() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    install_proving_worker(&repo);

    let out = repo.pulse_ok(&[
        "run",
        "worker",
        "--ticket",
        &ticket_id,
        "--isolation",
        "worktree",
        "--json",
    ]);
    let record: Value = serde_json::from_slice(
        &fs::read(
            repo.path()
                .join(".pulse/runtime/run")
                .join(&ticket_id)
                .join("worker-outcome.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let err_txt = fs::read_to_string(
        repo.path()
            .join(".pulse/runtime/run")
            .join(&ticket_id)
            .join("artifacts/err.txt"),
    )
    .unwrap_or_default();
    assert_eq!(
        record["status"].as_str(),
        Some("handed_off"),
        "outcome: {out}; record: {record}; err: {err_txt}"
    );
    assert!(worktree_path(&repo, &ticket_id).exists());
}

#[test]
fn terminal_ticket_reclaims_pulse_owned_worktree_only() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    let ticket_id = setup_ready_ticket(&repo);
    install_proving_worker(&repo);

    // A foreign directory at another Ticket's path must survive cleanup.
    let foreign = repo
        .path()
        .join(".pulse/runtime/worktrees/TK-FOREIGN000000000000000000")
        .join("keep.txt");
    fs::create_dir_all(foreign.parent().unwrap()).unwrap();
    fs::write(&foreign, b"not ours").unwrap();

    let outcome = repo.pulse_ok(&[
        "run",
        "worker",
        "--ticket",
        &ticket_id,
        "--isolation",
        "worktree",
        "--json",
    ]);
    assert_eq!(outcome["status"], "handed_off");
    let lease_id = outcome["lease_id"].as_str().unwrap().to_string();
    let tree = worktree_path(&repo, &ticket_id);
    assert!(tree.exists());

    // Verify then close: the close gate must accept the worktree-bound dirty
    // identity, and the terminal transition reclaims the worktree.
    let shown = repo.pulse_ok(&["work", "show", &ticket_id, "--json"]);
    assert_eq!(shown["node"]["status"], "verifying");
    let handoff_id = fs::read_dir(repo.path().join(".pulse/evidence/execution/handoffs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let handoff: Value = serde_json::from_slice(&fs::read(&handoff_id).unwrap()).unwrap();
    eprintln!(
        "BOUND: {} WORKSPACE: {}",
        handoff["source_dirty_hash"], handoff["workspace_id"]
    );
    let verify_stderr = {
        let output = repo.pulse(&[
            "work",
            "verify",
            &ticket_id,
            "--handoff",
            handoff["handoff_id"].as_str().unwrap(),
            "--actor",
            ACTOR,
            "--source-commit",
            handoff["source_commit"].as_str().unwrap(),
            "--summary",
            "worktree diff verified",
            "--check",
            "focused=node scripts/verify.mjs=0",
            "--proof",
            "AC-1=focused=",
            "--idempotency-key",
            "verify-iso-1",
            "--json",
        ]);
        assert!(
            output.status.success(),
            "verify stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stderr).to_string()
    };
    let _ = verify_stderr;
    // The worker was runner:worker, so the human operator is a legitimate
    // independent closer and holds work.close from init.
    repo.pulse_ok(&[
        "--idempotency-key",
        "close-iso-1",
        "work",
        "close",
        &ticket_id,
        "--actor",
        ACTOR,
        "--source-commit",
        handoff["source_commit"].as_str().unwrap(),
        "--summary",
        "closed from isolated run",
        "--json",
    ]);
    assert_eq!(node_status(&repo, &ticket_id), "done");
    assert!(!tree.exists(), "pulse-owned worktree must be reclaimed");

    // Foreign directory untouched.
    assert!(foreign.exists());

    // Release path also reclaims: release the holder lease for the other
    // ticket (no worktree) is a no-op; ensure the API is callable.
    let _ = store.release_reservation(lease_id.as_str(), "agent:runner:worker", "post-close");
}
