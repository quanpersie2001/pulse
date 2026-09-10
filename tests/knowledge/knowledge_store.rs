use chrono::{TimeZone, Utc};
use pulse::graph::store::OperationContext as WorkCtx;
use pulse::id::WorkKind;
use pulse::knowledge::model::*;
use pulse::knowledge::relation::EndpointKind;
use pulse::knowledge::store::{KnowledgeStore, OperationContext};
use pulse::storage::transaction::{persist_intent, FileState, TransactionIntent};
use pulse::JsonGraphStore;
use serde_json::json;
use std::fs;

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
        scope: None,
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
        provenance_targets: vec![ProvenanceTargetDraft {
            relation: pulse::knowledge::relation::RelationType::DerivedFrom,
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

#[test]
fn status_on_absent_plane_is_observational_and_not_installed() {
    let repo = tempfile::tempdir().unwrap();
    let knowledge = KnowledgeStore::new(repo.path());
    let before = fs::read_dir(repo.path()).unwrap().count();

    let status = knowledge.status().unwrap();

    assert_eq!(status.code, "not_installed");
    assert_eq!(status.manifest, "not_installed");
    assert_eq!(fs::read_dir(repo.path()).unwrap().count(), before);
    assert!(!repo.path().join(".pulse").exists());
}

#[test]
fn status_refuses_to_observe_while_transaction_recovery_is_pending() {
    let (repo, _graph, knowledge, _work) = setup();
    knowledge.bootstrap().unwrap();
    let target = repo.path().join(".pulse/knowledge/entries/LRN-999.json");
    let intent = TransactionIntent::prepared(
        "evt_status_pending",
        "knowledge.test",
        "test",
        target,
        repo.path().join(".pulse/events/2026-01-01.jsonl"),
        FileState::Absent,
        FileState::Present {
            hash: "sha256:test".to_string(),
            revision: 1,
        },
        json!({"id": "evt_status_pending", "event": "pending"}),
    )
    .unwrap();
    persist_intent(repo.path(), &intent).unwrap();

    let error = knowledge.status().unwrap_err();

    assert_eq!(error.code(), "knowledge_recovery_required");
    assert!(repo
        .path()
        .join(".pulse/runtime/transactions")
        .join(format!("{}.json", intent.transaction_id))
        .exists());
}

#[test]
fn create_show_list_validate_and_export_candidate() {
    let (_repo, _graph, knowledge, work) = setup();
    let out = knowledge.create(draft(&work), ctx(10)).unwrap();
    assert_eq!(out.code, "created");
    assert_eq!(out.value.id, "LRN-001");
    assert_eq!(out.value.status, LearningStatus::Candidate);
    assert_eq!(out.value.validation.confidence, Confidence::Low);
    assert_eq!(out.relations.len(), 1);

    let shown = knowledge.show("LRN-001").unwrap();
    assert_eq!(shown.learning.id, "LRN-001");
    assert_eq!(shown.relations.len(), 1);

    let listed = knowledge
        .list(Some(LearningStatus::Candidate), None)
        .unwrap();
    assert_eq!(listed.items.len(), 1);

    let report = knowledge.validate().unwrap();
    assert!(report.valid, "{report:?}");

    let snapshot = knowledge.export().unwrap();
    assert_eq!(snapshot.counts.entries, 1);
    assert_eq!(snapshot.counts.relations, 1);
    assert_eq!(
        snapshot.eligibility.future_default_search.excluded[0].reason_codes,
        vec!["learning_candidate"]
    );
}

#[test]
fn create_rejects_missing_guidance_applicability_and_provenance() {
    let (_repo, _graph, knowledge, work) = setup();

    let mut missing_guidance = draft(&work);
    missing_guidance.guidance = Guidance::default();
    assert_eq!(
        knowledge
            .create(missing_guidance, ctx(10))
            .unwrap_err()
            .code(),
        "learning_guidance_missing"
    );

    let mut broad = draft(&work);
    broad.applicability = Applicability {
        paths: vec!["backend".to_string()],
        ..Applicability::default()
    };
    assert!(knowledge.create(broad, ctx(11)).is_ok());

    let mut no_provenance = draft(&work);
    no_provenance.provenance_targets.clear();
    assert_eq!(
        knowledge.create(no_provenance, ctx(12)).unwrap_err().code(),
        "learning_provenance_missing"
    );
}

#[test]
fn edit_uses_cas_and_relation_retry_is_idempotent() {
    let (_repo, _graph, knowledge, work) = setup();
    knowledge.create(draft(&work), ctx(10)).unwrap();

    let patch = LearningPatch {
        summary: Some("Updated concise summary.".to_string()),
        ..LearningPatch::default()
    };
    let edited = knowledge.edit("LRN-001", 1, patch, ctx(11)).unwrap();
    assert_eq!(edited.value.revision, 2);

    let stale = LearningPatch {
        title: Some("stale".to_string()),
        ..LearningPatch::default()
    };
    assert_eq!(
        knowledge
            .edit("LRN-001", 1, stale, ctx(12))
            .unwrap_err()
            .code(),
        "cas_conflict"
    );

    let rel = pulse::knowledge::store::RelationAdd {
        relation_type: pulse::knowledge::relation::RelationType::AppliedTo,
        to_kind: EndpointKind::Work,
        to: work,
        target_revision: Some(1),
        target_hash: None,
        expected_revision: 2,
    };
    let first = knowledge
        .add_relation("LRN-001", rel.clone(), ctx(13))
        .unwrap();
    assert_eq!(first.code, "created");
    let second = knowledge.add_relation("LRN-001", rel, ctx(14)).unwrap();
    assert_eq!(second.code, "unchanged");
}

#[test]
fn learning_scope_defaults_to_repository_and_round_trips() {
    let (_repo, _graph, knowledge, work_id) = setup();
    let ctx = OperationContext {
        actor: "human:test".to_string(),
        now: Utc.timestamp_opt(2, 0).unwrap(),
    };

    // No scope in the draft: repository is the default.
    let plain = knowledge
        .create(
            draft(&work_id),
            OperationContext {
                actor: "human:test".to_string(),
                now: Utc.timestamp_opt(1, 0).unwrap(),
            },
        )
        .unwrap()
        .value;
    assert_eq!(plain.scope, LearningScope::Repository);

    let mut harness_draft = draft(&work_id);
    harness_draft.scope = Some(LearningScope::Harness);
    let harness = knowledge.create(harness_draft, ctx.clone()).unwrap().value;
    assert_eq!(harness.scope, LearningScope::Harness);

    // Patch can move a learning between scopes.
    let patched = knowledge
        .edit(
            &plain.id,
            plain.revision,
            LearningPatch {
                scope: Some(LearningScope::Harness),
                ..Default::default()
            },
            ctx,
        )
        .unwrap()
        .value;
    assert_eq!(patched.scope, LearningScope::Harness);
}

#[test]
fn learning_scope_is_backward_compatible_with_scopeless_records() {
    // A pre-scope record on disk must still deserialize; the default is
    // repository.
    let value = json!({
        "schema_version": 1,
        "id": "LRN-001",
        "revision": 1,
        "title": "Old record",
        "status": "candidate",
        "kind": "failure_pattern",
        "severity": "low",
        "summary": "s",
        "guidance": {"do": [], "avoid": [], "required_checks": []},
        "applicability": {},
        "provenance": {"relation_ids": [], "source_commits": []},
        "validation": {
            "confidence": "low",
            "validated_by": [],
            "validated_at": null,
            "reproduction_count": 1,
            "contradiction_status": "none"
        },
        "routing": {
            "audiences": ["implementer"],
            "moments": ["execute"],
            "prompt_priority": "suggested",
            "max_summary_tokens": 90
        },
        "promotion": {"state": "unresolved", "rationale": null, "relation_ids": []},
        "freshness": {"review_after": null, "invalidated_by_paths": [], "version_constraints": [], "platform_constraints": []},
        "trust": {"source": "review_required", "contains_untrusted_text": false, "redaction_status": "caller_asserted"},
        "content": null,
        "created_at": "2026-09-05T00:00:00Z",
        "updated_at": "2026-09-05T00:00:00Z"
    });
    let learning: Learning = serde_json::from_value(value).unwrap();
    assert_eq!(learning.scope, LearningScope::Repository);
}

#[test]
fn post_candidate_edits_carry_administrative_fields_like_scope() {
    let (_repo, _graph, knowledge, work_id) = setup();
    let ctx = |seconds: i64| OperationContext {
        actor: "human:test".to_string(),
        now: Utc.timestamp_opt(seconds, 0).unwrap(),
    };
    // A resolvable evidence receipt backs the validate transition.
    let receipt_dir = _repo.path().join(".pulse/evidence/receipts");
    fs::create_dir_all(&receipt_dir).unwrap();
    fs::write(
        receipt_dir.join("rcpt_01J00000000000000000000001.json"),
        json!({
            "schema_version": 1,
            "receipt_version": 2,
            "id": "rcpt_01J00000000000000000000001",
            "kind": "qa_checkpoint",
            "result": "passed",
            "actor": {"kind": "human", "id": "test"},
            "recorded_at": "2026-09-06T00:00:00Z",
            "subject": {"kind": "work", "id": work_id},
            "bindings": {},
            "payload": {
                "payload_version": 1,
                "qa_scope": "ticket_checkpoint",
                "story_id": "ST-000",
                "ticket_id": work_id,
                "baseline_content_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
                "cases": [{"case_id": "QA-001", "case_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000", "outcome": "passed"}],
                "executor": {"name": "t", "version": "1"},
                "observations": ["observed"]
            }
        })
        .to_string(),
    )
    .unwrap();
    let learning = knowledge.create(draft(&work_id), ctx(1)).unwrap().value;
    let validated = knowledge
        .transition_status(
            &learning.id,
            LearningStatus::Validated,
            Some("rcpt_01J00000000000000000000001"),
            None,
            None,
            ctx(2),
        )
        .unwrap()
        .value;

    // Scope maintenance is administrative: allowed past the candidate phase.
    let patched = knowledge
        .edit(
            &validated.id,
            validated.revision,
            LearningPatch {
                scope: Some(LearningScope::Harness),
                ..Default::default()
            },
            ctx(3),
        )
        .unwrap()
        .value;
    assert_eq!(patched.scope, LearningScope::Harness);

    // The candidate freeze still holds: a candidate cannot claim a promotion.
    let fresh = knowledge.create(draft(&work_id), ctx(4)).unwrap().value;
    let error = knowledge
        .edit(
            &fresh.id,
            fresh.revision,
            LearningPatch {
                promotion: Some(Promotion {
                    state: PromotionState::Promoted,
                    rationale: None,
                    relation_ids: vec![],
                }),
                ..Default::default()
            },
            ctx(5),
        )
        .unwrap_err();
    assert_eq!(error.code(), "learning_promotion_invalid");
}
