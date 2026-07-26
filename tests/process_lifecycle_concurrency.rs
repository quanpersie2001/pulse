use serde_json::Value;
use std::process::{Command, Stdio};
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

fn error_json(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stderr).expect("json stderr")
}

#[test]
fn l32_two_processes_transitioning_same_expected_revision_have_one_winner() {
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

    let first = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "work",
            "transition",
            &id,
            "--to",
            "cancelled",
            "--expected-revision",
            "1",
            "--reason-code",
            "obsolete",
            "--reason",
            "first cancellation",
            "--actor",
            "test:first",
            "--json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn first");
    let second = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "work",
            "transition",
            &id,
            "--to",
            "cancelled",
            "--expected-revision",
            "1",
            "--reason-code",
            "obsolete",
            "--reason",
            "second cancellation",
            "--actor",
            "test:second",
            "--json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn second");

    let first_output = first.wait_with_output().unwrap();
    let second_output = second.wait_with_output().unwrap();
    let outputs = [&first_output, &second_output];
    assert_eq!(
        outputs
            .iter()
            .filter(|output| output.status.success())
            .count(),
        1,
        "stdout/stderr: first=({},{}) second=({},{})",
        String::from_utf8_lossy(&first_output.stdout),
        String::from_utf8_lossy(&first_output.stderr),
        String::from_utf8_lossy(&second_output.stdout),
        String::from_utf8_lossy(&second_output.stderr),
    );
    let loser = outputs
        .into_iter()
        .find(|output| !output.status.success())
        .expect("one loser");
    let err = error_json(loser);
    assert_eq!(err["code"], "cas_conflict");
    assert_eq!(err["subject"], id);
    assert_eq!(err["expected_revision"], 1);
    assert_eq!(err["current_revision"], 2);

    let shown = run_ok(&repo, &["work", "show", &id, "--json"]);
    assert_eq!(shown["node"]["revision"], 2);
    assert_eq!(shown["node"]["status"], "cancelled");
    assert_eq!(shown["node"]["status_reason"]["code"], "obsolete");
}
