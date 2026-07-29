//! Cross-domain compile-time public-path compatibility baseline.
//!
//! This crate locks the Rust paths that external-style integration coverage,
//! benches and `src/bin/pulse.rs` currently import across Pulse domains. It is
//! deliberately lightweight: compile and a few stable constants/constructors,
//! not exhaustive API snapshots.

use pulse::assignment::{
    AssignmentDispatch, AssignmentGateFamily, AssignmentLeaseRecordV1, AssignmentLeaseSummary,
    AssignmentLifecycle, AssignmentSubjectSnapshot, AssignmentTombstoneV1, AssignmentTransaction,
    AssignmentWorkspaceRecordV1, AssignmentWorkspaceSummary, CapabilityInventoryV1,
    CapabilityMatchReport, PreparedAssignmentRecordV1, PreparedAssignmentV1, RevalidatedSnapshot,
    ASSIGNMENT_LEASE_SCHEMA, ASSIGNMENT_SCHEMA_VERSION, ASSIGNMENT_TOMBSTONE_SCHEMA,
    ASSIGNMENT_WORKSPACE_SCHEMA, CAPABILITY_INVENTORY_SCHEMA, CAPABILITY_INVENTORY_SCHEMA_VERSION,
    CAPABILITY_MATCH_SCHEMA, CAP_MATCH_MATCHED, DEFAULT_TTL_SECONDS, DISPATCH_AUTHORIZED_STATUS,
    LEASE_KIND_IMPLEMENTATION, LEASE_SCHEMA_VERSION, LEASE_STATE_PREPARED, LIFECYCLE_GATE_PROFILE,
    LIFECYCLE_READY_TO_ACTIVE, MAX_TTL_SECONDS, MIN_TTL_SECONDS, PREPARED_ASSIGNMENT_PROFILE,
    PREPARED_ASSIGNMENT_RECORD_SCHEMA, PREPARED_ASSIGNMENT_SCHEMA, RUNNER_STATUS_NOT_STARTED,
    TOMBSTONE_SCHEMA_VERSION, TOMBSTONE_STATE_EXPIRED, TOMBSTONE_STATE_RELEASED,
    TOMBSTONE_STATE_STALE, WORKSPACE_MODE_IN_PLACE, WORKSPACE_MODE_ISOLATED,
    WORKSPACE_SCHEMA_VERSION, WORKSPACE_STATE_BOUND,
};
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
use pulse::process::{supervisor_packaging_probe, PLATFORM_SUPPORT};
use pulse::run::{runner_profile_threat_model, PUBLIC_CODEX_ADAPTER};
use pulse::source::head_commit;
use pulse::storage::transaction::{recover_prepared_transactions, TransactionFailpoint};
use pulse::storage::{bootstrap as storage_bootstrap, safe_repo_relative, MANIFEST_JSON};
use pulse::work_packet::{
    PacketBudget, PacketCapabilities, PacketDispatch, PacketKnowledge, PacketSource,
    PacketWorkspace, WorkPacketV1, BUDGET_PROFILE, MAX_CANONICAL_JSON_BYTES, PACKET_PROFILE,
    WORK_PACKET_SCHEMA,
};
use pulse::workspace::{BindingStatus, WorkspaceMode, WorkspaceStrategy};
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
    assert!(!PLATFORM_SUPPORT.is_empty());
    assert_eq!(
        runner_profile_threat_model().public_adapter,
        PUBLIC_CODEX_ADAPTER
    );
    assert_eq!(
        supervisor_packaging_probe().unwrap().hidden_command,
        "__run-supervisor"
    );
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

#[test]
fn work_packet_public_paths_compile() {
    // Verify `pulse::work_packet` public types and constants are reachable
    // from integration tests (external crate consumers).

    // Constants.
    assert_eq!(PACKET_PROFILE, "phase2_work_packet_preview_v1");
    assert_eq!(BUDGET_PROFILE, "phase2_work_packet_preview_budget_v1");
    assert_eq!(MAX_CANONICAL_JSON_BYTES, 131_072);

    // Schema & defaults.
    assert!(WORK_PACKET_SCHEMA.contains("WorkPacketV1"));

    let budget = PacketBudget::default();
    assert_eq!(budget.profile, BUDGET_PROFILE);

    let caps = PacketCapabilities {
        evaluation_status: "not_evaluated".to_string(),
        required: vec!["source.read".to_string()],
        optional: vec![],
        missing: vec![],
        inventory_identity: None,
    };
    assert!(caps.required.contains(&"source.read".to_string()));

    let knowledge = PacketKnowledge {
        status: "not_installed".to_string(),
        owner_phase: 4,
        knowledge_fingerprint: None,
        required: vec![],
        recommended: vec![],
        suggested: vec![],
        excluded: vec![],
    };
    assert_eq!(knowledge.owner_phase, 4);

    let workspace = PacketWorkspace {
        binding_status: "not_allocated".to_string(),
        workspace_id: None,
        required_strategy: "isolated_worktree_required".to_string(),
        base_repository_id: "repo".to_string(),
        base_commit: "0000000000000000000000000000000000000000".to_string(),
        requirements: vec![],
    };
    // Verify typed access.
    let _s: &str = &workspace.required_strategy;

    // Verify that all major public DTO paths compile.
    fn _accepts_packet(_: WorkPacketV1) {}
    fn _accepts_dispatch(_: PacketDispatch) {}
    fn _accepts_source(_: PacketSource) {}
    _accepts_dispatch(PacketDispatch::default());
    let dispatch = PacketDispatch::default();
    assert!(dispatch.reservation_candidate);
    assert!(!dispatch.dispatch_authorized);
    assert_eq!(dispatch.authorization_status, "not_reserved");
}

#[test]
fn assignment_public_paths_compile() {
    // Verify `pulse::assignment` public types and constants are reachable
    // from integration tests (external crate consumers).

    // Constants.
    assert_eq!(ASSIGNMENT_SCHEMA_VERSION, 1);
    assert_eq!(PREPARED_ASSIGNMENT_PROFILE, "phase2_prepared_assignment_v1");
    assert_eq!(LEASE_SCHEMA_VERSION, 1);
    assert_eq!(LEASE_KIND_IMPLEMENTATION, "implementation_assignment");
    assert_eq!(WORKSPACE_SCHEMA_VERSION, 1);
    assert_eq!(CAPABILITY_INVENTORY_SCHEMA_VERSION, 1);
    assert_eq!(DEFAULT_TTL_SECONDS, 1800);
    assert_eq!(MIN_TTL_SECONDS, 60);
    assert_eq!(MAX_TTL_SECONDS, 86_400);
    assert_eq!(LEASE_STATE_PREPARED, "prepared");
    assert_eq!(CAP_MATCH_MATCHED, "matched");
    assert_eq!(DISPATCH_AUTHORIZED_STATUS, "prepared_assignment");
    assert_eq!(RUNNER_STATUS_NOT_STARTED, "not_started");
    assert_eq!(LIFECYCLE_READY_TO_ACTIVE, "ready_to_active");
    assert_eq!(LIFECYCLE_GATE_PROFILE, "phase2_prepared_assignment_v1");
    assert_eq!(WORKSPACE_MODE_IN_PLACE, "in_place");
    assert_eq!(WORKSPACE_MODE_ISOLATED, "isolated_worktree");
    assert_eq!(WORKSPACE_STATE_BOUND, "bound");
    assert!(PREPARED_ASSIGNMENT_SCHEMA.contains("PreparedAssignmentV1"));
    assert!(PREPARED_ASSIGNMENT_RECORD_SCHEMA.contains("PreparedAssignmentRecordV1"));
    assert!(ASSIGNMENT_LEASE_SCHEMA.contains("AssignmentLeaseRecordV1"));
    assert!(ASSIGNMENT_WORKSPACE_SCHEMA.contains("AssignmentWorkspaceRecordV1"));
    assert!(CAPABILITY_INVENTORY_SCHEMA.contains("CapabilityInventoryV1"));
    assert!(CAPABILITY_MATCH_SCHEMA.contains("CapabilityMatchReport"));

    // DTO construction.
    let _subject = AssignmentSubjectSnapshot {
        id: "TK-001".to_string(),
        kind: "ticket".to_string(),
        revision_before: 1,
        revision_after: 2,
        contract_revision: 1,
        status_before: "ready".to_string(),
        status_after: "active".to_string(),
    };
    assert_eq!(_subject.id, "TK-001");

    let _snapshot = RevalidatedSnapshot {
        graph_fingerprint:
            "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        readiness_profile: "phase1_contract_readiness_v1".to_string(),
        readiness_fingerprint:
            "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
        authority_policy_fingerprint:
            "sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
        docs_registry_fingerprint:
            "sha256:3333333333333333333333333333333333333333333333333333333333333333".to_string(),
        docs_index_fingerprint:
            "sha256:4444444444444444444444444444444444444444444444444444444444444444".to_string(),
        source_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
        source_cleanliness: "clean".to_string(),
        repository_id: "repo_test".to_string(),
    };

    let _lease_summary = AssignmentLeaseSummary {
        lease_id: "lease_01Jtest".to_string(),
        state: LEASE_STATE_PREPARED.to_string(),
        assignee: "agent:codex-local".to_string(),
        issued_by: "human:test".to_string(),
        issued_at: "2026-07-28T10:00:00Z".to_string(),
        expires_at: "2026-07-28T10:30:00Z".to_string(),
        ttl_seconds: DEFAULT_TTL_SECONDS,
        exclusive: true,
    };
    assert_eq!(_lease_summary.ttl_seconds, 1800);

    let _workspace_summary = AssignmentWorkspaceSummary {
        workspace_id: "wt_test".to_string(),
        binding_status: WORKSPACE_STATE_BOUND.to_string(),
        mode: WORKSPACE_MODE_ISOLATED.to_string(),
        path: ".pulse/runtime/workspaces/wt_test".to_string(),
        repository_id: "repo_test".to_string(),
        base_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
        cleanliness: "clean".to_string(),
        owner_lease_id: "lease_01Jtest".to_string(),
    };

    let _cap_match = CapabilityMatchReport {
        inventory_identity:
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        principal: "agent:codex-local".to_string(),
        status: CAP_MATCH_MATCHED.to_string(),
        required: vec!["source.read".to_string()],
        matched: vec!["source.read".to_string()],
        missing: vec![],
        extra: vec!["test.run".to_string()],
        reason_codes: vec![],
    };

    let _lifecycle = AssignmentLifecycle {
        transition: LIFECYCLE_READY_TO_ACTIVE.to_string(),
        gate_profile: LIFECYCLE_GATE_PROFILE.to_string(),
        gate_status: "passed".to_string(),
        expected_revision: 1,
        new_revision: 2,
        event_id: "evt_01Jtest".to_string(),
    };

    let _dispatch = AssignmentDispatch {
        dispatch_authorized: true,
        authorization_status: DISPATCH_AUTHORIZED_STATUS.to_string(),
        runner_status: RUNNER_STATUS_NOT_STARTED.to_string(),
        gate_families: vec![AssignmentGateFamily {
            family: "lease".to_string(),
            status: "passed".to_string(),
            reason_codes: vec![],
        }],
    };
    assert!(_dispatch.dispatch_authorized);

    let _transaction = AssignmentTransaction::default();
    assert_eq!(_transaction.recovery_state, "complete");

    let _inventory = CapabilityInventoryV1 {
        schema_version: CAPABILITY_INVENTORY_SCHEMA_VERSION,
        principal: "agent:codex-local".to_string(),
        inventory_id: "local-default".to_string(),
        capabilities: vec!["source.read".to_string(), "source.write".to_string()],
    };
    assert_eq!(_inventory.schema_version, 1);

    let _lease_record = AssignmentLeaseRecordV1 {
        schema_version: LEASE_SCHEMA_VERSION,
        lease_id: "lease_01Jtest".to_string(),
        kind: LEASE_KIND_IMPLEMENTATION.to_string(),
        subject: pulse::assignment::AssignmentLeaseSubject {
            kind: "ticket".to_string(),
            id: "TK-001".to_string(),
            revision: 8,
            contract_revision: 4,
            status_at_claim: "ready".to_string(),
        },
        assignee: pulse::assignment::AssignmentLeaseAssignee {
            principal: "agent:codex-local".to_string(),
        },
        issued_by: "human:test".to_string(),
        issued_at: "2026-07-28T10:00:00Z".to_string(),
        expires_at: "2026-07-28T10:30:00Z".to_string(),
        ttl_seconds: DEFAULT_TTL_SECONDS,
        state: LEASE_STATE_PREPARED.to_string(),
        packet_fingerprint:
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
        readiness_fingerprint:
            "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
        workspace_id: "wt_test".to_string(),
        prepared_assignment_id: "pa_test".to_string(),
        capability_inventory_identity:
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        source: pulse::assignment::AssignmentLeaseSource {
            repository_id: "repo_test".to_string(),
            base_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
        },
    };

    let _workspace_record = AssignmentWorkspaceRecordV1 {
        schema_version: WORKSPACE_SCHEMA_VERSION,
        workspace_id: "wt_test".to_string(),
        lease_id: "lease_01Jtest".to_string(),
        prepared_assignment_id: "pa_test".to_string(),
        subject: pulse::assignment::WorkspaceSubjectRef {
            kind: "ticket".to_string(),
            id: "TK-001".to_string(),
            revision: 8,
        },
        mode: WORKSPACE_MODE_ISOLATED.to_string(),
        path: ".pulse/runtime/workspaces/wt_test".to_string(),
        repository_id: "repo_test".to_string(),
        base_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
        head_commit_at_bind: "0123456789abcdef0123456789abcdef01234567".to_string(),
        cleanliness_at_bind: "clean".to_string(),
        state: WORKSPACE_STATE_BOUND.to_string(),
        created_at: "2026-07-28T10:00:00Z".to_string(),
        released_at: None,
        cleanup: pulse::assignment::WorkspaceCleanupPolicy {
            policy: "safe_remove_if_clean_at_base".to_string(),
            status: "not_requested".to_string(),
        },
    };

    let _prepared_record = PreparedAssignmentRecordV1 {
        schema_version: ASSIGNMENT_SCHEMA_VERSION,
        profile: PREPARED_ASSIGNMENT_PROFILE.to_string(),
        code: "prepared_assignment".to_string(),
        prepared_assignment_id: "pa_test".to_string(),
        subject: _subject.clone(),
        packet_fingerprint:
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
        revalidated_snapshot: _snapshot.clone(),
        lease: _lease_summary.clone(),
        workspace: _workspace_summary.clone(),
        capability_match: _cap_match.clone(),
        lifecycle: _lifecycle.clone(),
        dispatch: _dispatch.clone(),
        transaction: _transaction.clone(),
        prepared_assignment_fingerprint: String::new(),
        reason_codes: vec![],
    };

    // Tombstone DTO checks.
    let _tombstone = AssignmentTombstoneV1 {
        schema_version: TOMBSTONE_SCHEMA_VERSION,
        lease_id: "lease_01Jtomb".to_string(),
        subject_id: "TK-001".to_string(),
        state: TOMBSTONE_STATE_RELEASED.to_string(),
        recorded_at: "2026-07-28T11:00:00Z".to_string(),
        actor: "human:test".to_string(),
        reason: None,
        reason_codes: vec![],
    };
    assert_eq!(_tombstone.schema_version, 1);
    assert_eq!(TOMBSTONE_SCHEMA_VERSION, 1);
    assert_eq!(TOMBSTONE_STATE_RELEASED, "released");
    assert_eq!(TOMBSTONE_STATE_EXPIRED, "expired");
    assert_eq!(TOMBSTONE_STATE_STALE, "stale_needs_operator");
    assert!(ASSIGNMENT_TOMBSTONE_SCHEMA.contains("AssignmentTombstoneV1"));

    // Type-acceptance checks.
    fn _accepts_prepared(_: PreparedAssignmentV1) {}
    fn _accepts_lease_record(_: AssignmentLeaseRecordV1) {}
    fn _accepts_workspace_record(_: AssignmentWorkspaceRecordV1) {}
    fn _accepts_cap_inventory(_: CapabilityInventoryV1) {}
    fn _accepts_cap_match(_: CapabilityMatchReport) {}
    fn _accepts_tombstone(_: AssignmentTombstoneV1) {}

    // Workspace mode checks.
    assert_eq!(WorkspaceMode::InPlace.as_str(), "in_place");
    assert_eq!(
        WorkspaceMode::IsolatedWorktree.as_str(),
        "isolated_worktree"
    );
    assert!(!WorkspaceStrategy::IsolatedWorktreeRequired.allows_in_place());
    assert_eq!(BindingStatus::Bound.as_str(), "bound");
}
