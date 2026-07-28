use serde_json::Value;
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

#[test]
fn l31_transition_json_output_and_error_contracts_are_stable() {
    let repo = tempfile::tempdir().unwrap();
    let created = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Original",
            "--role",
            "implementation",
            "--risk",
            "low",
            "--materialization",
            "R0",
            "--json",
        ],
    );
    let id = created["value"]["id"].as_str().unwrap().to_string();

    let ok = run_ok(
        &repo,
        &[
            "work",
            "transition",
            &id,
            "--to",
            "cancelled",
            "--expected-revision",
            "1",
            "--actor",
            "test:actor",
            "--reason-code",
            "obsolete",
            "--reason",
            "No longer needed",
            "--reference",
            "DEC-001",
            "--json",
        ],
    );
    assert_eq!(ok["schema_version"], 1);
    assert_eq!(ok["code"], "transitioned");
    assert_eq!(ok["value"]["status"], "cancelled");
    assert_eq!(ok["value"]["status_reason"]["code"], "obsolete");

    let stale = run_err(
        &repo,
        &[
            "work",
            "transition",
            &id,
            "--to",
            "blocked",
            "--expected-revision",
            "1",
            "--actor",
            "test:actor",
            "--reason-code",
            "blocked",
            "--reason",
            "Blocked",
            "--json",
        ],
    );
    assert_eq!(stale["schema_version"], 1);
    assert_eq!(stale["code"], "cas_conflict");
    assert_eq!(stale["subject"], id);
    assert_eq!(stale["expected_revision"], 1);
    assert_eq!(stale["current_revision"], 2);
}

#[test]
fn cli_missing_reason_fails_before_commit() {
    let repo = tempfile::tempdir().unwrap();
    let created = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Original",
            "--role",
            "implementation",
            "--risk",
            "low",
            "--materialization",
            "R0",
            "--json",
        ],
    );
    let id = created["value"]["id"].as_str().unwrap();
    let err = run_err(
        &repo,
        &[
            "work",
            "transition",
            id,
            "--to",
            "cancelled",
            "--expected-revision",
            "1",
            "--actor",
            "test:actor",
            "--json",
        ],
    );
    assert_eq!(err["code"], "missing_status_reason");

    let shown = run_ok(&repo, &["work", "show", id, "--json"]);
    assert_eq!(shown["node"]["status"], "draft");
    assert_eq!(shown["node"]["revision"], 1);
}

#[test]
fn cli_supersede_requires_receipt_and_rejects_inline_assertion() {
    let repo = tempfile::tempdir().unwrap();
    let old = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Old",
            "--role",
            "implementation",
            "--risk",
            "low",
            "--materialization",
            "R0",
            "--json",
        ],
    );
    let old_id = old["value"]["id"].as_str().unwrap().to_string();
    let replacement = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "story",
            "--title",
            "Replacement",
            "--json",
        ],
    );
    let replacement_id = replacement["value"]["id"].as_str().unwrap().to_string();

    let missing = run_err(
        &repo,
        &[
            "work",
            "supersede",
            &old_id,
            "--by",
            &replacement_id,
            "--expected-revision",
            "1",
            "--reason",
            "absorbed",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(missing["code"], "supersession_receipt_required");

    let assertion = repo.path().join("assertion.json");
    std::fs::write(
        &assertion,
        r#"{
  "assertion_version": 1,
  "asserted_by": "human:test",
  "source_revisions": ["TK-001@1"],
  "claim": "absorbed",
  "references": ["ST-001"]
}"#,
    )
    .unwrap();
    let inline = run_err(
        &repo,
        &[
            "work",
            "supersede",
            &old_id,
            "--by",
            &replacement_id,
            "--expected-revision",
            "1",
            "--reason",
            "absorbed",
            "--assertion",
            assertion.to_str().unwrap(),
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(inline["code"], "inline_supersession_assertion_unsupported");

    let shown = run_ok(&repo, &["work", "show", &old_id, "--json"]);
    assert_eq!(shown["node"]["status"], "draft");
    assert_eq!(shown["node"]["revision"], 1);
}

#[test]
fn public_cli_ticket_create_requires_explicit_assessed_classification() {
    let repo = tempfile::tempdir().unwrap();

    let missing = run_err(
        &repo,
        &[
            "work", "create", "--kind", "ticket", "--title", "Missing", "--json",
        ],
    );
    assert_eq!(missing["code"], "work_classification_missing");

    let unassessed = run_err(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Unassessed",
            "--role",
            "implementation",
            "--risk",
            "unassessed",
            "--materialization",
            "R0",
            "--json",
        ],
    );
    assert_eq!(unassessed["code"], "risk_materialization_unassessed");

    let created = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Assessed",
            "--role",
            "decision_work",
            "--risk",
            "medium",
            "--materialization",
            "R2",
            "--json",
        ],
    );
    assert_eq!(created["code"], "created");
    assert_eq!(created["value"]["role"], "decision_work");
    assert_eq!(created["value"]["risk"], "medium");
    assert_eq!(created["value"]["materialization"], "R2");
    assert!(created["value"].get("decision_work").is_none());
}

#[test]
fn cli_ready_to_active_public_transition_rejects_with_prepared_assignment_required() {
    let repo = tempfile::tempdir().unwrap();
    let created = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "ReadyTicket",
            "--role",
            "implementation",
            "--risk",
            "low",
            "--materialization",
            "R0",
            "--json",
        ],
    );
    let id = created["value"]["id"].as_str().unwrap().to_string();

    // Manually set the node to Ready (bypass readiness gate for test setup).
    let node_path = repo
        .path()
        .join(format!(".pulse/workgraph/nodes/{id}.json"));
    let mut node: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&node_path).unwrap()).unwrap();
    node["status"] = serde_json::json!("ready");
    node["revision"] = serde_json::json!(2u64);
    std::fs::write(&node_path, serde_json::to_vec_pretty(&node).unwrap()).unwrap();

    // Now call the CLI with --expected-revision 2 (matching the manual bump).
    let err = run_err(
        &repo,
        &[
            "work",
            "transition",
            &id,
            "--to",
            "active",
            "--expected-revision",
            "2",
            "--actor",
            "test:actor",
            "--json",
        ],
    );
    assert_eq!(
        err["code"], "prepared_assignment_required",
        "CLI transition Ready->Active must report prepared_assignment_required not {:?}",
        err["code"]
    );

    // The node must remain unchanged.
    let shown = run_ok(&repo, &["work", "show", &id, "--json"]);
    assert_eq!(shown["node"]["status"], "ready");
    assert_eq!(shown["node"]["revision"], 2);
}

#[test]
fn public_cli_rejects_classification_flags_for_non_tickets() {
    let repo = tempfile::tempdir().unwrap();
    let err = run_err(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "story",
            "--title",
            "Story",
            "--role",
            "implementation",
            "--risk",
            "low",
            "--materialization",
            "R0",
            "--json",
        ],
    );
    assert_eq!(err["code"], "work_classification_not_allowed");
}
