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
    pub lease_id: Option<String>,
    pub prepared_assignment_id: Option<String>,
    pub assignee: Option<String>,
    pub issued_by: Option<String>,
    pub expires_at: Option<String>,
    pub workspace_id: Option<String>,
    pub workspace_mode: Option<String>,
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
    #[serde(rename = "for")]
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

        let runtime_report = assignment_store::classify_assignment_recovery_state(&self.repo_root)
            .unwrap_or_else(|_| assignment_store::AssignmentRecoveryReport {
                entries: vec![],
                live_count: 0,
                expired_count: 0,
                tombstoned_count: 0,
                ambiguous_count: 0,
                invalid_count: 0,
                orphan_workspace_ids: vec![],
            });
        let lookup_by_subject = build_lease_lookup_by_subject(&self.repo_root, &runtime_report);

        let mut enriched_items: Vec<EnrichedFrontierItem> = Vec::new();
        for item in &report.items {
            let lookup = lookup_by_subject.get(&item.id);
            let claim_state = match lookup {
                Some(lookup) => ready_claim_state(lookup),
                None => FrontierClaimState::NotClaimed,
            };

            enriched_items.push(EnrichedFrontierItem {
                id: item.id.clone(),
                revision: item.revision,
                readiness_fingerprint: item.readiness_fingerprint.clone(),
                frontier_eligible: item.frontier_eligible,
                claim_state,
                lease_id: lookup.map(|entry| entry.lease_id.clone()),
                prepared_assignment_id: lookup
                    .and_then(|entry| entry.prepared_assignment_id.clone()),
                assignee: lookup.and_then(|entry| entry.assignee.clone()),
                expires_at: lookup.and_then(|entry| entry.expires_at.clone()),
                reason_codes: item.reason_codes.clone(),
            });
        }

        let projection = self.export_unlocked()?;
        let mut active_assignments: Vec<ActiveAssignmentEntry> = Vec::new();
        for node in &projection.nodes {
            if node.kind != WorkKind::Ticket || node.status != NodeStatus::Active {
                continue;
            }
            let entry = match lookup_by_subject.get(&node.id) {
                Some(lookup) => active_assignment_from_lookup(node, lookup),
                None => ActiveAssignmentEntry {
                    ticket_id: node.id.clone(),
                    ticket_revision: node.revision,
                    lease_id: None,
                    prepared_assignment_id: None,
                    assignee: None,
                    issued_by: None,
                    expires_at: None,
                    workspace_id: None,
                    workspace_mode: None,
                    claim_state: FrontierClaimState::Ambiguous,
                },
            };
            active_assignments.push(entry);
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

#[derive(Debug, Clone)]
struct LeaseLookup {
    lease_id: String,
    prepared_assignment_id: Option<String>,
    assignee: Option<String>,
    issued_by: Option<String>,
    expires_at: Option<String>,
    workspace_id: Option<String>,
    workspace_mode: Option<String>,
    classification: assignment_store::LeaseClassification,
}

fn build_lease_lookup_by_subject(
    repo_root: &std::path::Path,
    report: &assignment_store::AssignmentRecoveryReport,
) -> BTreeMap<String, LeaseLookup> {
    let mut by_subject = BTreeMap::new();
    for entry in &report.entries {
        if entry.subject_id.is_empty() {
            continue;
        }
        let lease = assignment_store::load_lease(repo_root, &entry.lease_id).ok();
        let workspace = lease.as_ref().and_then(|lease| {
            assignment_store::load_workspace(repo_root, &lease.workspace_id).ok()
        });
        let lookup = LeaseLookup {
            lease_id: entry.lease_id.clone(),
            prepared_assignment_id: lease
                .as_ref()
                .map(|lease| lease.prepared_assignment_id.clone()),
            assignee: lease.as_ref().map(|lease| lease.assignee.principal.clone()),
            issued_by: lease.as_ref().map(|lease| lease.issued_by.clone()),
            expires_at: lease.as_ref().map(|lease| lease.expires_at.clone()),
            workspace_id: if entry.workspace_id.is_empty() {
                lease.as_ref().map(|lease| lease.workspace_id.clone())
            } else {
                Some(entry.workspace_id.clone())
            },
            workspace_mode: workspace.map(|workspace| workspace.mode),
            classification: entry.classification.clone(),
        };
        by_subject
            .entry(entry.subject_id.clone())
            .and_modify(|existing: &mut LeaseLookup| {
                existing.classification = assignment_store::LeaseClassification::Ambiguous(
                    "multiple runtime lease entries for subject".to_string(),
                );
                existing.prepared_assignment_id = None;
                existing.assignee = None;
                existing.issued_by = None;
                existing.expires_at = None;
                existing.workspace_id = None;
                existing.workspace_mode = None;
            })
            .or_insert(lookup);
    }
    by_subject
}

fn ready_claim_state(lookup: &LeaseLookup) -> FrontierClaimState {
    match lookup.classification {
        assignment_store::LeaseClassification::Live => FrontierClaimState::BlockedByLiveLease,
        assignment_store::LeaseClassification::Expired
        | assignment_store::LeaseClassification::Tombstoned => FrontierClaimState::Stale,
        assignment_store::LeaseClassification::Ambiguous(_)
        | assignment_store::LeaseClassification::Invalid(_) => FrontierClaimState::Ambiguous,
    }
}

fn active_claim_state(lookup: &LeaseLookup) -> FrontierClaimState {
    match lookup.classification {
        assignment_store::LeaseClassification::Live => FrontierClaimState::Prepared,
        assignment_store::LeaseClassification::Expired
        | assignment_store::LeaseClassification::Tombstoned => FrontierClaimState::Stale,
        assignment_store::LeaseClassification::Ambiguous(_)
        | assignment_store::LeaseClassification::Invalid(_) => FrontierClaimState::Ambiguous,
    }
}

fn active_assignment_from_lookup(node: &Node, lookup: &LeaseLookup) -> ActiveAssignmentEntry {
    ActiveAssignmentEntry {
        ticket_id: node.id.clone(),
        ticket_revision: node.revision,
        lease_id: Some(lookup.lease_id.clone()),
        prepared_assignment_id: lookup.prepared_assignment_id.clone(),
        assignee: lookup.assignee.clone(),
        issued_by: lookup.issued_by.clone(),
        expires_at: lookup.expires_at.clone(),
        workspace_id: lookup.workspace_id.clone(),
        workspace_mode: lookup.workspace_mode.clone(),
        claim_state: active_claim_state(lookup),
    }
}
