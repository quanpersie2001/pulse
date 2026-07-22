use pulse::canonical_json::hash_bytes;
use serde_json::Value;
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

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn git_init_commit_all(repo: &Path) {
    git(repo, &["init"]);
    git(repo, &["config", "user.email", "test@example.com"]);
    git(repo, &["config", "user.name", "Test User"]);
    git(repo, &["add", "."]);
    git(repo, &["commit", "--allow-empty", "-m", "snapshot"]);
}

fn git_head(repo: &Path) -> String {
    git(repo, &["rev-parse", "HEAD"])
}

fn event_count(repo: &TempDir) -> usize {
    let events = repo.path().join(".pulse/events");
    if !events.exists() {
        return 0;
    }
    fs::read_dir(events)
        .unwrap()
        .flat_map(|date| fs::read_dir(date.unwrap().path()).unwrap())
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .path()
                .extension()
                .and_then(|s| s.to_str())
                == Some("json")
        })
        .count()
}

fn write_json(path: &Path, value: &impl serde::Serialize) {
    fs::write(
        path,
        pulse::canonical_json::to_canonical_bytes(value).unwrap(),
    )
    .unwrap();
}

fn event_count_by_type_and_subject(repo: &TempDir, event_type: &str, subject: &str) -> usize {
    let events = repo.path().join(".pulse/events");
    if !events.exists() {
        return 0;
    }
    let mut count = 0;
    for date in fs::read_dir(events).unwrap() {
        for entry in fs::read_dir(date.unwrap().path()).unwrap() {
            let value: Value =
                serde_json::from_slice(&fs::read(entry.unwrap().path()).unwrap()).unwrap();
            if value.get("event_type").and_then(Value::as_str) == Some(event_type)
                && value.get("subject").and_then(Value::as_str) == Some(subject)
            {
                count += 1;
            }
        }
    }
    count
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

fn kill_at_failpoint(repo: &TempDir, failpoint: &str, id: &str) {
    let child = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .arg("--test-failpoint")
        .arg(failpoint)
        .args([
            "work",
            "edit",
            id,
            "--expected-revision",
            "1",
            "--title",
            "Crashed",
            "--json",
        ])
        .env("PULSE_FAILPOINT_SLEEP_MS", "30000")
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn failpoint command");
    kill_spawned_at_intent(child, repo);
}

#[test]
fn killed_after_intent_recovers_by_cleaning_uncommitted_transaction() {
    let repo = tempfile::tempdir().unwrap();
    let created = run_ok(
        &repo,
        &[
            "work", "create", "--kind", "ticket", "--title", "Before", "--json",
        ],
    );
    let id = created["value"]["id"].as_str().unwrap().to_string();
    let before_events = event_count(&repo);

    kill_at_failpoint(&repo, "after_intent", &id);
    run_ok(&repo, &["graph", "recover", "--json"]);

    let shown = run_ok(&repo, &["work", "show", &id, "--json"]);
    assert_eq!(shown["node"]["revision"], 1);
    assert_eq!(shown["node"]["title"], "Before");
    assert_eq!(event_count(&repo), before_events);
}

#[test]
fn killed_after_canonical_recovers_missing_event_without_reapplying_node() {
    let repo = tempfile::tempdir().unwrap();
    let created = run_ok(
        &repo,
        &[
            "work", "create", "--kind", "ticket", "--title", "Before", "--json",
        ],
    );
    let id = created["value"]["id"].as_str().unwrap().to_string();
    let before_events = event_count(&repo);

    kill_at_failpoint(&repo, "after_canonical", &id);
    let crashed = run_ok(&repo, &["work", "show", &id, "--json"]);
    assert_eq!(crashed["node"]["revision"], 2);
    assert_eq!(crashed["node"]["title"], "Crashed");

    run_ok(&repo, &["graph", "recover", "--json"]);
    let recovered = run_ok(&repo, &["work", "show", &id, "--json"]);
    assert_eq!(recovered["node"]["revision"], 2);
    assert_eq!(recovered["node"]["title"], "Crashed");
    assert_eq!(event_count(&repo), before_events + 1);

    run_ok(&repo, &["graph", "recover", "--json"]);
    assert_eq!(event_count(&repo), before_events + 1);
}

#[test]
fn killed_after_event_recovers_by_cleaning_intent_without_duplicate_event() {
    let repo = tempfile::tempdir().unwrap();
    let created = run_ok(
        &repo,
        &[
            "work", "create", "--kind", "ticket", "--title", "Before", "--json",
        ],
    );
    let id = created["value"]["id"].as_str().unwrap().to_string();
    let before_events = event_count(&repo);

    kill_at_failpoint(&repo, "after_event", &id);
    assert_eq!(event_count(&repo), before_events + 1);
    run_ok(&repo, &["graph", "recover", "--json"]);
    run_ok(&repo, &["graph", "recover", "--json"]);
    assert_eq!(event_count(&repo), before_events + 1);
}

#[test]
fn killed_artifact_put_after_content_recovers_metadata_and_event_once() {
    let repo = tempfile::tempdir().unwrap();
    run_ok(&repo, &["evidence", "bootstrap", "--json"]);
    let input = repo.path().join("artifact-notes.txt");
    fs::write(&input, b"artifact process recovery notes").unwrap();
    let digest = hash_bytes(&fs::read(&input).unwrap());
    let hex = digest.strip_prefix("sha256:").unwrap();
    let artifact_dir = repo
        .path()
        .join(".pulse/evidence/artifacts/sha256")
        .join(&hex[0..2])
        .join(hex);

    let child = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .arg("--test-failpoint")
        .arg("after_multi_target_first")
        .args([
            "evidence",
            "artifact",
            "put",
            input.to_str().unwrap(),
            "--kind",
            "review_notes",
            "--media-type",
            "text/plain",
            "--json",
        ])
        .env("PULSE_FAILPOINT_SLEEP_MS", "30000")
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn artifact failpoint command");
    kill_spawned_after_path(child, &repo, &artifact_dir.join("content"));

    assert!(artifact_dir.join("content").exists());
    assert!(!artifact_dir.join("metadata.json").exists());
    assert_eq!(
        event_count_by_type_and_subject(&repo, "evidence.artifact.recorded", &digest),
        0
    );

    run_ok(&repo, &["graph", "recover", "--json"]);
    run_ok(
        &repo,
        &["evidence", "artifact", "verify", &digest, "--json"],
    );
    assert!(artifact_dir.join("metadata.json").exists());
    assert_eq!(
        event_count_by_type_and_subject(&repo, "evidence.artifact.recorded", &digest),
        1
    );

    run_ok(&repo, &["graph", "recover", "--json"]);
    assert_eq!(
        event_count_by_type_and_subject(&repo, "evidence.artifact.recorded", &digest),
        1
    );
}

#[test]
fn killed_receipt_record_after_file_recovers_event_once() {
    let repo = tempfile::tempdir().unwrap();
    let created = run_ok(
        &repo,
        &[
            "work", "create", "--kind", "ticket", "--title", "Receipt", "--json",
        ],
    );
    let id = created["value"]["id"].as_str().unwrap();
    run_ok(&repo, &["evidence", "bootstrap", "--json"]);
    let content_rel = format!("works/{id}/ticket.md");
    let content_path = repo.path().join(&content_rel);
    fs::create_dir_all(content_path.parent().unwrap()).unwrap();
    fs::write(&content_path, b"receipt process recovery content").unwrap();
    git_init_commit_all(repo.path());
    let source_commit = git_head(repo.path());
    let manifest: Value = serde_json::from_slice(
        &fs::read(repo.path().join(".pulse/evidence/manifest.json")).unwrap(),
    )
    .unwrap();
    let repository_id = manifest["repository_id"].as_str().unwrap();
    let receipt_id = "rcpt_01J00000000000000000000300";
    let receipt = serde_json::json!({
        "schema_version": 1,
        "receipt_version": 1,
        "id": receipt_id,
        "kind": "shaping_validation",
        "result": "passed",
        "actor": {"kind": "human", "id": "tester"},
        "recorded_at": "2026-07-22T00:00:00Z",
        "subject": {"kind": "work", "id": id},
        "bindings": {
            "work": [{"id": id, "revision": 1}],
            "source": {"kind": "git_commit", "commit": source_commit, "repository_id": repository_id},
            "content": [{"path": content_rel, "sha256": hash_bytes(&fs::read(&content_path).unwrap())}],
            "artifacts": []
        },
        "payload": {
            "payload_version": 1,
            "owning_work": {"id": id, "revision": 1},
            "risk": "R1",
            "destination": null,
            "branch_summary": {"resolved": [], "rejected": [], "delegated": [], "deferred": [], "blocking": []},
            "remaining_uncertainty": [],
            "approval_assertion": {"required": false, "reference": null}
        }
    });
    let receipt_file = repo.path().join("receipt.json");
    write_json(&receipt_file, &receipt);

    let stored_receipt = repo
        .path()
        .join(".pulse/evidence/receipts")
        .join(format!("{receipt_id}.json"));
    let child = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .arg("--test-failpoint")
        .arg("after_canonical")
        .args([
            "evidence",
            "receipt",
            "record",
            "--file",
            receipt_file.to_str().unwrap(),
            "--json",
        ])
        .env("PULSE_FAILPOINT_SLEEP_MS", "30000")
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn receipt failpoint command");
    kill_spawned_after_path(child, &repo, &stored_receipt);

    assert!(stored_receipt.exists());
    assert_eq!(
        event_count_by_type_and_subject(&repo, "evidence.receipt.recorded", receipt_id),
        0
    );

    run_ok(&repo, &["graph", "recover", "--json"]);
    run_ok(
        &repo,
        &[
            "evidence",
            "receipt",
            "verify",
            receipt_id,
            "--current",
            "--json",
        ],
    );
    assert_eq!(
        event_count_by_type_and_subject(&repo, "evidence.receipt.recorded", receipt_id),
        1
    );
    run_ok(&repo, &["graph", "recover", "--json"]);
    assert_eq!(
        event_count_by_type_and_subject(&repo, "evidence.receipt.recorded", receipt_id),
        1
    );
}
