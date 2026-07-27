use std::collections::BTreeMap;
use std::fs;

use crate::graph::contract::TicketRole;
use crate::graph::frontier::{
    self, branch_context_from_shaping, DecisionBranchContext, FrontierKind, FrontierReport,
};
use crate::graph::node::{Node, NodeStatus};
use crate::graph::projection::GraphProjection;
use crate::graph::readiness::{evaluate as evaluate_readiness, EvalProfile, ReadinessReport};
use crate::graph::store::JsonGraphStore;
use crate::id::WorkKind;
use crate::storage::transaction::recover_prepared_transactions;
use crate::storage::WriteGuard;
use crate::{PulseError, PulseResult};

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
}
