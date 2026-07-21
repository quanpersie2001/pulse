use chrono::Utc;
use pulse::canonical_json::{hash_bytes, to_canonical_bytes};
use pulse::docs::model::{
    DocsRegistry, DocumentAuthority, DocumentKind, DocumentLifecycle, DocumentRecord,
    DocumentScope, ReviewPolicy,
};
use pulse::evidence::model::*;
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
    fs::write(path, to_canonical_bytes(value).unwrap()).unwrap();
}

fn setup_repo(
    doc_id: &str,
    revision: u64,
    lifecycle: DocumentLifecycle,
    policy: ReviewPolicy,
) -> (tempfile::TempDir, String, String, String) {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let manifest = pulse::evidence::bootstrap(repo).unwrap().manifest;
    let path = "docs/domain/token-lifecycle.md".to_string();
    let full = repo.join(&path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(&full, b"# Token lifecycle\n").unwrap();
    let hash = hash_bytes(&fs::read(&full).unwrap());
    fs::create_dir_all(repo.join(".pulse/docs")).unwrap();
    let registry = DocsRegistry {
        schema_version: 1,
        revision: 1,
        repository_id: manifest.repository_id.clone(),
        documents: vec![document(doc_id, revision, &path, lifecycle, policy, None)],
    };
    write_json(&repo.join(".pulse/docs/registry.json"), &registry);
    let source_commit = commit_all(repo);
    (tmp, manifest.repository_id, source_commit, hash)
}

fn document(
    id: &str,
    revision: u64,
    path: &str,
    lifecycle: DocumentLifecycle,
    review_policy: ReviewPolicy,
    superseded_by: Option<String>,
) -> DocumentRecord {
    DocumentRecord {
        id: id.to_string(),
        revision,
        path: path.to_string(),
        kind: DocumentKind::Domain,
        authority: DocumentAuthority::Approved,
        lifecycle,
        owner: "team:identity".to_string(),
        summary: "Token lifecycle".to_string(),
        aliases: vec![],
        scope: DocumentScope::default(),
        review_policy,
        verification_profile: "domain-doc".to_string(),
        generated: None,
        superseded_by,
    }
}

fn receipt(
    id: &str,
    repository_id: &str,
    source_commit: &str,
    doc: DocumentationValidationDocument,
    checks: Vec<DocumentCheck>,
) -> ReceiptEnvelope {
    let path = doc.path.clone();
    let content_hash = doc.content_hash.clone();
    ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 1,
        id: id.to_string(),
        kind: ReceiptKind::DocumentationValidation,
        result: ReceiptResult::Passed,
        actor: ActorRef {
            kind: ActorKind::Human,
            id: "tester".to_string(),
        },
        recorded_at: Utc::now(),
        subject: SubjectRef {
            kind: "documentation".to_string(),
            id: "DOC-AUTH-DOMAIN".to_string(),
        },
        bindings: ReceiptBindings {
            work: vec![],
            source: Some(SourceBinding {
                kind: "git_commit".to_string(),
                commit: source_commit.to_string(),
                repository_id: repository_id.to_string(),
            }),
            content: vec![ContentBinding {
                path,
                sha256: content_hash,
            }],
            artifacts: vec![],
            graph_fingerprint_observed: None,
        },
        payload: ReceiptPayload::DocumentationValidation(DocumentationValidationPayload {
            payload_version: 1,
            documents: vec![doc],
            checks,
        }),
    }
}

fn record(repo: &std::path::Path, receipt: &ReceiptEnvelope) {
    let file = repo.join(format!("{}.json", receipt.id));
    write_json(&file, receipt);
    pulse::evidence::record_receipt(repo, None, &file).unwrap();
}

fn receipt_doc(id: &str, revision: u64, path: &str, hash: &str) -> DocumentationValidationDocument {
    DocumentationValidationDocument {
        document_id: id.to_string(),
        document_revision: revision,
        path: path.to_string(),
        content_hash: hash.to_string(),
        result: ReceiptResult::Passed,
    }
}

#[test]
fn v1_current_registry_match_is_gate_eligible_for_none_policy() {
    let (tmp, repository_id, source_commit, hash) = setup_repo(
        "DOC-AUTH-DOMAIN",
        3,
        DocumentLifecycle::Current,
        ReviewPolicy::None,
    );
    let repo = tmp.path();
    let rcpt = receipt(
        "rcpt_01J00000000000000000000100",
        &repository_id,
        &source_commit,
        receipt_doc(
            "DOC-AUTH-DOMAIN",
            3,
            "docs/domain/token-lifecycle.md",
            &hash,
        ),
        vec![],
    );
    record(repo, &rcpt);

    let report = pulse::evidence::verify_receipt(repo, &rcpt.id, true, None).unwrap();
    assert_eq!(report.schema_version, 2);
    assert_eq!(report.integrity.status, "valid");
    assert_eq!(report.bindings.status, "current");
    assert_eq!(report.registry.status, "current");
    assert_eq!(report.policy.status, "structurally_satisfied");
    assert!(report.gate_eligible);
}

#[test]
fn wrong_id_for_same_path_reports_registry_mismatch() {
    let (tmp, repository_id, source_commit, hash) = setup_repo(
        "DOC-AUTH-DOMAIN",
        3,
        DocumentLifecycle::Current,
        ReviewPolicy::None,
    );
    let repo = tmp.path();
    let rcpt = receipt(
        "rcpt_01J00000000000000000000101",
        &repository_id,
        &source_commit,
        receipt_doc("DOC-WRONG-ID", 3, "docs/domain/token-lifecycle.md", &hash),
        vec![],
    );
    record(repo, &rcpt);

    let report = pulse::evidence::verify_receipt(repo, &rcpt.id, true, None).unwrap();
    assert_eq!(report.integrity.status, "valid");
    assert_eq!(report.registry.status, "mismatch");
    assert!(report
        .registry
        .reason_codes
        .contains(&"document_receipt_registry_mismatch".to_string()));
    assert!(report
        .registry
        .reason_codes
        .contains(&"document_receipt_wrong_id_for_path".to_string()));
    assert!(!report.gate_eligible);
}

#[test]
fn document_revision_stale_is_current_verification_mismatch() {
    let (tmp, repository_id, source_commit, hash) = setup_repo(
        "DOC-AUTH-DOMAIN",
        4,
        DocumentLifecycle::Current,
        ReviewPolicy::None,
    );
    let repo = tmp.path();
    let rcpt = receipt(
        "rcpt_01J00000000000000000000102",
        &repository_id,
        &source_commit,
        receipt_doc(
            "DOC-AUTH-DOMAIN",
            3,
            "docs/domain/token-lifecycle.md",
            &hash,
        ),
        vec![],
    );
    record(repo, &rcpt);

    let report = pulse::evidence::verify_receipt(repo, &rcpt.id, true, None).unwrap();
    assert_eq!(report.registry.status, "mismatch");
    assert!(report
        .registry
        .reason_codes
        .contains(&"document_receipt_revision_stale".to_string()));
    assert!(!report.gate_eligible);
}

#[test]
fn retired_superseded_and_stale_receipts_remain_historical_integrity_valid_but_ineligible() {
    for (index, lifecycle, code) in [
        (3, DocumentLifecycle::Retired, "document_retired"),
        (4, DocumentLifecycle::Superseded, "document_superseded"),
        (5, DocumentLifecycle::Stale, "document_stale"),
    ] {
        let (tmp, repository_id, source_commit, hash) =
            setup_repo("DOC-AUTH-DOMAIN", 3, lifecycle, ReviewPolicy::None);
        let repo = tmp.path();
        let rcpt = receipt(
            &format!("rcpt_01J0000000000000000000010{index}"),
            &repository_id,
            &source_commit,
            receipt_doc(
                "DOC-AUTH-DOMAIN",
                3,
                "docs/domain/token-lifecycle.md",
                &hash,
            ),
            vec![],
        );
        record(repo, &rcpt);

        let report = pulse::evidence::verify_receipt(repo, &rcpt.id, true, None).unwrap();
        assert_eq!(report.integrity.status, "valid");
        assert_eq!(report.registry.status, "not_current");
        assert!(report.registry.reason_codes.contains(&code.to_string()));
        assert!(!report.gate_eligible);
    }
}

#[test]
fn documentation_payload_version_two_is_unsupported_pre_public_reset() {
    let (tmp, repository_id, source_commit, hash) = setup_repo(
        "DOC-AUTH-DOMAIN",
        3,
        DocumentLifecycle::Current,
        ReviewPolicy::None,
    );
    let repo = tmp.path();
    let mut rcpt = receipt(
        "rcpt_01J00000000000000000000106",
        &repository_id,
        &source_commit,
        receipt_doc(
            "DOC-AUTH-DOMAIN",
            3,
            "docs/domain/token-lifecycle.md",
            &hash,
        ),
        vec![],
    );
    let ReceiptPayload::DocumentationValidation(payload) = &mut rcpt.payload else {
        panic!("documentation payload expected");
    };
    payload.payload_version = 2;
    let file = repo.join(format!("{}.json", rcpt.id));
    write_json(&file, &rcpt);

    let error = pulse::evidence::record_receipt(repo, None, &file).unwrap_err();
    assert_eq!(error.code(), "receipt_version_unsupported");
}

#[test]
fn independent_policy_has_structural_checks_but_authorization_unresolved() {
    let (tmp, repository_id, source_commit, hash) = setup_repo(
        "DOC-AUTH-DOMAIN",
        3,
        DocumentLifecycle::Current,
        ReviewPolicy::Independent,
    );
    let repo = tmp.path();
    let rcpt = receipt(
        "rcpt_01J00000000000000000000107",
        &repository_id,
        &source_commit,
        receipt_doc(
            "DOC-AUTH-DOMAIN",
            3,
            "docs/domain/token-lifecycle.md",
            &hash,
        ),
        vec![
            DocumentCheck {
                kind: "link_check".to_string(),
                result: ReceiptResult::Passed,
                artifact: None,
            },
            DocumentCheck {
                kind: "semantic_review".to_string(),
                result: ReceiptResult::Passed,
                artifact: None,
            },
        ],
    );
    record(repo, &rcpt);

    let report = pulse::evidence::verify_receipt(repo, &rcpt.id, true, None).unwrap();
    assert_eq!(report.registry.status, "current");
    assert_eq!(report.policy.status, "structurally_satisfied");
    assert_eq!(report.authorization.status, "unresolved");
    assert!(report
        .authorization
        .reason_codes
        .contains(&"independent_authorization_unresolved".to_string()));
    assert!(!report.gate_eligible);
}
