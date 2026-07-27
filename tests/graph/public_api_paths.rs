//! Compile-time public-path baseline for graph and CLI-facing contracts.
//!
//! These tests intentionally exercise only the public paths that integration
//! tests, the binary and benches rely on during the source-tree refactor. They
//! are not a snapshot of every internal API.

use chrono::Utc;
use pulse::graph::contract::{
    ContractValidationMode, ImplementationMode, Materialization, PlanPolicy,
    PublicCreateClassification, QaImpactPosture, Risk, TicketRole, WorkSurface,
};
use pulse::graph::edge::{deterministic_edge_id, Edge, EdgeType};
use pulse::graph::frontier::{FrontierKind, FRONTIER_CLAIM_STATE};
use pulse::graph::lifecycle::TransitionReason;
use pulse::graph::node::{DocumentationImpactPosture, NodeStatus};
use pulse::graph::readiness::READINESS_PROFILE;
use pulse::graph::store::{ContractSetRequest, OperationContext, QaImpactUpdate};
use pulse::id::{format_id, WorkId, WorkKind};
use pulse::JsonGraphStore;

#[test]
fn graph_public_paths_used_by_tests_and_binary_compile() {
    let repo = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(repo.path());
    store.bootstrap().unwrap();

    let outcome = store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Public API baseline".to_string(),
            PublicCreateClassification {
                role: Some(TicketRole::Implementation),
                risk: Some(Risk::Low),
                materialization: Some(Materialization::R0),
            },
            OperationContext {
                actor: "human:test".to_string(),
                now: Utc::now(),
            },
        )
        .unwrap();

    assert_eq!(outcome.value.kind, WorkKind::Ticket);
    assert_eq!(outcome.value.status, NodeStatus::Draft);
    assert_eq!(outcome.value.role, Some(TicketRole::Implementation));
    assert_eq!(format_id(WorkKind::Ticket, 7), "TK-007");
    assert_eq!(
        WorkId::new(&outcome.value.id).unwrap().kind().unwrap(),
        WorkKind::Ticket
    );

    let _request = ContractSetRequest {
        role: TicketRole::Implementation,
        implementation: None,
        decision_work: None,
    };
    let _qa_update = QaImpactUpdate {
        posture: QaImpactPosture::None,
        rationale: Some("internal refactor only".to_string()),
        behavioral_owner: None,
        affected_case_ids: vec![],
    };

    assert_eq!(
        ContractValidationMode::Completeness,
        ContractValidationMode::Completeness
    );
    assert_eq!(ImplementationMode::Guided, ImplementationMode::Guided);
    assert_eq!(WorkSurface::Code, WorkSurface::Code);
    assert_eq!(PlanPolicy::WorkerOptional, PlanPolicy::WorkerOptional);
    assert_eq!(
        DocumentationImpactPosture::None,
        DocumentationImpactPosture::None
    );
    assert_eq!(QaImpactPosture::None, QaImpactPosture::None);
    let reason = TransitionReason {
        code: "baseline".to_string(),
        summary: "baseline transition reason".to_string(),
        reference: None,
    };
    assert_eq!(reason.into_status_reason().code, "baseline");
    assert_eq!(READINESS_PROFILE, "phase1_contract_readiness_v1");
    assert_eq!(FRONTIER_CLAIM_STATE, "not_evaluated");
    assert!(matches!(FrontierKind::Execution, FrontierKind::Execution));
}

#[test]
fn edge_public_paths_preserve_deterministic_ids() {
    let edge = Edge::new(
        EdgeType::BlockedBy,
        "TK-002".to_string(),
        "TK-001".to_string(),
        "human:test".to_string(),
        Utc::now(),
    )
    .unwrap();

    assert_eq!(
        edge.id,
        deterministic_edge_id(EdgeType::BlockedBy, "TK-002", "TK-001")
    );
    assert_eq!(EdgeType::PreferredAfter.slug(), "preferred-after");
}
