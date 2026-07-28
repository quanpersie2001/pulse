use pulse::canonical_json::hash_bytes;
use serde_json::Value;
use std::fs;
use std::path::Path;
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
                && value
                    .get("subject")
                    .and_then(|value| value.get("id"))
                    .and_then(Value::as_str)
                    == Some(subject)
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

fn kill_spawned_after_path(mut child: std::process::Child, repo: &TempDir, path: &Path) {
    wait_for_transaction_intent(repo);
    wait_for_path_exists(path);
    thread::sleep(Duration::from_millis(100));
    child.kill().expect("kill child");
    let _ = child.wait();
}

/// Wait until the canonical node for `id` reports `revision`.
///
/// The canonical node is written with an atomic temp+rename, so once the
/// revision is observable the `after_canonical` failpoint (which sleeps after
/// that rename) has been reached and the child is parked.
fn wait_for_node_revision(repo: &TempDir, id: &str, revision: u64) {
    let path = repo
        .path()
        .join(".pulse/workgraph/nodes")
        .join(format!("{id}.json"));
    let start = Instant::now();
    loop {
        if let Ok(bytes) = fs::read(&path) {
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                if value.get("revision").and_then(Value::as_u64) == Some(revision) {
                    return;
                }
            }
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "node {id} did not reach revision {revision} before failpoint"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

/// Wait until at least `expected` event files are present.
///
/// The event file is written with an atomic temp+rename, so a present event
/// file is always complete and correctly hashed (never torn). Reaching the
/// expected count therefore means the `after_event` failpoint has been reached
/// and the child is parked in its sleep, regardless of test parallelism.
fn wait_for_event_count(repo: &TempDir, expected: usize) {
    let start = Instant::now();
    while event_count(repo) < expected {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "event count did not reach {expected} before failpoint"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

/// Spawn a `work edit` under a failpoint and kill the child only once the
/// failpoint-specific durable state is observable.
///
/// Synchronization is chosen per failpoint so the kill always lands inside the
/// failpoint's 30s sleep window instead of relying on a fixed timing margin:
///
/// - `after_intent` sleeps once the intent is persisted (waited above);
/// - `after_canonical` sleeps once the canonical node is renamed into place;
/// - `after_event` sleeps once the (atomic) event file appears.
///
/// This keeps the tests deterministic under the default high parallelism of
/// `cargo test --all-targets`.
fn kill_at_failpoint(repo: &TempDir, failpoint: &str, id: &str, before_events: usize) {
    let mut child = Command::new(bin())
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
    wait_for_transaction_intent(repo);
    match failpoint {
        "after_canonical" => wait_for_node_revision(repo, id, 2),
        "after_event" => wait_for_event_count(repo, before_events + 1),
        _ => {}
    }
    child.kill().expect("kill child");
    let _ = child.wait();
}

#[test]
fn killed_after_intent_recovers_by_cleaning_uncommitted_transaction() {
    let repo = tempfile::tempdir().unwrap();
    let created = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Before",
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
    let before_events = event_count(&repo);

    kill_at_failpoint(&repo, "after_intent", &id, before_events);
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
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Before",
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
    let before_events = event_count(&repo);

    kill_at_failpoint(&repo, "after_canonical", &id, before_events);
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
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Before",
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
    let before_events = event_count(&repo);

    kill_at_failpoint(&repo, "after_event", &id, before_events);
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
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Receipt",
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
            "owning_work": {"id": id, "revision_observed": 1, "contract_revision": 1},
            "materialization": "R1",
            "shape_mode": "focused_branches",
            "source_posture": "clean_git_commit",
            "destination": null,
            "map": null,
            "affected_work": [],
            "branches": [],
            "fog": [],
            "out_of_scope": [],
            "resolution_pointers": [],
            "approval": {"approved_by": {"kind": "human", "id": "tester"}, "reference": "PULSE.md#human-judgment-boundaries"},
            "reconciliation": null,
            "remaining_uncertainty": []
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

/// Helper: artifact dir for known digest.
fn artifact_digest_dir(repo: &TempDir, digest: &str) -> std::path::PathBuf {
    let hex = digest.strip_prefix("sha256:").unwrap();
    repo.path()
        .join(".pulse/evidence/artifacts/sha256")
        .join(&hex[0..2])
        .join(hex)
}

#[test]
fn killed_artifact_put_after_intent_recovers_content_metadata_and_event_once() {
    let repo = tempfile::tempdir().unwrap();
    run_ok(&repo, &["evidence", "bootstrap", "--json"]);
    let input = repo.path().join("artifact-notes.txt");
    fs::write(&input, b"artifact intent failpoint recovery").unwrap();
    let digest = hash_bytes(&fs::read(&input).unwrap());
    let artifact_dir = artifact_digest_dir(&repo, &digest);

    let mut child = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .arg("--test-failpoint")
        .arg("after_intent")
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
        .expect("spawn artifact with after_intent failpoint");
    // Wait for intent to be persisted, then kill before any targets.
    wait_for_transaction_intent(&repo);
    // Brief sleep to ensure we're inside the failpoint sleep, not before it.
    thread::sleep(Duration::from_millis(200));
    child.kill().expect("kill child");
    let _ = child.wait();

    // After intent only: no content, no metadata, no event.
    assert!(!artifact_dir.join("content").exists());
    assert!(!artifact_dir.join("metadata.json").exists());
    assert_eq!(
        event_count_by_type_and_subject(&repo, "evidence.artifact.recorded", &digest),
        0
    );

    // Recovery should roll back (remove intent, no side effects).
    run_ok(&repo, &["graph", "recover", "--json"]);
    assert!(!artifact_dir.join("content").exists());
    assert!(!artifact_dir.join("metadata.json").exists());

    // Re-run succeeds as if nothing happened.
    let output = run(
        &repo,
        &[
            "evidence",
            "artifact",
            "put",
            input.to_str().unwrap(),
            "--kind",
            "review_notes",
            "--media-type",
            "text/plain",
            "--json",
        ],
    );
    assert!(
        output.status.success(),
        "re-run after intent failpoint: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(artifact_dir.join("content").exists());
    assert!(artifact_dir.join("metadata.json").exists());
    assert_eq!(
        event_count_by_type_and_subject(&repo, "evidence.artifact.recorded", &digest),
        1
    );
}

#[test]
fn killed_artifact_put_after_all_targets_recovers_event_once() {
    let repo = tempfile::tempdir().unwrap();
    run_ok(&repo, &["evidence", "bootstrap", "--json"]);
    let input = repo.path().join("artifact-notes.txt");
    fs::write(&input, b"artifact all targets failpoint recovery").unwrap();
    let digest = hash_bytes(&fs::read(&input).unwrap());
    let artifact_dir = artifact_digest_dir(&repo, &digest);
    let before_events = event_count(&repo);

    let mut child = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .arg("--test-failpoint")
        .arg("after_multi_target_all")
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
        .expect("spawn artifact with after_multi_target_all failpoint");

    // Wait until both targets (content + metadata) are written, then kill.
    wait_for_transaction_intent(&repo);
    wait_for_path_exists(&artifact_dir.join("content"));
    wait_for_path_exists(&artifact_dir.join("metadata.json"));
    // Ensure filesystem is settled before killing.
    thread::sleep(Duration::from_millis(500));
    child.kill().expect("kill child");
    let _ = child.wait();

    // Both targets exist, event not yet written.
    assert!(artifact_dir.join("content").exists());
    assert!(artifact_dir.join("metadata.json").exists());
    assert_eq!(event_count(&repo), before_events);

    // Recovery completes the event, cleans up intent.
    run_ok(&repo, &["graph", "recover", "--json"]);
    assert!(artifact_dir.join("content").exists());
    assert!(artifact_dir.join("metadata.json").exists());
    assert_eq!(event_count(&repo), before_events + 1);

    // Idempotent second recovery is a no-op (CleanComplete).
    run_ok(&repo, &["graph", "recover", "--json"]);
    assert_eq!(event_count(&repo), before_events + 1);
}

#[test]
fn killed_artifact_put_after_event_intent_cleaned_without_duplicate_event() {
    let repo = tempfile::tempdir().unwrap();
    run_ok(&repo, &["evidence", "bootstrap", "--json"]);
    let input = repo.path().join("artifact-notes.txt");
    fs::write(&input, b"artifact after-event failpoint recovery").unwrap();
    let digest = hash_bytes(&fs::read(&input).unwrap());
    let artifact_dir = artifact_digest_dir(&repo, &digest);
    let before_events = event_count(&repo);

    let mut child = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .arg("--test-failpoint")
        .arg("after_event")
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
        .expect("spawn artifact with after_event failpoint");

    // Wait until the event is written (after_event fires after write).
    wait_for_transaction_intent(&repo);
    wait_for_event_count(&repo, before_events + 1);
    child.kill().expect("kill child");
    let _ = child.wait();

    // Targets and event exist.
    assert!(artifact_dir.join("content").exists());
    assert!(artifact_dir.join("metadata.json").exists());
    assert_eq!(event_count(&repo), before_events + 1);

    // Recovery cleans up intent, no duplicate event.
    run_ok(&repo, &["graph", "recover", "--json"]);
    run_ok(&repo, &["graph", "recover", "--json"]);
    assert_eq!(event_count(&repo), before_events + 1);
}

// =========================================================================
// Claim failpoint crash tests (P2S2-I10)
// =========================================================================

use chrono::Utc;
use pulse::canonical_json::to_canonical_bytes;
use pulse::graph::contract::{
    ContentRef, ContractItem, ContractScope, EffortMetadata, ImplementationContract,
    ImplementationMode, ImplementationSemanticImpact, PlanPolicy, PublicCreateClassification,
    QaImpactPosture, SurfaceRef,
};
use pulse::graph::node::{DocumentationImpactPosture, NodeStatus};
use pulse::graph::store::{
    ContractSetRequest, DocumentationImpactUpdate, OperationContext, QaImpactUpdate,
};
use pulse::id::WorkKind;
use pulse::JsonGraphStore;

fn claim_ctx() -> OperationContext {
    OperationContext {
        actor: "human:tester".to_string(),
        now: Utc::now(),
    }
}

fn claim_valid_inventory_bytes(principal: &str) -> Vec<u8> {
    serde_json::json!({
        "schema_version": 1,
        "principal": principal,
        "inventory_id": "test-inventory",
        "capabilities": [
            "repository.inspect",
            "source.read",
            "source.write",
            "test.run",
            "workspace.worktree"
        ]
    })
    .to_string()
    .into_bytes()
}

fn claim_write_capability_file(root: &Path, principal: &str) -> std::path::PathBuf {
    let dir = root.join(".pulse/runtime/claim-test");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("capabilities.json");
    fs::write(&path, claim_valid_inventory_bytes(principal)).unwrap();
    path
}

fn build_claim_shaping_receipt(
    id: &str,
    revision: u64,
    contract_revision: u64,
    content_dir: &str,
    content_hash: &str,
) -> pulse::evidence::model::ReceiptEnvelope {
    use pulse::evidence::model::*;
    ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 1,
        id: format!("rcpt_{:0<26}", &id[3..]),
        kind: ReceiptKind::ShapingValidation,
        result: ReceiptResult::Passed,
        actor: ActorRef {
            kind: pulse::identity::actor::ActorKind::Human,
            id: "tester".to_string(),
        },
        recorded_at: chrono::Utc::now(),
        subject: SubjectRef {
            kind: "work".to_string(),
            id: id.to_string(),
        },
        bindings: ReceiptBindings {
            work: vec![WorkBinding {
                id: id.to_string(),
                revision,
            }],
            source: None,
            content: vec![pulse::evidence::model::ContentBinding {
                path: format!("{content_dir}/ticket.md"),
                sha256: content_hash.to_string(),
            }],
            artifacts: vec![],
            graph_fingerprint_observed: None,
        },
        payload: ReceiptPayload::ShapingValidation(ShapingValidationPayload {
            payload_version: 1,
            owning_work: ShapingWorkBinding {
                id: id.to_string(),
                revision_observed: revision,
                contract_revision,
            },
            materialization: "R1".to_string(),
            shape_mode: ShapeMode::FocusedBranches,
            source_posture: SourcePosture::NotRequiredContentBound,
            destination: Some(ShapingDestination {
                summary: "Test shaping".to_string(),
                scope_boundary: vec!["test".to_string()],
                exit_conditions: vec!["condition met".to_string()],
            }),
            map: None,
            affected_work: vec![],
            branches: vec![],
            fog: vec![],
            out_of_scope: vec![],
            resolution_pointers: vec![],
            approval: ShapingApproval {
                approved_by: ActorRef {
                    kind: pulse::identity::actor::ActorKind::Human,
                    id: "tester".to_string(),
                },
                reference: "test".to_string(),
            },
            reconciliation: None,
            remaining_uncertainty: vec![],
        }),
    }
}

fn setup_ready_ticket(repo: &Path, store: &JsonGraphStore) -> String {
    let mut policy = pulse::policy::AuthorityPolicy {
        schema_version: 1,
        revision: 1,
        principals: vec![
            pulse::policy::AuthorityPrincipal {
                kind: pulse::identity::actor::ActorKind::Agent,
                id: "tester".to_string(),
                grants: vec![
                    "shape.apply".to_string(),
                    "shape.approve.R1".to_string(),
                    "qa.none.approve".to_string(),
                    "work.transition.shaped".to_string(),
                    "work.transition.ready".to_string(),
                    "work.assignment.prepare".to_string(),
                    "work.node.create".to_string(),
                ],
            },
            pulse::policy::AuthorityPrincipal {
                kind: pulse::identity::actor::ActorKind::Human,
                id: "tester".to_string(),
                grants: vec![
                    "shape.apply".to_string(),
                    "shape.approve.R1".to_string(),
                    "qa.none.approve".to_string(),
                    "work.transition.shaped".to_string(),
                    "work.transition.ready".to_string(),
                    "work.assignment.prepare".to_string(),
                    "work.node.create".to_string(),
                ],
            },
        ],
    };
    policy.normalize();
    let policy_path = repo.join(".pulse/policy/authority.json");
    fs::create_dir_all(policy_path.parent().unwrap()).unwrap();
    fs::write(&policy_path, to_canonical_bytes(&policy).unwrap()).unwrap();

    store.bootstrap().unwrap();
    pulse::evidence::manifest::load(repo).unwrap();
    pulse::docs::manifest::bootstrap(repo).unwrap();

    let node = store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Crash claim ticket".to_string(),
            PublicCreateClassification {
                role: Some(pulse::graph::contract::TicketRole::Implementation),
                risk: Some(pulse::graph::contract::Risk::Low),
                materialization: Some(pulse::graph::contract::Materialization::R1),
            },
            claim_ctx(),
        )
        .unwrap()
        .value;
    let ticket_id = node.id.clone();

    let brief_rel = format!("{}/ticket.md", node.content_dir);
    let brief_path = repo.join(&brief_rel);
    fs::create_dir_all(brief_path.parent().unwrap()).unwrap();
    fs::write(&brief_path, b"# Crash claim test").unwrap();
    let brief_hash = pulse::canonical_json::hash_bytes(&fs::read(&brief_path).unwrap());

    store
        .set_contract_with_context(
            &ticket_id,
            node.revision,
            ContractSetRequest {
                role: pulse::graph::contract::TicketRole::Implementation,
                implementation: Some(ImplementationContract {
                    mode: ImplementationMode::Guided,
                    work_surface: pulse::graph::contract::WorkSurface::Code,
                    plan_policy: PlanPolicy::None,
                    semantic_impact: ImplementationSemanticImpact::NoBehaviorOrPublicRiskChange,
                    effort: EffortMetadata::default(),
                    verification_profile: "service-change".to_string(),
                    brief: Some(ContentRef {
                        path: brief_rel,
                        content_hash: brief_hash.clone(),
                    }),
                    objective: "Test claim objective.".to_string(),
                    current_behavior: "Current behavior.".to_string(),
                    target_behavior: "Target behavior.".to_string(),
                    code_anchors: vec![SurfaceRef::path("src/main.rs")],
                    documentation_anchors: vec![],
                    configuration_anchors: vec![],
                    data_anchors: vec![],
                    research_refs: vec![],
                    required_changes: vec![ContractItem {
                        id: "CHG-1".to_string(),
                        summary: "Make test claimable.".to_string(),
                    }],
                    invariants: vec![ContractItem {
                        id: "INV-1".to_string(),
                        summary: "Invariant holds.".to_string(),
                    }],
                    acceptance: vec![ContractItem {
                        id: "AC-1".to_string(),
                        summary: "Claim works.".to_string(),
                    }],
                    scope: ContractScope::default(),
                    implementation_freedom: vec![],
                    required_decisions: vec![],
                    shared_approach_refs: vec![],
                    expected_evidence: vec![],
                    expected_handoff: vec![],
                }),
                decision_work: None,
            },
            claim_ctx(),
        )
        .unwrap();

    let node = store.show_node(&ticket_id).unwrap();
    store
        .set_qa_impact_with_context(
            &ticket_id,
            node.revision,
            QaImpactUpdate {
                posture: QaImpactPosture::None,
                rationale: Some("No QA.".to_string()),
                behavioral_owner: None,
                affected_case_ids: vec![],
            },
            claim_ctx(),
        )
        .unwrap();

    let node = store.show_node(&ticket_id).unwrap();
    store
        .update_documentation_impact(
            &ticket_id,
            node.revision,
            DocumentationImpactUpdate {
                posture: DocumentationImpactPosture::None,
                rationale: Some("No docs impact.".to_string()),
                required_documents: vec![],
                deferred_to: vec![],
                paths: vec![],
                domains: vec!["development".to_string()],
                labels: vec!["claim".to_string()],
            },
            "human:tester".to_string(),
        )
        .unwrap();

    let node = store.show_node(&ticket_id).unwrap();
    let receipt = build_claim_shaping_receipt(
        &ticket_id,
        node.revision,
        node.contract_revision,
        &node.content_dir,
        &brief_hash,
    );
    let receipt_file = repo.join("shaping_claim.json");
    fs::write(&receipt_file, to_canonical_bytes(&receipt).unwrap()).unwrap();
    pulse::evidence::receipt::record_receipt(repo, None, &receipt_file).unwrap();

    let node = store.show_node(&ticket_id).unwrap();
    store
        .apply_shaping_with_context(&ticket_id, node.revision, &receipt.id, None, claim_ctx())
        .unwrap();

    let node = store.show_node(&ticket_id).unwrap();
    store
        .transition_node_with_context(
            &ticket_id,
            NodeStatus::Shaped,
            node.revision,
            None,
            claim_ctx(),
        )
        .unwrap();

    let node = store.show_node(&ticket_id).unwrap();
    store
        .transition_node_with_context(
            &ticket_id,
            NodeStatus::Ready,
            node.revision,
            None,
            claim_ctx(),
        )
        .unwrap();

    // Write .gitignore and commit for clean baseline.
    let gitignore_path = repo.join(".gitignore");
    if !gitignore_path.exists() {
        fs::write(&gitignore_path, b".pulse/runtime/\n.pulse/cache/\n").unwrap();
    }
    let _ = std::process::Command::new("git")
        .args(["init"])
        .current_dir(repo)
        .output();
    let _ = std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo)
        .output();
    let _ = std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo)
        .output();
    let _ = std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(repo)
        .output();
    let _ = std::process::Command::new("git")
        .args(["commit", "--allow-empty", "-m", "snapshot"])
        .current_dir(repo)
        .output();

    ticket_id
}

/// Helper: read the transaction intent JSON to get target paths and IDs.
fn read_claim_intent_paths(
    repo: &TempDir,
) -> Option<(Vec<std::path::PathBuf>, std::path::PathBuf)> {
    let tx_dir = repo.path().join(".pulse/runtime/transactions");
    if !tx_dir.exists() {
        return None;
    }
    for entry in fs::read_dir(&tx_dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        if let Ok(bytes) = fs::read(&path) {
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                if value.get("operation").and_then(Value::as_str)
                    == Some("work.assignment.prepared")
                {
                    let targets: Vec<std::path::PathBuf> = value["targets"]
                        .as_array()
                        .unwrap_or(&vec![])
                        .iter()
                        .map(|t| repo.path().join(t["path"].as_str().unwrap_or("")))
                        .collect();
                    let event_path = value["event_path"]
                        .as_str()
                        .map(|p| repo.path().join(p))
                        .unwrap_or_default();
                    return Some((targets, event_path));
                }
            }
        }
    }
    None
}

fn wait_for_target_files(repo: &TempDir, expected_count: usize) {
    let start = Instant::now();
    loop {
        if let Some((targets, _)) = read_claim_intent_paths(repo) {
            let existing = targets.iter().filter(|p| p.exists()).count();
            if existing >= expected_count {
                return;
            }
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "timed out waiting for {expected_count} claim target files"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn killed_claim_after_intent_records_rollback_and_no_side_effects() {
    let repo = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(repo.path());
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let cap_file = claim_write_capability_file(repo.path(), "agent:codex-local");
    let before_events = event_count(&repo);

    let mut child = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .arg("--test-failpoint")
        .arg("after_intent")
        .args([
            "work",
            "claim",
            &ticket_id,
            "--actor",
            "agent:tester",
            "--assignee",
            "agent:codex-local",
            "--capabilities",
            cap_file.to_str().unwrap(),
            "--json",
        ])
        .env("PULSE_FAILPOINT_SLEEP_MS", "30000")
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn claim after_intent failpoint");

    // Wait for intent to be persisted, then kill before any targets.
    wait_for_transaction_intent(&repo);
    thread::sleep(Duration::from_millis(200));
    child.kill().expect("kill child");
    let _ = child.wait();

    // No targets should exist.
    assert!(
        !repo.path().join(".pulse/runtime/assignment").exists(),
        "no assignment runtime files expected after intent-only crash"
    );
    // Node should still be Ready.
    let shown = run_ok(&repo, &["work", "show", &ticket_id, "--json"]);
    assert_eq!(shown["node"]["status"], "ready");
    assert_eq!(before_events, event_count(&repo));

    // Recovery should clean up the intent.
    run_ok(&repo, &["graph", "recover", "--json"]);

    // Re-running the claim should succeed.
    let cap_file2 = claim_write_capability_file(repo.path(), "agent:codex-local");
    let redo = run(
        &repo,
        &[
            "work",
            "claim",
            &ticket_id,
            "--actor",
            "agent:tester",
            "--assignee",
            "agent:codex-local",
            "--capabilities",
            cap_file2.to_str().unwrap(),
            "--json",
        ],
    );
    assert!(
        redo.status.success(),
        "claim should succeed after intent-only crash recovery: {}",
        String::from_utf8_lossy(&redo.stderr)
    );
}

#[test]
fn killed_claim_after_multi_target_first_recovery_completes_event() {
    let repo = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(repo.path());
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let cap_file = claim_write_capability_file(repo.path(), "agent:codex-local");
    let before_events = event_count(&repo);

    let mut child = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .arg("--test-failpoint")
        .arg("after_multi_target_first")
        .args([
            "work",
            "claim",
            &ticket_id,
            "--actor",
            "agent:tester",
            "--assignee",
            "agent:codex-local",
            "--capabilities",
            cap_file.to_str().unwrap(),
            "--json",
        ])
        .env("PULSE_FAILPOINT_SLEEP_MS", "30000")
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn claim after_multi_target_first failpoint");

    // Wait for transaction intent and for at least the first target (lease).
    wait_for_transaction_intent(&repo);
    wait_for_target_files(&repo, 1);
    thread::sleep(Duration::from_millis(200));
    child.kill().expect("kill child");
    let _ = child.wait();

    // Recovery should complete remaining targets + event.
    run_ok(&repo, &["graph", "recover", "--json"]);

    // Verify node transitioned to Active.
    let shown = run_ok(&repo, &["work", "show", &ticket_id, "--json"]);
    assert_eq!(shown["node"]["status"], "active");
    assert_eq!(before_events + 1, event_count(&repo));

    // Verify runtime records exist.
    let leases_dir = repo.path().join(".pulse/runtime/assignment/leases");
    assert!(leases_dir.exists() && fs::read_dir(&leases_dir).unwrap().count() >= 1);
}

#[test]
fn killed_claim_after_multi_target_all_recovery_writes_event() {
    let repo = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(repo.path());
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let cap_file = claim_write_capability_file(repo.path(), "agent:codex-local");
    let before_events = event_count(&repo);

    let mut child = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .arg("--test-failpoint")
        .arg("after_multi_target_all")
        .args([
            "work",
            "claim",
            &ticket_id,
            "--actor",
            "agent:tester",
            "--assignee",
            "agent:codex-local",
            "--capabilities",
            cap_file.to_str().unwrap(),
            "--json",
        ])
        .env("PULSE_FAILPOINT_SLEEP_MS", "30000")
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn claim after_multi_target_all failpoint");

    // Wait for all 4 targets to be written.
    wait_for_transaction_intent(&repo);
    wait_for_target_files(&repo, 4);
    thread::sleep(Duration::from_millis(200));
    child.kill().expect("kill child");
    let _ = child.wait();

    // No event yet.
    assert_eq!(before_events, event_count(&repo));

    // Recovery should write the event and clean up.
    run_ok(&repo, &["graph", "recover", "--json"]);
    assert_eq!(before_events + 1, event_count(&repo));

    // Node should be Active.
    let shown = run_ok(&repo, &["work", "show", &ticket_id, "--json"]);
    assert_eq!(shown["node"]["status"], "active");

    // Second recovery is idempotent.
    run_ok(&repo, &["graph", "recover", "--json"]);
    assert_eq!(before_events + 1, event_count(&repo));
}

#[test]
fn killed_claim_after_event_cleans_intent_no_duplicate_event() {
    let repo = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(repo.path());
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let cap_file = claim_write_capability_file(repo.path(), "agent:codex-local");
    let before_events = event_count(&repo);

    let mut child = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .arg("--test-failpoint")
        .arg("after_event")
        .args([
            "work",
            "claim",
            &ticket_id,
            "--actor",
            "agent:tester",
            "--assignee",
            "agent:codex-local",
            "--capabilities",
            cap_file.to_str().unwrap(),
            "--json",
        ])
        .env("PULSE_FAILPOINT_SLEEP_MS", "30000")
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn claim after_event failpoint");

    // Wait until the prepared event is written.
    wait_for_transaction_intent(&repo);
    wait_for_event_count(&repo, before_events + 1);
    child.kill().expect("kill child");
    let _ = child.wait();

    // Event exists, node should be Active (event written before cleanup).
    assert_eq!(before_events + 1, event_count(&repo));
    let shown = run_ok(&repo, &["work", "show", &ticket_id, "--json"]);
    assert_eq!(shown["node"]["status"], "active");

    // Recovery cleans up intent, no duplicate event.
    run_ok(&repo, &["graph", "recover", "--json"]);
    run_ok(&repo, &["graph", "recover", "--json"]);
    assert_eq!(before_events + 1, event_count(&repo));
}

#[test]
fn killed_claim_after_intent_re_run_succeeds_without_duplicate_records() {
    // Verify that after an after_intent crash + recovery, a re-run of the
    // claim produces exactly one lease, one prepared, one event.
    let repo = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(repo.path());
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let cap_file = claim_write_capability_file(repo.path(), "agent:codex-local");

    // First attempt: crash at after_intent.
    let mut child = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .arg("--test-failpoint")
        .arg("after_intent")
        .args([
            "work",
            "claim",
            &ticket_id,
            "--actor",
            "agent:tester",
            "--assignee",
            "agent:codex-local",
            "--capabilities",
            cap_file.to_str().unwrap(),
            "--json",
        ])
        .env("PULSE_FAILPOINT_SLEEP_MS", "30000")
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn claim after_intent");
    wait_for_transaction_intent(&repo);
    thread::sleep(Duration::from_millis(200));
    child.kill().expect("kill child");
    let _ = child.wait();

    // Recover.
    run_ok(&repo, &["graph", "recover", "--json"]);

    // Re-run claim (should succeed).
    let cap_file2 = claim_write_capability_file(repo.path(), "agent:codex-local");
    let out = run_ok(
        &repo,
        &[
            "work",
            "claim",
            &ticket_id,
            "--actor",
            "agent:tester",
            "--assignee",
            "agent:codex-local",
            "--capabilities",
            cap_file2.to_str().unwrap(),
            "--json",
        ],
    );
    assert_eq!(out["subject"]["status_after"], "active");

    // Verify exactly one lease, one prepared, one event.
    let leases_dir = repo.path().join(".pulse/runtime/assignment/leases");
    assert_eq!(fs::read_dir(&leases_dir).unwrap().count(), 1);
    let prepared_dir = repo.path().join(".pulse/runtime/assignment/prepared");
    assert_eq!(fs::read_dir(&prepared_dir).unwrap().count(), 1);
    let mut event_count_val = 0usize;
    let events_dir = repo.path().join(".pulse/events");
    if events_dir.exists() {
        for date in fs::read_dir(&events_dir).unwrap() {
            for entry in fs::read_dir(date.unwrap().path()).unwrap() {
                let content = fs::read_to_string(entry.unwrap().path()).unwrap();
                if content.contains("work.assignment.prepared") {
                    event_count_val += 1;
                }
            }
        }
    }
    assert_eq!(event_count_val, 1);
}
