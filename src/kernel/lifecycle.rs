use std::fs;

use chrono::Utc;
use serde_json::json;

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
    /// The reservation-activation gate rejects generic/public callers because
    /// only the Core reservation service can supply the exact acknowledgement.
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

        if matches!(profile_kind, GateProfile::ReservationActivation) {
            return Err(PulseError::validation(
                "reservation_activation_required",
                format!(
                    "transition {} -> {:?} requires an acknowledged Core reservation",
                    node.id, to,
                ),
            ));
        }

        let policy_report = crate::policy::load_authority_policy(&self.repo_root)?;
        let caller = crate::policy::parse_actor(&ctx.actor);
        let grant = match profile_kind {
            GateProfile::Shaped => "work.transition.shaped",
            GateProfile::Ready => "work.transition.ready",
            GateProfile::ReservationActivation => unreachable!("handled above"),
        };
        crate::policy::authorize(&policy_report, &caller, &[grant])?;

        let eval_profile = match profile_kind {
            GateProfile::Shaped => EvalProfile::Shaped,
            GateProfile::Ready => EvalProfile::Ready,
            GateProfile::ReservationActivation => unreachable!("handled above"),
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
