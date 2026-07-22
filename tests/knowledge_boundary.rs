use chrono::{TimeZone, Utc};
use pulse::canonical_json::{hash_bytes, to_canonical_bytes};
use pulse::docs::model::{
    DocsRegistry, DocumentAuthority, DocumentKind, DocumentLifecycle, DocumentRecord,
    DocumentScope, RetrievalConfig, ReviewPolicy,
};
use pulse::evidence::model::{
    ActorKind, ActorRef, ApprovalAssertion, BranchSummary, ReceiptBindings, ReceiptEnvelope,
    ReceiptKind, ReceiptPayload, ReceiptResult, ShapingValidationPayload, SourceBinding,
    SubjectRef, WorkBinding, WorkRevisionRef,
};
use pulse::graph::store::OperationContext as WorkCtx;
use pulse::id::WorkKind;
use pulse::knowledge::model::*;
use pulse::knowledge::relation::{EndpointKind, RelationType};
use pulse::knowledge::store::{KnowledgeStore, OperationContext, RelationAdd};
use pulse::JsonGraphStore;
use std::fs;

#[cfg(unix)]
use std::os::unix::fs as unix_fs;

type Repo = tempfile::TempDir;

fn setup() -> (Repo, JsonGraphStore, KnowledgeStore, String) {
    let repo = tempfile::tempdir().unwrap();
    let graph = JsonGraphStore::new(repo.path());
    graph.bootstrap().unwrap();
    let work = graph
        .create_node_with_context(
            WorkKind::Ticket,
            "Knowledge source".to_string(),
            WorkCtx {
                actor: "test".to_string(),
                now: Utc.timestamp_opt(1, 0).unwrap(),
            },
        )
        .unwrap()
        .value
        .id;
    let knowledge = KnowledgeStore::new(repo.path());
    (repo, graph, knowledge, work)
}

fn draft(work_id: &str) -> LearningDraft {
    LearningDraft {
        title: "Token rotation requires atomic mutation".to_string(),
        kind: LearningKind::FailurePattern,
        severity: Severity::High,
        summary: "Concurrent refresh can issue invalid tokens when rotation uses check-then-act."
            .to_string(),
        guidance: Guidance {
            r#do: vec!["Use an atomic state transition.".to_string()],
            avoid: vec!["Do not split rotation into unguarded read then write.".to_string()],
            required_checks: vec!["Exercise concurrent refresh attempts.".to_string()],
        },
        applicability: Applicability {
            paths: vec!["src/auth/**".to_string()],
            symbols: vec!["rotateRefreshToken".to_string()],
            risks: vec!["concurrency".to_string()],
            ..Applicability::default()
        },
        provenance_targets: vec![pulse::knowledge::model::ProvenanceTargetDraft {
            relation: RelationType::DerivedFrom,
            kind: EndpointKind::Work,
            id: work_id.to_string(),
            revision: Some(1),
            content_hash: None,
        }],
        source_commits: vec![],
        routing: None,
        promotion: None,
        freshness: None,
        trust: None,
        content: None,
    }
}

fn ctx(sec: i64) -> OperationContext {
    OperationContext {
        actor: "human:test".to_string(),
        now: Utc.timestamp_opt(sec, 0).unwrap(),
    }
}

fn doc_record(id: &str, revision: u64) -> DocumentRecord {
    DocumentRecord {
        id: id.to_string(),
        revision,
        path: "docs/domain/auth.md".to_string(),
        kind: DocumentKind::Domain,
        authority: DocumentAuthority::Approved,
        lifecycle: DocumentLifecycle::Current,
        owner: "team:test".to_string(),
        summary: "Auth domain".to_string(),
        aliases: Vec::new(),
        scope: DocumentScope {
            paths: vec!["src/auth/**".to_string()],
            ..DocumentScope::default()
        },
        review_policy: ReviewPolicy::Standard,
        verification_profile: "standard".to_string(),
        generated: None,
        superseded_by: None,
        retrieval: None,
    }
}

fn make_receipt(
    id: &str,
    node: &pulse::graph::node::Node,
    manifest: &pulse::evidence::manifest::EvidenceManifest,
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
        recorded_at: Utc.timestamp_opt(20, 0).unwrap(),
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
            content: vec![],
            artifacts: vec![],
            graph_fingerprint_observed: None,
        },
        payload: ReceiptPayload::ShapingValidation(ShapingValidationPayload {
            payload_version: 1,
            owning_work: WorkRevisionRef {
                id: node.id.clone(),
                revision: node.revision,
            },
            risk: "R1".to_string(),
            destination: None,
            branch_summary: BranchSummary::default(),
            remaining_uncertainty: vec![],
            approval_assertion: ApprovalAssertion {
                required: false,
                reference: None,
            },
        }),
    }
}

fn init_git(repo: &std::path::Path) -> String {
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.name", "Tester"])
        .current_dir(repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-m", "snapshot"])
        .current_dir(repo)
        .output()
        .unwrap();
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn write_docs_registry(repo: &std::path::Path, manifest_repo_id: String, document: DocumentRecord) {
    fs::create_dir_all(repo.join("docs/domain")).unwrap();
    fs::write(repo.join("docs/domain/auth.md"), b"# Auth\n").unwrap();
    fs::create_dir_all(repo.join(".pulse/docs")).unwrap();
    let registry = DocsRegistry {
        schema_version: 2,
        revision: 1,
        repository_id: manifest_repo_id,
        documents: vec![document],
        retrieval: Some(RetrievalConfig::defaults()),
    };
    fs::write(
        repo.join(".pulse/docs/registry.json"),
        to_canonical_bytes(&registry).unwrap(),
    )
    .unwrap();
}

#[test]
fn content_binding_rejects_unsafe_paths_and_stale_hash() {
    let (repo, _graph, knowledge, work) = setup();

    let mut unsafe_draft = draft(&work);
    unsafe_draft.content = Some(ContentBinding {
        path: "knowledge/learnings/../escape.md".to_string(),
        content_hash: hash_bytes(b"content"),
    });
    assert_eq!(
        knowledge.create(unsafe_draft, ctx(10)).unwrap_err().code(),
        "learning_content_path_unsafe"
    );

    fs::create_dir_all(repo.path().join("knowledge/learnings")).unwrap();
    fs::write(
        repo.path().join("knowledge/learnings/safe.md"),
        b"new content",
    )
    .unwrap();
    let mut stale_hash = draft(&work);
    stale_hash.content = Some(ContentBinding {
        path: "knowledge/learnings/safe.md".to_string(),
        content_hash: hash_bytes(b"old content"),
    });
    assert_eq!(
        knowledge.create(stale_hash, ctx(11)).unwrap_err().code(),
        "learning_content_hash_stale"
    );
}

#[cfg(unix)]
#[test]
fn content_binding_rejects_symlink_escape() {
    let (repo, _graph, knowledge, work) = setup();
    fs::write(repo.path().join("outside.md"), b"outside").unwrap();
    fs::create_dir_all(repo.path().join("knowledge/learnings")).unwrap();
    unix_fs::symlink(
        repo.path().join("outside.md"),
        repo.path().join("knowledge/learnings/escaped.md"),
    )
    .unwrap();

    let mut draft = draft(&work);
    draft.content = Some(ContentBinding {
        path: "knowledge/learnings/escaped.md".to_string(),
        content_hash: hash_bytes(b"outside"),
    });
    assert_eq!(
        knowledge.create(draft, ctx(10)).unwrap_err().code(),
        "learning_content_path_unsafe"
    );
}

#[test]
fn applicability_and_freshness_paths_use_same_safe_subset() {
    let (_repo, _graph, knowledge, work) = setup();
    let mut app = draft(&work);
    app.applicability.paths = vec![".pulse/evidence/receipts/**".to_string()];
    assert_eq!(
        knowledge.create(app, ctx(10)).unwrap_err().code(),
        "learning_content_path_unsafe"
    );

    let mut fresh = draft(&work);
    fresh.freshness = Some(Freshness {
        invalidated_by_paths: vec![".pulse/cache/knowledge.snapshot.json".to_string()],
        ..Freshness::default()
    });
    assert_eq!(
        knowledge.create(fresh, ctx(11)).unwrap_err().code(),
        "learning_content_path_unsafe"
    );
}

#[test]
fn relation_endpoint_revision_and_hash_bindings_are_checked() {
    let (repo, graph, knowledge, work) = setup();
    knowledge.create(draft(&work), ctx(10)).unwrap();
    graph
        .edit_title_with_context(
            &work,
            1,
            "Changed title".to_string(),
            WorkCtx {
                actor: "test".to_string(),
                now: Utc.timestamp_opt(11, 0).unwrap(),
            },
        )
        .unwrap();

    let stale_work = RelationAdd {
        relation_type: RelationType::AppliedTo,
        to_kind: EndpointKind::Work,
        to: work.clone(),
        target_revision: Some(1),
        target_hash: None,
        expected_revision: 1,
    };
    assert_eq!(
        knowledge
            .add_relation("LRN-001", stale_work, ctx(12))
            .unwrap_err()
            .code(),
        "knowledge_relation_endpoint_revision_mismatch"
    );

    let docs_registry = pulse::docs::bootstrap(repo.path()).unwrap().registry;
    write_docs_registry(
        repo.path(),
        docs_registry.repository_id,
        doc_record("DOC-AUTH", 3),
    );
    let stale_doc = RelationAdd {
        relation_type: RelationType::PromotedTo,
        to_kind: EndpointKind::Document,
        to: "DOC-AUTH".to_string(),
        target_revision: Some(2),
        target_hash: None,
        expected_revision: 1,
    };
    assert_eq!(
        knowledge
            .add_relation("LRN-001", stale_doc, ctx(13))
            .unwrap_err()
            .code(),
        "knowledge_relation_endpoint_revision_mismatch"
    );

    let source_commit = init_git(repo.path());
    let evidence_manifest = pulse::evidence::bootstrap(repo.path()).unwrap().manifest;
    let current_node = graph.show_node(&work).unwrap();
    let receipt = make_receipt(
        "rcpt_01J00000000000000000000998",
        &current_node,
        &evidence_manifest,
        source_commit,
    );
    let receipt_file = repo.path().join("receipt.json");
    fs::write(&receipt_file, to_canonical_bytes(&receipt).unwrap()).unwrap();
    pulse::evidence::record_receipt(repo.path(), None, &receipt_file).unwrap();
    let stale_receipt = RelationAdd {
        relation_type: RelationType::DerivedFrom,
        to_kind: EndpointKind::Receipt,
        to: receipt.id,
        target_revision: None,
        target_hash: Some(hash_bytes(b"other")),
        expected_revision: 1,
    };
    assert_eq!(
        knowledge
            .add_relation("LRN-001", stale_receipt, ctx(14))
            .unwrap_err()
            .code(),
        "knowledge_relation_endpoint_hash_mismatch"
    );
}

#[test]
fn schema_drift_is_detected_after_bootstrap() {
    let (repo, _graph, knowledge, _work) = setup();
    knowledge.bootstrap().unwrap();
    fs::write(
        repo.path()
            .join(".pulse/knowledge/schemas/learning.schema.json"),
        b"{}\n",
    )
    .unwrap();
    assert_eq!(
        knowledge.bootstrap().unwrap_err().code(),
        "knowledge_schema_hash_mismatch"
    );
}
