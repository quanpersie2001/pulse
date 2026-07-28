use std::fs;

use chrono::Utc;
use serde_json::json;

use crate::assignment::{
    AssignmentLeaseRecordV1, AssignmentWorkspaceRecordV1, PreparedAssignmentRecordV1,
    CAP_MATCH_MATCHED, DISPATCH_AUTHORIZED_STATUS, GATE_STATUS_PASSED, LEASE_STATE_PREPARED,
    LIFECYCLE_GATE_PROFILE, LIFECYCLE_READY_TO_ACTIVE, PREPARED_ASSIGNMENT_PROFILE,
    WORKSPACE_STATE_BOUND,
};
use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::graph::lifecycle::{
    installed_gate, status_requires_reason, validate_transition, GateProfile, TransitionReason,
};
use crate::graph::node::{Node, NodeStatus};
use crate::graph::readiness::{evaluate as evaluate_readiness, EvalProfile};
use crate::graph::store::{JsonGraphStore, MutationOutcome, MutationStatus, OperationContext};
use crate::graph::validate::validate_graph;
use crate::storage::transaction::{recover_prepared_transactions, FileState};
use crate::storage::WriteGuard;
use crate::{PulseError, PulseResult};

impl JsonGraphStore {
    pub fn transition_node_with_context(
        &self,
        id: &str,
        to: crate::graph::node::NodeStatus,
        expected_revision: u64,
        reason: Option<TransitionReason>,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        self.transition_node_gated_with_context(id, to, expected_revision, reason, None, ctx)
    }

    /// Transition a node, evaluating the installed gate (if any) under the held
    /// repository fence. `expected_readiness_fingerprint` lets strict callers
    /// (automation/orchestrator) guard against acting on a stale readiness
    /// query; interactive callers may pass `None`.
    pub fn transition_node_gated_with_context(
        &self,
        id: &str,
        to: crate::graph::node::NodeStatus,
        expected_revision: u64,
        reason: Option<TransitionReason>,
        expected_readiness_fingerprint: Option<&str>,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let path = self.node_path(id);
        if !path.exists() {
            return Err(PulseError::NotFound {
                subject: id.to_string(),
            });
        }
        let before_bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        let mut node: Node = serde_json::from_slice(&before_bytes)
            .map_err(|error| PulseError::json(&path, error))?;
        if node.revision != expected_revision {
            return Err(PulseError::CasConflict {
                subject: id.to_string(),
                expected_revision,
                current_revision: node.revision,
            });
        }
        let from = node.status;
        let exp = validate_transition(from, to, reason.as_ref())?;
        // Evaluate the installed gate (if any) under the held fence on the
        // current pre-transition node, before any status mutation. The gate
        // result (profile + evaluated fingerprint) is embedded in the committed
        // event; recovery replays the prepared result rather than recomputing.
        let gate_outcome =
            self.evaluate_transition_gate(&node, from, to, &ctx, expected_readiness_fingerprint)?;
        let transition_reason = reason.clone();
        node.status = to;
        node.status_reason = if status_requires_reason(to) {
            Some(
                reason
                    .clone()
                    .ok_or_else(|| {
                        PulseError::validation(
                            "missing_status_reason",
                            "transition requires a non-empty reason",
                        )
                    })?
                    .into_status_reason(),
            )
        } else {
            None
        };
        node.revision += 1;
        node.updated_at = ctx.now;
        let node_values = self
            .load_nodes_with_override(node.clone())?
            .into_values()
            .collect::<Vec<_>>();
        let edge_values = self
            .load_edges()?
            .iter()
            .map(|(_, e)| e.clone())
            .collect::<Vec<_>>();
        validate_graph(
            &self.repo_root,
            &self.manifest()?,
            &node_values,
            &edge_values,
        )
        .into_result()?;
        let graph_fingerprint_before = self.graph_fingerprint_current_unlocked()?;
        let graph_fingerprint_after = self.graph_fingerprint_with_planned_workgraph(&node, None)?;
        let after_bytes = to_canonical_bytes(&node)?;
        let mut payload = json!({
            "from": from,
            "to": to,
            "expected_revision": expected_revision,
            "reason": transition_reason,
            "graph_fingerprint_before": graph_fingerprint_before,
            "graph_fingerprint_after": graph_fingerprint_after,
            "gate_coverage": gate_coverage_for(from, to, gate_outcome.as_ref()),
            "target_requires_status_reason": exp.target_requires_status_reason,
        });
        if let Some(gate) = &gate_outcome {
            payload["gate_profile"] = json!(gate.profile);
            payload["input_fingerprint"] = json!(gate.fingerprint);
            payload["gate_status"] = json!(gate.status);
            if let Some(shaping) = &gate.shaping_receipt {
                payload["shaping_receipt"] = json!(shaping);
            }
        }
        self.commit_mutation(
            "work.node.transitioned",
            ctx.actor,
            id,
            payload,
            &path,
            FileState::Present {
                hash: hash_bytes(&before_bytes),
                revision: expected_revision,
            },
            FileState::Present {
                hash: hash_bytes(&after_bytes),
                revision: expected_revision + 1,
            },
            &after_bytes,
            ctx.now,
        )?;
        Ok(MutationOutcome {
            schema_version: 1,
            code: "transitioned".to_string(),
            status: MutationStatus::Updated,
            value: node,
        })
    }

    pub fn transition_node(
        &self,
        id: &str,
        to: crate::graph::node::NodeStatus,
        expected_revision: u64,
        reason: Option<TransitionReason>,
        actor: String,
    ) -> PulseResult<MutationOutcome<Node>> {
        self.transition_node_with_context(
            id,
            to,
            expected_revision,
            reason,
            OperationContext {
                actor,
                now: Utc::now(),
            },
        )
    }

    /// Evaluate the installed gate for a transition direction on the current
    /// node. Returns `None` when no gate is installed (ordinary supported
    /// direction); otherwise evaluates the readiness/shaping gate under the
    /// held fence and embeds the result in the committed event.
    ///
    /// Authority is default-deny: the transition caller must hold the
    /// direction-specific grant (`work.transition.shaped` /
    /// `work.transition.ready`) in addition to the readiness `authority` family
    /// which checks policy availability and the shaping approver grant.
    ///
    /// The `PreparedAssignment` gate rejects all generic/public callers with
    /// `prepared_assignment_required` because the assignment-runtime context
    /// needed to evaluate it is unavailable on the generic transition path.
    /// Use [`evaluate_prepared_assignment_gate_for_active`] for the internal
    /// kernel-owned evaluation seam.
    fn evaluate_transition_gate(
        &self,
        node: &Node,
        from: NodeStatus,
        to: NodeStatus,
        ctx: &OperationContext,
        expected_readiness_fingerprint: Option<&str>,
    ) -> PulseResult<Option<GateEvaluationOutcome>> {
        let Some(profile_kind) = installed_gate(from, to) else {
            return Ok(None);
        };

        // The PreparedAssignment gate requires assignment-runtime context
        // that the generic transition path cannot supply. No user-supplied
        // proof bypass is accepted. Only the internal seam evaluates this
        // gate.
        if matches!(profile_kind, GateProfile::PreparedAssignment) {
            return Err(PulseError::validation(
                "prepared_assignment_required",
                format!(
                    "transition {} -> {:?} requires a prepared assignment produced by the claim runtime",
                    node.id, to,
                ),
            ));
        }

        let policy_report = crate::policy::load_authority_policy(&self.repo_root)?;
        let caller = crate::policy::parse_actor(&ctx.actor);
        let grant = match profile_kind {
            GateProfile::Shaped => "work.transition.shaped",
            GateProfile::Ready => "work.transition.ready",
            GateProfile::PreparedAssignment => unreachable!("handled above"),
        };
        crate::policy::authorize(&policy_report, &caller, &[grant])?;

        let eval_profile = match profile_kind {
            GateProfile::Shaped => EvalProfile::Shaped,
            GateProfile::Ready => EvalProfile::Ready,
            GateProfile::PreparedAssignment => unreachable!("handled above"),
        };
        let snapshot = self.build_readiness_snapshot(node)?;
        let inputs = snapshot.as_inputs(node);
        let report = evaluate_readiness(&inputs, eval_profile)?;

        if !report.transition_eligible {
            return Err(PulseError::validation(
                "readiness_not_ready",
                format!(
                    "transition {} -> {:?} requires a passing {} gate; status={}, reason_codes={:?}",
                    node.id,
                    to,
                    report.profile,
                    report.status_as_word(),
                    report.reason_codes
                ),
            ));
        }

        if let Some(expected) = expected_readiness_fingerprint {
            if expected != report.readiness_fingerprint {
                return Err(PulseError::validation(
                    "readiness_fingerprint_mismatch",
                    "expected readiness fingerprint no longer matches current inputs; reload and retry",
                ));
            }
        }

        let shaping_receipt = snapshot.shaping.as_ref().map(
            |shaping| serde_json::json!({"id": shaping.receipt_id, "hash": shaping.receipt_hash}),
        );

        Ok(Some(GateEvaluationOutcome {
            profile: profile_kind.as_str().to_string(),
            fingerprint: report.readiness_fingerprint.clone(),
            status: report.status_as_word().to_string(),
            shaping_receipt,
        }))
    }

    /// Internal prepared-assignment gate evaluation and mutation planner (P2S2-I7).
    ///
    /// Claim holds the repository fence before calling this method. This method
    /// deliberately does not acquire `WriteGuard`, does not recover prepared
    /// transactions, and does not call `commit_mutation`; it validates the
    /// already assembled claim records and returns the node bytes/fingerprints
    /// that the claim transaction will commit atomically with the runtime
    /// records and assignment event.
    #[allow(dead_code)]
    pub(crate) fn evaluate_prepared_assignment_gate_for_active(
        &self,
        id: &str,
        expected_revision: u64,
        gate_ctx: PreparedAssignmentGateContext<'_>,
    ) -> PulseResult<PreparedAssignmentGatePlan> {
        let path = self.node_path(id);
        if !path.exists() {
            return Err(PulseError::NotFound {
                subject: id.to_string(),
            });
        }
        let before_bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        let mut node: Node = serde_json::from_slice(&before_bytes)
            .map_err(|error| PulseError::json(&path, error))?;

        if node.revision != expected_revision {
            return Err(PulseError::CasConflict {
                subject: id.to_string(),
                expected_revision,
                current_revision: node.revision,
            });
        }

        if node.status != NodeStatus::Ready {
            return Err(PulseError::validation(
                "invalid_status",
                format!(
                    "prepared-assignment gate requires Ready status, but {} is {:?}",
                    node.id, node.status
                ),
            ));
        }

        // Validate the transition direction is legal and the prepared-assignment
        // gate is installed in the lifecycle model without invoking the generic
        // transition evaluator's public rejection path.
        let _exp = validate_transition(NodeStatus::Ready, NodeStatus::Active, None)?;
        debug_assert_eq!(
            installed_gate(NodeStatus::Ready, NodeStatus::Active),
            Some(GateProfile::PreparedAssignment)
        );

        let graph_fingerprint_before = self.graph_fingerprint_current_unlocked()?;
        validate_prepared_assignment_gate_context(
            &node,
            expected_revision,
            &graph_fingerprint_before,
            &gate_ctx,
        )?;

        node.status = NodeStatus::Active;
        node.status_reason = None;
        node.revision += 1;
        node.updated_at = gate_ctx.now;

        let node_values = self
            .load_nodes_with_override(node.clone())?
            .into_values()
            .collect::<Vec<_>>();
        let edge_values = self
            .load_edges()?
            .iter()
            .map(|(_, e)| e.clone())
            .collect::<Vec<_>>();
        validate_graph(
            &self.repo_root,
            &self.manifest()?,
            &node_values,
            &edge_values,
        )
        .into_result()?;

        let graph_fingerprint_after = self.graph_fingerprint_with_planned_workgraph(&node, None)?;
        let after_bytes = to_canonical_bytes(&node)?;
        Ok(PreparedAssignmentGatePlan {
            node,
            after_bytes,
            before_node_hash: hash_bytes(&before_bytes),
            graph_fingerprint_before,
            graph_fingerprint_after,
        })
    }
}

#[allow(dead_code)]
pub(crate) struct PreparedAssignmentGateContext<'a> {
    pub now: chrono::DateTime<Utc>,
    pub prepared: &'a PreparedAssignmentRecordV1,
    pub lease: &'a AssignmentLeaseRecordV1,
    pub workspace: &'a AssignmentWorkspaceRecordV1,
}

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct PreparedAssignmentGatePlan {
    pub node: Node,
    pub after_bytes: Vec<u8>,
    pub before_node_hash: String,
    pub graph_fingerprint_before: String,
    pub graph_fingerprint_after: String,
}

fn validate_prepared_assignment_gate_context(
    node: &Node,
    expected_revision: u64,
    graph_fingerprint_before: &str,
    ctx: &PreparedAssignmentGateContext<'_>,
) -> PulseResult<()> {
    let prepared = ctx.prepared;
    let lease = ctx.lease;
    let workspace = ctx.workspace;

    if prepared.profile != PREPARED_ASSIGNMENT_PROFILE || prepared.code != "prepared_assignment" {
        return Err(PulseError::validation(
            "invalid_prepared_assignment",
            "prepared assignment record has unsupported profile/code",
        ));
    }
    if prepared.prepared_assignment_fingerprint.trim().is_empty() {
        return Err(PulseError::validation(
            "invalid_prepared_assignment",
            "prepared assignment fingerprint is required",
        ));
    }
    let computed = prepared.compute_fingerprint()?;
    if prepared.prepared_assignment_fingerprint != computed {
        return Err(PulseError::validation(
            "prepared_assignment_fingerprint_mismatch",
            "prepared assignment fingerprint does not match its canonical projection",
        ));
    }

    if prepared.subject.id != node.id
        || prepared.subject.kind != node.kind.as_str()
        || prepared.subject.revision_before != expected_revision
        || prepared.subject.revision_after != expected_revision + 1
        || prepared.subject.contract_revision != node.contract_revision
        || prepared.subject.status_before != "ready"
        || prepared.subject.status_after != "active"
    {
        return Err(PulseError::validation(
            "prepared_assignment_subject_mismatch",
            "prepared assignment subject does not match the current ready node and planned active revision",
        ));
    }

    if prepared.lifecycle.transition != LIFECYCLE_READY_TO_ACTIVE
        || prepared.lifecycle.gate_profile != LIFECYCLE_GATE_PROFILE
        || prepared.lifecycle.gate_status != GATE_STATUS_PASSED
        || prepared.lifecycle.expected_revision != expected_revision
        || prepared.lifecycle.new_revision != expected_revision + 1
        || prepared.lifecycle.event_id.trim().is_empty()
    {
        return Err(PulseError::validation(
            "prepared_assignment_lifecycle_mismatch",
            "prepared assignment lifecycle does not match ready -> active gate requirements",
        ));
    }

    if prepared.dispatch.authorization_status != DISPATCH_AUTHORIZED_STATUS
        || !prepared.dispatch.dispatch_authorized
    {
        return Err(PulseError::validation(
            "prepared_assignment_not_authorized",
            "prepared assignment dispatch authorization is not claim-only prepared_assignment",
        ));
    }

    if prepared.revalidated_snapshot.graph_fingerprint != graph_fingerprint_before {
        return Err(PulseError::validation(
            "prepared_assignment_graph_fingerprint_mismatch",
            "prepared assignment graph fingerprint no longer matches current work graph",
        ));
    }
    if prepared.revalidated_snapshot.readiness_fingerprint != lease.readiness_fingerprint {
        return Err(PulseError::validation(
            "prepared_assignment_readiness_mismatch",
            "prepared assignment readiness fingerprint does not match lease",
        ));
    }
    if prepared.packet_fingerprint != lease.packet_fingerprint {
        return Err(PulseError::validation(
            "prepared_assignment_packet_mismatch",
            "prepared assignment packet fingerprint does not match lease",
        ));
    }

    if lease.prepared_assignment_id != prepared.prepared_assignment_id
        || workspace.prepared_assignment_id != prepared.prepared_assignment_id
        || lease.lease_id != prepared.lease.lease_id
        || workspace.lease_id != lease.lease_id
        || workspace.workspace_id != prepared.workspace.workspace_id
        || workspace.workspace_id != lease.workspace_id
        || lease.state != LEASE_STATE_PREPARED
        || workspace.state != WORKSPACE_STATE_BOUND
        || prepared.lease.state != LEASE_STATE_PREPARED
        || prepared.workspace.binding_status != WORKSPACE_STATE_BOUND
    {
        return Err(PulseError::validation(
            "prepared_assignment_runtime_record_mismatch",
            "prepared assignment lease/workspace records are not a matching live prepared assignment",
        ));
    }

    if lease.subject.id != node.id
        || lease.subject.kind != node.kind.as_str()
        || lease.subject.revision != expected_revision
        || lease.subject.contract_revision != node.contract_revision
        || lease.subject.status_at_claim != "ready"
        || workspace.subject.id != node.id
        || workspace.subject.kind != node.kind.as_str()
        || workspace.subject.revision != expected_revision
    {
        return Err(PulseError::validation(
            "prepared_assignment_subject_mismatch",
            "lease/workspace subject snapshots do not match the current node",
        ));
    }

    if prepared.lease.assignee != lease.assignee.principal
        || prepared.lease.issued_by != lease.issued_by
        || prepared.lease.issued_at != lease.issued_at
        || prepared.lease.expires_at != lease.expires_at
        || prepared.lease.ttl_seconds != lease.ttl_seconds
        || prepared.workspace.repository_id != workspace.repository_id
        || prepared.workspace.base_commit != workspace.base_commit
        || prepared.workspace.path != workspace.path
        || prepared.workspace.mode != workspace.mode
        || prepared.workspace.owner_lease_id != lease.lease_id
        || workspace.base_commit != lease.source.base_commit
        || workspace.repository_id != lease.source.repository_id
        || prepared.revalidated_snapshot.repository_id != lease.source.repository_id
        || prepared.revalidated_snapshot.source_commit != lease.source.base_commit
    {
        return Err(PulseError::validation(
            "prepared_assignment_binding_mismatch",
            "prepared assignment lease/workspace/source summaries do not match runtime records",
        ));
    }

    if prepared.capability_match.status != CAP_MATCH_MATCHED
        || prepared.capability_match.inventory_identity != lease.capability_inventory_identity
        || !prepared.capability_match.missing.is_empty()
    {
        return Err(PulseError::validation(
            "prepared_assignment_capability_mismatch",
            "prepared assignment capability match is not a complete match for the lease inventory",
        ));
    }

    Ok(())
}

struct GateEvaluationOutcome {
    profile: String,
    fingerprint: String,
    status: String,
    shaping_receipt: Option<serde_json::Value>,
}

/// Stable `gate_coverage` list recorded on transition events. Installed gates
/// add their evaluated coverage; ordinary supported transitions keep the
/// minimal structural coverage.
fn gate_coverage_for(
    from: NodeStatus,
    to: NodeStatus,
    gate: Option<&GateEvaluationOutcome>,
) -> Vec<&'static str> {
    let mut coverage = vec!["transition_direction", "graph_integrity"];
    if gate.is_some() {
        coverage.push("gate_evaluation");
        if installed_gate(from, to).is_some() {
            coverage.push("authority");
            coverage.push("readiness_fingerprint");
        }
    }
    coverage
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assignment::{
        AssignmentDispatch, AssignmentLeaseAssignee, AssignmentLeaseSource, AssignmentLeaseSubject,
        AssignmentSubjectSnapshot, AssignmentTransaction, AssignmentWorkspaceSummary,
        CapabilityMatchReport, RevalidatedSnapshot, WorkspaceCleanupPolicy, WorkspaceSubjectRef,
        ASSIGNMENT_SCHEMA_VERSION, LEASE_KIND_IMPLEMENTATION, WORKSPACE_MODE_ISOLATED,
    };
    use crate::graph::store::JsonGraphStore;
    use crate::id::WorkKind;
    use chrono::TimeZone;
    use tempfile::TempDir;

    fn op_ctx(sec: i64) -> OperationContext {
        OperationContext {
            actor: "test:actor".to_string(),
            now: Utc.timestamp_opt(sec, 0).single().expect("valid timestamp"),
        }
    }

    fn ready_ticket_repo() -> (TempDir, JsonGraphStore, Node) {
        let dir = tempfile::tempdir().expect("create temp repo");
        let store = JsonGraphStore::new(dir.path());
        store.bootstrap().expect("bootstrap repo");
        let mut node = store
            .create_node_with_context(WorkKind::Ticket, "Ticket".to_string(), op_ctx(1))
            .expect("create ticket")
            .value;
        node.status = NodeStatus::Ready;
        std::fs::write(
            store.node_path(&node.id),
            to_canonical_bytes(&node).expect("canonical node"),
        )
        .expect("write ready node");
        (dir, store, node)
    }

    fn valid_gate_records(
        store: &JsonGraphStore,
        node: &Node,
    ) -> (
        AssignmentLeaseRecordV1,
        AssignmentWorkspaceRecordV1,
        PreparedAssignmentRecordV1,
    ) {
        let graph_fingerprint = store
            .graph_fingerprint_current_unlocked()
            .expect("graph fingerprint");
        let packet_fingerprint =
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let readiness_fingerprint =
            "sha256:1111111111111111111111111111111111111111111111111111111111111111";
        let repository_id = "repo_test";
        let source_commit = "0123456789abcdef0123456789abcdef01234567";
        let lease_id = "lease_01JTEST";
        let workspace_id = "wt_TEST";
        let prepared_assignment_id = "pa_01JTEST";
        let inventory_identity =
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        let lease = AssignmentLeaseRecordV1 {
            schema_version: crate::assignment::LEASE_SCHEMA_VERSION,
            lease_id: lease_id.to_string(),
            kind: LEASE_KIND_IMPLEMENTATION.to_string(),
            subject: AssignmentLeaseSubject {
                kind: node.kind.as_str().to_string(),
                id: node.id.clone(),
                revision: node.revision,
                contract_revision: node.contract_revision,
                status_at_claim: "ready".to_string(),
            },
            assignee: AssignmentLeaseAssignee {
                principal: "agent:test".to_string(),
            },
            issued_by: "human:test".to_string(),
            issued_at: "2026-07-28T10:00:00Z".to_string(),
            expires_at: "2030-07-28T10:30:00Z".to_string(),
            ttl_seconds: 1800,
            state: LEASE_STATE_PREPARED.to_string(),
            packet_fingerprint: packet_fingerprint.to_string(),
            readiness_fingerprint: readiness_fingerprint.to_string(),
            workspace_id: workspace_id.to_string(),
            prepared_assignment_id: prepared_assignment_id.to_string(),
            capability_inventory_identity: inventory_identity.to_string(),
            source: AssignmentLeaseSource {
                repository_id: repository_id.to_string(),
                base_commit: source_commit.to_string(),
            },
        };
        let workspace = AssignmentWorkspaceRecordV1 {
            schema_version: crate::assignment::WORKSPACE_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            lease_id: lease_id.to_string(),
            prepared_assignment_id: prepared_assignment_id.to_string(),
            subject: WorkspaceSubjectRef {
                kind: node.kind.as_str().to_string(),
                id: node.id.clone(),
                revision: node.revision,
            },
            mode: WORKSPACE_MODE_ISOLATED.to_string(),
            path: ".pulse/runtime/workspaces/wt_TEST".to_string(),
            repository_id: repository_id.to_string(),
            base_commit: source_commit.to_string(),
            head_commit_at_bind: source_commit.to_string(),
            cleanliness_at_bind: "clean".to_string(),
            state: WORKSPACE_STATE_BOUND.to_string(),
            created_at: "2026-07-28T10:00:00Z".to_string(),
            released_at: None,
            cleanup: WorkspaceCleanupPolicy {
                policy: "manual".to_string(),
                status: "pending".to_string(),
            },
        };
        let mut prepared = PreparedAssignmentRecordV1 {
            schema_version: ASSIGNMENT_SCHEMA_VERSION,
            profile: PREPARED_ASSIGNMENT_PROFILE.to_string(),
            code: "prepared_assignment".to_string(),
            prepared_assignment_id: prepared_assignment_id.to_string(),
            subject: AssignmentSubjectSnapshot {
                id: node.id.clone(),
                kind: node.kind.as_str().to_string(),
                revision_before: node.revision,
                revision_after: node.revision + 1,
                contract_revision: node.contract_revision,
                status_before: "ready".to_string(),
                status_after: "active".to_string(),
            },
            packet_fingerprint: packet_fingerprint.to_string(),
            revalidated_snapshot: RevalidatedSnapshot {
                graph_fingerprint,
                readiness_profile: "phase1_contract_readiness_v1".to_string(),
                readiness_fingerprint: readiness_fingerprint.to_string(),
                authority_policy_fingerprint:
                    "sha256:2222222222222222222222222222222222222222222222222222222222222222"
                        .to_string(),
                docs_registry_fingerprint:
                    "sha256:3333333333333333333333333333333333333333333333333333333333333333"
                        .to_string(),
                docs_index_fingerprint:
                    "sha256:4444444444444444444444444444444444444444444444444444444444444444"
                        .to_string(),
                source_commit: source_commit.to_string(),
                source_cleanliness: "clean".to_string(),
                repository_id: repository_id.to_string(),
            },
            lease: crate::assignment::AssignmentLeaseSummary {
                lease_id: lease_id.to_string(),
                state: LEASE_STATE_PREPARED.to_string(),
                assignee: "agent:test".to_string(),
                issued_by: "human:test".to_string(),
                issued_at: "2026-07-28T10:00:00Z".to_string(),
                expires_at: "2030-07-28T10:30:00Z".to_string(),
                ttl_seconds: 1800,
                exclusive: true,
            },
            workspace: AssignmentWorkspaceSummary {
                workspace_id: workspace_id.to_string(),
                binding_status: WORKSPACE_STATE_BOUND.to_string(),
                mode: WORKSPACE_MODE_ISOLATED.to_string(),
                path: ".pulse/runtime/workspaces/wt_TEST".to_string(),
                repository_id: repository_id.to_string(),
                base_commit: source_commit.to_string(),
                cleanliness: "clean".to_string(),
                owner_lease_id: lease_id.to_string(),
            },
            capability_match: CapabilityMatchReport {
                inventory_identity: inventory_identity.to_string(),
                principal: "agent:test".to_string(),
                status: CAP_MATCH_MATCHED.to_string(),
                required: vec!["source.read".to_string()],
                matched: vec!["source.read".to_string()],
                missing: vec![],
                extra: vec![],
                reason_codes: vec![],
            },
            lifecycle: crate::assignment::AssignmentLifecycle {
                transition: LIFECYCLE_READY_TO_ACTIVE.to_string(),
                gate_profile: LIFECYCLE_GATE_PROFILE.to_string(),
                gate_status: GATE_STATUS_PASSED.to_string(),
                expected_revision: node.revision,
                new_revision: node.revision + 1,
                event_id: "evt_01JTEST".to_string(),
            },
            dispatch: AssignmentDispatch::default(),
            transaction: AssignmentTransaction {
                transaction_id: "txn_01JTEST".to_string(),
                committed_targets: vec![],
                event_path: ".pulse/events/2026-07-28/evt_01JTEST.json".to_string(),
                recovery_state: "complete".to_string(),
            },
            prepared_assignment_fingerprint: String::new(),
            reason_codes: vec![],
        };
        prepared.prepared_assignment_fingerprint = prepared
            .compute_fingerprint()
            .expect("prepared assignment fingerprint");
        (lease, workspace, prepared)
    }

    #[test]
    fn internal_prepared_assignment_gate_plans_active_node_without_commit() {
        let (_dir, store, node) = ready_ticket_repo();
        let (lease, workspace, prepared) = valid_gate_records(&store, &node);
        let before_fingerprint = store
            .graph_fingerprint_current_unlocked()
            .expect("graph fingerprint");
        let _guard = WriteGuard::acquire(&store.repo_root).expect("acquire fence");

        let plan = store
            .evaluate_prepared_assignment_gate_for_active(
                &node.id,
                node.revision,
                PreparedAssignmentGateContext {
                    now: op_ctx(2).now,
                    prepared: &prepared,
                    lease: &lease,
                    workspace: &workspace,
                },
            )
            .expect("plan prepared assignment gate");

        assert_eq!(plan.node.status, NodeStatus::Active);
        assert_eq!(plan.node.revision, node.revision + 1);
        assert_eq!(plan.graph_fingerprint_before, before_fingerprint);
        assert_ne!(plan.graph_fingerprint_after, before_fingerprint);
        let stored: Node = serde_json::from_slice(
            &std::fs::read(store.node_path(&node.id)).expect("read stored node"),
        )
        .expect("decode stored node");
        assert_eq!(stored.status, NodeStatus::Ready);
        assert_eq!(stored.revision, node.revision);
    }

    #[test]
    fn internal_prepared_assignment_gate_rejects_stale_packet_binding() {
        let (_dir, store, node) = ready_ticket_repo();
        let (mut lease, workspace, prepared) = valid_gate_records(&store, &node);
        lease.packet_fingerprint =
            "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string();
        let _guard = WriteGuard::acquire(&store.repo_root).expect("acquire fence");

        let err = store
            .evaluate_prepared_assignment_gate_for_active(
                &node.id,
                node.revision,
                PreparedAssignmentGateContext {
                    now: op_ctx(2).now,
                    prepared: &prepared,
                    lease: &lease,
                    workspace: &workspace,
                },
            )
            .expect_err("packet mismatch must reject");

        assert_eq!(err.code(), "prepared_assignment_packet_mismatch");
    }
}
