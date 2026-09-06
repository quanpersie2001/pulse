//! Redaction on the tracked evidence plane (Decision 0012 §4).
//!
//! The five write paths — receipt record, work handoff, work verify, note,
//! knowledge create — refuse secret-shaped strings and absolute paths
//! outside the repository, and rewrite in-repo absolute paths to
//! repository-relative before anything is committed.

use std::fs;

use serde_json::{json, Value};

use crate::common_fixture_repo::TestRepo;

fn error_code(output: &std::process::Output) -> (String, String) {
    assert!(
        !output.status.success(),
        "expected failure: stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    (
        err["code"].as_str().unwrap().to_string(),
        err["message"].as_str().unwrap().to_string(),
    )
}

#[test]
fn note_refuses_secret_and_outside_paths_and_rewrites_repo_paths() {
    let repo = TestRepo::from_fixture("minimal-service");
    repo.pulse_ok(&["init", "--actor", "human:tester", "--json"]);
    let created = repo.pulse_ok(&[
        "work",
        "create",
        "--kind",
        "ticket",
        "--title",
        "Redaction target",
        "--risk",
        "low",
        "--json",
    ]);
    let ticket_id = created["value"]["id"].as_str().unwrap().to_string();

    // A secret-shaped string is refused; the value never reaches disk.
    let output = repo.pulse(&[
        "note",
        "--ticket",
        &ticket_id,
        "--message",
        "key was AKIAIOSFODNN7EXAMPLE rotated",
        "--from",
        "human:tester",
        "--json",
    ]);
    let (code, message) = error_code(&output);
    assert_eq!(code, "receipt_privacy_violation");
    assert!(message.contains("aws_access_key"));
    assert!(!message.contains("AKIAIOSFODNN7"));

    // A value starting with an absolute path outside the repository is
    // refused (Decision 0012 §4 keys on the leading path, not prose).
    let output = repo.pulse(&[
        "note",
        "--ticket",
        &ticket_id,
        "--message",
        "/Users/dev/secrets/notes.md has the context",
        "--from",
        "human:tester",
        "--json",
    ]);
    let (code, _) = error_code(&output);
    assert_eq!(code, "receipt_privacy_violation");

    // A value that is exactly an absolute path inside the repository is
    // rewritten to repository-relative.
    let inside = repo.path().join("docs/context.md");
    fs::write(&inside, b"ok").unwrap();
    let shown = repo.pulse_ok(&[
        "note",
        "--ticket",
        &ticket_id,
        "--message",
        inside.to_string_lossy().as_ref(),
        "--from",
        "human:tester",
        "--json",
    ]);
    assert_eq!(shown["message"], "docs/context.md");
}

#[test]
fn knowledge_create_refuses_secret_shaped_text() {
    let repo = TestRepo::from_fixture("minimal-service");
    repo.pulse_ok(&["init", "--actor", "human:tester", "--json"]);
    let created = repo.pulse_ok(&[
        "work",
        "create",
        "--kind",
        "ticket",
        "--title",
        "Learning source",
        "--risk",
        "low",
        "--json",
    ]);
    let ticket_id = created["value"]["id"].as_str().unwrap().to_string();
    let draft = json!({
        "title": "Token handling",
        "kind": "failure_pattern",
        "severity": "high",
        "summary": "use sk-0123456789abcdefghijklmnop in tests",
        "guidance": {"do": [], "avoid": [], "required_checks": []},
        "applicability": {"paths": ["src/**"], "symbols": [], "risks": []},
        "provenance_targets": [],
        "source_commits": [],
        "routing": null,
        "promotion": null,
        "freshness": null,
        "trust": null,
        "content": null
    });
    let draft_file = repo.path().join("learning.json");
    fs::write(&draft_file, serde_json::to_vec_pretty(&draft).unwrap()).unwrap();
    let output = repo.pulse(&[
        "knowledge",
        "capture",
        "--from",
        &ticket_id,
        "--file",
        draft_file.to_string_lossy().as_ref(),
        "--actor",
        "human:tester",
        "--json",
    ]);
    let (code, message) = error_code(&output);
    assert_eq!(code, "receipt_privacy_violation");
    assert!(message.contains("api_key"));
    assert!(!message.contains("sk-0123456789"));
}
