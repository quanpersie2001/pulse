use serde_json::Value;
use std::process::Command;
use tempfile::TempDir;

fn bin() -> String {
    std::env::var("CARGO_BIN_EXE_pulse").unwrap_or_else(|_| "target/debug/pulse".to_string())
}

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
            "work", "create", "--kind", "ticket", "--title", "Original", "--json",
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
            "work", "create", "--kind", "ticket", "--title", "Original", "--json",
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
