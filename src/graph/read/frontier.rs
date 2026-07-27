//! Deterministic decision/execution frontier projections (S7-I5).
//!
//! Frontiers are **derived membership projections**, never persisted state. They
//! consume a coherent graph snapshot (plus, for the execution frontier, current
//! readiness reports) that the graph store assembles under the repository fence,
//! and produce one stable report per kind with explainable per-item
//! inclusion/exclusion reason codes.
//!
//! Boundary rules (see `proposals/phase1-slice7-shaping-readiness-frontier.md`,
//! sections "Decision frontier" / "Execution frontier" / S7-D6):
//!
//! * frontier never mutates state and never persists claim/lease/assignment;
//! * before the Phase 2 lease resolver, `claim_state` is always
//!   `not_evaluated` — it is never fabricated as `unclaimed`;
//! * `dispatch_authorized` is always `false` — readiness is not run permission;
//! * ordering is deterministic membership by subject ID, **not** a semantic
//!   priority ranking. Priority/foundation/cost-of-delay belong to semantic
//!   reconciliation (`06-priority-reconciliation.md`), not to a hidden kernel
//!   score;
//! * the decision frontier admits precise decision-work Tickets (including
//!   `draft` work) whose hard dependencies are satisfied, without demanding a
//!   nested shaping receipt for the decision work itself;
//! * the execution frontier admits only implementation Tickets whose lifecycle
//!   status is exactly `ready` *and* whose current readiness report passes
//!   under the requested profile; a stale-ready node is excluded with an
//!   explicit reason rather than silently demoted.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::graph::contract::{validate_node_contract, ContractValidationMode, GapKind, TicketRole};
use crate::graph::executability::{
    structural_executability, BlockerResolution, StructuralExecutabilityReport, StructuralState,
};
use crate::graph::node::NodeStatus;
use crate::graph::projection::GraphProjection;
use crate::graph::readiness::{ReadinessReport, ReadinessStatus, READINESS_PROFILE};
use crate::id::WorkKind;
use crate::PulseResult;

/// Current frontier projection schema baseline (pre-release current v1).
pub const FRONTIER_SCHEMA_VERSION: u32 = 1;

/// Frontier claim state before the Phase 2 lease resolver exists. This is an
/// explicit "consumer did not evaluate" value; it must never be persisted or
/// fabricated as `unclaimed`.
pub const FRONTIER_CLAIM_STATE: &str = "not_evaluated";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontierKind {
    Decision,
    Execution,
}

/// Owned frontier report. The store dispatcher returns this enum so the CLI can
/// render the correct stable shape per kind. Not serialized directly — each
/// variant's inner report carries its own `kind` discriminator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrontierReport {
    Decision(DecisionFrontierReport),
    Execution(ExecutionFrontierReport),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecisionShapingContext {
    pub receipt_id: String,
    pub receipt_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_revision: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecisionFrontierItem {
    pub id: String,
    pub revision: u64,
    pub gap_kind: String,
    pub branch_id: String,
    pub structural_state: StructuralState,
    pub reason_codes: Vec<String>,
}

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
pub struct DecisionFrontierReport {
    pub schema_version: u32,
    pub code: String,
    pub kind: &'static str,
    #[serde(rename = "for", skip_serializing_if = "Option::is_none")]
    pub for_: Option<String>,
    pub graph_fingerprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shaping_context: Option<DecisionShapingContext>,
    pub claim_state: String,
    pub items: Vec<DecisionFrontierItem>,
    pub excluded: Vec<FrontierExcluded>,
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

/// Branch-disposal context for a destination owner's current shaping receipt,
/// assembled by the store. A decision-work Ticket's linked branch is "disposed"
/// when the current receipt records it as `resolved`/`rejected` or lists it in
/// `reconciliation.invalidated_branch_ids`.
#[derive(Debug, Clone, Default)]
pub struct DecisionBranchContext {
    pub shaping: Option<DecisionShapingContext>,
    /// Branch IDs that the current shaping receipt disposes of (resolved /
    /// rejected / invalidated). Empty when no current receipt is available.
    pub disposed_branch_ids: BTreeSet<String>,
}

impl DecisionBranchContext {
    fn available(&self) -> bool {
        self.shaping.is_some()
    }
}

/// Pure decision-frontier projection.
///
/// Iterates every decision-work Ticket in the coherent snapshot, applies the
/// `--for` destination filter, lifecycle/structural/contract/branch-disposal
/// membership rules, and returns a deterministic report. The caller (store)
/// owns all I/O: graph projection, receipt loading and branch-disposal
/// derivation.
#[allow(clippy::too_many_arguments)]
pub fn project_decision_frontier(
    projection: &GraphProjection,
    for_owner: Option<&str>,
    branch_contexts: &BTreeMap<String, DecisionBranchContext>,
    graph_fingerprint: &str,
    include_excluded: bool,
) -> PulseResult<DecisionFrontierReport> {
    let mut items: Vec<DecisionFrontierItem> = Vec::new();
    let mut excluded: Vec<FrontierExcluded> = Vec::new();

    for node in projection
        .nodes
        .iter()
        .filter(|n| n.kind == WorkKind::Ticket && n.role == Some(TicketRole::DecisionWork))
    {
        let contract = match node.decision_work.as_ref() {
            Some(contract) => contract,
            None => {
                push_excluded(
                    &mut excluded,
                    include_excluded,
                    &node.id,
                    &["decision_work_contract_missing".to_string()],
                );
                continue;
            }
        };

        // Destination-owner filter (--for). Decision work names its destination
        // owner explicitly, so we match that rather than reconstructing scope
        // from parent edges.
        if let Some(owner) = for_owner {
            if contract.destination_owner.id != owner {
                push_excluded(
                    &mut excluded,
                    include_excluded,
                    &node.id,
                    &["decision_work_wrong_destination".to_string()],
                );
                continue;
            }
        }

        // Contract validity: precise question/gap/output/evidence structurally
        // valid. Canonical-storage validation permits draft state but still
        // rejects malformed decision-work contracts.
        let contract_report =
            validate_node_contract(node, ContractValidationMode::CanonicalStorage);
        if !contract_report.valid {
            push_excluded(
                &mut excluded,
                include_excluded,
                &node.id,
                &["decision_work_contract_invalid".to_string()],
            );
            continue;
        }

        // Lifecycle: draft|shaped|ready eligible; terminal/cancelled/superseded
        // excluded. This is the specialized decision-work projection, so draft
        // is *not* treated as `work_not_shaped`.
        if is_terminal_lifecycle(node.status) {
            push_excluded(
                &mut excluded,
                include_excluded,
                &node.id,
                &[decision_terminal_code(node.status).to_string()],
            );
            continue;
        }

        // Structural executability for the decision-work candidate: hard
        // dependencies must be mechanically satisfied. We compute the structural
        // report and inspect hard blockers / supersession directly so that the
        // draft lifecycle does not falsely mark decision work as blocked.
        let structural = structural_executability(projection, &node.id).ok();
        let hard_blocker_open = structural
            .as_ref()
            .map(has_open_hard_blocker)
            .unwrap_or(true);
        let superseded = structural
            .as_ref()
            .map(|r| {
                r.supersession
                    .as_ref()
                    .and_then(|s| s.replacement.clone())
                    .is_some()
            })
            .unwrap_or(false);
        let structural_state =
            decision_structural_state(node.status, hard_blocker_open, superseded);

        if hard_blocker_open {
            push_excluded(
                &mut excluded,
                include_excluded,
                &node.id,
                &["decision_work_blocked".to_string()],
            );
            continue;
        }
        if superseded {
            push_excluded(
                &mut excluded,
                include_excluded,
                &node.id,
                &["decision_work_superseded".to_string()],
            );
            continue;
        }

        // Branch relevance: the linked branch must remain relevant in the
        // destination owner's current shaping receipt. Disposal is only checked
        // when a current receipt is available — decision work does not need a
        // nested shaping receipt merely to enter the frontier.
        let context = branch_contexts.get(&contract.destination_owner.id);
        if let Some(context) = context {
            if context.available() && context.disposed_branch_ids.contains(&contract.branch_id) {
                push_excluded(
                    &mut excluded,
                    include_excluded,
                    &node.id,
                    &["decision_work_branch_disposed".to_string()],
                );
                continue;
            }
        }

        items.push(DecisionFrontierItem {
            id: node.id.clone(),
            revision: node.revision,
            gap_kind: gap_kind_string(contract.gap_kind),
            branch_id: contract.branch_id.clone(),
            structural_state,
            reason_codes: vec!["open_decision_work".to_string()],
        });
    }

    items.sort_by(|a, b| a.id.cmp(&b.id));
    excluded.sort_by(|a, b| a.id.cmp(&b.id));

    let shaping_context = for_owner
        .and_then(|owner| branch_contexts.get(owner))
        .and_then(|context| context.shaping.clone());

    Ok(DecisionFrontierReport {
        schema_version: FRONTIER_SCHEMA_VERSION,
        code: "decision_frontier".to_string(),
        kind: "decision",
        for_: for_owner.map(str::to_string),
        graph_fingerprint: graph_fingerprint.to_string(),
        shaping_context,
        claim_state: FRONTIER_CLAIM_STATE.to_string(),
        items,
        excluded,
    })
}

/// Pure execution-frontier projection.
///
/// Iterates every implementation Ticket in scope, requires lifecycle status
/// exactly `ready`, and includes only those whose *current* readiness report
/// passes under the requested profile. A `ready` node whose current inputs no
/// longer pass is excluded as stale rather than silently demoted.
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

fn is_terminal_lifecycle(status: NodeStatus) -> bool {
    matches!(
        status,
        NodeStatus::Done | NodeStatus::Cancelled | NodeStatus::Superseded
    )
}

/// Render a `GapKind` as its stable snake_case string. `GapKind` is
/// `Copy + Serialize` (snake_case) without a `Display` impl, so we round-trip
/// through serde to avoid duplicating the variant names.
fn gap_kind_string(kind: GapKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn decision_terminal_code(status: NodeStatus) -> &'static str {
    match status {
        NodeStatus::Cancelled => "decision_work_cancelled",
        NodeStatus::Superseded => "decision_work_superseded",
        // `done`/`verifying`/etc. are not decision-frontier terminal per se, but
        // any non-eligible lifecycle that is not draft|shaped|ready is excluded.
        _ => "decision_work_terminal",
    }
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

/// Derive the structural-state label reported for a decision-work candidate.
/// Draft decision work with satisfied hard dependencies is a `candidate`; the
/// generic structural report would label draft work as `blocked`, which does
/// not apply to this specialized projection.
fn decision_structural_state(
    status: NodeStatus,
    hard_blocker_open: bool,
    superseded: bool,
) -> StructuralState {
    if is_terminal_lifecycle(status) || superseded {
        StructuralState::Terminal
    } else if hard_blocker_open {
        StructuralState::Blocked
    } else {
        StructuralState::Candidate
    }
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

/// Helper for the store dispatcher: derive branch-disposal context from a
/// destination owner's current shaping receipt snapshot.
pub fn branch_context_from_shaping(
    shaping: Option<&crate::graph::readiness::ShapingReceiptSnapshot>,
) -> DecisionBranchContext {
    let Some(shaping) = shaping else {
        return DecisionBranchContext::default();
    };
    if !shaping.integrity_valid {
        // A corrupt/ineligible current receipt cannot authoritatively dispose
        // of branches; treat the context as unavailable so decision work is not
        // silently dropped on receipt corruption.
        return DecisionBranchContext::default();
    }
    let payload = &shaping.payload;
    let shaping_ctx = DecisionShapingContext {
        receipt_id: shaping.receipt_id.clone(),
        receipt_hash: shaping.receipt_hash.clone(),
        map_revision: payload.map.as_ref().map(|m| m.revision),
    };
    let mut disposed = BTreeSet::new();
    for branch in &payload.branches {
        if matches!(
            branch.disposition,
            crate::evidence::model::BranchDisposition::Resolved { .. }
                | crate::evidence::model::BranchDisposition::Rejected { .. }
        ) {
            disposed.insert(branch.id.clone());
        }
    }
    if let Some(reconciliation) = &payload.reconciliation {
        for id in &reconciliation.invalidated_branch_ids {
            disposed.insert(id.clone());
        }
    }
    DecisionBranchContext {
        shaping: Some(shaping_ctx),
        disposed_branch_ids: disposed,
    }
}

/// Readiness profile identifier honored by the execution frontier.
pub fn execution_readiness_profile() -> &'static str {
    READINESS_PROFILE
}
