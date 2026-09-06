//! Pulse-recorded `qa_checkpoint` receipts for `pulse run qa` (Decision
//! 0014).
//!
//! The qa runner only prints the final output JSON; Pulse hashes artifacts,
//! detects baseline drift and records the receipt itself. Contract cases: a
//! passed run records a passed receipt with artifact bindings, a failed
//! product case records `failed`, a baseline drift under the run records an
//! inconclusive receipt and demotes the run, and a bad artifact declaration
//! leaves no receipt at all.

use std::fs;

use serde_json::Value;

use crate::assignment_fixture::setup_ready_ticket_with_required_qa;
use crate::cli_run::{install_worker_script, set_command, set_worker_command, ACTOR};
use crate::common_fixture_repo::TestRepo;
use pulse::JsonGraphStore;

const HANDOFF_WORKER: &str = r#"
RUN_DIR="$(dirname "$1")"
. "$RUN_DIR/worker-env"
"$PULSE" work handoff --lease "$LEASE_ID" --session "$SESSION_ID" \
  --source-commit "$SOURCE_COMMIT" --summary "did the work" \
  --idempotency-key handoff-1 --json
echo '{"status": "handed_off", "summary": "done"}'
"#;

fn qa_fixture(repo: &TestRepo, qa_body: &str) -> (String, String) {
    repo.pulse_ok(&["init", "--actor", ACTOR, "--json"]);
    let store = JsonGraphStore::new(repo.path());
    let ticket_id = setup_ready_ticket_with_required_qa(repo.path(), &store);
    let node = store.show_node(&ticket_id).unwrap();
    let story_id = node
        .qa
        .as_ref()
        .and_then(|qa| qa.impact.behavioral_owner.clone())
        .expect("required-QA fixture ticket must have a behavioral owner Story");

    install_worker_script(repo, HANDOFF_WORKER);
    set_worker_command(repo, "sh scripts/fake-worker.sh {input}");
    let worker = repo.pulse_ok(&["run", "worker", "--ticket", &ticket_id, "--json"]);
    assert_eq!(worker["status"], "handed_off");

    let script = format!(
        "#!/bin/sh\nset -e\nRUN_DIR=\"$(dirname \"$1\")\"\nSTORY_ID=\"$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))[\"story_id\"])' \"$1\")\"\nPULSE=\"{pulse}\"\n{qa_body}\n",
        pulse = crate::common_bin::bin(),
    );
    fs::create_dir_all(repo.path().join("scripts")).unwrap();
    fs::write(repo.path().join("scripts/fake-qa.sh"), script).unwrap();
    set_command(repo, "qa", "sh scripts/fake-qa.sh {input}");
    (ticket_id, story_id)
}

fn run_qa(repo: &TestRepo, ticket_id: &str) -> Value {
    let output = repo.pulse(&["run", "qa", "--ticket", ticket_id, "--json"]);
    assert!(
        output.status.success(),
        "qa run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn load_single_receipt(repo: &TestRepo) -> Value {
    let dir = repo.path().join(".pulse/evidence/receipts");
    let mut entries = fs::read_dir(&dir)
        .unwrap_or_else(|_| panic!("receipts directory missing under {}", dir.display()));
    let path = entries.next().unwrap().unwrap().path();
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn run_record(repo: &TestRepo, ticket_id: &str) -> Value {
    serde_json::from_slice(
        &fs::read(
            repo.path()
                .join(".pulse/runtime/run")
                .join(ticket_id)
                .join("qa-outcome.json"),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn passed_qa_run_records_a_passed_receipt_with_artifacts() {
    let repo = TestRepo::from_fixture("minimal-service");
    let (ticket_id, _story_id) = qa_fixture(
        &repo,
        r#"
mkdir -p "$RUN_DIR/artifacts"
printf 'QA execution log\n' > "$RUN_DIR/artifacts/qa-run.log"
echo '{"cases": [{"id": "QA-001", "status": "passed", "observation": "reservation stayed single"}], "artifacts": [{"path": "'"$RUN_DIR"'/artifacts/qa-run.log", "role": "log", "case_id": "QA-001"}], "findings": []}'
"#,
    );
    let out = run_qa(&repo, &ticket_id);
    assert_eq!(out["status"], "completed", "outcome: {out}");
    let receipt_id = out["receipt_id"].as_str().unwrap().to_string();
    assert!(receipt_id.starts_with("rcpt_"));

    // The receipt content: passed, runner:qa actor, artifact binding.
    let receipt = load_single_receipt(&repo);
    assert_eq!(receipt["id"], receipt_id.as_str());
    assert_eq!(receipt["kind"], "qa_checkpoint");
    assert_eq!(receipt["result"], "passed");
    assert_eq!(receipt["actor"]["id"], "runner:qa");
    assert_eq!(receipt["subject"]["id"], ticket_id.as_str());
    let artifacts = receipt["bindings"]["artifacts"].as_array().unwrap();
    assert_eq!(artifacts.len(), 1, "receipt: {receipt}");
    assert!(artifacts[0]["sha256"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert_eq!(artifacts[0]["role"], "log");
    // The artifact digest resolves in the content-addressed store
    // (sharded: sha256/<2-char prefix>/<hex>/content).
    let digest = artifacts[0]["sha256"].as_str().unwrap();
    let hex = digest.strip_prefix("sha256:").unwrap();
    assert!(repo
        .path()
        .join(".pulse/evidence/artifacts/sha256")
        .join(&hex[0..2])
        .join(hex)
        .join("content")
        .exists());

    // Run record and event point at the receipt.
    let record = run_record(&repo, &ticket_id);
    assert_eq!(record["receipt_id"], receipt_id.as_str());
    let events = walk_events(&repo.path().join(".pulse/events"));
    assert!(events.iter().any(|event| {
        event["event_type"] == "run.completed"
            && event["payload"]["receipt_id"] == receipt_id.as_str()
    }));
}

#[test]
fn failed_case_records_a_failed_receipt() {
    let repo = TestRepo::from_fixture("minimal-service");
    let (ticket_id, _story_id) = qa_fixture(
        &repo,
        r#"
echo '{"cases": [{"id": "QA-001", "status": "failed", "observation": "duplicated reservation"}], "artifacts": [], "findings": []}'
"#,
    );
    let out = run_qa(&repo, &ticket_id);
    // The script produced evidence; the product failure lives in the receipt.
    assert_eq!(out["status"], "completed", "outcome: {out}");
    let receipt = load_single_receipt(&repo);
    assert_eq!(receipt["result"], "failed");
    assert_eq!(receipt["payload"]["cases"][0]["outcome"], "product_failure");
}

#[test]
fn baseline_drift_under_the_run_records_inconclusive_and_demotes_the_run() {
    let repo = TestRepo::from_fixture("minimal-service");
    let (ticket_id, _story_id) = qa_fixture(
        &repo,
        r#"
# The executor itself moves the baseline while the run is in flight.
printf '\n<!-- drifted -->\n' >> "works/$STORY_ID/qa.md"
echo '{"cases": [{"id": "QA-001", "status": "passed", "observation": "claims"}], "artifacts": [], "findings": []}'
"#,
    );
    let out = run_qa(&repo, &ticket_id);
    assert_eq!(out["status"], "inconclusive", "outcome: {out}");
    assert_eq!(out["inconclusive_reason"], "qa_baseline_drift");

    // Pulse recorded what actually happened: inconclusive, script ignored.
    let receipt = load_single_receipt(&repo);
    assert_eq!(receipt["result"], "inconclusive");
    assert_eq!(receipt["payload"]["cases"][0]["outcome"], "inconclusive");
    assert!(receipt["payload"]["observations"][0]
        .as_str()
        .unwrap()
        .contains("qa_baseline_drift:"));
    let record = run_record(&repo, &ticket_id);
    assert_eq!(record["receipt_id"], receipt["id"]);
}

#[test]
fn unresolvable_artifact_demotes_the_run_and_records_nothing() {
    let repo = TestRepo::from_fixture("minimal-service");
    let (ticket_id, _story_id) = qa_fixture(
        &repo,
        r#"
echo '{"cases": [{"id": "QA-001", "status": "passed", "observation": "ok"}], "artifacts": [{"path": "works/missing/artifact.log", "role": "log"}], "findings": []}'
"#,
    );
    let out = run_qa(&repo, &ticket_id);
    assert_eq!(out["status"], "inconclusive", "outcome: {out}");
    assert_eq!(out["inconclusive_reason"], "artifact_ingest_failed");
    assert!(out.get("receipt_id").is_none());
    assert_eq!(
        count_receipts(&repo),
        0,
        "an unclean run must not record a qa_checkpoint"
    );
}

#[test]
fn output_without_artifacts_still_records_an_artifact_free_receipt() {
    let repo = TestRepo::from_fixture("minimal-service");
    let (ticket_id, _story_id) = qa_fixture(
        &repo,
        r#"
echo '{"cases": [{"id": "QA-001", "status": "passed", "observation": "ok"}], "findings": []}'
"#,
    );
    let out = run_qa(&repo, &ticket_id);
    assert_eq!(out["status"], "completed", "outcome: {out}");
    let receipt = load_single_receipt(&repo);
    assert_eq!(receipt["result"], "passed");
    assert_eq!(
        receipt["bindings"]["artifacts"].as_array().unwrap().len(),
        0
    );
}

#[test]
fn malformed_output_reports_leave_no_receipt() {
    let repo = TestRepo::from_fixture("minimal-service");
    let (ticket_id, _story_id) = qa_fixture(
        &repo,
        r#"
echo '{"cases": [{"id": "QA-UNKNOWN", "status": "passed", "observation": "invented"}], "artifacts": [], "findings": []}'
"#,
    );
    let out = run_qa(&repo, &ticket_id);
    assert_eq!(out["status"], "inconclusive", "outcome: {out}");
    assert_eq!(out["inconclusive_reason"], "qa_receipt_invalid");
    assert_eq!(count_receipts(&repo), 0);
}

fn count_receipts(repo: &TestRepo) -> usize {
    fs::read_dir(repo.path().join(".pulse/evidence/receipts"))
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
                .count()
        })
        .unwrap_or(0)
}

fn walk_events(dir: &std::path::Path) -> Vec<Value> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_events(&path));
        } else if let Ok(bytes) = fs::read(&path) {
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                out.push(value);
            }
        }
    }
    out
}
