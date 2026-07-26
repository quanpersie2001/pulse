use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

fn bin() -> String {
    std::env::var("CARGO_BIN_EXE_pulse").unwrap_or_else(|_| "target/debug/pulse".to_string())
}

fn run(repo: &TempDir, args: &[&str]) -> Output {
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

fn json_from_output(output: &Output) -> Value {
    if output.status.success() {
        serde_json::from_slice(&output.stdout).expect("json stdout")
    } else {
        serde_json::from_slice(&output.stderr).expect("json stderr")
    }
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
            "decision",
            "--title",
            "Knowledge source",
            "--json",
        ],
    );
    (repo, work["value"]["id"].as_str().unwrap().to_string())
}

fn draft(work_id: &str, title: &str) -> Value {
    json!({
        "title": title,
        "kind": "failure_pattern",
        "severity": "high",
        "summary": "Concurrent decision handling can drift when learnings are not stored atomically.",
        "guidance": {
            "do": ["Use an atomic state transition."],
            "avoid": ["Do not split rotation into unguarded read then write."],
            "required_checks": ["Exercise concurrent refresh attempts."]
        },
        "applicability": {
            "paths": ["src/decision/**"],
            "symbols": ["recordDecision"],
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

fn create_learning(repo: &TempDir, work_id: &str) -> Value {
    let draft_file = write_json(
        &repo.path().join("learning.json"),
        &draft(work_id, "Concurrent token learning"),
    );
    run_ok(
        repo,
        &[
            "knowledge",
            "create",
            "--file",
            &draft_file,
            "--actor",
            "human:test",
            "--json",
        ],
    )
}

#[test]
fn two_processes_creating_knowledge_records_allocate_unique_ids() {
    let (repo, work_id) = setup_repo();
    let first_file = write_json(
        &repo.path().join("learning-a.json"),
        &draft(&work_id, "Learning A"),
    );
    let second_file = write_json(
        &repo.path().join("learning-b.json"),
        &draft(&work_id, "Learning B"),
    );

    let first = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "knowledge",
            "create",
            "--file",
            &first_file,
            "--actor",
            "human:test",
            "--json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn first create");
    let second = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "knowledge",
            "create",
            "--file",
            &second_file,
            "--actor",
            "human:test",
            "--json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn second create");

    let first = first.wait_with_output().expect("first output");
    let second = second.wait_with_output().expect("second output");
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );

    let list = run_ok(&repo, &["knowledge", "list", "--json"]);
    let ids: Vec<_> = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["LRN-001", "LRN-002"]);
}

#[test]
fn two_processes_editing_same_learning_revision_have_one_winner() {
    let (repo, work_id) = setup_repo();
    create_learning(&repo, &work_id);
    let first_patch = write_json(
        &repo.path().join("patch-a.json"),
        &json!({"title": "First"}),
    );
    let second_patch = write_json(
        &repo.path().join("patch-b.json"),
        &json!({"title": "Second"}),
    );

    let first = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "knowledge",
            "edit",
            "LRN-001",
            "--expected-revision",
            "1",
            "--patch",
            &first_patch,
            "--actor",
            "human:test",
            "--json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn first edit");
    let second = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "knowledge",
            "edit",
            "LRN-001",
            "--expected-revision",
            "1",
            "--patch",
            &second_patch,
            "--actor",
            "human:test",
            "--json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn second edit");
    let first = first.wait_with_output().expect("first output");
    let second = second.wait_with_output().expect("second output");

    assert_ne!(first.status.success(), second.status.success());
    let loser = if first.status.success() {
        &second
    } else {
        &first
    };
    let loser_json = json_from_output(loser);
    assert_eq!(loser_json["code"], "cas_conflict");
    assert_eq!(loser_json["expected_revision"], 1);
    assert_eq!(loser_json["current_revision"], 2);

    let shown = run_ok(&repo, &["knowledge", "show", "LRN-001", "--json"]);
    assert_eq!(shown["learning"]["revision"], 2);
    let title = shown["learning"]["title"].as_str().unwrap();
    assert!(title == "First" || title == "Second");
}

#[test]
fn concurrent_relation_retry_is_idempotent_after_entry_revision_bump() {
    let (repo, work_id) = setup_repo();
    create_learning(&repo, &work_id);

    let first = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "knowledge",
            "relation",
            "add",
            "LRN-001",
            "--type",
            "promoted_to",
            "--to-kind",
            "decision",
            "--to",
            &work_id,
            "--target-revision",
            "1",
            "--expected-revision",
            "1",
            "--actor",
            "human:test",
            "--json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn first relation add");
    let second = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "knowledge",
            "relation",
            "add",
            "LRN-001",
            "--type",
            "promoted_to",
            "--to-kind",
            "decision",
            "--to",
            &work_id,
            "--target-revision",
            "1",
            "--expected-revision",
            "1",
            "--actor",
            "human:test",
            "--json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn second relation add");
    let first = first.wait_with_output().expect("first output");
    let second = second.wait_with_output().expect("second output");

    assert!(first.status.success());
    assert!(second.status.success());
    let mut codes = [
        json_from_output(&first)["code"]
            .as_str()
            .unwrap()
            .to_string(),
        json_from_output(&second)["code"]
            .as_str()
            .unwrap()
            .to_string(),
    ];
    codes.sort();
    assert_eq!(codes, ["created", "unchanged"]);

    let shown = run_ok(&repo, &["knowledge", "show", "LRN-001", "--json"]);
    assert_eq!(shown["learning"]["revision"], 2);
    let promotion_relations = shown["learning"]["promotion"]["relation_ids"]
        .as_array()
        .unwrap();
    assert_eq!(promotion_relations.len(), 1);
    assert_eq!(shown["relations"].as_array().unwrap().len(), 2);
}
