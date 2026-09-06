//! Reviewer outcome classification for `pulse run reviewer`.
//!
//! The final reviewer JSON is only a summary; classification proves the claim
//! against graph truth. Contract cases: a `pass` backed by a real verification
//! receipt classifies `passed`, a `rework` backed by a real rework
//! verification moves the Ticket to `rework`, and a verdict that misses its
//! disposition, acceptance coverage or proof stays `inconclusive`.

use std::fs;

use serde_json::Value;

use crate::cli_run::{
    install_worker_script, node_status, run_outcome, set_command, set_worker_command,
    setup_ready_ticket,
};
use crate::common_bin::bin;
use crate::common_fixture_repo::TestRepo;

const HANDOFF_WORKER: &str = r#"
RUN_DIR="$(dirname "$1")"
. "$RUN_DIR/worker-env"
"$PULSE" work handoff --lease "$LEASE_ID" --session "$SESSION_ID" \
  --source-commit "$SOURCE_COMMIT" --summary "did the work" \
  --check "focused=node scripts/verify.mjs=0" \
  --proof "AC-1=focused=" \
  --idempotency-key handoff-1 --json
echo '{"status": "handed_off", "summary": "done"}'
"#;

/// Bring the Ticket to `verifying` with a real handoff, then install a
/// reviewer script that can record its verdict through the CLI. The script
/// gets TICKET_ID, HANDOFF_ID, SOURCE_COMMIT and the pulse binary as PULSE.
fn verify_fixture(repo: &TestRepo, ticket_id: &str, reviewer_body: &str) {
    install_worker_script(repo, HANDOFF_WORKER);
    let script = format!(
        "#!/bin/sh\nset -e\nRUN_DIR=\"$(dirname \"$1\")\"\nTICKET_ID=\"$(basename \"$RUN_DIR\")\"\nHANDOFF_ID=\"$(ls .pulse/evidence/execution/handoffs | head -1 | sed 's/\\.json$//')\"\nSOURCE_COMMIT=\"$(git rev-parse HEAD)\"\nPULSE=\"{}\"\n{reviewer_body}\n",
        bin()
    );
    fs::write(repo.path().join("scripts/fake-reviewer.sh"), script).unwrap();
    // Point both roles at the repo-local fake scripts BEFORE running; the
    // init-bootstrapped default would otherwise launch a real agent CLI.
    set_worker_command(repo, "sh scripts/fake-worker.sh {input}");
    set_command(repo, "reviewer", "sh scripts/fake-reviewer.sh {input}");
    let worker = run_outcome(repo, ticket_id);
    assert_eq!(worker["status"], "handed_off");
}

fn reviewer_outcome(repo: &TestRepo, ticket_id: &str) -> Value {
    let output = repo.pulse(&["run", "reviewer", "--ticket", ticket_id, "--json"]);
    assert!(
        output.status.success(),
        "reviewer run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn reviewer_input(repo: &TestRepo, ticket_id: &str) -> Value {
    let path = repo
        .path()
        .join(".pulse/runtime/run")
        .join(ticket_id)
        .join("reviewer-input.json");
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

/// Decision 0012 §2: the reviewer input carries the worker's machine-readable
/// claims (checks, acceptance proofs) — never the worker's prose. A verdict
/// must be proven by re-running the checks, not by trusting a summary.
#[test]
fn reviewer_input_carries_claims_and_no_worker_prose() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    verify_fixture(
        &repo,
        &ticket_id,
        r#"echo '{"disposition": "pass", "acceptance": {"AC-1": "re-ran the verify command"}, "findings": []}'"#,
    );
    let out = reviewer_outcome(&repo, &ticket_id);
    // The claim is well-formed but no verification receipt was recorded, so
    // the run stays inconclusive; the input contract is what this test reads.
    assert_eq!(out["status"], "inconclusive");

    let input = reviewer_input(&repo, &ticket_id);
    assert!(input["contract_revision"].is_u64());
    assert_eq!(input["reviewers_required"], 1);
    let handoff = &input["handoffs"][0];
    assert!(
        handoff.get("summary").is_none(),
        "worker prose must not reach the reviewer input: {handoff}"
    );
    assert_eq!(handoff["checks"][0]["name"], "focused");
    assert_eq!(handoff["checks"][0]["command"], "node scripts/verify.mjs");
    assert_eq!(handoff["checks"][0]["exit_code"], 0);
    assert_eq!(handoff["acceptance_proofs"][0]["acceptance_id"], "AC-1");
    assert_eq!(
        handoff["acceptance_proofs"][0]["check_names"]
            .as_array()
            .unwrap(),
        &["focused".to_string()]
    );
}

#[test]
fn proven_pass_classifies_passed() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    verify_fixture(
        &repo,
        &ticket_id,
        r#"
"$PULSE" work verify "$TICKET_ID" \
  --handoff "$HANDOFF_ID" \
  --actor agent:runner:reviewer \
  --source-commit "$SOURCE_COMMIT" \
  --disposition passed \
  --summary "verified the change" \
  --check "focused=node scripts/verify.mjs=0" \
  --proof "AC-1=focused=" \
  --idempotency-key verify-reviewer-1 --json
echo '{"disposition": "pass", "acceptance": {"AC-1": "re-ran node scripts/verify.mjs"}, "findings": []}'
"#,
    );
    let out = reviewer_outcome(&repo, &ticket_id);
    assert_eq!(out["status"], "passed", "outcome: {out}");
    assert_eq!(out["code"], "run_passed");
    assert_eq!(node_status(&repo, &ticket_id), "verifying");
}

#[test]
fn proven_rework_classifies_rework_and_moves_the_ticket() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    verify_fixture(
        &repo,
        &ticket_id,
        r#"
"$PULSE" work verify "$TICKET_ID" \
  --handoff "$HANDOFF_ID" \
  --actor agent:runner:reviewer \
  --source-commit "$SOURCE_COMMIT" \
  --disposition rework \
  --summary "the expired branch is missing" \
  --check "focused=node scripts/verify.mjs=1" \
  --idempotency-key verify-reviewer-r1 --json
echo '{"disposition": "rework", "acceptance": {"AC-1": "check failed, see findings"}, "findings": [{"summary": "expired branch missing", "severity": "high"}]}'
"#,
    );
    let out = reviewer_outcome(&repo, &ticket_id);
    assert_eq!(out["status"], "rework", "outcome: {out}");
    assert_eq!(out["code"], "run_rework");
    // The rework verdict really moved the Ticket, not just the summary.
    assert_eq!(node_status(&repo, &ticket_id), "rework");
}

#[test]
fn missing_disposition_is_inconclusive() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    verify_fixture(
        &repo,
        &ticket_id,
        r#"echo '{"acceptance": {"AC-1": "checked"}, "findings": []}'"#,
    );
    let out = reviewer_outcome(&repo, &ticket_id);
    assert_eq!(out["status"], "inconclusive");
    assert_eq!(out["inconclusive_reason"], "malformed_output");
    assert_eq!(node_status(&repo, &ticket_id), "verifying");
}

#[test]
fn acceptance_map_missing_an_acceptance_id_is_inconclusive() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    verify_fixture(
        &repo,
        &ticket_id,
        r#"echo '{"disposition": "pass", "acceptance": {}, "findings": []}'"#,
    );
    let out = reviewer_outcome(&repo, &ticket_id);
    assert_eq!(out["status"], "inconclusive");
    assert_eq!(out["inconclusive_reason"], "acceptance_coverage_incomplete");
    assert_eq!(node_status(&repo, &ticket_id), "verifying");
}

#[test]
fn claimed_pass_without_a_recorded_receipt_is_inconclusive() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    verify_fixture(
        &repo,
        &ticket_id,
        r#"echo '{"disposition": "pass", "acceptance": {"AC-1": "trust me"}, "findings": []}'"#,
    );
    let out = reviewer_outcome(&repo, &ticket_id);
    assert_eq!(out["status"], "inconclusive");
    assert_eq!(out["inconclusive_reason"], "unproven_claim");
    assert_eq!(node_status(&repo, &ticket_id), "verifying");
}
