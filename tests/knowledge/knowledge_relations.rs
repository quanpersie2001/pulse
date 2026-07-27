use chrono::{TimeZone, Utc};
use pulse::canonical_json::to_canonical_bytes;
use pulse::docs::{
    DocsRegistry, DocumentAuthority, DocumentKind, DocumentLifecycle, DocumentRecord,
    DocumentScope, RetrievalConfig, ReviewPolicy,
};
use pulse::evidence::manifest;
use pulse::graph::store::OperationContext as WorkCtx;
use pulse::id::WorkKind;
use pulse::knowledge::model::*;
use pulse::knowledge::relation::{Endpoint, EndpointKind, KnowledgeRelation, RelationType};
use pulse::knowledge::store::{KnowledgeStore, MutationStatus, OperationContext, RelationAdd};
use pulse::storage::atomic_write;
use pulse::JsonGraphStore;

fn setup() -> (tempfile::TempDir, JsonGraphStore, KnowledgeStore, String) {
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

fn draft(work_id: &str, title: &str) -> LearningDraft {
    LearningDraft {
        title: title.to_string(),
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
        provenance_targets: vec![ProvenanceTargetDraft {
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

fn write_learning(repo: &tempfile::TempDir, learning: &Learning) {
    let path = repo
        .path()
        .join(".pulse/knowledge/entries")
        .join(format!("{}.json", learning.id));
    atomic_write(&path, &to_canonical_bytes(learning).unwrap()).unwrap();
}

fn write_relation(repo: &tempfile::TempDir, relation: &KnowledgeRelation) {
    let path = repo
        .path()
        .join(".pulse/knowledge/relations")
        .join(format!("{}.json", relation.id));
    atomic_write(&path, &to_canonical_bytes(relation).unwrap()).unwrap();
}

#[test]
fn validate_accepts_lifecycle_records_but_public_relation_mutation_rejects_them() {
    let (repo, _graph, knowledge, work) = setup();
    let created = knowledge
        .create(draft(&work, "Lifecycle learning"), ctx(10))
        .unwrap();
    let mut learning = created.value;
    learning.status = LearningStatus::Reviewed;
    learning.validation.confidence = Confidence::Medium;
    learning.routing.prompt_priority = PromptPriority::Recommended;
    learning.revision += 1;
    learning.updated_at = ctx(11).now;
    write_learning(&repo, &learning);

    let report = knowledge.validate().unwrap();
    assert!(report.valid, "{report:#?}");

    let err = knowledge
        .add_relation(
            "LRN-001",
            RelationAdd {
                relation_type: RelationType::AppliedTo,
                to_kind: EndpointKind::Work,
                to: work,
                target_revision: Some(1),
                target_hash: None,
                expected_revision: learning.revision,
            },
            ctx(12),
        )
        .unwrap_err();
    assert_eq!(err.code(), "learning_status_claim_unsupported");
}

#[test]
fn derived_from_retry_is_idempotent_with_stale_expected_revision() {
    let (_repo, graph, knowledge, work) = setup();
    knowledge
        .create(draft(&work, "Derived retry"), ctx(10))
        .unwrap();
    let other_work = graph
        .create_node_with_context(
            WorkKind::Ticket,
            "Second source".to_string(),
            WorkCtx {
                actor: "test".to_string(),
                now: Utc.timestamp_opt(2, 0).unwrap(),
            },
        )
        .unwrap()
        .value
        .id;

    let args = RelationAdd {
        relation_type: RelationType::DerivedFrom,
        to_kind: EndpointKind::Work,
        to: other_work,
        target_revision: Some(1),
        target_hash: None,
        expected_revision: 1,
    };
    let first = knowledge
        .add_relation("LRN-001", args.clone(), ctx(11))
        .unwrap();
    assert_eq!(first.status, MutationStatus::Created);
    let after_first = knowledge.show("LRN-001").unwrap().learning;
    assert_eq!(after_first.revision, 2);

    let retry = knowledge.add_relation("LRN-001", args, ctx(12)).unwrap();
    assert_eq!(retry.status, MutationStatus::Unchanged);
}

#[test]
fn same_relation_id_with_different_payload_hard_fails_before_cas() {
    let (repo, _graph, knowledge, work) = setup();
    knowledge
        .create(draft(&work, "Relation conflict"), ctx(10))
        .unwrap();
    let mut relation = KnowledgeRelation::new(
        RelationType::DerivedFrom,
        "LRN-001".to_string(),
        Endpoint {
            kind: EndpointKind::Work,
            id: work.clone(),
            revision: Some(2),
            content_hash: None,
        },
        ctx(11).now,
        "human:test".to_string(),
    )
    .unwrap();
    relation.id = format!("derived-from--LRN-001--work--{work}");
    write_relation(&repo, &relation);

    let err = knowledge
        .add_relation(
            "LRN-001",
            RelationAdd {
                relation_type: RelationType::DerivedFrom,
                to_kind: EndpointKind::Work,
                to: work,
                target_revision: Some(1),
                target_hash: None,
                expected_revision: 999,
            },
            ctx(12),
        )
        .unwrap_err();
    assert_eq!(err.code(), "knowledge_relation_conflict");
}

#[test]
fn validates_supersession_status_and_cycle_invariants() {
    let (repo, _graph, knowledge, work) = setup();
    let first = knowledge
        .create(draft(&work, "First supersession learning"), ctx(10))
        .unwrap()
        .value;
    let second = knowledge
        .create(draft(&work, "Second supersession learning"), ctx(11))
        .unwrap()
        .value;
    let mut l1 = first;
    l1.status = LearningStatus::Superseded;
    l1.revision += 1;
    write_learning(&repo, &l1);

    let missing = knowledge.validate().unwrap();
    assert!(!missing.valid);
    assert!(missing
        .errors
        .iter()
        .any(|e| e.code == "learning_supersession_mismatch"));

    let relation = KnowledgeRelation::new(
        RelationType::SupersededBy,
        l1.id.clone(),
        Endpoint {
            kind: EndpointKind::Learning,
            id: second.id.clone(),
            revision: None,
            content_hash: None,
        },
        ctx(12).now,
        "human:test".to_string(),
    )
    .unwrap();
    write_relation(&repo, &relation);
    let valid = knowledge.validate().unwrap();
    assert!(valid.valid, "{valid:#?}");

    let back = KnowledgeRelation::new(
        RelationType::SupersededBy,
        second.id.clone(),
        Endpoint {
            kind: EndpointKind::Learning,
            id: l1.id.clone(),
            revision: None,
            content_hash: None,
        },
        ctx(13).now,
        "human:test".to_string(),
    )
    .unwrap();
    write_relation(&repo, &back);
    let cyclic = knowledge.validate().unwrap();
    assert!(!cyclic.valid);
    assert!(cyclic
        .errors
        .iter()
        .any(|e| e.code == "knowledge_relation_cycle"));
    assert!(cyclic
        .errors
        .iter()
        .any(|e| e.code == "learning_supersession_mismatch"));
}

#[test]
fn validates_disputed_and_promoted_structural_invariants() {
    let (repo, graph, knowledge, work) = setup();
    let doc_id = "DOC-TEST";
    let repository_id = manifest::load(repo.path()).unwrap().repository_id;
    std::fs::create_dir_all(repo.path().join(".pulse/docs")).unwrap();
    let registry = DocsRegistry {
        schema_version: 1,
        revision: 1,
        repository_id,
        documents: vec![DocumentRecord {
            id: doc_id.to_string(),
            revision: 1,
            path: "docs/test.md".to_string(),
            kind: DocumentKind::Domain,
            authority: DocumentAuthority::Approved,
            lifecycle: DocumentLifecycle::Current,
            owner: "team:docs".to_string(),
            summary: "Test doc".to_string(),
            aliases: Vec::new(),
            scope: DocumentScope::default(),
            review_policy: ReviewPolicy::None,
            verification_profile: "domain-doc".to_string(),
            generated: None,
            superseded_by: None,
            retrieval: None,
        }],
        retrieval: Some(RetrievalConfig::defaults()),
    };
    atomic_write(
        &repo.path().join(".pulse/docs/registry.json"),
        &to_canonical_bytes(&registry).unwrap(),
    )
    .unwrap();

    let mut disputed = knowledge
        .create(draft(&work, "Disputed learning"), ctx(10))
        .unwrap()
        .value;
    disputed.status = LearningStatus::Disputed;
    disputed.validation.confidence = Confidence::Medium;
    disputed.routing.prompt_priority = PromptPriority::Recommended;
    disputed.revision += 1;
    write_learning(&repo, &disputed);
    let disputed_report = knowledge.validate().unwrap();
    assert!(!disputed_report.valid);
    assert!(disputed_report
        .errors
        .iter()
        .any(|e| e.code == "learning_dispute_mismatch"));

    disputed.validation.contradiction_status = ContradictionStatus::Suspected;
    disputed.revision += 1;
    write_learning(&repo, &disputed);
    let disputed_valid = knowledge.validate().unwrap();
    assert!(disputed_valid.valid, "{disputed_valid:#?}");

    let mut promoted = knowledge
        .create(draft(&work, "Promoted learning"), ctx(20))
        .unwrap()
        .value;
    promoted.status = LearningStatus::Promoted;
    promoted.validation.confidence = Confidence::High;
    promoted.routing.prompt_priority = PromptPriority::RequiredWhenApplicable;
    promoted.promotion.state = PromotionState::Promoted;
    promoted.promotion.rationale = Some("Captured in docs.".to_string());
    promoted.revision += 1;
    write_learning(&repo, &promoted);
    let promoted_missing = knowledge.validate().unwrap();
    assert!(!promoted_missing.valid);
    assert!(promoted_missing
        .errors
        .iter()
        .any(|e| e.code == "learning_promotion_mismatch"));

    let relation = KnowledgeRelation::new(
        RelationType::PromotedTo,
        promoted.id.clone(),
        Endpoint {
            kind: EndpointKind::Document,
            id: doc_id.to_string(),
            revision: None,
            content_hash: None,
        },
        ctx(21).now,
        "human:test".to_string(),
    )
    .unwrap();
    promoted.promotion.relation_ids = vec![relation.id.clone()];
    promoted.revision += 1;
    write_learning(&repo, &promoted);
    write_relation(&repo, &relation);
    let promoted_valid = knowledge.validate().unwrap();
    assert!(promoted_valid.valid, "{promoted_valid:#?}");

    drop(graph);
}
