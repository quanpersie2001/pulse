use std::collections::BTreeMap;
use std::fs;

use serde::{Deserialize, Serialize};

use crate::graph::contract::TicketRole;
use crate::graph::frontier::{
    self, branch_context_from_shaping, DecisionBranchContext, FrontierKind, FrontierReport,
};
use crate::graph::node::{Node, NodeStatus};
use crate::graph::projection::GraphProjection;
use crate::graph::readiness::{evaluate as evaluate_readiness, EvalProfile, ReadinessReport};
use crate::graph::store::JsonGraphStore;
use crate::id::WorkKind;
use crate::kernel::assignment_store;
use crate::storage::transaction::recover_prepared_transactions;
use crate::storage::WriteGuard;
use crate::{PulseError, PulseResult};

// ---------------------------------------------------------------------------
// Enriched execution-frontier report (P2S2-I9 / P2S2-D2)
// ---------------------------------------------------------------------------

/// Claim-state value for a ready work item in the enriched frontier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FrontierClaimState {
    /// No lease or assignment is associated with this item.
    NotClaimed,
    /// An active prepared assignment exists.
    Prepared,
    /// A lease exists but has expired / the workspace is stale.
    Stale,
    /// A live lease exists for this item; new claim is blocked.
    BlockedByLiveLease,
    /// Runtime state is inconsistent and needs operator review.
    Ambiguous,
    /// Claim state could not be evaluated (e.g., runtime store unavailable).
    NotEvaluated,
}

/// A work item in the enriched execution frontier with joined claim state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnrichedFrontierItem {
    pub id: String,
    pub revision: u64,
    pub readiness_fingerprint: String,
    pub frontier_eligible: bool,
    pub claim_state: FrontierClaimState,
    pub lease_id: Option<String>,
    pub prepared_assignment_id: Option<String>,
    pub assignee: Option<String>,
    pub expires_at: Option<String>,
    pub reason_codes: Vec<String>,
}

/// An active assignment entry for an `active` Ticket with a matching lease.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActiveAssignmentEntry {
    pub ticket_id: String,
    pub ticket_revision: u64,
    pub lease_id: String,
    pub prepared_assignment_id: String,
    pub assignee: String,
    pub issued_by: String,
    pub expires_at: String,
    pub workspace_id: String,
    pub workspace_mode: String,
    pub claim_state: FrontierClaimState,
}

/// Enriched execution-frontier report that includes claim-state per item
/// and an `active_assignments` section for active Tickets with matching
/// runtime assignments.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnrichedExecutionFrontierReport {
    pub schema_version: u32,
    pub code: String,
    pub kind: String,
    #[serde(rename = "for", skip_serializing_if = "Option::is_none")]
    pub for_: Option<String>,
    pub graph_fingerprint: String,
    pub readiness_profile: String,
    pub items: Vec<EnrichedFrontierItem>,
    pub active_assignments: Vec<ActiveAssignmentEntry>,
    pub excluded: Vec<crate::graph::frontier::FrontierExcluded>,
}

impl JsonGraphStore {
    /// Read-only deterministic decision/execution frontier projection
    /// (`pulse work frontier`). Never mutates state and never persists
    /// claim/lease/assignment: before the Phase 2 lease resolver the report
    /// always carries `claim_state=not_evaluated`.
    ///
    /// Recovers under the repository fence and captures a coherent graph
    /// snapshot. The execution frontier additionally recomputes the current
    /// readiness report for every in-scope `ready` implementation Ticket so
    /// stale-ready nodes are excluded with an explicit reason.
    pub fn frontier(
        &self,
        kind: FrontierKind,
        for_owner: Option<&str>,
        profile: Option<&str>,
        include_excluded: bool,
    ) -> PulseResult<FrontierReport> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let projection = self.export_unlocked()?;
        let graph_fingerprint = projection.graph_fingerprint.clone();

        // Validate optional `--for` destination owner: must be an Epic or Story
        // that exists in the coherent snapshot.
        if let Some(owner) = for_owner {
            let owner_kind = crate::id::kind_for_id(owner).map_err(|_| {
                PulseError::validation(
                    "frontier_destination_invalid",
                    format!("--for must be an Epic or Story id: {owner}"),
                )
            })?;
            if !matches!(owner_kind, WorkKind::Epic | WorkKind::Story) {
                return Err(PulseError::validation(
                    "frontier_destination_invalid",
                    format!("--for must be an Epic or Story id, not {owner}"),
                ));
            }
            if !projection.nodes.iter().any(|node| node.id == owner) {
                return Err(PulseError::NotFound {
                    subject: owner.to_string(),
                });
            }
        }

        match kind {
            FrontierKind::Decision => {
                let branch_contexts = self.build_decision_branch_contexts(&projection)?;
                let report = frontier::project_decision_frontier(
                    &projection,
                    for_owner,
                    &branch_contexts,
                    &graph_fingerprint,
                    include_excluded,
                )?;
                Ok(FrontierReport::Decision(report))
            }
            FrontierKind::Execution => {
                let readiness_profile = match profile {
                    Some(profile) => profile,
                    None => frontier::execution_readiness_profile(),
                };
                if readiness_profile != frontier::execution_readiness_profile() {
                    return Err(PulseError::validation(
                        "readiness_profile_unsupported",
                        format!(
                            "unsupported readiness profile; only {} is available in this release",
                            frontier::execution_readiness_profile()
                        ),
                    ));
                }
                let reports = self.build_execution_readiness_reports(&projection, for_owner)?;
                let report = frontier::project_execution_frontier(
                    &projection,
                    for_owner,
                    &reports,
                    &graph_fingerprint,
                    readiness_profile,
                    include_excluded,
                )?;
                Ok(FrontierReport::Execution(report))
            }
        }
    }

    /// Build per-destination-owner branch-disposal context for the decision
    /// frontier from each owner's current shaping receipt snapshot.
    pub(crate) fn build_decision_branch_contexts(
        &self,
        projection: &GraphProjection,
    ) -> PulseResult<BTreeMap<String, DecisionBranchContext>> {
        let mut owners: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for node in &projection.nodes {
            if node.kind == WorkKind::Ticket && node.role == Some(TicketRole::DecisionWork) {
                if let Some(contract) = &node.decision_work {
                    owners.insert(contract.destination_owner.id.clone());
                }
            }
        }
        let mut contexts = BTreeMap::new();
        for owner in owners {
            let path = self.node_path(&owner);
            if !path.exists() {
                contexts.insert(owner, DecisionBranchContext::default());
                continue;
            }
            let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
            let node: Node =
                serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))?;
            let shaping = self.build_shaping_snapshot(&node)?;
            contexts.insert(owner, branch_context_from_shaping(shaping.as_ref()));
        }
        Ok(contexts)
    }

    /// Recompute the current readiness report for every in-scope `ready`
    /// implementation Ticket, keyed by id. The execution frontier includes only
    /// those whose current readiness passes under the requested profile.
    pub(crate) fn build_execution_readiness_reports(
        &self,
        projection: &GraphProjection,
        for_owner: Option<&str>,
    ) -> PulseResult<BTreeMap<String, ReadinessReport>> {
        let scope = for_owner.map(|owner| frontier::scope_tickets(projection, owner));
        let mut reports = BTreeMap::new();
        for node in &projection.nodes {
            if node.kind != WorkKind::Ticket
                || node.role != Some(TicketRole::Implementation)
                || node.status != NodeStatus::Ready
            {
                continue;
            }
            if let Some(scope) = &scope {
                if !scope.contains(&node.id) {
                    continue;
                }
            }
            let snapshot = self.build_readiness_snapshot(node)?;
            let inputs = snapshot.as_inputs(node);
            let report = evaluate_readiness(&inputs, EvalProfile::Ready)?;
            reports.insert(node.id.clone(), report);
        }
        Ok(reports)
    }

    /// Enriched execution frontier that wraps the pure projection with
    /// runtime claim-state per item and an `active_assignments` section
    /// for active Tickets with matching runtime assignments (P2S2-I9).
    ///
    /// Pure `graph::read` frontier remains unchanged. This wrapper
    /// joins runtime lease/workspace records after the graph projection,
    /// computing:
    /// - `claim_state` for each ready work item
    /// - `lease_id`, `assignee`, `expires_at` when a prepared lease exists
    /// - An `active_assignments` section for active+claimed Tickets
    ///
    /// An active node without a matching assignment reports as ambiguous.
    pub fn frontier_with_claim_state(
        &self,
        kind: FrontierKind,
        for_owner: Option<&str>,
        profile: Option<&str>,
        include_excluded: bool,
    ) -> PulseResult<EnrichedExecutionFrontierReport> {
        // Reuse the existing pure frontier.
        let base = self.frontier(kind, for_owner, profile, include_excluded)?;

        let report = match base {
            FrontierReport::Decision(_) => {
                return Err(PulseError::validation(
                    "frontier_claim_state_unsupported",
                    "claim-state enrichment is only available for execution frontier".to_string(),
                ));
            }
            FrontierReport::Execution(exec) => exec,
        };

        // Build lease lookup: lease_id -> (assignee, expires_at, prepared_assignment_id, state).
        let all_lease_ids = assignment_store::list_lease_ids(&self.repo_root).unwrap_or_default();
        let mut lease_by_subject: BTreeMap<String, LeaseLookup> = BTreeMap::new();
        for lid in &all_lease_ids {
            if let Ok(lease) = assignment_store::load_lease(&self.repo_root, lid) {
                let lookup = LeaseLookup {
                    lease_id: lease.lease_id.clone(),
                    prepared_assignment_id: lease.prepared_assignment_id.clone(),
                    assignee: lease.assignee.principal.clone(),
                    expires_at: lease.expires_at.clone(),
                    state: lease.state.clone(),
                };
                lease_by_subject.insert(lease.subject.id.clone(), lookup);
            }
        }

        // Check for expired/tombstoned state.
        let recovery_report =
            assignment_store::classify_assignment_recovery_state(&self.repo_root).ok();

        // Build enriched items from the pure execution items.
        let mut enriched_items: Vec<EnrichedFrontierItem> = Vec::new();
        for item in &report.items {
            let subject_id = &item.id;

            let (claim_state, lease_id, pa_id, assignee, expires_at) = match lease_by_subject
                .get(subject_id)
            {
                Some(lookup) if lookup.state == "prepared" => (
                    FrontierClaimState::Prepared,
                    Some(lookup.lease_id.clone()),
                    Some(lookup.prepared_assignment_id.clone()),
                    Some(lookup.assignee.clone()),
                    Some(lookup.expires_at.clone()),
                ),
                Some(lookup) => {
                    // Lease exists but not in prepared state (expired/stale).
                    (
                        FrontierClaimState::Stale,
                        Some(lookup.lease_id.clone()),
                        Some(lookup.prepared_assignment_id.clone()),
                        Some(lookup.assignee.clone()),
                        Some(lookup.expires_at.clone()),
                    )
                }
                None => {
                    if let Some(ref report) = recovery_report {
                        let entry = report.entries.iter().find(|e| e.subject_id == *subject_id);
                        match entry {
                            Some(entry) => {
                                let state = match &entry.classification {
                                        crate::kernel::assignment_store::LeaseClassification::Live => FrontierClaimState::Prepared,
                                        crate::kernel::assignment_store::LeaseClassification::Expired => FrontierClaimState::Stale,
                                        crate::kernel::assignment_store::LeaseClassification::Ambiguous(_) => FrontierClaimState::Ambiguous,
                                        _ => FrontierClaimState::Stale,
                                    };
                                (state, Some(entry.lease_id.clone()), None, None, None)
                            }
                            None => (FrontierClaimState::NotClaimed, None, None, None, None),
                        }
                    } else {
                        (FrontierClaimState::NotClaimed, None, None, None, None)
                    }
                }
            };

            enriched_items.push(EnrichedFrontierItem {
                id: item.id.clone(),
                revision: item.revision,
                readiness_fingerprint: item.readiness_fingerprint.clone(),
                frontier_eligible: item.frontier_eligible,
                claim_state,
                lease_id,
                prepared_assignment_id: pa_id,
                assignee,
                expires_at,
                reason_codes: item.reason_codes.clone(),
            });
        }

        // Build active_assignments section: scan active nodes, join with leases.
        let mut active_assignments: Vec<ActiveAssignmentEntry> = Vec::new();
        let projection = self.export_unlocked()?;
        for node in &projection.nodes {
            if node.status != NodeStatus::Active {
                continue;
            }
            let lookup = lease_by_subject.get(&node.id);
            match lookup {
                Some(lookup) => {
                    let ws_id = assignment_store::load_lease(&self.repo_root, &lookup.lease_id)
                        .ok()
                        .map(|l| l.workspace_id.clone())
                        .unwrap_or_default();

                    let ws_mode = assignment_store::load_workspace(&self.repo_root, &ws_id)
                        .ok()
                        .map(|ws| ws.mode)
                        .unwrap_or_default();

                    active_assignments.push(ActiveAssignmentEntry {
                        ticket_id: node.id.clone(),
                        ticket_revision: node.revision,
                        lease_id: lookup.lease_id.clone(),
                        prepared_assignment_id: lookup.prepared_assignment_id.clone(),
                        assignee: lookup.assignee.clone(),
                        issued_by: String::new(),
                        expires_at: lookup.expires_at.clone(),
                        workspace_id: ws_id,
                        workspace_mode: ws_mode,
                        claim_state: match lookup.state.as_str() {
                            "prepared" => FrontierClaimState::Prepared,
                            "expired" | "stale" => FrontierClaimState::Stale,
                            _ => FrontierClaimState::NotEvaluated,
                        },
                    });
                }
                None => {
                    // Active node without matching lease: report ambiguous.
                    active_assignments.push(ActiveAssignmentEntry {
                        ticket_id: node.id.clone(),
                        ticket_revision: node.revision,
                        lease_id: String::new(),
                        prepared_assignment_id: String::new(),
                        assignee: String::new(),
                        issued_by: String::new(),
                        expires_at: String::new(),
                        workspace_id: String::new(),
                        workspace_mode: String::new(),
                        claim_state: FrontierClaimState::Ambiguous,
                    });
                }
            }
        }
        active_assignments.sort_by(|a, b| a.ticket_id.cmp(&b.ticket_id));

        Ok(EnrichedExecutionFrontierReport {
            schema_version: 1,
            code: "enriched_execution_frontier".to_string(),
            kind: "execution".to_string(),
            for_: report.for_.clone(),
            graph_fingerprint: report.graph_fingerprint.clone(),
            readiness_profile: report.readiness_profile.clone(),
            items: enriched_items,
            active_assignments,
            excluded: report.excluded,
        })
    }
}

/// Internal lease lookup entry for frontier enrichment.
#[derive(Debug, Clone)]
struct LeaseLookup {
    lease_id: String,
    prepared_assignment_id: String,
    assignee: String,
    expires_at: String,
    state: String,
}
