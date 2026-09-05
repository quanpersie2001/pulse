use std::collections::BTreeMap;

use crate::graph::model::contract::TicketRole;
use crate::graph::model::node::NodeStatus;
use crate::graph::read::frontier::{self, ExecutionFrontierReport};
use crate::graph::read::readiness::{evaluate as evaluate_readiness, EvalProfile, ReadinessReport};
use crate::graph::store::JsonGraphStore;
use crate::id::WorkKind;
use crate::storage::transaction::recover_prepared_transactions;
use crate::storage::WriteGuard;
use crate::{PulseError, PulseResult};

impl JsonGraphStore {
    /// Read-only deterministic execution-frontier projection
    /// (`pulse work frontier`). Never mutates state and never persists
    /// claim/lease/assignment: the report always carries
    /// `claim_state=not_evaluated`.
    ///
    /// Recovers under the repository fence and captures a coherent graph
    /// snapshot. The frontier recomputes the current readiness report for every
    /// in-scope `ready` implementation Ticket so stale-ready nodes are excluded
    /// with an explicit reason.
    pub fn frontier(
        &self,
        for_owner: Option<&str>,
        profile: Option<&str>,
        include_excluded: bool,
    ) -> PulseResult<ExecutionFrontierReport> {
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
        frontier::project_execution_frontier(
            &projection,
            for_owner,
            &reports,
            &graph_fingerprint,
            readiness_profile,
            include_excluded,
        )
    }

    /// Recompute the current readiness report for every in-scope `ready`
    /// implementation Ticket, keyed by id. The execution frontier includes only
    /// those whose current readiness passes under the requested profile.
    pub(crate) fn build_execution_readiness_reports(
        &self,
        projection: &crate::graph::read::projection::GraphProjection,
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
