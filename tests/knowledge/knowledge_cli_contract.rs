use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

use crate::common_bin::bin;

fn run(repo: &TempDir, args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args(args)
        .output()
        .expect("run pulse")
}

fn run_ok(repo: &TempDir, args: &[&str]) -> Value {
    let output = run(repo, args);
    assert!(
        output.status.success(),
        "command failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("json stdout")
}

fn run_err(repo: &TempDir, args: &[&str]) -> Value {
    let output = run(repo, args);
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stderr).expect("json stderr")
}

fn write_json(path: &Path, value: &Value) -> String {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    path.to_string_lossy().to_string()
}

fn setup_repo() -> (TempDir, String) {
    let repo = tempfile::tempdir().unwrap();
    let work = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Knowledge source",
            "--role",
            "implementation",
            "--risk",
            "low",
            "--materialization",
            "R0",
            "--json",
        ],
    );
    let work_id = work["value"]["id"].as_str().unwrap().to_string();
    (repo, work_id)
}

fn draft(work_id: &str) -> Value {
    json!({
        "title": "Token rotation requires atomic mutation",
        "kind": "failure_pattern",
        "severity": "high",
        "summary": "Concurrent refresh can issue invalid tokens when rotation uses check-then-act.",
        "guidance": {
            "do": ["Use an atomic state transition."],
            "avoid": ["Do not split rotation into unguarded read then write."],
            "required_checks": ["Exercise concurrent refresh attempts."]
        },
        "applicability": {
            "paths": ["src/auth/**"],
            "symbols": ["rotateRefreshToken"],
            "risks": ["concurrency"]
        },
        "provenance_targets": [{
            "relation": "derived_from",
            "kind": "work",
            "id": work_id,
            "revision": 1,
            "content_hash": null
        }],
        "source_commits": [],
        "routing": null,
        "promotion": null,
        "freshness": null,
        "trust": null,
        "content": null
    })
}

#[test]
fn knowledge_cli_json_contracts_cover_crud_relations_validation_export_status() {
    let (repo, work_id) = setup_repo();
    let draft_file = write_json(&repo.path().join("learning.json"), &draft(&work_id));

    let created = run_ok(
        &repo,
        &[
            "knowledge",
            "create",
            "--file",
            &draft_file,
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(created["schema_version"], 1);
    assert_eq!(created["code"], "created");
    assert_eq!(created["status"], "created");
    assert_eq!(created["value"]["id"], "LRN-001");
    assert_eq!(created["value"]["revision"], 1);
    assert_eq!(created["value"]["status"], "candidate");
    assert_eq!(created["value"]["validation"]["confidence"], "low");
    assert_eq!(created["relations"].as_array().unwrap().len(), 1);
    assert!(created["knowledge_fingerprint"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));

    let shown = run_ok(&repo, &["knowledge", "show", "LRN-001", "--json"]);
    assert_eq!(shown["schema_version"], 1);
    assert_eq!(shown["code"], "ok");
    assert_eq!(shown["learning"]["id"], "LRN-001");
    assert_eq!(shown["relations"].as_array().unwrap().len(), 1);

    let listed = run_ok(
        &repo,
        &[
            "knowledge",
            "list",
            "--status",
            "candidate",
            "--kind",
            "failure_pattern",
            "--json",
        ],
    );
    assert_eq!(listed["schema_version"], 1);
    assert_eq!(listed["code"], "ok");
    assert_eq!(listed["items"].as_array().unwrap().len(), 1);

    let patch_file = write_json(
        &repo.path().join("learning-patch.json"),
        &json!({"summary": "Updated concise summary."}),
    );
    let edited = run_ok(
        &repo,
        &[
            "knowledge",
            "edit",
            "LRN-001",
            "--expected-revision",
            "1",
            "--patch",
            &patch_file,
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(edited["code"], "updated");
    assert_eq!(edited["status"], "updated");
    assert_eq!(edited["value"]["revision"], 2);
    assert_eq!(edited["value"]["summary"], "Updated concise summary.");

    let relation = run_ok(
        &repo,
        &[
            "knowledge",
            "relation",
            "add",
            "LRN-001",
            "--type",
            "applied_to",
            "--to-kind",
            "work",
            "--to",
            &work_id,
            "--target-revision",
            "1",
            "--expected-revision",
            "2",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(relation["schema_version"], 1);
    assert_eq!(relation["code"], "created");
    assert_eq!(relation["status"], "created");
    assert_eq!(
        relation["relation_id"],
        format!("applied-to--LRN-001--work--{work_id}")
    );

    let valid = run_ok(&repo, &["knowledge", "validate", "--json"]);
    assert_eq!(valid["schema_version"], 1);
    assert_eq!(valid["code"], "ok");
    assert_eq!(valid["valid"], true);
    assert!(valid["errors"].as_array().unwrap().is_empty());

    let exported = run_ok(&repo, &["knowledge", "export", "--json"]);
    assert_eq!(exported["schema_version"], 1);
    assert_eq!(exported["code"], "ok");
    assert_eq!(exported["counts"]["entries"], 1);
    assert_eq!(exported["counts"]["relations"], 2);
    assert_eq!(
        exported["eligibility"]["future_default_search"]["excluded"][0]["id"],
        "LRN-001"
    );
    assert_eq!(
        exported["eligibility"]["future_default_search"]["excluded"][0]["reason_codes"],
        json!(["learning_candidate"])
    );

    let status = run_ok(&repo, &["knowledge", "status", "--json"]);
    assert_eq!(status["schema_version"], 1);
    assert_eq!(status["code"], "ok");
    assert_eq!(status["counts"]["entries"], 1);
    assert_eq!(status["cache_state"], "current");
}

#[test]
fn knowledge_cli_errors_are_json_and_non_zero() {
    let (repo, work_id) = setup_repo();
    let draft_file = write_json(&repo.path().join("learning.json"), &draft(&work_id));
    run_ok(
        &repo,
        &[
            "knowledge",
            "create",
            "--file",
            &draft_file,
            "--actor",
            "human:test",
            "--json",
        ],
    );

    let stale_patch = write_json(
        &repo.path().join("stale-patch.json"),
        &json!({"title": "stale edit"}),
    );
    let stale = run_err(
        &repo,
        &[
            "knowledge",
            "edit",
            "LRN-001",
            "--expected-revision",
            "99",
            "--patch",
            &stale_patch,
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(stale["schema_version"], 1);
    assert_eq!(stale["code"], "cas_conflict");
    assert_eq!(stale["subject"], "LRN-001");
    assert_eq!(stale["expected_revision"], 99);
    assert_eq!(stale["current_revision"], 1);

    let missing = run_err(&repo, &["knowledge", "show", "LRN-404", "--json"]);
    assert_eq!(missing["schema_version"], 1);
    assert_eq!(missing["code"], "learning_not_found");

    let invalid = run_err(
        &repo,
        &[
            "knowledge",
            "relation",
            "add",
            "LRN-001",
            "--type",
            "applied_to",
            "--to-kind",
            "work",
            "--to",
            "TICKET-NOPE",
            "--expected-revision",
            "1",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(invalid["schema_version"], 1);
    assert_eq!(invalid["code"], "knowledge_relation_endpoint_missing");
}
