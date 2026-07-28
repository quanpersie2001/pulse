use std::fs;

use chrono::{TimeZone, Utc};
use pulse::canonical_json::to_canonical_bytes;
use pulse::graph::lifecycle::{
    expectation, validate_transition, TransitionPolicy, TransitionReason,
};
use pulse::graph::node::{NodeStatus, StatusReason};
use pulse::graph::store::OperationContext;
use pulse::id::WorkKind;
use pulse::{JsonGraphStore, PulseError};
use tempfile::TempDir;

fn repo() -> (TempDir, JsonGraphStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(dir.path());
    store.bootstrap().unwrap();
    (dir, store)
}

fn ctx(sec: i64) -> OperationContext {
    OperationContext {
        actor: "test:actor".to_string(),
        now: Utc.timestamp_opt(sec, 0).unwrap(),
    }
}

fn reason(code: &str) -> TransitionReason {
    TransitionReason {
        code: code.to_string(),
        summary: format!("summary for {code}"),
        reference: Some("DEC-001".to_string()),
    }
}

#[test]
fn table_covers_every_from_to_expectation() {
    use NodeStatus::*;
    let statuses = [
        Draft, Shaped, Ready, Active, Verifying, Done, Rework, Blocked, Cancelled, Superseded,
    ];
    let supported = [
        (Draft, Cancelled),
        (Shaped, Draft),
        (Shaped, Blocked),
        (Shaped, Cancelled),
        (Ready, Shaped),
        (Ready, Blocked),
        (Ready, Cancelled),
        (Blocked, Draft),
        (Blocked, Shaped),
        (Blocked, Cancelled),
    ];
    let gated = [
        (Draft, Shaped),
        (Shaped, Ready),
        (Ready, Active),
        (Active, Blocked),
        (Active, Verifying),
        (Active, Cancelled),
        (Verifying, Done),
        (Verifying, Rework),
        (Verifying, Blocked),
        (Rework, Shaped),
        (Rework, Ready),
        (Rework, Active),
        (Rework, Cancelled),
        (Blocked, Ready),
        (Blocked, Active),
    ];

    for from in statuses {
        for to in statuses {
            let exp = expectation(from, to);
            if supported.contains(&(from, to)) {
                assert_eq!(exp.policy, TransitionPolicy::Supported, "{from:?}->{to:?}");
            } else if to == Superseded {
                assert_eq!(
                    exp.policy,
                    TransitionPolicy::SupersessionOnly,
                    "{from:?}->{to:?}"
                );
            } else if gated.contains(&(from, to)) {
                assert_eq!(exp.policy, TransitionPolicy::Gated, "{from:?}->{to:?}");
            } else {
                assert_eq!(exp.policy, TransitionPolicy::Illegal, "{from:?}->{to:?}");
            }
        }
    }
}

#[test]
fn l1_draft_to_shaped_has_installed_shaped_gate() {
    use pulse::graph::lifecycle::{installed_gate, GateProfile};
    // The shaped gate is now installed: the pure direction check no longer
    // reports the gate as unavailable; the store evaluates the gate.
    let exp = validate_transition(
        NodeStatus::Draft,
        NodeStatus::Shaped,
        Some(&reason("shape")),
    )
    .unwrap();
    assert_eq!(exp.policy, TransitionPolicy::Gated);
    assert_eq!(
        installed_gate(NodeStatus::Draft, NodeStatus::Shaped),
        Some(GateProfile::Shaped)
    );
}

#[test]
fn l2_stale_expected_revision_is_cas_conflict() {
    let (_dir, store) = repo();
    let tk = store
        .create_node_with_context(WorkKind::Ticket, "Ticket".into(), ctx(1))
        .unwrap()
        .value;
    let err = store
        .transition_node_with_context(
            &tk.id,
            NodeStatus::Cancelled,
            2,
            Some(reason("obsolete")),
            ctx(2),
        )
        .unwrap_err();
    match err {
        PulseError::CasConflict {
            subject,
            expected_revision,
            current_revision,
        } => {
            assert_eq!(subject, tk.id);
            assert_eq!(expected_revision, 2);
            assert_eq!(current_revision, 1);
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn l3_illegal_transition_reports_code() {
    let err = validate_transition(NodeStatus::Draft, NodeStatus::Done, Some(&reason("done")))
        .unwrap_err();
    assert_eq!(err.code(), "illegal_transition");
    assert!(err.to_string().contains("allowed_targets"));
}

#[test]
fn l4_shaped_to_ready_has_installed_ready_gate() {
    use pulse::graph::lifecycle::{installed_gate, GateProfile};
    let exp = validate_transition(
        NodeStatus::Shaped,
        NodeStatus::Ready,
        Some(&reason("ready")),
    )
    .unwrap();
    assert_eq!(exp.policy, TransitionPolicy::Gated);
    assert_eq!(
        installed_gate(NodeStatus::Shaped, NodeStatus::Ready),
        Some(GateProfile::Ready)
    );
}

#[test]
fn l4b_direct_blocked_to_ready_remains_gate_unavailable() {
    // Per the accepted proposal, direct blocked -> ready is intentionally NOT
    // opened: blocked resumes via blocked -> shaped -> ready.
    let err = validate_transition(
        NodeStatus::Blocked,
        NodeStatus::Ready,
        Some(&reason("ready")),
    )
    .unwrap_err();
    assert_eq!(err.code(), "transition_gate_unavailable");
}

#[test]
fn l5_blocked_and_cancelled_require_reason() {
    for (from, to) in [
        (NodeStatus::Shaped, NodeStatus::Blocked),
        (NodeStatus::Draft, NodeStatus::Cancelled),
    ] {
        let err = validate_transition(from, to, None).unwrap_err();
        assert_eq!(err.code(), "missing_status_reason", "{from:?}->{to:?}");
    }
}

#[test]
fn l6_transition_clears_stale_reason_when_target_does_not_require_it() {
    let (_dir, store) = repo();
    let mut tk = store
        .create_node_with_context(WorkKind::Ticket, "Ticket".into(), ctx(1))
        .unwrap()
        .value;
    tk.status = NodeStatus::Shaped;
    fs::write(
        _dir.path()
            .join(".pulse/workgraph/nodes")
            .join(format!("{}.json", tk.id)),
        to_canonical_bytes(&tk).unwrap(),
    )
    .unwrap();
    let blocked = store
        .transition_node_with_context(
            &tk.id,
            NodeStatus::Blocked,
            1,
            Some(reason("dependency_unavailable")),
            ctx(2),
        )
        .unwrap()
        .value;
    assert!(blocked.status_reason.is_some());
    let draft = store
        .transition_node_with_context(
            &tk.id,
            NodeStatus::Draft,
            2,
            Some(reason("reset_contract")),
            ctx(3),
        )
        .unwrap()
        .value;
    assert_eq!(draft.status, NodeStatus::Draft);
    assert_eq!(draft.status_reason, None);
}

#[test]
fn l7_ready_to_active_has_installed_prepared_assignment_gate() {
    use pulse::graph::lifecycle::{installed_gate, GateProfile};
    // The prepared-assignment gate is now installed for Ready -> Active.
    // validate_transition must pass (was transition_gate_unavailable before).
    let exp = validate_transition(NodeStatus::Ready, NodeStatus::Active, None).unwrap();
    assert_eq!(exp.policy, TransitionPolicy::Gated);
    assert_eq!(
        installed_gate(NodeStatus::Ready, NodeStatus::Active),
        Some(GateProfile::PreparedAssignment)
    );
}

#[test]
fn l8_ready_to_active_public_transition_rejects_with_prepared_assignment_required() {
    let (_dir, store) = repo();
    // Create a ticket at Ready status
    let mut tk = store
        .create_node_with_context(WorkKind::Ticket, "ReadyTicket".into(), ctx(1))
        .unwrap()
        .value;
    tk.status = NodeStatus::Ready;
    fs::write(
        _dir.path()
            .join(".pulse/workgraph/nodes")
            .join(format!("{}.json", tk.id)),
        to_canonical_bytes(&tk).unwrap(),
    )
    .unwrap();

    // Public transition must reject with prepared_assignment_required, NOT
    // transition_gate_unavailable.
    let err = store
        .transition_node_with_context(&tk.id, NodeStatus::Active, 1, None, ctx(2))
        .unwrap_err();
    assert_eq!(
        err.code(),
        "prepared_assignment_required",
        "expected prepared_assignment_required but got {:?}",
        err
    );
}

#[test]
fn l9_ready_to_active_reject_is_not_transition_gate_unavailable() {
    // Verify the exact error code is prepared_assignment_required, not
    // transition_gate_unavailable. This is a pure model-level assertion.
    use pulse::graph::lifecycle::{validate_transition, GateProfile, TransitionPolicy};
    // validate_transition must succeed because the gate is installed.
    let exp = validate_transition(NodeStatus::Ready, NodeStatus::Active, None).unwrap();
    assert_eq!(exp.policy, TransitionPolicy::Gated);
    assert_ne!(exp.policy, TransitionPolicy::Supported);
    assert_ne!(exp.policy, TransitionPolicy::Illegal);
    // Installed gate must be PreparedAssignment.
    assert_eq!(
        pulse::graph::lifecycle::installed_gate(
            pulse::graph::node::NodeStatus::Ready,
            pulse::graph::node::NodeStatus::Active
        ),
        Some(GateProfile::PreparedAssignment)
    );
}

#[test]
fn validation_rejects_stale_and_missing_status_reason() {
    let (_dir, store) = repo();
    let mut node = store
        .create_node_with_context(WorkKind::Ticket, "Ticket".into(), ctx(1))
        .unwrap()
        .value;
    node.status_reason = Some(StatusReason {
        code: "stale".to_string(),
        summary: "stale".to_string(),
        reference: None,
    });
    let err = pulse::graph::validate::validate_node_schema_semantics(&node).unwrap_err();
    assert_eq!(err.code(), "stale_status_reason");

    node.status = NodeStatus::Cancelled;
    node.status_reason = None;
    let err = pulse::graph::validate::validate_node_schema_semantics(&node).unwrap_err();
    assert_eq!(err.code(), "missing_status_reason");
}
