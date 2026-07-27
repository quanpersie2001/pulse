use chrono::Utc;
use pulse::canonical_json::{hash_bytes, to_canonical_bytes};
use pulse::evidence::model::*;
use serde_json::Value;
use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
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

fn event_count(repo: &TempDir, event_type: &str) -> usize {
    let events = repo.path().join(".pulse/events");
    if !events.exists() {
        return 0;
    }
    let mut count = 0;
    for date in fs::read_dir(events).unwrap() {
        for entry in fs::read_dir(date.unwrap().path()).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let value: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
            if value["event_type"] == event_type {
                count += 1;
            }
        }
    }
    count
}

fn transaction_count(repo: &TempDir) -> usize {
    let transactions = repo.path().join(".pulse/runtime/transactions");
    if !transactions.exists() {
        return 0;
    }
    fs::read_dir(transactions)
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                == Some("json")
        })
        .count()
}

fn supersession_edge_path(
    repo: &TempDir,
    old_id: &str,
    replacement_id: &str,
) -> std::path::PathBuf {
    repo.path().join(format!(
        ".pulse/workgraph/edges/superseded-by--{old_id}--{replacement_id}.json"
    ))
}

fn raw_node_status(repo: &TempDir, id: &str) -> Option<String> {
    let path = repo
        .path()
        .join(".pulse/workgraph/nodes")
        .join(format!("{id}.json"));
    let value: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    value["status"].as_str().map(str::to_string)
}

fn wait_for_any_supersession_target(repo: &TempDir, old_id: &str, replacement_id: &str) {
    let start = Instant::now();
    while raw_node_status(repo, old_id).as_deref() != Some("superseded")
        && !supersession_edge_path(repo, old_id, replacement_id).exists()
    {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "no supersession target reached after failpoint"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn wait_for_all_supersession_targets(repo: &TempDir, old_id: &str, replacement_id: &str) {
    let start = Instant::now();
    while raw_node_status(repo, old_id).as_deref() != Some("superseded")
        || !supersession_edge_path(repo, old_id, replacement_id).exists()
    {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "all supersession targets were not written after failpoint"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn git(repo: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn commit_all(repo: &std::path::Path) -> String {
    if !repo.join(".git").exists() {
        git(repo, &["init"]);
        git(repo, &["config", "user.email", "test@example.com"]);
        git(repo, &["config", "user.name", "Test User"]);
    }
    git(repo, &["add", "."]);
    git(repo, &["commit", "--allow-empty", "-m", "snapshot"]);
    git(repo, &["rev-parse", "HEAD"])
}

fn setup(repo: &TempDir) -> (String, String, String) {
    let old = run_ok(
        repo,
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
    let replacement = run_ok(
        repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "New",
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
    let replacement_id = replacement["value"]["id"].as_str().unwrap().to_string();
    let receipt_id = record_reconciliation_receipt(repo, &old_id, &replacement_id);
    (old_id, replacement_id, receipt_id)
}

fn record_reconciliation_receipt(repo: &TempDir, old_id: &str, replacement_id: &str) -> String {
    let manifest = pulse::evidence::bootstrap(repo.path()).unwrap().manifest;
    for id in [old_id, replacement_id] {
        let path = repo.path().join(format!("works/{id}/ticket.md"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("content {id}")).unwrap();
    }
    let old_rel = format!("works/{old_id}/ticket.md");
    let replacement_rel = format!("works/{replacement_id}/ticket.md");
    let source_commit = commit_all(repo.path());
    let receipt_id = "rcpt_01J00000000000000000000999".to_string();
    let receipt = ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 1,
        id: receipt_id.clone(),
        kind: ReceiptKind::SupersessionReconciliation,
        result: ReceiptResult::Passed,
        actor: ActorRef {
            kind: ActorKind::Human,
            id: "tester".to_string(),
        },
        recorded_at: Utc::now(),
        subject: SubjectRef {
            kind: "work".to_string(),
            id: old_id.to_string(),
        },
        bindings: ReceiptBindings {
            work: vec![
                WorkBinding {
                    id: old_id.to_string(),
                    revision: 1,
                },
                WorkBinding {
                    id: replacement_id.to_string(),
                    revision: 1,
                },
            ],
            source: Some(SourceBinding {
                kind: "git_commit".to_string(),
                commit: source_commit,
                repository_id: manifest.repository_id,
            }),
            content: vec![
                ContentBinding {
                    path: old_rel.clone(),
                    sha256: hash_bytes(&fs::read(repo.path().join(&old_rel)).unwrap()),
                },
                ContentBinding {
                    path: replacement_rel.clone(),
                    sha256: hash_bytes(&fs::read(repo.path().join(&replacement_rel)).unwrap()),
                },
            ],
            artifacts: vec![],
            graph_fingerprint_observed: None,
        },
        payload: ReceiptPayload::SupersessionReconciliation(SupersessionReconciliationPayload {
            payload_version: 1,
            old: WorkRevisionRef {
                id: old_id.to_string(),
                revision: 1,
            },
            target: SupersessionReceiptTarget::Replacement {
                id: replacement_id.to_string(),
                revision: 1,
            },
            claim: SupersessionReceiptClaim::Absorbed,
            follow_up_work: vec![],
            review_summary: "absorbed".to_string(),
            reviewed_references: vec![old_id.to_string(), replacement_id.to_string()],
        }),
    };
    let file = repo.path().join("supersession-receipt.json");
    fs::write(&file, to_canonical_bytes(&receipt).unwrap()).unwrap();
    pulse::evidence::record_receipt(repo.path(), None, &file).unwrap();
    receipt_id
}

fn wait_for_intent(repo: &TempDir) {
    let transactions = repo.path().join(".pulse/runtime/transactions");
    let start = Instant::now();
    while transaction_count(repo) == 0 {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "transaction intent was not persisted in {}",
            transactions.display()
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn run_supersede_with_failpoint(
    repo: &TempDir,
    failpoint: &str,
    old_id: &str,
    replacement_id: &str,
    receipt_id: &str,
) -> std::process::Output {
    Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .arg("--test-failpoint")
        .arg(failpoint)
        .args([
            "work",
            "supersede",
            old_id,
            "--by",
            replacement_id,
            "--expected-revision",
            "1",
            "--reason",
            "absorbed by replacement",
            "--reconciliation-receipt",
            receipt_id,
        ])
        .args(["--actor", "human:test", "--json"])
        .output()
        .expect("run failpoint supersede")
}

fn spawn_sleeping_supersede_at_failpoint(
    repo: &TempDir,
    failpoint: &str,
    old_id: &str,
    replacement_id: &str,
    receipt_id: &str,
) -> std::process::Child {
    Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .arg("--test-failpoint")
        .arg(failpoint)
        .args([
            "work",
            "supersede",
            old_id,
            "--by",
            replacement_id,
            "--expected-revision",
            "1",
            "--reason",
            "absorbed by replacement",
            "--reconciliation-receipt",
            receipt_id,
        ])
        .args(["--actor", "human:test", "--json"])
        .env("PULSE_FAILPOINT_SLEEP_MS", "30000")
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn failpoint supersede")
}

fn kill_supersede_at_failpoint(
    repo: &TempDir,
    failpoint: &str,
    old_id: &str,
    replacement_id: &str,
    receipt_id: &str,
) {
    let mut child =
        spawn_sleeping_supersede_at_failpoint(repo, failpoint, old_id, replacement_id, receipt_id);

    wait_for_intent(repo);
    match failpoint {
        "after_multi_target_all" => wait_for_all_supersession_targets(repo, old_id, replacement_id),
        "after_multi_target_first" => {
            wait_for_any_supersession_target(repo, old_id, replacement_id)
        }
        _ => thread::sleep(Duration::from_millis(100)),
    }
    child.kill().expect("kill child");
    let _ = child.wait();
}

#[test]
fn failpoint_after_first_supersession_target_rolls_forward_remaining_target_and_event() {
    let repo = tempfile::tempdir().unwrap();
    let (old_id, replacement_id, receipt_id) = setup(&repo);
    let before_events = event_count(&repo, "work.node.superseded");

    let output = run_supersede_with_failpoint(
        &repo,
        "after_multi_target_first",
        &old_id,
        &replacement_id,
        &receipt_id,
    );
    assert!(!output.status.success());
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["code"], "failpoint");

    assert_eq!(transaction_count(&repo), 1);
    assert!(supersession_edge_path(&repo, &old_id, &replacement_id).exists());
    assert_ne!(
        raw_node_status(&repo, &old_id).as_deref(),
        Some("superseded")
    );

    run_ok(&repo, &["graph", "recover", "--json"]);
    assert_eq!(transaction_count(&repo), 0);
    assert_eq!(
        event_count(&repo, "work.node.superseded"),
        before_events + 1
    );
    let export = run_ok(&repo, &["graph", "export", "--json"]);
    assert!(export["edges"].as_array().unwrap().iter().any(|edge| {
        edge["type"] == "superseded_by" && edge["from"] == old_id && edge["to"] == replacement_id
    }));
    let retry = run_ok(
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
            "absorbed by replacement",
            "--reconciliation-receipt",
            &receipt_id,
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(retry["code"], "unchanged");
    assert_eq!(
        event_count(&repo, "work.node.superseded"),
        before_events + 1
    );
}

#[test]
fn killed_after_all_supersession_targets_recovers_event_without_duplicate() {
    let repo = tempfile::tempdir().unwrap();
    let (old_id, replacement_id, receipt_id) = setup(&repo);
    let before_events = event_count(&repo, "work.node.superseded");

    kill_supersede_at_failpoint(
        &repo,
        "after_multi_target_all",
        &old_id,
        &replacement_id,
        &receipt_id,
    );
    assert_eq!(event_count(&repo, "work.node.superseded"), before_events);
    assert!(supersession_edge_path(&repo, &old_id, &replacement_id).exists());

    run_ok(&repo, &["graph", "recover", "--json"]);
    run_ok(&repo, &["graph", "recover", "--json"]);
    assert_eq!(transaction_count(&repo), 0);
    assert_eq!(
        event_count(&repo, "work.node.superseded"),
        before_events + 1
    );
}

#[test]
fn reader_waits_for_supersession_recovery_and_never_returns_half_valid_projection() {
    let repo = tempfile::tempdir().unwrap();
    let (old_id, replacement_id, receipt_id) = setup(&repo);
    let before_events = event_count(&repo, "work.node.superseded");
    let mut writer = spawn_sleeping_supersede_at_failpoint(
        &repo,
        "after_multi_target_first",
        &old_id,
        &replacement_id,
        &receipt_id,
    );
    wait_for_intent(&repo);
    wait_for_any_supersession_target(&repo, &old_id, &replacement_id);

    let mut reader = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args(["graph", "export", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn reader");
    thread::sleep(Duration::from_millis(100));
    assert!(
        reader.try_wait().unwrap().is_none(),
        "reader should wait on the repository guard while supersession is half-applied"
    );

    writer.kill().expect("kill writer");
    let _ = writer.wait();
    let output = reader.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "reader failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let projection: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "reader stdout was not JSON: {error}; stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert!(projection["edges"].as_array().unwrap().iter().any(|edge| {
        edge["type"] == "superseded_by" && edge["from"] == old_id && edge["to"] == replacement_id
    }));
    assert!(projection["nodes"].as_array().unwrap().iter().any(|node| {
        node["id"] == old_id && node["status"] == "superseded" && node["revision"] == 2
    }));
    assert_eq!(transaction_count(&repo), 0);
    assert_eq!(
        event_count(&repo, "work.node.superseded"),
        before_events + 1
    );
}
