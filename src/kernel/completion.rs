//! Proof-driven execution handoff and completion gates.

use chrono::Utc;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::event::{new_event_id, EventEnvelope};
use crate::execution::{
    validate_checks, CompleteVerificationArgs, HandoffReceiptV1, SubmitHandoffArgs,
    VerificationDisposition, VerificationReceiptV1,
};
use crate::graph::lifecycle::TransitionReason;
use crate::graph::node::{Node, NodeStatus};
use crate::graph::store::JsonGraphStore;
use crate::reservation::ReservationState;
use crate::storage::transaction::{
    commit_prepared_multi_target_transaction, new_transaction_id, prepare_multi_target_transaction,
    recover_prepared_transactions, FileState, MultiTargetTransactionIntent, TransactionTarget,
};
use crate::storage::WriteGuard;
use crate::{PulseError, Result};

impl JsonGraphStore {
    pub fn submit_execution_handoff(
        &self,
        mut args: SubmitHandoffArgs,
    ) -> Result<HandoffReceiptV1> {
        if args.idempotency_key.trim().is_empty() {
            return Err(PulseError::validation(
                "handoff_idempotency_key_required",
                "handoff requires an idempotency key",
            ));
        }
        normalize_strings(&mut args.changed_paths);
        normalize_strings(&mut args.evidence_receipt_ids);
        if args.summary.trim().is_empty() {
            return Err(PulseError::validation(
                "handoff_summary_missing",
                "typed handoff requires a summary",
            ));
        }
        for path in &args.changed_paths {
            crate::storage::paths::validate_relative_path(Path::new(path)).map_err(|_| {
                PulseError::validation(
                    "handoff_path_invalid",
                    format!("handoff changed path is unsafe: {path}"),
                )
            })?;
        }
        let handoff_id = deterministic_evidence_id("handoff", &args.idempotency_key);
        let handoff_path = handoff_path(&self.repo_root, &handoff_id);
        if handoff_path.exists() {
            return load_handoff(&self.repo_root, &handoff_id);
        }
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        recover_prepared_transactions(&self.repo_root)?;
        authorize(&self.repo_root, &args.actor, "work.assignment.handoff")?;
        let reservation =
            crate::kernel::reservation::load_reservation(&self.repo_root, &args.lease_id)?;
        if reservation.state != ReservationState::Active {
            return Err(PulseError::validation(
                "handoff_assignment_not_active",
                "handoff requires an active acknowledged assignment",
            ));
        }
        let binding = reservation.runtime_binding.as_ref().ok_or_else(|| {
            PulseError::validation(
                "reservation_record_invalid",
                "active reservation lacks runtime binding",
            )
        })?;
        if binding.session_id != args.session_id {
            return Err(PulseError::validation(
                "handoff_session_mismatch",
                "handoff session does not match the active reservation",
            ));
        }
        if reservation.source.commit != args.source_commit
            || crate::source::head_commit(&self.repo_root)? != args.source_commit
        {
            return Err(PulseError::validation(
                "handoff_source_mismatch",
                "handoff source is not the exact active assignment source",
            ));
        }
        for receipt_id in &args.evidence_receipt_ids {
            crate::evidence::receipt::verify_receipt(&self.repo_root, receipt_id, true, None)?;
        }
        let node_path = self.node_path(&reservation.subject.ticket_id);
        let node_before_bytes =
            fs::read(&node_path).map_err(|error| PulseError::io(&node_path, error))?;
        let mut node: Node = serde_json::from_slice(&node_before_bytes)
            .map_err(|error| PulseError::json(&node_path, error))?;
        if node.status != NodeStatus::Active
            || Some(node.revision) != reservation.activated_revision
        {
            return Err(PulseError::validation(
                "handoff_ticket_changed",
                "Ticket is not the exact active revision bound to the assignment",
            ));
        }
        let active_revision = node.revision;
        node.status = NodeStatus::Verifying;
        node.status_reason = None;
        node.revision += 1;
        node.updated_at = Utc::now();
        let mut handoff = HandoffReceiptV1 {
            schema_version: 1,
            handoff_id: handoff_id.clone(),
            idempotency_key_hash: hash_bytes(args.idempotency_key.as_bytes()),
            ticket_id: reservation.subject.ticket_id.clone(),
            active_revision,
            verifying_revision: node.revision,
            lease_id: reservation.lease_id,
            project_id: binding.project_id.clone(),
            workspace_id: binding.workspace_id.clone(),
            session_id: binding.session_id.clone(),
            repository_id: reservation.source.repository_id,
            source_commit: args.source_commit,
            summary: args.summary.trim().to_string(),
            changed_paths: args.changed_paths,
            evidence_receipt_ids: args.evidence_receipt_ids,
            recorded_by: args.actor.clone(),
            recorded_at: Utc::now().to_rfc3339(),
            handoff_fingerprint: String::new(),
        };
        handoff.handoff_fingerprint = handoff.compute_fingerprint()?;
        commit_proof_transition(
            &self.repo_root,
            "work.assignment.handoff_submitted",
            &args.actor,
            &handoff.ticket_id,
            &node_path,
            &node_before_bytes,
            &node,
            &handoff_path,
            &handoff,
            json!({
                "handoff_id": handoff.handoff_id,
                "lease_id": handoff.lease_id,
                "session_id": handoff.session_id,
                "source_commit": handoff.source_commit,
                "from": "active",
                "to": "verifying",
            }),
            self.failpoint,
        )?;
        Ok(handoff)
    }

    pub fn complete_execution_verification(
        &self,
        args: CompleteVerificationArgs,
    ) -> Result<VerificationReceiptV1> {
        if args.idempotency_key.trim().is_empty() {
            return Err(PulseError::validation(
                "verification_idempotency_key_required",
                "verification completion requires an idempotency key",
            ));
        }
        validate_checks(args.disposition, &args.checks)?;
        if args.summary.trim().is_empty() {
            return Err(PulseError::validation(
                "verification_summary_missing",
                "verification summary must not be empty",
            ));
        }
        let verification_id = deterministic_evidence_id("verify", &args.idempotency_key);
        let verification_path = verification_path(&self.repo_root, &verification_id);
        if verification_path.exists() {
            return load_verification(&self.repo_root, &verification_id);
        }
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        recover_prepared_transactions(&self.repo_root)?;
        authorize(&self.repo_root, &args.actor, "work.assignment.verify")?;
        let handoff = load_handoff(&self.repo_root, &args.handoff_id)?;
        if handoff.recorded_by == args.actor {
            return Err(PulseError::validation(
                "verification_independence_required",
                "the handoff author cannot verify their own completion proof",
            ));
        }
        if handoff.source_commit != args.source_commit
            || crate::source::head_commit(&self.repo_root)? != args.source_commit
        {
            return Err(PulseError::validation(
                "verification_source_mismatch",
                "verification is not bound to the handoff source commit",
            ));
        }
        let node_path = self.node_path(&handoff.ticket_id);
        let node_before_bytes =
            fs::read(&node_path).map_err(|error| PulseError::io(&node_path, error))?;
        let mut node: Node = serde_json::from_slice(&node_before_bytes)
            .map_err(|error| PulseError::json(&node_path, error))?;
        if node.status != NodeStatus::Verifying || node.revision != handoff.verifying_revision {
            return Err(PulseError::validation(
                "verification_ticket_changed",
                "Ticket is not the exact verifying revision from the handoff",
            ));
        }
        let (target, reason) = match args.disposition {
            VerificationDisposition::Passed => (NodeStatus::Done, None),
            VerificationDisposition::Rework => (
                NodeStatus::Rework,
                Some(TransitionReason {
                    code: "verification_rework".to_string(),
                    summary: args.summary.trim().to_string(),
                    reference: Some(verification_id.clone()),
                }),
            ),
            VerificationDisposition::Blocked => (
                NodeStatus::Blocked,
                Some(TransitionReason {
                    code: "verification_blocked".to_string(),
                    summary: args.summary.trim().to_string(),
                    reference: Some(verification_id.clone()),
                }),
            ),
        };
        node.status = target;
        node.status_reason = reason.map(TransitionReason::into_status_reason);
        node.revision += 1;
        node.updated_at = Utc::now();
        let mut verification = VerificationReceiptV1 {
            schema_version: 1,
            verification_id: verification_id.clone(),
            idempotency_key_hash: hash_bytes(args.idempotency_key.as_bytes()),
            handoff_id: handoff.handoff_id,
            ticket_id: handoff.ticket_id,
            lease_id: handoff.lease_id,
            source_commit: args.source_commit,
            disposition: args.disposition,
            summary: args.summary.trim().to_string(),
            checks: args.checks,
            verified_by: args.actor.clone(),
            recorded_at: Utc::now().to_rfc3339(),
            resulting_status: status_name(target).to_string(),
            resulting_revision: node.revision,
            verification_fingerprint: String::new(),
        };
        verification.verification_fingerprint = verification.compute_fingerprint()?;
        commit_proof_transition(
            &self.repo_root,
            "work.assignment.verification_completed",
            &args.actor,
            &verification.ticket_id,
            &node_path,
            &node_before_bytes,
            &node,
            &verification_path,
            &verification,
            json!({
                "verification_id": verification.verification_id,
                "handoff_id": verification.handoff_id,
                "lease_id": verification.lease_id,
                "source_commit": verification.source_commit,
                "disposition": verification.disposition,
                "to": verification.resulting_status,
            }),
            self.failpoint,
        )?;
        Ok(verification)
    }
}

#[allow(clippy::too_many_arguments)]
fn commit_proof_transition<T: serde::Serialize>(
    repo_root: &Path,
    operation: &str,
    actor: &str,
    ticket_id: &str,
    node_path: &Path,
    node_before_bytes: &[u8],
    node_after: &Node,
    proof_path: &Path,
    proof: &T,
    payload: serde_json::Value,
    failpoint: Option<crate::storage::transaction::TransactionFailpoint>,
) -> Result<()> {
    let node_after_bytes = to_canonical_bytes(node_after)?;
    let proof_bytes = to_canonical_bytes(proof)?;
    let event_id = new_event_id();
    let now = Utc::now();
    let event_path = repo_root
        .join(".pulse/events")
        .join(now.format("%Y-%m-%d").to_string())
        .join(format!("{event_id}.json"));
    let event = EventEnvelope::new(event_id.clone(), operation, actor, ticket_id, payload, now);
    let before_node: Node = serde_json::from_slice(node_before_bytes).map_err(PulseError::from)?;
    let targets = vec![
        TransactionTarget::new(
            node_path.to_path_buf(),
            FileState::Present {
                hash: hash_bytes(node_before_bytes),
                revision: before_node.revision,
            },
            FileState::Present {
                hash: hash_bytes(&node_after_bytes),
                revision: node_after.revision,
            },
            &node_after_bytes,
        ),
        TransactionTarget::new(
            proof_path.to_path_buf(),
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&proof_bytes),
                revision: 0,
            },
            &proof_bytes,
        ),
    ];
    let intent = MultiTargetTransactionIntent::prepared_with_transaction_id(
        new_transaction_id(),
        event_id,
        operation,
        actor,
        targets,
        event_path,
        serde_json::to_value(event)?,
    )?;
    let transaction = prepare_multi_target_transaction(repo_root, intent)?;
    commit_prepared_multi_target_transaction(&transaction, failpoint)
}

pub fn load_handoff(repo_root: &Path, handoff_id: &str) -> Result<HandoffReceiptV1> {
    load_json(&handoff_path(repo_root, handoff_id), "handoff")
}

pub fn load_verification(repo_root: &Path, verification_id: &str) -> Result<VerificationReceiptV1> {
    load_json(
        &verification_path(repo_root, verification_id),
        "verification",
    )
}

fn load_json<T: serde::de::DeserializeOwned>(path: &Path, kind: &str) -> Result<T> {
    let bytes = fs::read(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            PulseError::NotFound {
                subject: format!("{kind} proof {}", path.display()),
            }
        } else {
            PulseError::io(path, error)
        }
    })?;
    serde_json::from_slice(&bytes).map_err(|error| PulseError::json(path, error))
}

fn handoff_path(repo_root: &Path, handoff_id: &str) -> PathBuf {
    repo_root
        .join(".pulse/evidence/execution/handoffs")
        .join(format!("{handoff_id}.json"))
}

fn verification_path(repo_root: &Path, verification_id: &str) -> PathBuf {
    repo_root
        .join(".pulse/evidence/execution/verifications")
        .join(format!("{verification_id}.json"))
}

fn deterministic_evidence_id(prefix: &str, key: &str) -> String {
    let digest = hash_bytes(key.as_bytes());
    format!(
        "{prefix}_{}",
        digest
            .trim_start_matches("sha256:")
            .chars()
            .take(26)
            .collect::<String>()
    )
}

fn authorize(repo_root: &Path, actor: &str, grant: &str) -> Result<()> {
    let report = crate::policy::load_authority_policy(repo_root)?;
    let principal = crate::policy::parse_actor(actor);
    crate::policy::authorize(&report, &principal, &[grant])
}

fn normalize_strings(values: &mut Vec<String>) {
    values.retain(|value| !value.trim().is_empty());
    for value in values.iter_mut() {
        *value = value.trim().to_string();
    }
    values.sort();
    values.dedup();
}

fn status_name(status: NodeStatus) -> &'static str {
    match status {
        NodeStatus::Done => "done",
        NodeStatus::Rework => "rework",
        NodeStatus::Blocked => "blocked",
        _ => "invalid",
    }
}
