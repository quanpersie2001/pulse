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
  --finding "AC-1|expired branch missing|src/token.mjs|node scripts/verify.mjs --grep expired|high" \
  --idempotency-key verify-reviewer-r1 --json
echo '{"disposition": "rework", "acceptance": {"AC-1": "check failed, see findings"}, "findings": [{"acceptance_id": "AC-1", "summary": "expired branch missing", "owner": "src/token.mjs", "check": "node scripts/verify.mjs --grep expired", "severity": "high"}]}'
"#,
    );
    let out = reviewer_outcome(&repo, &ticket_id);
    assert_eq!(out["status"], "rework", "outcome: {out}");
    assert_eq!(out["code"], "run_rework");
    // The rework verdict really moved the Ticket, not just the summary.
    assert_eq!(node_status(&repo, &ticket_id), "rework");

    // The receipt carries the shaped finding.
    let mut entries =
        fs::read_dir(repo.path().join(".pulse/evidence/execution/verifications")).unwrap();
    let receipt: Value =
        serde_json::from_slice(&fs::read(entries.next().unwrap().unwrap().path()).unwrap())
            .unwrap();
    let finding = &receipt["findings"][0];
    assert_eq!(finding["acceptance_id"], "AC-1");
    assert_eq!(finding["summary"], "expired branch missing");
    assert_eq!(finding["owner"], "src/token.mjs");
    assert_eq!(finding["severity"], "high");
    assert_eq!(finding["unverifiable"], false);
}

/// A rework backed only by unverifiable findings is not a verdict: the CLI
/// refuses to record it and the Ticket never leaves `verifying`.
#[test]
fn rework_without_a_verifiable_finding_is_refused() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    verify_fixture(
        &repo,
        &ticket_id,
        r#"
if "$PULSE" work verify "$TICKET_ID" \
  --handoff "$HANDOFF_ID" \
  --actor agent:runner:reviewer \
  --source-commit "$SOURCE_COMMIT" \
  --disposition rework \
  --summary "something is off" \
  --idempotency-key verify-reviewer-nofinding --json >/dev/null 2>&1; then
  echo 'verify should have refused'
  exit 1
fi
echo '{"disposition": "rework", "acceptance": {"AC-1": "gut feeling"}, "findings": [{"summary": "looks wrong", "owner": "src/token.mjs", "severity": "high"}]}'
"#,
    );
    let out = reviewer_outcome(&repo, &ticket_id);
    assert_eq!(out["status"], "inconclusive", "outcome: {out}");
    assert_eq!(node_status(&repo, &ticket_id), "verifying");
    assert!(
        !repo
            .path()
            .join(".pulse/evidence/execution/verifications")
            .exists(),
        "the refused rework must not leave a verification receipt"
    );
}

/// The same rule at classification: a recorded rework whose reported
/// findings all lack `check` classifies `findings_unverifiable`.
#[test]
fn rework_report_with_only_unverifiable_findings_is_inconclusive() {
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
  --finding "AC-1|expired branch missing|src/token.mjs|node scripts/verify.mjs --grep expired|high" \
  --idempotency-key verify-reviewer-r2 --json
echo '{"disposition": "rework", "acceptance": {"AC-1": "see findings"}, "findings": [{"summary": "expired branch missing", "owner": "src/token.mjs", "severity": "high"}]}'
"#,
    );
    let out = reviewer_outcome(&repo, &ticket_id);
    assert_eq!(out["status"], "inconclusive", "outcome: {out}");
    assert_eq!(out["inconclusive_reason"], "findings_unverifiable");
}

/// Findings are shaped: an entry without `summary`/`owner` is malformed
/// output, not a verdict with a hole in it.
#[test]
fn finding_without_owner_is_malformed_output() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    verify_fixture(
        &repo,
        &ticket_id,
        r#"echo '{"disposition": "pass", "acceptance": {"AC-1": "ok"}, "findings": [{"summary": "a note"}]}'"#,
    );
    let out = reviewer_outcome(&repo, &ticket_id);
    assert_eq!(out["status"], "inconclusive");
    assert_eq!(out["inconclusive_reason"], "malformed_output");
}

/// Decision 0016 §3: `proof_receipts.documentation_validation` lists receipts
/// bound to the commit under review.
///
/// The old filter matched `subject.id == ticket_id`, but a documentation
/// receipt is subject-bound to the documentation registry and carries no work
/// binding, so that list was empty for every Ticket that ever ran — while the
/// reviewer prompt told the reviewer to find one there. That is what left
/// three of six Track B Tickets rework'd on the same gap.
#[test]
fn reviewer_input_lists_docs_receipts_bound_to_the_reviewed_commit() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    verify_fixture(
        &repo,
        &ticket_id,
        r#"echo '{"disposition": "pass", "acceptance": {"AC-1": "ok"}, "findings": []}'"#,
    );

    // A registered, current document: without one there is nothing for a
    // documentation receipt to bind. Registered after the handoff so the
    // receipt binds the commit the reviewer is about to review, which is what
    // a real worker run produces.
    fs::create_dir_all(repo.path().join("docs/domain")).unwrap();
    fs::write(
        repo.path().join("docs/domain/tokens.md"),
        b"# Token contract\n\nToken outcomes are stable.\n",
    )
    .unwrap();
    pulse::docs::register(
        repo.path(),
        1,
        pulse::docs::model::DocumentRecord {
            tags: vec![],
            id: "DOC-TOKEN-CONTRACT".to_string(),
            revision: 1,
            path: "docs/domain/tokens.md".to_string(),
            kind: pulse::docs::model::DocumentKind::Domain,
            status: pulse::docs::model::DocumentStatus::Approved,
            owner: "team:platform".to_string(),
            summary: "Token contract".to_string(),
            scope: pulse::docs::model::DocumentScope::default(),
            generated: None,
            superseded_by: None,
        },
        "human:tester",
    )
    .unwrap();
    repo.pulse_ok(&["docs", "index", "--json"]);
    let run = pulse::kernel::documentation::run_documentation_validation(
        repo.path(),
        None,
        Some("agent:runner:worker"),
    )
    .unwrap();
    let recorded = run
        .receipt
        .clone()
        .unwrap_or_else(|| panic!("no receipt: {:?}", run.validation))
        .receipt
        .id;

    let _ = reviewer_outcome(&repo, &ticket_id);

    let input = reviewer_input(&repo, &ticket_id);
    let listed = input["proof_receipts"]["documentation_validation"]
        .as_array()
        .expect("documentation_validation must be an array");
    assert!(
        listed
            .iter()
            .any(|id| id == &Value::String(recorded.clone())),
        "the worker's docs receipt must reach the reviewer: {listed:?}"
    );
}

/// An undecodable receipt file must reach the reviewer as a named gap in the
/// evidence store, never as an empty proof list.
///
/// `list_receipts` used to fail the whole listing on the first file it could
/// not parse, and both callsites here swallowed that with
/// `unwrap_or_default()`. One receipt left behind by a payload shape change
/// therefore told the reviewer "no qa_checkpoint exists" — the same silence
/// that reworked three of six Track B Tickets before Decision 0016, and the
/// same class that broke `evidence receipt list` for the whole dogfood
/// repository when Decision 0010 moved the qa_checkpoint payload.
#[test]
fn unreadable_receipt_reaches_the_reviewer_instead_of_emptying_its_proof_list() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    verify_fixture(
        &repo,
        &ticket_id,
        r#"echo '{"disposition": "pass", "acceptance": {"AC-1": "ok"}, "findings": []}'"#,
    );

    let corrupt = "rcpt_01J00000000000000000000099";
    fs::create_dir_all(repo.path().join(".pulse/evidence/receipts")).unwrap();
    fs::write(
        repo.path()
            .join(".pulse/evidence/receipts")
            .join(format!("{corrupt}.json")),
        br#"{"schema_version": 1, "kind": "qa_checkpoint", "payload": {"from": "a shape that no longer decodes"}}"#,
    )
    .unwrap();

    // The listing still answers, and names what it could not show.
    let listed = repo.pulse_ok(&["evidence", "receipt", "list", "--json"]);
    let unreadable = listed["unreadable"]
        .as_array()
        .expect("receipt list must always carry an `unreadable` array");
    assert_eq!(unreadable.len(), 1, "listed: {listed}");
    assert_eq!(unreadable[0]["id"], corrupt);
    assert!(!unreadable[0]["reason"].as_str().unwrap().is_empty());

    // And the reviewer is told, rather than being handed a clean-looking gap.
    let _ = reviewer_outcome(&repo, &ticket_id);
    let input = reviewer_input(&repo, &ticket_id);
    let reported = input["proof_receipts"]["unreadable"]
        .as_array()
        .expect("reviewer input must carry proof_receipts.unreadable");
    assert_eq!(reported.len(), 1, "input: {input}");
    assert_eq!(reported[0]["id"], corrupt);
    assert!(
        input["proof_receipts"]["qa_checkpoint"]
            .as_array()
            .unwrap()
            .is_empty(),
        "the corrupt file is not a usable proof: {input}"
    );

    // The prompt has to explain what that field means, or the reviewer will
    // read the empty proof list as the worker's fault anyway.
    let prompt = fs::read_to_string(
        repo.path()
            .join(".pulse/runtime/run")
            .join(&ticket_id)
            .join("reviewer-prompt.md"),
    )
    .unwrap();
    assert!(prompt.contains("proof_receipts.unreadable"), "{prompt}");
}
