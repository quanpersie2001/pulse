//! Deterministic execution-frontier projection.
//!
//! Frontiers are **derived membership projections**, never persisted state.
//! They consume a coherent graph snapshot (plus current readiness reports)
//! that the graph store assembles under the repository fence, and produce one
//! stable report with explainable per-item inclusion/exclusion reason codes.
//!
//! Boundary rules:
//!
//! * frontier never mutates state and never persists claim/lease/assignment;
//! * `claim_state` is always `not_evaluated` — it is never fabricated as
//!   `unclaimed`;
//! * `dispatch_authorized` is always `false` — readiness is not run permission;
//! * ordering is deterministic membership by subject ID, **not** a semantic
//!   priority ranking;
//! * the execution frontier admits only implementation Tickets whose lifecycle
//!   status is exactly `ready` *and* whose current readiness report passes
//!   under the requested profile; a stale-ready node is excluded with an
//!   explicit reason rather than silently demoted.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::graph::model::contract::TicketRole;
use crate::graph::model::node::NodeStatus;
use crate::graph::read::executability::{
    structural_executability, BlockerResolution, StructuralExecutabilityReport, StructuralState,
};
use crate::graph::read::projection::GraphProjection;
use crate::graph::read::readiness::{ReadinessReport, ReadinessStatus, READINESS_PROFILE};
use crate::id::WorkKind;
use crate::PulseResult;

/// Current frontier projection schema baseline.
pub const FRONTIER_SCHEMA_VERSION: u32 = 1;

/// Frontier claim state before the lease resolver exists. This is an explicit
/// "consumer did not evaluate" value; it must never be persisted or fabricated
/// as `unclaimed`.
pub const FRONTIER_CLAIM_STATE: &str = "not_evaluated";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionFrontierItem {
    pub id: String,
    pub revision: u64,
    pub readiness_fingerprint: String,
    /// Always `true` for included items; kept for explicit machine consumption.
    pub frontier_eligible: bool,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FrontierExcluded {
    pub id: String,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionFrontierReport {
    pub schema_version: u32,
    pub code: String,
    pub kind: &'static str,
    #[serde(rename = "for", skip_serializing_if = "Option::is_none")]
    pub for_: Option<String>,
    pub graph_fingerprint: String,
    pub readiness_profile: String,
    pub claim_state: String,
    pub dispatch_authorized: bool,
    pub items: Vec<ExecutionFrontierItem>,
    pub excluded: Vec<FrontierExcluded>,
}

/// Pure execution-frontier projection.
///
/// Iterates every implementation Ticket in scope, requires lifecycle status
/// exactly `ready`, and includes only those whose *current* readiness report
/// passes under the requested profile. A `ready` node whose current inputs no
/// longer passes is excluded as stale rather than silently demoted.
#[allow(clippy::too_many_arguments)]
pub fn project_execution_frontier(
    projection: &GraphProjection,
    for_owner: Option<&str>,
    readiness_reports: &BTreeMap<String, ReadinessReport>,
    graph_fingerprint: &str,
    readiness_profile: &str,
    include_excluded: bool,
) -> PulseResult<ExecutionFrontierReport> {
    let scope = for_owner.map(|owner| collect_scope(projection, owner));
    let mut items: Vec<ExecutionFrontierItem> = Vec::new();
    let mut excluded: Vec<FrontierExcluded> = Vec::new();

    for node in projection
        .nodes
        .iter()
        .filter(|n| n.kind == WorkKind::Ticket && n.role == Some(TicketRole::Implementation))
    {
        // Scope filter (--for): descendants reached through `parent` and
        // standalone Tickets explicitly related to the destination owner.
        if let Some(scope) = &scope {
            if !scope.contains(&node.id) {
                push_excluded(
                    &mut excluded,
                    include_excluded,
                    &node.id,
                    &["execution_wrong_destination".to_string()],
                );
                continue;
            }
        }

        // Lifecycle: exactly `ready`. A shaped Ticket with a readiness pass is a
        // readiness candidate but not in the execution frontier until an
        // explicit transition records the authority boundary.
        if node.status != NodeStatus::Ready {
            push_excluded(
                &mut excluded,
                include_excluded,
                &node.id,
                &[execution_lifecycle_code(node.status).to_string()],
            );
            continue;
        }

        // Current readiness under the requested profile. The store pre-computes
        // reports for ready implementation Tickets in scope; a missing report
        // means readiness could not be evaluated.
        let Some(report) = readiness_reports.get(&node.id) else {
            push_excluded(
                &mut excluded,
                include_excluded,
                &node.id,
                &["execution_readiness_not_evaluated".to_string()],
            );
            continue;
        };

        if report.status != ReadinessStatus::Ready {
            let code = if report.status == ReadinessStatus::Stale {
                "ready_state_stale".to_string()
            } else {
                "execution_readiness_not_ready".to_string()
            };
            push_excluded(&mut excluded, include_excluded, &node.id, &[code]);
            continue;
        }

        // Structural candidate / no hard blocker is implied by a passing
        // readiness report (structural_executability and hard-blocker families
        // must pass). Defense-in-depth: confirm directly.
        let structural = structural_executability(projection, &node.id).ok();
        let structural_state = structural
            .as_ref()
            .map(|r| r.structural_state.clone())
            .unwrap_or(StructuralState::Blocked);
        if structural
            .as_ref()
            .map(has_open_hard_blocker)
            .unwrap_or(false)
            || !matches!(structural_state, StructuralState::Candidate)
        {
            push_excluded(
                &mut excluded,
                include_excluded,
                &node.id,
                &["execution_hard_blocker".to_string()],
            );
            continue;
        }

        items.push(ExecutionFrontierItem {
            id: node.id.clone(),
            revision: node.revision,
            readiness_fingerprint: report.readiness_fingerprint.clone(),
            frontier_eligible: true,
            reason_codes: vec!["contract_ready".to_string()],
        });
    }

    items.sort_by(|a, b| a.id.cmp(&b.id));
    excluded.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(ExecutionFrontierReport {
        schema_version: FRONTIER_SCHEMA_VERSION,
        code: "execution_frontier".to_string(),
        kind: "execution",
        for_: for_owner.map(str::to_string),
        graph_fingerprint: graph_fingerprint.to_string(),
        readiness_profile: readiness_profile.to_string(),
        claim_state: FRONTIER_CLAIM_STATE.to_string(),
        dispatch_authorized: false,
        items,
        excluded,
    })
}

fn push_excluded(
    excluded: &mut Vec<FrontierExcluded>,
    include_excluded: bool,
    id: &str,
    codes: &[String],
) {
    if !include_excluded {
        return;
    }
    excluded.push(FrontierExcluded {
        id: id.to_string(),
        reason_codes: codes.to_vec(),
    });
}

fn execution_lifecycle_code(status: NodeStatus) -> &'static str {
    match status {
        NodeStatus::Shaped => "execution_not_transitioned",
        NodeStatus::Draft => "execution_not_transitioned",
        NodeStatus::Blocked => "execution_blocked",
        NodeStatus::Active | NodeStatus::Verifying | NodeStatus::Rework => "execution_in_progress",
        NodeStatus::Done | NodeStatus::Cancelled | NodeStatus::Superseded => "execution_terminal",
        NodeStatus::Ready => "execution_not_ready",
    }
}

fn has_open_hard_blocker(report: &StructuralExecutabilityReport) -> bool {
    report
        .hard_blockers
        .iter()
        .any(|b| b.resolution != BlockerResolution::Satisfied)
}

/// Collect the in-scope ticket set for a `--for` destination owner: the owner
/// itself, all descendants reached transitively through `parent` edges, and any
/// node explicitly `related` to the owner. Bounded and cycle-safe.
pub fn scope_tickets(projection: &GraphProjection, owner: &str) -> BTreeSet<String> {
    collect_scope(projection, owner)
}

fn collect_scope(projection: &GraphProjection, owner: &str) -> BTreeSet<String> {
    let mut scope = BTreeSet::new();
    scope.insert(owner.to_string());

    // Descendants via the inverse `children` index (parent edges).
    let mut queue: Vec<String> = vec![owner.to_string()];
    let mut seen: BTreeSet<String> = scope.clone();
    while let Some(current) = queue.pop() {
        if let Some(children) = projection.inverse.children.get(&current) {
            for child in children {
                if seen.insert(child.clone()) {
                    scope.insert(child.clone());
                    queue.push(child.clone());
                }
            }
        }
    }

    // Explicit `related` edges (symmetric projection).
    if let Some(related) = projection.inverse.related.get(owner) {
        for node in related {
            scope.insert(node.clone());
        }
    }

    scope
}

/// Readiness profile identifier honored by the execution frontier.
pub fn execution_readiness_profile() -> &'static str {
    READINESS_PROFILE
}
