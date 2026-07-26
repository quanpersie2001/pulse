use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
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

fn write_json(path: &Path, value: &Value) -> String {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    path.to_string_lossy().to_string()
}

fn wait_for_transaction_intent(repo: &TempDir) {
    let transactions = repo.path().join(".pulse/runtime/transactions");
    let start = Instant::now();
    while transactions
        .read_dir()
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(true)
    {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "transaction intent was not persisted"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn wait_for_path_exists(path: &Path) {
    let start = Instant::now();
    while !path.exists() {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "{} was not written before failpoint",
            path.display()
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn kill_spawned_at_intent(mut child: std::process::Child, repo: &TempDir) {
    wait_for_transaction_intent(repo);
    thread::sleep(Duration::from_millis(100));
    child.kill().expect("kill child");
    let _ = child.wait();
}

fn kill_spawned_after_path(mut child: std::process::Child, repo: &TempDir, path: &Path) {
    wait_for_transaction_intent(repo);
    wait_for_path_exists(path);
    thread::sleep(Duration::from_millis(100));
    child.kill().expect("kill child");
    let _ = child.wait();
}

fn kill_spawned_after_event(
    mut child: std::process::Child,
    repo: &TempDir,
    event_type: &str,
    subject: &str,
) {
    wait_for_transaction_intent(repo);
    let start = Instant::now();
    while event_count_by_type_and_subject(repo, event_type, subject) == 0 {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "event {event_type} for {subject} was not written before failpoint"
        );
        thread::sleep(Duration::from_millis(20));
    }
    thread::sleep(Duration::from_millis(100));
    child.kill().expect("kill child");
    let _ = child.wait();
}

fn event_count_by_type_and_subject(repo: &TempDir, event_type: &str, subject: &str) -> usize {
    let events = repo.path().join(".pulse/events");
    if !events.exists() {
        return 0;
    }
    let mut count = 0;
    let mut stack = vec![events];
    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                stack.push(entry.path());
            } else if entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                let value: Value =
                    serde_json::from_slice(&fs::read(entry.path()).unwrap()).unwrap();
                if value.get("event_type").and_then(Value::as_str) == Some(event_type)
                    && value.get("subject").and_then(Value::as_str) == Some(subject)
                {
                    count += 1;
                }
            }
        }
    }
    count
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
    (repo, work["value"]["id"].as_str().unwrap().to_string())
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

fn create_learning(repo: &TempDir, work_id: &str) {
    let draft_file = write_json(&repo.path().join("learning.json"), &draft(work_id));
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
    );
}

fn spawn_knowledge_create(
    repo: &TempDir,
    failpoint: &str,
    draft_file: &str,
) -> std::process::Child {
    Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .arg("--test-failpoint")
        .arg(failpoint)
        .args([
            "knowledge",
            "create",
            "--file",
            draft_file,
            "--actor",
            "human:test",
            "--json",
        ])
        .env("PULSE_FAILPOINT_SLEEP_MS", "30000")
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn knowledge create failpoint command")
}

fn spawn_relation_add(repo: &TempDir, failpoint: &str) -> std::process::Child {
    Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .arg("--test-failpoint")
        .arg(failpoint)
        .args([
            "knowledge",
            "relation",
            "add",
            "LRN-001",
            "--type",
            "promoted_to",
            "--to-kind",
            "document",
            "--to",
            "DOC-KNOWLEDGE",
            "--expected-revision",
            "1",
            "--actor",
            "human:test",
            "--json",
        ])
        .env("PULSE_FAILPOINT_SLEEP_MS", "30000")
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn relation add failpoint command")
}

fn write_document_registry(repo: &TempDir) {
    fs::create_dir_all(repo.path().join("docs/knowledge")).unwrap();
    fs::write(repo.path().join("docs/knowledge/learning.md"), b"learning").unwrap();
    fs::create_dir_all(repo.path().join(".pulse/docs")).unwrap();
    let registry = json!({
        "schema_version": 1,
        "revision": 1,
        "repository_id": "repo_test",
        "retrieval": null,
        "documents": [{
            "id": "DOC-KNOWLEDGE",
            "revision": 1,
            "path": "docs/knowledge/learning.md",
            "kind": "domain",
            "authority": "approved",
            "lifecycle": "current",
            "owner": "team:docs",
            "summary": "Knowledge doc",
            "aliases": [],
            "scope": {"paths": [], "domains": [], "work_labels": []},
            "review_policy": "none",
            "verification_profile": "domain-doc",
            "generated": null,
            "superseded_by": null
        }]
    });
    fs::write(
        repo.path().join(".pulse/docs/registry.json"),
        serde_json::to_vec_pretty(&registry).unwrap(),
    )
    .unwrap();
}

#[test]
fn knowledge_create_failpoints_recover_to_coherent_store_and_single_event() {
    let (repo, work_id) = setup_repo();
    let draft_file = write_json(&repo.path().join("learning.json"), &draft(&work_id));

    let child = spawn_knowledge_create(&repo, "after_intent", &draft_file);
    kill_spawned_at_intent(child, &repo);
    run_ok(&repo, &["graph", "recover", "--json"]);
    assert!(!repo
        .path()
        .join(".pulse/knowledge/entries/LRN-001.json")
        .exists());
    assert_eq!(
        event_count_by_type_and_subject(&repo, "knowledge.learning.created", "LRN-001"),
        0
    );

    let child = spawn_knowledge_create(&repo, "after_multi_target_first", &draft_file);
    kill_spawned_after_path(
        child,
        &repo,
        &repo.path().join(".pulse/knowledge/entries/LRN-001.json"),
    );
    run_ok(&repo, &["graph", "recover", "--json"]);
    let shown = run_ok(&repo, &["knowledge", "show", "LRN-001", "--json"]);
    assert_eq!(shown["learning"]["id"], "LRN-001");
    assert_eq!(shown["relations"].as_array().unwrap().len(), 1);
    assert_eq!(
        event_count_by_type_and_subject(&repo, "knowledge.learning.created", "LRN-001"),
        1
    );

    run_ok(&repo, &["graph", "recover", "--json"]);
    assert_eq!(
        event_count_by_type_and_subject(&repo, "knowledge.learning.created", "LRN-001"),
        1
    );

    let (repo_event, work_id_event) = setup_repo();
    let draft_file_event = write_json(
        &repo_event.path().join("learning.json"),
        &draft(&work_id_event),
    );
    let child = spawn_knowledge_create(&repo_event, "after_event", &draft_file_event);
    kill_spawned_after_event(child, &repo_event, "knowledge.learning.created", "LRN-001");
    assert_eq!(
        event_count_by_type_and_subject(&repo_event, "knowledge.learning.created", "LRN-001"),
        1
    );
    run_ok(&repo_event, &["graph", "recover", "--json"]);
    run_ok(&repo_event, &["graph", "recover", "--json"]);
    assert_eq!(
        event_count_by_type_and_subject(&repo_event, "knowledge.learning.created", "LRN-001"),
        1
    );
}

#[test]
fn knowledge_relation_add_failpoints_recover_entry_relation_and_event_once() {
    let (repo, work_id) = setup_repo();
    create_learning(&repo, &work_id);
    write_document_registry(&repo);
    let relation_path = repo
        .path()
        .join(".pulse/knowledge/relations/promoted-to--LRN-001--document--DOC-KNOWLEDGE.json");

    let child = spawn_relation_add(&repo, "after_multi_target_first");
    kill_spawned_after_path(
        child,
        &repo,
        &repo.path().join(".pulse/knowledge/entries/LRN-001.json"),
    );
    run_ok(&repo, &["graph", "recover", "--json"]);
    assert!(relation_path.exists());
    let shown = run_ok(&repo, &["knowledge", "show", "LRN-001", "--json"]);
    assert_eq!(shown["learning"]["revision"], 2);
    assert_eq!(
        shown["learning"]["promotion"]["relation_ids"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(shown["relations"].as_array().unwrap().len(), 2);
    assert_eq!(
        event_count_by_type_and_subject(&repo, "knowledge.relation.added", "LRN-001"),
        1
    );

    run_ok(&repo, &["graph", "recover", "--json"]);
    assert_eq!(
        event_count_by_type_and_subject(&repo, "knowledge.relation.added", "LRN-001"),
        1
    );

    let retry = run_ok(
        &repo,
        &[
            "knowledge",
            "relation",
            "add",
            "LRN-001",
            "--type",
            "promoted_to",
            "--to-kind",
            "document",
            "--to",
            "DOC-KNOWLEDGE",
            "--expected-revision",
            "1",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(retry["code"], "unchanged");
    assert_eq!(
        event_count_by_type_and_subject(&repo, "knowledge.relation.added", "LRN-001"),
        1
    );
}
