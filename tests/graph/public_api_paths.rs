//! Compile-time public-path baseline for graph and CLI-facing contracts.
//!
//! These tests intentionally exercise only the public paths that integration
//! tests, the binary and benches rely on during the source-tree refactor. They
//! are not a snapshot of every internal API.

use chrono::Utc;
use pulse::graph::model::contract::{
    ContractValidationMode, Materialization, PublicCreateClassification, QaImpactPosture, Risk,
    TicketRole,
};
use pulse::graph::model::edge::{deterministic_edge_id, Edge, EdgeType};
use pulse::graph::model::lifecycle::TransitionReason;
use pulse::graph::model::node::{DocumentationImpactPosture, NodeStatus};
use pulse::graph::read::frontier::{FRONTIER_CLAIM_STATE, FRONTIER_SCHEMA_VERSION};
use pulse::graph::read::readiness::READINESS_PROFILE;
use pulse::graph::store::OperationContext;
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

    assert_eq!(
        ContractValidationMode::CanonicalStorage,
        ContractValidationMode::CanonicalStorage
    );
    assert_eq!(
        ContractValidationMode::PublicCreate,
        ContractValidationMode::PublicCreate
    );
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
    assert_eq!(READINESS_PROFILE, "contract_readiness");
    assert_eq!(FRONTIER_CLAIM_STATE, "not_evaluated");
    assert_eq!(FRONTIER_SCHEMA_VERSION, 1);
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
