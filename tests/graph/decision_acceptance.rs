use crate::common_canon::write_json;
use crate::common_git::commit_all;
use chrono::Utc;
use pulse::canonical_json::hash_bytes;
use pulse::evidence::model::*;
use pulse::id::WorkKind;
use pulse::JsonGraphStore;
use std::fs;

fn make_receipt(
    id: &str,
    decision: &pulse::graph::node::Node,
    manifest: &pulse::evidence::manifest::EvidenceManifest,
    path: &str,
    content_hash: String,
    source_commit: String,
) -> ReceiptEnvelope {
    ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 1,
        id: id.to_string(),
        kind: ReceiptKind::DecisionAcceptance,
        result: ReceiptResult::Passed,
        actor: ActorRef {
            kind: ActorKind::Human,
            id: "quannv".to_string(),
        },
        recorded_at: Utc::now(),
        subject: SubjectRef {
            kind: "work".to_string(),
            id: decision.id.clone(),
        },
        bindings: ReceiptBindings {
            work: vec![WorkBinding {
                id: decision.id.clone(),
                revision: decision.revision,
            }],
            source: Some(SourceBinding {
                kind: "git_commit".to_string(),
                commit: source_commit,
                repository_id: manifest.repository_id.clone(),
            }),
            content: vec![ContentBinding {
                path: path.to_string(),
                sha256: content_hash.clone(),
            }],
            artifacts: vec![],
            graph_fingerprint_observed: None,
        },
        payload: ReceiptPayload::DecisionAcceptance(DecisionAcceptancePayload {
            payload_version: 1,
            decision: DecisionAcceptanceDecision {
                id: decision.id.clone(),
                revision_observed: decision.revision,
                contract_revision: decision.contract_revision,
                content: DecisionContentSnapshot {
                    path: path.to_string(),
                    content_hash,
                },
            },
            accepted_outcome: "Accept compatibility-preserving refresh-token semantics."
                .to_string(),
            approver: ActorRef {
                kind: ActorKind::Human,
                id: "quannv".to_string(),
            },
            source_posture: SourcePosture::CleanGitCommit,
        }),
    }
}

#[test]
fn decision_acceptance_receipt_records_and_detects_content_staleness() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let decision = store
        .create_node(WorkKind::Decision, "Token compatibility".to_string())
        .unwrap()
        .value;
    let manifest = pulse::evidence::bootstrap(repo).unwrap().manifest;
    let rel = format!("works/{}/decision.md", decision.id);
    let path = repo.join(&rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"# Decision\nPreserve compatibility.").unwrap();
    let content_hash = hash_bytes(&fs::read(&path).unwrap());
    let source_commit = commit_all(repo);

    let receipt = make_receipt(
        "rcpt_01J00000000000000000000400",
        &decision,
        &manifest,
        &rel,
        content_hash,
        source_commit,
    );
    let file = repo.join("decision-acceptance.json");
    write_json(&file, &receipt);
    let out = pulse::evidence::record_receipt(repo, None, &file).unwrap();
    assert_eq!(out.receipt.kind, ReceiptKind::DecisionAcceptance);

    let report = pulse::evidence::verify_receipt(repo, &out.receipt.id, true, None).unwrap();
    assert_eq!(report.integrity.status, "valid");
    assert_eq!(report.bindings.status, "current");

    fs::write(&path, b"# Decision\nChanged.").unwrap();
    let report = pulse::evidence::verify_receipt(repo, &out.receipt.id, true, None).unwrap();
    assert_eq!(report.bindings.status, "stale");
    assert!(report
        .bindings
        .reason_codes
        .contains(&"content_binding_stale".to_string()));
}

#[test]
fn decision_acceptance_requires_decision_subject_and_content_binding() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let decision = store
        .create_node(WorkKind::Decision, "Token compatibility".to_string())
        .unwrap()
        .value;
    let manifest = pulse::evidence::bootstrap(repo).unwrap().manifest;
    let rel = format!("works/{}/decision.md", decision.id);
    let path = repo.join(&rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"# Decision\nPreserve compatibility.").unwrap();
    let content_hash = hash_bytes(&fs::read(&path).unwrap());
    let source_commit = commit_all(repo);
    let mut receipt = make_receipt(
        "rcpt_01J00000000000000000000401",
        &decision,
        &manifest,
        &rel,
        content_hash,
        source_commit,
    );
    receipt.subject.id = "TK-999".to_string();
    let file = repo.join("decision-acceptance-bad.json");
    write_json(&file, &receipt);
    let err = pulse::evidence::record_receipt(repo, None, &file).unwrap_err();
    assert_eq!(err.code(), "decision_acceptance_stale");
}
