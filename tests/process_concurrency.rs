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

#[test]
fn two_processes_editing_same_revision_have_one_winner() {
    let repo = tempfile::tempdir().unwrap();
    let created = run_ok(
        &repo,
        &[
            "work", "create", "--kind", "ticket", "--title", "Original", "--json",
        ],
    );
    let id = created["value"]["id"].as_str().unwrap().to_string();

    let mut first = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "work",
            "edit",
            &id,
            "--expected-revision",
            "1",
            "--title",
            "First",
            "--json",
        ])
        .spawn()
        .expect("spawn first");
    let mut second = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "work",
            "edit",
            &id,
            "--expected-revision",
            "1",
            "--title",
            "Second",
            "--json",
        ])
        .spawn()
        .expect("spawn second");

    let first_status = first.wait().unwrap();
    let second_status = second.wait().unwrap();
    assert_ne!(first_status.success(), second_status.success());

    let shown = run_ok(&repo, &["work", "show", &id, "--json"]);
    assert_eq!(shown["node"]["revision"], 2);
    let title = shown["node"]["title"].as_str().unwrap();
    assert!(title == "First" || title == "Second");
}

#[test]
fn two_processes_editing_different_nodes_both_commit() {
    let repo = tempfile::tempdir().unwrap();
    let a = run_ok(
        &repo,
        &[
            "work", "create", "--kind", "ticket", "--title", "A", "--json",
        ],
    );
    let b = run_ok(
        &repo,
        &[
            "work", "create", "--kind", "ticket", "--title", "B", "--json",
        ],
    );
    let a_id = a["value"]["id"].as_str().unwrap().to_string();
    let b_id = b["value"]["id"].as_str().unwrap().to_string();

    let mut first = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "work",
            "edit",
            &a_id,
            "--expected-revision",
            "1",
            "--title",
            "A2",
            "--json",
        ])
        .spawn()
        .expect("spawn first");
    let mut second = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "work",
            "edit",
            &b_id,
            "--expected-revision",
            "1",
            "--title",
            "B2",
            "--json",
        ])
        .spawn()
        .expect("spawn second");

    assert!(first.wait().unwrap().success());
    assert!(second.wait().unwrap().success());

    assert_eq!(
        run_ok(&repo, &["work", "show", &a_id, "--json"])["node"]["title"],
        "A2"
    );
    assert_eq!(
        run_ok(&repo, &["work", "show", &b_id, "--json"])["node"]["title"],
        "B2"
    );
    assert!(repo
        .path()
        .join(".pulse/workgraph/nodes")
        .join(format!("{a_id}.json"))
        .exists());
    assert!(repo
        .path()
        .join(".pulse/workgraph/nodes")
        .join(format!("{b_id}.json"))
        .exists());
}
