//! Run-declared artifact ingest: `artifacts[] {path, role, case_id?}` in the
//! final output JSON. Valid declarations are hashed and copied into the
//! content-addressed evidence store and recorded on the run record; invalid
//! declarations (missing, outside the repository) demote the run to
//! inconclusive instead of silently dropping evidence.

use std::fs;

use serde_json::Value;

use crate::cli_run::{
    install_worker_script, run_outcome, set_command, set_worker_command, setup_ready_ticket,
};
use crate::common_fixture_repo::TestRepo;

fn verifying_ticket(repo: &TestRepo) -> String {
    let ticket_id = setup_ready_ticket(repo);
    install_worker_script(
        repo,
        r#"
RUN_DIR="$(dirname "$1")"
. "$RUN_DIR/worker-env"
"$PULSE" work handoff --lease "$LEASE_ID" --session "$SESSION_ID" \
  --source-commit "$SOURCE_COMMIT" --summary "did the work" \
  --idempotency-key handoff-1 --json
echo '{"status": "handed_off", "summary": "done"}'
"#,
    );
    set_worker_command(repo, "sh scripts/fake-worker.sh {input}");
    let worker = run_outcome(repo, &ticket_id);
    assert_eq!(worker["status"], "handed_off");
    ticket_id
}

fn set_qa_script(repo: &TestRepo, body: &str) {
    let script = format!("#!/bin/sh\nset -e\nRUN_DIR=\"$(dirname \"$1\")\"\n{body}\n");
    fs::write(repo.path().join("scripts/fake-qa.sh"), script).unwrap();
    set_command(repo, "qa", "sh scripts/fake-qa.sh {input}");
}

fn qa_outcome(repo: &TestRepo, ticket_id: &str) -> Value {
    let output = repo.pulse(&["run", "qa", "--ticket", ticket_id, "--json"]);
    assert!(
        output.status.success(),
        "qa run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn declared_artifacts_are_hashed_into_the_evidence_store() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = verifying_ticket(&repo);
    set_qa_script(
        &repo,
        r#"
mkdir -p "$RUN_DIR/artifacts"
printf 'qa observation log\n' > "$RUN_DIR/artifacts/run.log"
echo "{\"cases\": [], \"artifacts\": [{\"path\": \"$RUN_DIR/artifacts/run.log\", \"role\": \"log\", \"case_id\": \"QA-001\"}]}"
"#,
    );

    let out = qa_outcome(&repo, &ticket_id);
    assert_eq!(out["status"], "completed", "outcome: {out}");
    let artifacts = out["artifacts"].as_array().unwrap();
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0]["role"], "log");
    assert_eq!(artifacts[0]["case_id"], "QA-001");
    assert_eq!(artifacts[0]["size_bytes"], 19);
    let digest = artifacts[0]["sha256"].as_str().unwrap().to_string();
    assert!(digest.starts_with("sha256:"));

    // The content is content-addressed in the evidence store.
    let hex = digest.trim_start_matches("sha256:");
    let content_path = repo
        .path()
        .join(".pulse/evidence/artifacts/sha256")
        .join(&hex[0..2])
        .join(hex)
        .join("content");
    assert_eq!(
        fs::read(&content_path).unwrap(),
        b"qa observation log\n".to_vec()
    );

    // The store's own verification accepts the artifact.
    let verify = repo.pulse_ok(&["evidence", "artifact", "verify", &digest, "--json"]);
    assert_eq!(verify["code"], "artifact_valid");

    // The run record carries the ingest result.
    let record: Value = serde_json::from_slice(
        &fs::read(
            repo.path()
                .join(".pulse/runtime/run")
                .join(&ticket_id)
                .join("qa-outcome.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(record["artifacts"][0]["sha256"], digest.as_str());
    assert_eq!(record["artifacts"][0]["path"], artifacts[0]["path"]);
}

#[test]
fn artifact_outside_the_repository_demotes_the_run_to_inconclusive() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = verifying_ticket(&repo);
    set_qa_script(
        &repo,
        r#"
mkdir -p "$RUN_DIR/artifacts"
echo '{"cases": [], "artifacts": [{"path": "/etc/hosts", "role": "log"}]}'
"#,
    );

    let out = qa_outcome(&repo, &ticket_id);
    assert_eq!(out["status"], "inconclusive", "outcome: {out}");
    assert_eq!(out["inconclusive_reason"], "artifact_ingest_failed");
    assert!(out["summary"]
        .as_str()
        .unwrap()
        .contains("outside the repository"));
    // Nothing was copied into the evidence store.
    let store = repo.path().join(".pulse/evidence/artifacts/sha256");
    assert!(!store.exists() || fs::read_dir(&store).unwrap().next().is_none());
}

#[test]
fn missing_artifact_path_demotes_the_run_to_inconclusive() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = verifying_ticket(&repo);
    set_qa_script(
        &repo,
        r#"
echo '{"cases": [], "artifacts": [{"path": "works/never-written.log", "role": "trace"}]}'
"#,
    );

    let out = qa_outcome(&repo, &ticket_id);
    assert_eq!(out["status"], "inconclusive", "outcome: {out}");
    assert_eq!(out["inconclusive_reason"], "artifact_ingest_failed");
    assert!(out["summary"]
        .as_str()
        .unwrap()
        .contains("does not resolve"));
}

#[test]
fn traversal_artifact_paths_are_refused() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = verifying_ticket(&repo);
    set_qa_script(
        &repo,
        r#"
echo '{"cases": [], "artifacts": [{"path": "../../../etc/hosts", "role": "log"}]}'
"#,
    );

    let out = qa_outcome(&repo, &ticket_id);
    assert_eq!(out["status"], "inconclusive", "outcome: {out}");
    assert_eq!(out["inconclusive_reason"], "artifact_ingest_failed");
}
