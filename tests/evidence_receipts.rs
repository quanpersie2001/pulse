use chrono::Utc;
use pulse::canonical_json::hash_bytes;
use pulse::evidence::model::*;
use pulse::graph::store::SupersessionTarget;
use pulse::id::WorkKind;
use pulse::storage::transaction::{recover_prepared_transactions, TransactionFailpoint};
use pulse::JsonGraphStore;
use std::fs;
use std::process::Command;

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

fn write_json(path: &std::path::Path, value: &impl serde::Serialize) {
    let bytes = pulse::canonical_json::to_canonical_bytes(value).unwrap();
    fs::write(path, bytes).unwrap();
}

fn event_count(repo: &std::path::Path, event_type: &str, subject: &str) -> usize {
    let events = repo.join(".pulse/events");
    if !events.exists() {
        return 0;
    }
    let mut count = 0;
    for day in fs::read_dir(events).unwrap() {
        let day = day.unwrap().path();
        if !day.is_dir() {
            continue;
        }
        for entry in fs::read_dir(day).unwrap() {
            let value: serde_json::Value =
                serde_json::from_slice(&fs::read(entry.unwrap().path()).unwrap()).unwrap();
            if value.get("event_type").and_then(|value| value.as_str()) == Some(event_type)
                && value
                    .get("subject")
                    .and_then(|value| value.get("id"))
                    .and_then(|value| value.as_str())
                    == Some(subject)
            {
                count += 1;
            }
        }
    }
    count
}

fn make_shaping_receipt(
    id: &str,
    node: &pulse::graph::node::Node,
    manifest: &pulse::evidence::manifest::EvidenceManifest,
    content_rel: &str,
    content_hash: String,
    source_commit: String,
) -> ReceiptEnvelope {
    ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 1,
        id: id.to_string(),
        kind: ReceiptKind::ShapingValidation,
        result: ReceiptResult::Passed,
        actor: ActorRef {
            kind: ActorKind::Human,
            id: "tester".to_string(),
        },
        recorded_at: Utc::now(),
        subject: SubjectRef {
            kind: "work".to_string(),
            id: node.id.clone(),
        },
        bindings: ReceiptBindings {
            work: vec![WorkBinding {
                id: node.id.clone(),
                revision: node.revision,
            }],
            source: Some(SourceBinding {
                kind: "git_commit".to_string(),
                commit: source_commit,
                repository_id: manifest.repository_id.clone(),
            }),
            content: vec![ContentBinding {
                path: content_rel.to_string(),
                sha256: content_hash,
            }],
            artifacts: vec![],
            graph_fingerprint_observed: None,
        },
        payload: ReceiptPayload::ShapingValidation(ShapingValidationPayload {
            payload_version: 1,
            owning_work: ShapingWorkBinding {
                id: node.id.clone(),
                revision_observed: node.revision,
                contract_revision: node.contract_revision,
            },
            materialization: "R1".to_string(),
            shape_mode: ShapeMode::FocusedBranches,
            source_posture: SourcePosture::CleanGitCommit,
            destination: None,
            map: None,
            affected_work: vec![],
            branches: vec![],
            fog: vec![],
            out_of_scope: vec![],
            resolution_pointers: vec![],
            approval: ShapingApproval {
                approved_by: ActorRef {
                    kind: ActorKind::Human,
                    id: "tester".to_string(),
                },
                reference: "PULSE.md#human-judgment-boundaries".to_string(),
            },
            reconciliation: None,
            remaining_uncertainty: vec![],
        }),
    }
}

#[test]
fn evidence_artifact_put_verify_and_tamper_detection() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    pulse::evidence::bootstrap(repo).unwrap();
    let input = repo.join("notes.txt");
    fs::write(&input, b"review notes").unwrap();

    let manifest = pulse::evidence::manifest::load(repo).unwrap();
    let out = pulse::evidence::put_artifact(
        repo,
        None,
        &input,
        "review_notes".to_string(),
        Some("text/plain".to_string()),
        None,
        manifest.max_artifact_bytes,
    )
    .unwrap();
    assert_eq!(out.code, "artifact_recorded");
    pulse::evidence::verify_artifact(repo, &out.artifact.digest).unwrap();

    let hex = out.artifact.digest.strip_prefix("sha256:").unwrap();
    fs::write(
        repo.join(".pulse/evidence/artifacts/sha256")
            .join(&hex[0..2])
            .join(hex)
            .join("content"),
        b"tampered",
    )
    .unwrap();
    let err = pulse::evidence::verify_artifact(repo, &out.artifact.digest).unwrap_err();
    assert_eq!(err.code(), "artifact_hash_mismatch");
}

#[test]
fn receipt_record_verify_and_content_staleness() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = store
        .create_node(WorkKind::Ticket, "Ticket".to_string())
        .unwrap()
        .value;
    let manifest = pulse::evidence::bootstrap(repo).unwrap().manifest;
    let content_rel = format!("works/{}/ticket.md", node.id);
    let content_path = repo.join(&content_rel);
    fs::create_dir_all(content_path.parent().unwrap()).unwrap();
    fs::write(&content_path, b"acceptance").unwrap();
    let content_hash = hash_bytes(&fs::read(&content_path).unwrap());
    let source_commit = commit_all(repo);

    let receipt = make_shaping_receipt(
        "rcpt_01J00000000000000000000000",
        &node,
        &manifest,
        &content_rel,
        content_hash,
        source_commit,
    );
    let file = repo.join("receipt.json");
    write_json(&file, &receipt);
    let out = pulse::evidence::record_receipt(repo, None, &file).unwrap();
    assert_eq!(out.code, "receipt_recorded");
    let report = pulse::evidence::verify_receipt(repo, &out.receipt.id, true, None).unwrap();
    assert_eq!(report.integrity.status, "valid");
    assert_eq!(report.bindings.status, "current");
    assert_eq!(report.authorization.status, "not_evaluated");
    assert!(!report.gate_eligible);

    fs::write(&content_path, b"changed").unwrap();
    let report = pulse::evidence::verify_receipt(repo, &out.receipt.id, true, None).unwrap();
    assert_eq!(report.bindings.status, "stale");
    assert!(report
        .bindings
        .reason_codes
        .contains(&"content_binding_stale".to_string()));
}

#[test]
fn supersession_can_use_reconciliation_receipt_without_inline_assertion() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let old = store
        .create_node(WorkKind::Ticket, "Old".to_string())
        .unwrap()
        .value;
    let replacement = store
        .create_node(WorkKind::Story, "Replacement".to_string())
        .unwrap()
        .value;
    let manifest = pulse::evidence::bootstrap(repo).unwrap().manifest;
    for node in [&old, &replacement] {
        let path = repo.join(format!("works/{}/ticket.md", node.id));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("content {}", node.id)).unwrap();
    }
    let old_rel = format!("works/{}/ticket.md", old.id);
    let repl_rel = format!("works/{}/ticket.md", replacement.id);
    let source_commit = commit_all(repo);
    let receipt = ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 1,
        id: "rcpt_01J00000000000000000000001".to_string(),
        kind: ReceiptKind::SupersessionReconciliation,
        result: ReceiptResult::Passed,
        actor: ActorRef {
            kind: ActorKind::Human,
            id: "tester".to_string(),
        },
        recorded_at: Utc::now(),
        subject: SubjectRef {
            kind: "work".to_string(),
            id: old.id.clone(),
        },
        bindings: ReceiptBindings {
            work: vec![
                WorkBinding {
                    id: old.id.clone(),
                    revision: old.revision,
                },
                WorkBinding {
                    id: replacement.id.clone(),
                    revision: replacement.revision,
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
                    sha256: hash_bytes(&fs::read(repo.join(&old_rel)).unwrap()),
                },
                ContentBinding {
                    path: repl_rel.clone(),
                    sha256: hash_bytes(&fs::read(repo.join(&repl_rel)).unwrap()),
                },
            ],
            artifacts: vec![],
            graph_fingerprint_observed: None,
        },
        payload: ReceiptPayload::SupersessionReconciliation(SupersessionReconciliationPayload {
            payload_version: 1,
            old: WorkRevisionRef {
                id: old.id.clone(),
                revision: old.revision,
            },
            target: SupersessionReceiptTarget::Replacement {
                id: replacement.id.clone(),
                revision: replacement.revision,
            },
            claim: SupersessionReceiptClaim::Absorbed,
            follow_up_work: vec![],
            review_summary: "absorbed".to_string(),
            reviewed_references: vec![old.id.clone(), replacement.id.clone()],
        }),
    };
    let file = repo.join("supersession-receipt.json");
    write_json(&file, &receipt);
    pulse::evidence::record_receipt(repo, None, &file).unwrap();

    let out = store
        .supersede_work_with_receipt(
            &old.id,
            SupersessionTarget::Replacement {
                id: replacement.id.clone(),
            },
            old.revision,
            "absorbed".to_string(),
            receipt.id.clone(),
            "tester".to_string(),
        )
        .unwrap();
    assert_eq!(out.code, "superseded");
    assert!(out.value.assertion.is_none());
    assert_eq!(out.value.reconciliation_receipt.unwrap().id, receipt.id);
}

#[test]
fn receipt_record_recovery_completes_missing_event_and_retry_is_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = store
        .create_node(WorkKind::Ticket, "Recover".to_string())
        .unwrap()
        .value;
    let manifest = pulse::evidence::bootstrap(repo).unwrap().manifest;
    let content_rel = format!("works/{}/ticket.md", node.id);
    let content_path = repo.join(&content_rel);
    fs::create_dir_all(content_path.parent().unwrap()).unwrap();
    fs::write(&content_path, b"acceptance").unwrap();
    let content_hash = hash_bytes(&fs::read(&content_path).unwrap());
    let source_commit = commit_all(repo);
    let receipt = make_shaping_receipt(
        "rcpt_01J00000000000000000000002",
        &node,
        &manifest,
        &content_rel,
        content_hash,
        source_commit,
    );
    let file = repo.join("receipt-recovery.json");
    write_json(&file, &receipt);

    let err =
        pulse::evidence::record_receipt(repo, Some(TransactionFailpoint::AfterCanonical), &file)
            .unwrap_err();
    assert_eq!(err.code(), "failpoint");
    recover_prepared_transactions(repo).unwrap();
    let report = pulse::evidence::verify_receipt(repo, &receipt.id, true, None).unwrap();
    assert_eq!(report.integrity.status, "valid");
    let retry = pulse::evidence::record_receipt(repo, None, &file).unwrap();
    assert_eq!(retry.code, "unchanged");
}

#[test]
fn same_receipt_id_different_bytes_conflicts_and_concurrent_same_id_is_deterministic() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = store
        .create_node(WorkKind::Ticket, "Conflict".to_string())
        .unwrap()
        .value;
    let manifest = pulse::evidence::bootstrap(repo).unwrap().manifest;
    let content_rel = format!("works/{}/ticket.md", node.id);
    let content_path = repo.join(&content_rel);
    fs::create_dir_all(content_path.parent().unwrap()).unwrap();
    fs::write(&content_path, b"acceptance").unwrap();
    let content_hash = hash_bytes(&fs::read(&content_path).unwrap());
    let source_commit = commit_all(repo);
    let mut receipt = make_shaping_receipt(
        "rcpt_01J00000000000000000000003",
        &node,
        &manifest,
        &content_rel,
        content_hash,
        source_commit,
    );
    let file = repo.join("receipt-conflict.json");
    write_json(&file, &receipt);
    pulse::evidence::record_receipt(repo, None, &file).unwrap();
    if let ReceiptPayload::ShapingValidation(payload) = &mut receipt.payload {
        payload.materialization = "R2".to_string();
    }
    write_json(&file, &receipt);
    let err = pulse::evidence::record_receipt(repo, None, &file).unwrap_err();
    assert_eq!(err.code(), "receipt_id_conflict");
}

#[test]
fn dirty_bound_content_reports_unsupported_source_snapshot() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = store
        .create_node(WorkKind::Ticket, "Dirty".to_string())
        .unwrap()
        .value;
    let manifest = pulse::evidence::bootstrap(repo).unwrap().manifest;
    let content_rel = format!("works/{}/ticket.md", node.id);
    let content_path = repo.join(&content_rel);
    fs::create_dir_all(content_path.parent().unwrap()).unwrap();
    fs::write(&content_path, b"acceptance").unwrap();
    let content_hash = hash_bytes(&fs::read(&content_path).unwrap());
    let source_commit = commit_all(repo);
    let receipt = make_shaping_receipt(
        "rcpt_01J00000000000000000000004",
        &node,
        &manifest,
        &content_rel,
        content_hash.clone(),
        source_commit,
    );
    let file = repo.join("receipt-dirty.json");
    write_json(&file, &receipt);
    pulse::evidence::record_receipt(repo, None, &file).unwrap();
    fs::write(&content_path, b"changed but uncommitted").unwrap();
    let report = pulse::evidence::verify_receipt(repo, &receipt.id, true, None).unwrap();
    assert!(report
        .bindings
        .reason_codes
        .contains(&"dirty_source_unsupported".to_string()));
}

#[test]
fn artifact_put_recovery_rolls_forward_content_metadata_and_event() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    pulse::evidence::bootstrap(repo).unwrap();
    let input = repo.join("artifact-recover.txt");
    fs::write(&input, b"artifact recovery notes").unwrap();
    let expected_digest = hash_bytes(&fs::read(&input).unwrap());
    let manifest = pulse::evidence::manifest::load(repo).unwrap();

    let err = pulse::evidence::put_artifact(
        repo,
        Some(TransactionFailpoint::AfterMultiTargetFirst),
        &input,
        "review_notes".to_string(),
        Some("text/plain".to_string()),
        None,
        manifest.max_artifact_bytes,
    )
    .unwrap_err();
    assert_eq!(err.code(), "failpoint");

    let hex = expected_digest.strip_prefix("sha256:").unwrap();
    let artifact_dir = repo
        .join(".pulse/evidence/artifacts/sha256")
        .join(&hex[0..2])
        .join(hex);
    assert!(artifact_dir.join("content").exists());
    assert!(!artifact_dir.join("metadata.json").exists());
    assert_eq!(
        event_count(repo, "evidence.artifact.recorded", &expected_digest),
        0
    );

    recover_prepared_transactions(repo).unwrap();
    assert!(artifact_dir.join("metadata.json").exists());
    pulse::evidence::verify_artifact(repo, &expected_digest).unwrap();
    assert_eq!(
        event_count(repo, "evidence.artifact.recorded", &expected_digest),
        1
    );

    let retry = pulse::evidence::put_artifact(
        repo,
        None,
        &input,
        "review_notes".to_string(),
        Some("text/plain".to_string()),
        None,
        manifest.max_artifact_bytes,
    )
    .unwrap();
    assert_eq!(retry.code, "unchanged");
    assert_eq!(
        event_count(repo, "evidence.artifact.recorded", &expected_digest),
        1
    );
}
