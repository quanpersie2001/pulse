//! Cross-domain compile-time public-path compatibility baseline.
//!
//! This crate locks the Rust paths that external-style integration coverage,
//! benches and `src/bin/pulse.rs` currently import across Pulse domains. It is
//! deliberately lightweight: compile and a few stable constants/constructors,
//! not exhaustive API snapshots.

use pulse::docs::{
    ApplicabilityOptions, DocsRegistry, DocumentAuthority, DocumentKind, DocumentLifecycle,
    DocumentRecord, DocumentScope, GetOptions, IndexOptions, RetrievalConfig, ReviewPolicy,
    SearchOptions, TreeOptions, DOCUMENT_SCHEMA,
};
use pulse::event::{EventActor, EventActorKind, EventCorrelation, EventEnvelope, EventSubject};
use pulse::evidence::model::{
    ActorKind, ActorRef, ReceiptBindings, ReceiptEnvelope, ReceiptKind, ReceiptPayload,
    ReceiptResult, SourceBinding, SubjectRef,
};
use pulse::id::{new_event_id, new_transaction_id};
use pulse::identity::actor::{ActorKind as NeutralActorKind, ActorRef as NeutralActorRef};
use pulse::knowledge::{
    Applicability, Audience, Confidence, Guidance, KnowledgeStore, LearningDraft, LearningKind,
    LearningStatus, Moment, OperationContext as KnowledgeOperationContext, PromptPriority,
    Severity,
};
use pulse::policy::AuthorityPolicy;
use pulse::source::head_commit;
use pulse::storage::transaction::{recover_prepared_transactions, TransactionFailpoint};
use pulse::storage::{bootstrap as storage_bootstrap, safe_repo_relative, MANIFEST_JSON};
use pulse::{JsonGraphStore, PulseError, PulseResult, Result};

#[test]
fn docs_evidence_knowledge_storage_and_identity_public_paths_compile() {
    let repo = tempfile::tempdir().unwrap();
    storage_bootstrap(repo.path()).unwrap();
    JsonGraphStore::new(repo.path()).bootstrap().unwrap();
    let _knowledge = KnowledgeStore::new(repo.path());

    let _registry = DocsRegistry::empty("repo-test".to_string());
    let _record = DocumentRecord {
        id: "DOC-ARCH".to_string(),
        revision: 1,
        path: "docs/architecture/graph.md".to_string(),
        kind: DocumentKind::Architecture,
        authority: DocumentAuthority::Approved,
        lifecycle: DocumentLifecycle::Current,
        owner: "team:test".to_string(),
        summary: "Graph architecture.".to_string(),
        aliases: vec![],
        scope: DocumentScope::default(),
        review_policy: ReviewPolicy::Standard,
        verification_profile: "architecture-doc".to_string(),
        generated: None,
        superseded_by: None,
        retrieval: None,
    };

    let _docs_options = (
        ApplicabilityOptions::default(),
        IndexOptions::default(),
        SearchOptions::default(),
        GetOptions::default(),
        TreeOptions::default(),
    );
    assert_eq!(RetrievalConfig::defaults().default_search_limit, 8);
    assert!(DOCUMENT_SCHEMA.contains("Document Registry"));

    let actor = EventActor::new(EventActorKind::Human, "tester");
    let subject = EventSubject::new("ticket", "TK-001", Some(1));
    let event = EventEnvelope::new_typed(
        "evt_01J00000000000000000000000",
        "work.checked",
        actor,
        subject,
        Some(EventCorrelation {
            run_id: Some("run_01J00000000000000000000000".to_string()),
            lease_id: None,
            transaction_id: None,
            receipt_id: None,
        }),
        serde_json::json!({"ok": true}),
        chrono::Utc::now(),
    );
    assert_eq!(event.schema_version, 1);

    let receipt = ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 1,
        id: "rcpt_01J00000000000000000000000".to_string(),
        kind: ReceiptKind::DocumentationValidation,
        result: ReceiptResult::Passed,
        actor: ActorRef {
            kind: ActorKind::Agent,
            id: "reviewer".to_string(),
        },
        recorded_at: chrono::Utc::now(),
        subject: SubjectRef {
            kind: "document".to_string(),
            id: "DOC-ARCH".to_string(),
        },
        bindings: ReceiptBindings {
            source: Some(SourceBinding {
                kind: "git_commit".to_string(),
                commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
                repository_id: "repo-test".to_string(),
            }),
            ..ReceiptBindings::default()
        },
        payload: ReceiptPayload::DocumentationValidation(
            pulse::evidence::model::DocumentationValidationPayload {
                payload_version: 1,
                documents: vec![],
                checks: vec![],
            },
        ),
    };
    assert_eq!(receipt.kind.as_str(), "documentation_validation");

    let _learning = LearningDraft {
        title: "Use public paths intentionally".to_string(),
        kind: LearningKind::ContextRoutingInsight,
        severity: Severity::Low,
        summary: "Compile-time tests guard source-tree refactor paths.".to_string(),
        guidance: Guidance::default(),
        applicability: Applicability::default(),
        provenance_targets: vec![],
        source_commits: vec![],
        routing: None,
        promotion: None,
        freshness: None,
        trust: None,
        content: None,
    };
    assert_eq!(LearningStatus::Candidate, LearningStatus::Candidate);
    assert_eq!(Audience::Implementer, Audience::Implementer);
    assert_eq!(Moment::Execute, Moment::Execute);
    assert_eq!(PromptPriority::Recommended, PromptPriority::Recommended);
    assert_eq!(Confidence::Low, Confidence::Low);

    let _ctx = KnowledgeOperationContext {
        actor: "human:test".to_string(),
        now: chrono::Utc::now(),
    };
    let _policy = AuthorityPolicy {
        schema_version: 1,
        revision: 1,
        principals: vec![],
    };
    assert!(head_commit(repo.path()).is_err());
    let _ = recover_prepared_transactions(repo.path()).unwrap();
    assert!(matches!(
        TransactionFailpoint::AfterCanonical,
        TransactionFailpoint::AfterCanonical
    ));
    assert!(safe_repo_relative("docs/architecture/graph.md").is_ok());
    assert!(MANIFEST_JSON.contains("pulse-main"));

    // Neutral identity ownership: the neutral `pulse::identity::actor` path is the
    // real owner, and is type-identical to the historical `evidence::model` path
    // (re-export, not a redefinition).
    fn accepts_evidence_actor(_: ActorRef) {}
    accepts_evidence_actor(NeutralActorRef {
        kind: NeutralActorKind::Human,
        id: "neutral".to_string(),
    });
    let neutral = NeutralActorRef {
        kind: NeutralActorKind::System,
        id: "neutral-2".to_string(),
    };
    let _evidence_view: ActorRef = neutral;
    // ID generation compatibility re-exports remain reachable through `pulse::id`.
    assert!(new_event_id().starts_with("evt_"));
    assert!(new_transaction_id().starts_with("txn_"));

    fn accepts_result(_: PulseResult<()>) {}
    fn accepts_alias(_: Result<()>) {}
    accepts_result(Ok(()));
    accepts_alias(Ok(()));
    let error = PulseError::validation("baseline", "baseline");
    assert_eq!(error.code(), "baseline");
}

#[test]
fn evidence_receipt_public_paths_compile() {
    // The receipt module was split into cohesive submodules. These stable
    // public paths must remain reachable both via the `pulse::evidence::*`
    // re-exports (used by tests / CLI) and the direct
    // `pulse::evidence::receipt::*` paths (used by kernel / graph / knowledge
    // consumers). Paths are referenced, not invoked, so this is a pure
    // compile-time non-regression guard.

    // Re-export layer.
    let _ = pulse::evidence::record_receipt;
    let _ = pulse::evidence::show_receipt;
    let _ = pulse::evidence::list_receipts;
    let _ = pulse::evidence::verify_receipt;
    let _ = pulse::evidence::validate_for_supersession;

    // Direct receipt-module paths.
    let _ = pulse::evidence::receipt::record_receipt;
    let _ = pulse::evidence::receipt::show_receipt;
    let _ = pulse::evidence::receipt::list_receipts;
    let _ = pulse::evidence::receipt::verify_receipt;
    let _ = pulse::evidence::receipt::load_receipt;
    let _ = pulse::evidence::receipt::validate_for_supersession;
    let _ = pulse::evidence::receipt::content_source_binding_codes;
    let _ = pulse::evidence::receipt::code_to_static;

    // Facade outcome / list / summary types stay reachable at stable paths.
    let _: Option<pulse::evidence::ReceiptOutcome> = None;
    let _: Option<pulse::evidence::ReceiptList> = None;
    let _: Option<pulse::evidence::receipt::ReceiptStatus> = None;
    let _: Option<pulse::evidence::receipt::ReceiptSummary> = None;
}
