use chrono::{TimeZone, Utc};
use pulse::graph::edge::{Edge, EdgeType};
use pulse::graph::executability::{structural_executability, BlockerResolution, StructuralState};
use pulse::graph::node::{Node, NodeStatus};
use pulse::graph::projection::{GraphProjection, InverseIndexes, LifecycleProjection};
use pulse::graph::rollup::{rollup, CompletionClaim};
use pulse::graph::traversal::{affected_by, neighborhood};
use pulse::id::WorkKind;

fn node(id: &str, kind: WorkKind, status: NodeStatus) -> Node {
    let now = Utc.timestamp_opt(1, 0).unwrap();
    let mut node = Node::new(id.to_string(), kind, id.to_string(), now).unwrap();
    node.status = status;
    node
}

fn edge(edge_type: EdgeType, from: &str, to: &str) -> Edge {
    Edge::new(
        edge_type,
        from.to_string(),
        to.to_string(),
        "test".to_string(),
        Utc.timestamp_opt(2, 0).unwrap(),
    )
    .unwrap()
}

fn projection(nodes: Vec<Node>, edges: Vec<Edge>) -> GraphProjection {
    let mut nodes = nodes;
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    let mut edges = edges;
    edges.sort_by(|a, b| a.id.cmp(&b.id));
    GraphProjection {
        schema_version: 2,
        graph_fingerprint: "sha256:test".to_string(),
        nodes,
        edges,
        inverse: InverseIndexes::default(),
        lifecycle: LifecycleProjection::default(),
    }
}

#[test]
fn l7_open_hard_blocker_blocks_ticket_with_path() {
    let graph = projection(
        vec![
            node("TK-001", WorkKind::Ticket, NodeStatus::Shaped),
            node("TK-002", WorkKind::Ticket, NodeStatus::Ready),
        ],
        vec![edge(EdgeType::BlockedBy, "TK-001", "TK-002")],
    );

    let report = structural_executability(&graph, "TK-001").unwrap();

    assert_eq!(report.structural_state, StructuralState::Blocked);
    assert_eq!(report.hard_blockers[0].id, "TK-002");
    assert_eq!(
        report.hard_blockers[0].resolution,
        BlockerResolution::Unsatisfied
    );
    assert_eq!(report.hard_blockers[0].path, vec!["TK-001", "TK-002"]);
    assert!(report
        .reason_codes
        .contains(&"hard_blocker_open".to_string()));
    assert!(!report.dispatch_authorized);
}

#[test]
fn l8_done_hard_blocker_is_satisfied_candidate() {
    let graph = projection(
        vec![
            node("TK-001", WorkKind::Ticket, NodeStatus::Shaped),
            node("TK-002", WorkKind::Ticket, NodeStatus::Done),
        ],
        vec![edge(EdgeType::BlockedBy, "TK-001", "TK-002")],
    );

    let report = structural_executability(&graph, "TK-001").unwrap();

    assert_eq!(report.structural_state, StructuralState::Candidate);
    assert_eq!(
        report.hard_blockers[0].resolution,
        BlockerResolution::Satisfied
    );
    assert_eq!(report.hard_blockers[0].resolution_basis, "terminal_done");
}

#[test]
fn l9_soft_preference_is_advisory_only() {
    let graph = projection(
        vec![
            node("TK-001", WorkKind::Ticket, NodeStatus::Shaped),
            node("TK-002", WorkKind::Ticket, NodeStatus::Draft),
        ],
        vec![edge(EdgeType::PreferredAfter, "TK-001", "TK-002")],
    );

    let report = structural_executability(&graph, "TK-001").unwrap();

    assert_eq!(report.structural_state, StructuralState::Candidate);
    assert_eq!(report.soft_preferences[0].preferred_after, "TK-002");
    assert_eq!(report.soft_preferences[0].resolution_basis, "advisory_only");
}

#[test]
fn l10_l11_l12_superseded_and_cancelled_blocker_resolution() {
    let open_chain = projection(
        vec![
            node("TK-001", WorkKind::Ticket, NodeStatus::Shaped),
            node("TK-002", WorkKind::Ticket, NodeStatus::Superseded),
            node("TK-003", WorkKind::Ticket, NodeStatus::Ready),
        ],
        vec![
            edge(EdgeType::BlockedBy, "TK-001", "TK-002"),
            edge(EdgeType::SupersededBy, "TK-002", "TK-003"),
        ],
    );
    let report = structural_executability(&open_chain, "TK-001").unwrap();
    assert_eq!(
        report.hard_blockers[0].resolution,
        BlockerResolution::Unsatisfied
    );
    assert_eq!(
        report.hard_blockers[0].resolution_basis,
        "superseded_chain_open"
    );

    let done_chain = projection(
        vec![
            node("TK-001", WorkKind::Ticket, NodeStatus::Shaped),
            node("TK-002", WorkKind::Ticket, NodeStatus::Superseded),
            node("TK-003", WorkKind::Ticket, NodeStatus::Done),
        ],
        vec![
            edge(EdgeType::BlockedBy, "TK-001", "TK-002"),
            edge(EdgeType::SupersededBy, "TK-002", "TK-003"),
        ],
    );
    let report = structural_executability(&done_chain, "TK-001").unwrap();
    assert_eq!(report.structural_state, StructuralState::Candidate);
    assert_eq!(
        report.hard_blockers[0].resolution,
        BlockerResolution::Satisfied
    );
    assert_eq!(
        report.hard_blockers[0].path,
        vec!["TK-001", "TK-002", "TK-003"]
    );

    let cancelled = projection(
        vec![
            node("TK-001", WorkKind::Ticket, NodeStatus::Shaped),
            node("TK-002", WorkKind::Ticket, NodeStatus::Cancelled),
        ],
        vec![edge(EdgeType::BlockedBy, "TK-001", "TK-002")],
    );
    let report = structural_executability(&cancelled, "TK-001").unwrap();
    assert_eq!(
        report.hard_blockers[0].resolution,
        BlockerResolution::Unsatisfied
    );
    assert_eq!(
        report.hard_blockers[0].resolution_basis,
        "terminal_cancelled"
    );
}

#[test]
fn l23_rollup_counts_statuses_and_terminal_outcomes_without_completion_claim() {
    let graph = projection(
        vec![
            node("EP-001", WorkKind::Epic, NodeStatus::Shaped),
            node("ST-001", WorkKind::Story, NodeStatus::Shaped),
            node("TK-001", WorkKind::Ticket, NodeStatus::Done),
            node("TK-002", WorkKind::Ticket, NodeStatus::Superseded),
            node("TK-003", WorkKind::Ticket, NodeStatus::Draft),
            node("TK-004", WorkKind::Ticket, NodeStatus::Ready),
        ],
        vec![
            edge(EdgeType::Parent, "ST-001", "EP-001"),
            edge(EdgeType::Parent, "TK-001", "ST-001"),
            edge(EdgeType::Parent, "TK-002", "ST-001"),
            edge(EdgeType::Parent, "TK-003", "ST-001"),
            edge(EdgeType::Parent, "TK-004", "ST-001"),
            edge(EdgeType::BlockedBy, "TK-004", "TK-003"),
        ],
    );

    let report = rollup(&graph, "ST-001").unwrap();

    assert_eq!(report.direct_children, 4);
    assert_eq!(report.descendant_tickets, 4);
    assert_eq!(report.by_status.get(&NodeStatus::Done), Some(&1));
    assert_eq!(report.by_status.get(&NodeStatus::Superseded), Some(&1));
    assert_eq!(report.terminal_outcomes.done, 1);
    assert_eq!(report.terminal_outcomes.superseded, 1);
    assert_eq!(report.open_hard_blockers, vec!["TK-003"]);
    assert_eq!(report.completion_claim, CompletionClaim::NotEvaluated);
}

#[test]
fn l24_rollup_detects_hierarchy_cycle() {
    let graph = projection(
        vec![
            node("ST-001", WorkKind::Story, NodeStatus::Shaped),
            node("ST-002", WorkKind::Story, NodeStatus::Shaped),
        ],
        vec![
            edge(EdgeType::Parent, "ST-001", "ST-002"),
            edge(EdgeType::Parent, "ST-002", "ST-001"),
        ],
    );

    let err = rollup(&graph, "ST-001").unwrap_err();
    assert_eq!(err.code(), "hierarchy_cycle");
}

#[test]
fn l25_neighborhood_is_bounded_sorted_and_cycle_safe() {
    let graph = projection(
        vec![
            node("TK-001", WorkKind::Ticket, NodeStatus::Shaped),
            node("TK-002", WorkKind::Ticket, NodeStatus::Ready),
            node("TK-003", WorkKind::Ticket, NodeStatus::Ready),
        ],
        vec![
            edge(EdgeType::Related, "TK-001", "TK-002"),
            edge(EdgeType::Related, "TK-002", "TK-003"),
        ],
    );

    let report = neighborhood(&graph, "TK-001", 2).unwrap();

    assert_eq!(report.nodes, vec!["TK-001", "TK-002", "TK-003"]);
    assert_eq!(report.depth, 2);
    assert!(!report.truncated);
    assert!(report
        .edges
        .iter()
        .all(|edge| edge.path.first().unwrap() == "TK-001"));
}

#[test]
fn l26_affected_by_separates_hard_rollup_supersession_and_advisory() {
    let graph = projection(
        vec![
            node("ST-001", WorkKind::Story, NodeStatus::Shaped),
            node("TK-001", WorkKind::Ticket, NodeStatus::Done),
            node("TK-002", WorkKind::Ticket, NodeStatus::Ready),
            node("TK-003", WorkKind::Ticket, NodeStatus::Ready),
            node("TK-004", WorkKind::Ticket, NodeStatus::Superseded),
        ],
        vec![
            edge(EdgeType::Parent, "TK-001", "ST-001"),
            edge(EdgeType::BlockedBy, "TK-002", "TK-001"),
            edge(EdgeType::PreferredAfter, "TK-003", "TK-001"),
            edge(EdgeType::SupersededBy, "TK-004", "TK-001"),
        ],
    );

    let report = affected_by(&graph, "TK-001", None).unwrap();

    assert_eq!(report.hard[0].id, "TK-002");
    assert_eq!(report.rollup[0].id, "ST-001");
    assert_eq!(report.supersession[0].id, "TK-004");
    assert_eq!(report.advisory[0].id, "TK-003");
}

#[test]
fn l28_l29_l30_executability_boundary_states() {
    let non_ticket = projection(
        vec![node("ST-001", WorkKind::Story, NodeStatus::Shaped)],
        vec![],
    );
    assert_eq!(
        structural_executability(&non_ticket, "ST-001")
            .unwrap()
            .structural_state,
        StructuralState::NotExecutableKind
    );

    let draft = projection(
        vec![node("TK-001", WorkKind::Ticket, NodeStatus::Draft)],
        vec![],
    );
    let report = structural_executability(&draft, "TK-001").unwrap();
    assert_eq!(report.structural_state, StructuralState::Blocked);
    assert!(report.reason_codes.contains(&"work_not_shaped".to_string()));
    assert!(!report.dispatch_authorized);

    let shaped = projection(
        vec![node("TK-001", WorkKind::Ticket, NodeStatus::Shaped)],
        vec![],
    );
    let report = structural_executability(&shaped, "TK-001").unwrap();
    assert_eq!(report.structural_state, StructuralState::Candidate);
    assert!(!report.dispatch_authorized);
    assert!(report
        .missing_gate_families
        .contains(&"implementation_contract".to_string()));
}
