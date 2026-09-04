//! Proof-driven execution handoff and completion gates.

use chrono::Utc;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::event::{new_event_id, EventEnvelope};
use crate::execution::{
    validate_checks, AcceptanceProof, CloseReceipt, CloseTicketArgs, CompleteVerificationArgs,
    HandoffReceipt, SubmitHandoffArgs, VerificationCheck, VerificationDisposition,
    VerificationReceipt,
};
use crate::graph::model::contract::{QaImpactPosture, Risk};
use crate::graph::model::lifecycle::TransitionReason;
use crate::graph::model::node::{DocumentationImpactPosture, Node, NodeStatus};
use crate::graph::store::JsonGraphStore;
use crate::identity::actor::ActorKind;
use crate::reservation::ReservationState;
use crate::storage::transaction::{
    commit_prepared_multi_target_transaction, new_transaction_id, prepare_multi_target_transaction,
    recover_prepared_transactions, FileState, MultiTargetTransactionIntent, TransactionTarget,
};
use crate::storage::WriteGuard;
use crate::{PulseError, Result};

impl JsonGraphStore {
    pub fn submit_execution_handoff(&self, mut args: SubmitHandoffArgs) -> Result<HandoffReceipt> {
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
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        authorize(&self.repo_root, &args.actor, "work.assignment.handoff")?;
        recover_prepared_transactions(&self.repo_root)?;
        if handoff_path.exists() {
            return load_handoff(&self.repo_root, &handoff_id);
        }
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
        let mut handoff = HandoffReceipt {
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
        mut args: CompleteVerificationArgs,
    ) -> Result<VerificationReceipt> {
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
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        authorize(&self.repo_root, &args.actor, "work.assignment.verify")?;
        recover_prepared_transactions(&self.repo_root)?;
        if verification_path.exists() {
            return load_verification(&self.repo_root, &verification_id);
        }
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
        normalize_acceptance_proofs(&mut args.acceptance_proofs);
        if args.disposition == VerificationDisposition::Passed {
            validate_acceptance_proofs(
                &self.repo_root,
                &node,
                &args.checks,
                &args.acceptance_proofs,
            )?;
        }
        // Verification is an independent proof observation, not the final close
        // gate.  The Phase 3 QA baseline/case resolver is not represented by a
        // current typed schema, so a passed check must remain non-terminal until
        // that authority exists.  In particular, caller-provided command/exit
        // records cannot authorize Done by themselves.
        let (target, reason) = match args.disposition {
            VerificationDisposition::Passed => (NodeStatus::Verifying, None),
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
        let mut verification = VerificationReceipt {
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
            acceptance_proofs: args.acceptance_proofs,
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

    /// Resolve the current passed verification for a Ticket and close it.
    ///
    /// This is the Ticket-oriented entry point used by the CLI. Resolution is
    /// deliberately narrow: exactly one passed verification must bind the
    /// Ticket's current verifying revision. A live lease is checked when one
    /// exists so an ambiguous assignment can never be silently selected.
    pub fn close_execution_ticket_for_ticket(
        &self,
        ticket_id: &str,
        actor: String,
        source_commit: String,
        summary: String,
        idempotency_key: String,
    ) -> Result<CloseReceipt> {
        let node = self.show_node(ticket_id)?;
        let mut candidates = list_verifications(&self.repo_root)?
            .into_iter()
            .filter(|verification| {
                verification.ticket_id == ticket_id
                    && verification.disposition == VerificationDisposition::Passed
                    && verification.resulting_status == "verifying"
                    && verification.resulting_revision == node.revision
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| left.verification_id.cmp(&right.verification_id));
        let verification = match candidates.as_slice() {
            [] => {
                return Err(PulseError::validation(
                    "close_verification_missing",
                    format!("no passed verification binds current revision of Ticket {ticket_id}"),
                ));
            }
            [verification] => verification,
            _ => {
                return Err(PulseError::validation(
                    "close_verification_ambiguous",
                    format!("more than one passed verification binds Ticket {ticket_id}"),
                ));
            }
        };

        let live_leases = crate::kernel::reservation::list_reservations(&self.repo_root)?
            .into_iter()
            .filter(|reservation| {
                reservation.subject.ticket_id == ticket_id
                    && reservation.state == ReservationState::Active
            })
            .collect::<Vec<_>>();
        if live_leases.len() > 1 {
            return Err(PulseError::validation(
                "close_live_lease_ambiguous",
                format!("more than one live lease is bound to Ticket {ticket_id}"),
            ));
        }
        if let Some(lease) = live_leases.first() {
            if lease.lease_id != verification.lease_id {
                return Err(PulseError::validation(
                    "close_live_lease_mismatch",
                    "current live lease does not match the verification receipt",
                ));
            }
        }

        self.close_execution_ticket(CloseTicketArgs {
            verification_id: verification.verification_id.clone(),
            actor,
            source_commit,
            summary,
            idempotency_key,
        })
    }

    /// Close a verified Ticket through Core-owned proof gates.
    ///
    /// All assessed risk levels use the same evidence gates. High- and
    /// critical-risk Tickets additionally require a human closing actor.
    ///
    /// # Errors
    ///
    /// Returns a typed validation error when proof bindings are stale,
    /// authorization is missing, or a required assurance resolver is not yet
    /// installed.
    pub fn close_execution_ticket(&self, args: CloseTicketArgs) -> Result<CloseReceipt> {
        if args.idempotency_key.trim().is_empty() {
            return Err(PulseError::validation(
                "close_idempotency_key_required",
                "proof close requires an idempotency key",
            ));
        }
        if args.summary.trim().is_empty() {
            return Err(PulseError::validation(
                "close_summary_missing",
                "proof close summary must not be empty",
            ));
        }
        let close_id = deterministic_evidence_id("close", &args.idempotency_key);
        let close_path = close_path(&self.repo_root, &close_id);
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        authorize(&self.repo_root, &args.actor, "work.close")?;
        recover_prepared_transactions(&self.repo_root)?;
        if close_path.exists() {
            let existing = load_close(&self.repo_root, &close_id)?;
            if existing.verification_id != args.verification_id
                || existing.closed_by != args.actor
                || existing.source_commit != args.source_commit
                || existing.summary != args.summary.trim()
            {
                return Err(PulseError::validation(
                    "close_idempotency_conflict",
                    "close idempotency key was already used with different inputs",
                ));
            }
            return Ok(existing);
        }
        let verification = load_verification(&self.repo_root, &args.verification_id)?;
        if verification.disposition != VerificationDisposition::Passed
            || verification.resulting_status != "verifying"
        {
            return Err(PulseError::validation(
                "close_verification_not_passed",
                "proof close requires a passed nonterminal verification receipt",
            ));
        }
        let handoff = load_handoff(&self.repo_root, &verification.handoff_id)?;
        if verification.ticket_id != handoff.ticket_id
            || verification.lease_id != handoff.lease_id
            || verification.source_commit != handoff.source_commit
        {
            return Err(PulseError::validation(
                "close_proof_binding_mismatch",
                "verification and handoff proofs do not share exact execution bindings",
            ));
        }
        if verification.source_commit != args.source_commit
            || crate::source::head_commit(&self.repo_root)? != args.source_commit
        {
            return Err(PulseError::validation(
                "close_source_mismatch",
                "proof close is not bound to the current verified source commit",
            ));
        }
        let node_path = self.node_path(&verification.ticket_id);
        let node_before_bytes =
            fs::read(&node_path).map_err(|error| PulseError::io(&node_path, error))?;
        let mut node: Node = serde_json::from_slice(&node_before_bytes)
            .map_err(|error| PulseError::json(&node_path, error))?;
        if node.status != NodeStatus::Verifying || node.revision != verification.resulting_revision
        {
            return Err(PulseError::validation(
                "close_ticket_changed",
                "Ticket is not the exact verified revision",
            ));
        }
        validate_close_postures(&node, &args.actor)?;
        let implementation = node.implementation.as_ref().ok_or_else(|| {
            PulseError::validation(
                "close_contract_missing",
                "proof close requires an implementation contract",
            )
        })?;
        validate_acceptance_proofs(
            &self.repo_root,
            &node,
            &verification.checks,
            &verification.acceptance_proofs,
        )?;
        validate_documentation_close(&self.repo_root, &node, &verification)?;
        validate_qa_close(&self.repo_root, &node, &handoff, &verification)?;
        if implementation.acceptance.is_empty() {
            return Err(PulseError::validation(
                "close_acceptance_missing",
                "proof close requires at least one contract acceptance item",
            ));
        }

        node.status = NodeStatus::Done;
        node.status_reason = None;
        node.revision += 1;
        node.updated_at = Utc::now();
        let mut close = CloseReceipt {
            schema_version: 1,
            close_id: close_id.clone(),
            idempotency_key_hash: hash_bytes(args.idempotency_key.as_bytes()),
            verification_id: verification.verification_id,
            handoff_id: verification.handoff_id,
            ticket_id: verification.ticket_id,
            lease_id: verification.lease_id,
            source_commit: args.source_commit,
            summary: args.summary.trim().to_string(),
            closed_by: args.actor.clone(),
            recorded_at: Utc::now().to_rfc3339(),
            resulting_revision: node.revision,
            close_fingerprint: String::new(),
        };
        close.close_fingerprint = close.compute_fingerprint()?;
        commit_proof_transition(
            &self.repo_root,
            "work.assignment.closed",
            &args.actor,
            &close.ticket_id,
            &node_path,
            &node_before_bytes,
            &node,
            &close_path,
            &close,
            json!({
                "close_id": close.close_id,
                "verification_id": close.verification_id,
                "handoff_id": close.handoff_id,
                "lease_id": close.lease_id,
                "source_commit": close.source_commit,
                "to": "done",
            }),
            self.failpoint,
        )?;
        Ok(close)
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

pub fn load_handoff(repo_root: &Path, handoff_id: &str) -> Result<HandoffReceipt> {
    let receipt: HandoffReceipt = load_json(&handoff_path(repo_root, handoff_id), "handoff")?;
    if receipt.compute_fingerprint()? != receipt.handoff_fingerprint {
        return Err(PulseError::validation(
            "handoff_fingerprint_mismatch",
            "handoff fingerprint does not match canonical contents",
        ));
    }
    Ok(receipt)
}

fn list_verifications(repo_root: &Path) -> Result<Vec<VerificationReceipt>> {
    let directory = repo_root.join(".pulse/evidence/execution/verifications");
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut paths = fs::read_dir(&directory)
        .map_err(|error| PulseError::io(&directory, error))?
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|error| PulseError::io(&directory, error))?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let verification_id =
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .ok_or_else(|| {
                        PulseError::validation(
                            "verification_record_invalid",
                            format!("verification path has no valid id: {}", path.display()),
                        )
                    })?;
            load_verification(repo_root, verification_id)
        })
        .collect()
}

pub fn load_verification(repo_root: &Path, verification_id: &str) -> Result<VerificationReceipt> {
    let receipt: VerificationReceipt = load_json(
        &verification_path(repo_root, verification_id),
        "verification",
    )?;
    if receipt.compute_fingerprint()? != receipt.verification_fingerprint {
        return Err(PulseError::validation(
            "verification_fingerprint_mismatch",
            "verification fingerprint does not match canonical contents",
        ));
    }
    Ok(receipt)
}

pub fn load_close(repo_root: &Path, close_id: &str) -> Result<CloseReceipt> {
    let receipt: CloseReceipt = load_json(&close_path(repo_root, close_id), "close")?;
    if receipt.compute_fingerprint()? != receipt.close_fingerprint {
        return Err(PulseError::validation(
            "close_fingerprint_mismatch",
            "close fingerprint does not match canonical contents",
        ));
    }
    Ok(receipt)
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

fn close_path(repo_root: &Path, close_id: &str) -> PathBuf {
    repo_root
        .join(".pulse/evidence/execution/closes")
        .join(format!("{close_id}.json"))
}

fn normalize_acceptance_proofs(proofs: &mut [AcceptanceProof]) {
    for proof in proofs.iter_mut() {
        proof.acceptance_id = proof.acceptance_id.trim().to_string();
        normalize_strings(&mut proof.check_names);
        normalize_strings(&mut proof.evidence_receipt_ids);
    }
    proofs.sort_by(|left, right| left.acceptance_id.cmp(&right.acceptance_id));
}

fn validate_acceptance_proofs(
    repo_root: &Path,
    node: &Node,
    checks: &[VerificationCheck],
    proofs: &[AcceptanceProof],
) -> Result<()> {
    let implementation = node.implementation.as_ref().ok_or_else(|| {
        PulseError::validation(
            "verification_contract_missing",
            "acceptance proof requires an implementation contract",
        )
    })?;
    let expected = implementation
        .acceptance
        .iter()
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();
    let actual = proofs
        .iter()
        .map(|proof| proof.acceptance_id.as_str())
        .collect::<Vec<_>>();
    if expected != actual {
        return Err(PulseError::validation(
            "verification_acceptance_coverage_incomplete",
            format!("acceptance proof IDs must exactly match the contract: expected={expected:?}, actual={actual:?}"),
        ));
    }
    let mut check_names = checks
        .iter()
        .map(|check| check.name.trim())
        .collect::<Vec<_>>();
    check_names.sort_unstable();
    if check_names.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(PulseError::validation(
            "verification_check_duplicate",
            "verification check names must be unique",
        ));
    }
    for proof in proofs {
        if proof.check_names.is_empty() && proof.evidence_receipt_ids.is_empty() {
            return Err(PulseError::validation(
                "verification_acceptance_proof_empty",
                format!(
                    "acceptance {} has no passing check or evidence receipt",
                    proof.acceptance_id
                ),
            ));
        }
        for check_name in &proof.check_names {
            let check = checks
                .iter()
                .find(|check| check.name.trim() == check_name)
                .ok_or_else(|| {
                    PulseError::validation(
                        "verification_acceptance_check_missing",
                        format!(
                            "acceptance {} references unknown check {check_name}",
                            proof.acceptance_id
                        ),
                    )
                })?;
            if check.exit_code != 0 {
                return Err(PulseError::validation(
                    "verification_acceptance_check_failed",
                    format!(
                        "acceptance {} references failed check {check_name}",
                        proof.acceptance_id
                    ),
                ));
            }
        }
        for receipt_id in &proof.evidence_receipt_ids {
            let (receipt, _) = crate::evidence::receipt::load_receipt(repo_root, receipt_id)?;
            if matches!(
                receipt.payload,
                crate::evidence::model::ReceiptPayload::DocumentationValidation(_)
            ) {
                let registry = crate::docs::manifest::load_unlocked_preserve(repo_root)?
                    .ok_or_else(|| {
                        PulseError::validation(
                            "verification_documentation_registry_missing",
                            "documentation receipt verification requires a current registry",
                        )
                    })?;
                crate::evidence::receipt::verify_receipt_under_fence(
                    repo_root, receipt_id, true, None, &registry,
                )?;
            } else {
                crate::evidence::receipt::verify_receipt(repo_root, receipt_id, true, None)?;
            }
        }
    }
    Ok(())
}

fn validate_close_postures(node: &Node, actor: &str) -> Result<()> {
    if matches!(node.risk, Some(Risk::High | Risk::Critical))
        && crate::policy::parse_actor(actor).kind != ActorKind::Human
    {
        return Err(PulseError::validation(
            "close_high_risk_human_required",
            "high- and critical-risk Tickets require a human closing actor",
        ));
    }
    let qa_posture = node
        .qa
        .as_ref()
        .map(|qa| qa.impact.posture)
        .unwrap_or(QaImpactPosture::Unknown);
    if qa_posture == QaImpactPosture::Unknown {
        return Err(PulseError::validation(
            "close_qa_gate_unavailable",
            "proof close requires assessed QA impact and its current assurance receipt",
        ));
    }
    match node.documentation_posture() {
        DocumentationImpactPosture::None | DocumentationImpactPosture::Required => {}
        DocumentationImpactPosture::Unknown | DocumentationImpactPosture::Deferred => {
            return Err(PulseError::validation(
                "close_documentation_gate_unavailable",
                "proof close requires documentation impact none or current required-document validation; deferred documentation needs promotion authority",
            ));
        }
    }
    Ok(())
}

fn validate_documentation_close(
    repo_root: &Path,
    node: &Node,
    verification: &VerificationReceipt,
) -> Result<()> {
    if node.documentation_posture() == DocumentationImpactPosture::None {
        return Ok(());
    }
    let documentation = node.documentation.as_ref().ok_or_else(|| {
        PulseError::validation(
            "close_documentation_gate_unavailable",
            "required documentation impact is missing its typed contract",
        )
    })?;
    let registry = crate::docs::manifest::load_unlocked_preserve(repo_root)?.ok_or_else(|| {
        PulseError::validation(
            "close_documentation_registry_invalid",
            "required documentation cannot close without a current registry",
        )
    })?;
    let work = crate::docs::WorkDocumentationContext::from((
        node.id.as_str(),
        node.revision,
        documentation,
    ));
    let applicable = crate::docs::applicable_docs(
        &work,
        &registry,
        &crate::docs::FsContentResolver::new(repo_root),
        crate::docs::ApplicabilityOptions::default(),
    )?;
    if applicable.gate.status != "complete" {
        return Err(PulseError::validation(
            "close_documentation_required_stale",
            format!(
                "required documentation is not current and authoritative: {:?}",
                applicable.gate.reason_codes
            ),
        ));
    }

    let expected = applicable
        .required
        .iter()
        .map(|document| (document.id.as_str(), document))
        .collect::<std::collections::BTreeMap<_, _>>();
    if expected.is_empty() {
        return Err(PulseError::validation(
            "close_documentation_required_missing",
            "required documentation impact must resolve at least one exact document",
        ));
    }
    let mut covered = std::collections::BTreeSet::new();
    let mut saw_documentation_receipt = false;
    let mut saw_eligible_receipt = false;
    for receipt_id in verification
        .acceptance_proofs
        .iter()
        .flat_map(|proof| proof.evidence_receipt_ids.iter())
    {
        let (receipt, _) = crate::evidence::receipt::load_receipt(repo_root, receipt_id)?;
        let crate::evidence::model::ReceiptPayload::DocumentationValidation(payload) =
            &receipt.payload
        else {
            continue;
        };
        saw_documentation_receipt = true;
        if payload.payload_version != 1 {
            continue;
        }
        let source = receipt.bindings.source.as_ref().ok_or_else(|| {
            PulseError::validation(
                "close_documentation_source_missing",
                "documentation validation receipt lacks an exact source binding",
            )
        })?;
        if source.commit != verification.source_commit {
            return Err(PulseError::validation(
                "close_documentation_source_mismatch",
                "documentation validation receipt is not bound to the verified source commit",
            ));
        }
        let report = crate::evidence::receipt::verify_receipt_under_fence(
            repo_root,
            receipt_id,
            true,
            Some(&verification.source_commit),
            &registry,
        )?;
        if !report.gate_eligible || receipt.result != crate::evidence::model::ReceiptResult::Passed
        {
            continue;
        }
        saw_eligible_receipt = true;
        for document in &payload.documents {
            let Some(document_id) = document.document_id.as_deref() else {
                continue;
            };
            let Some(required) = expected.get(document_id) else {
                continue;
            };
            if document.document_revision == Some(required.document_revision)
                && document.path == required.path
                && document.content_hash == required.content_hash
            {
                covered.insert(document_id.to_string());
            }
        }
    }
    if !saw_documentation_receipt {
        return Err(PulseError::validation(
            "close_documentation_receipt_missing",
            "required documentation needs a validation receipt in acceptance proof",
        ));
    }
    if !saw_eligible_receipt {
        return Err(PulseError::validation(
            "close_documentation_receipt_ineligible",
            "required documentation needs a current gate-eligible validation receipt",
        ));
    }
    let wanted = expected
        .keys()
        .map(|id| (*id).to_string())
        .collect::<Vec<_>>();
    let actual = covered.into_iter().collect::<Vec<_>>();
    if actual != wanted {
        return Err(PulseError::validation(
            "close_documentation_coverage_incomplete",
            format!(
                "documentation receipt coverage must exactly include required documents: expected={wanted:?}, actual={actual:?}"
            ),
        ));
    }
    Ok(())
}

fn validate_qa_close(
    repo_root: &Path,
    node: &Node,
    handoff: &HandoffReceipt,
    verification: &VerificationReceipt,
) -> Result<()> {
    let posture = node
        .qa
        .as_ref()
        .map(|qa| qa.impact.posture)
        .unwrap_or(QaImpactPosture::Unknown);
    if posture == QaImpactPosture::None {
        return Ok(());
    }
    let (baseline, expected_scope, expected_subject) = match posture {
        QaImpactPosture::Required => (
            crate::qa::resolve_ticket_cases(repo_root, node)?,
            crate::qa::QaExecutionScope::TicketCheckpoint,
            node.id.as_str(),
        ),
        QaImpactPosture::CoveredByStoryClose => {
            let story_id = node
                .qa
                .as_ref()
                .and_then(|qa| qa.impact.behavioral_owner.as_deref())
                .ok_or_else(|| {
                    PulseError::validation(
                        "close_qa_story_owner_missing",
                        "Story-deferred QA requires a behavioral owner",
                    )
                })?;
            (
                crate::qa::resolve_story_cases(repo_root, story_id)?,
                crate::qa::QaExecutionScope::StoryClose,
                story_id,
            )
        }
        QaImpactPosture::Unknown | QaImpactPosture::None => {
            return Err(PulseError::validation(
                "close_qa_gate_unavailable",
                "QA posture does not have a close resolver",
            ));
        }
    };
    let mut covered = std::collections::BTreeSet::new();
    let mut saw_checkpoint = false;
    let author = crate::policy::parse_actor(&handoff.recorded_by);

    for receipt_id in verification
        .acceptance_proofs
        .iter()
        .flat_map(|proof| proof.evidence_receipt_ids.iter())
    {
        let (receipt, _) = crate::evidence::receipt::load_receipt(repo_root, receipt_id)?;
        let crate::evidence::model::ReceiptPayload::QaCheckpoint(payload) = &receipt.payload else {
            continue;
        };
        if payload.qa_scope != expected_scope {
            continue;
        }
        saw_checkpoint = true;
        crate::evidence::receipt::verify_receipt(repo_root, receipt_id, true, None)?;
        if receipt.result != crate::evidence::model::ReceiptResult::Passed {
            return Err(PulseError::validation(
                "close_qa_checkpoint_not_passed",
                format!("QA checkpoint {receipt_id} is not passed"),
            ));
        }
        if receipt.actor == author {
            return Err(PulseError::validation(
                "close_qa_independence_required",
                "the implementation handoff author cannot attest their own QA checkpoint",
            ));
        }
        let source = receipt.bindings.source.as_ref().ok_or_else(|| {
            PulseError::validation(
                "close_qa_source_missing",
                "QA checkpoint lacks an exact source binding",
            )
        })?;
        if source.commit != verification.source_commit
            || (expected_scope == crate::qa::QaExecutionScope::TicketCheckpoint
                && payload.ticket_id != node.id)
            || receipt.subject.id != expected_subject
            || payload.story_id != baseline.owner_id
            || payload.baseline_revision != baseline.revision
            || payload.baseline_content_hash != baseline.content_hash
        {
            return Err(PulseError::validation(
                "close_qa_checkpoint_stale",
                "QA checkpoint does not bind the current Ticket, source, or Story baseline",
            ));
        }
        let expected = baseline
            .cases
            .iter()
            .map(|case| (case.id.as_str(), case))
            .collect::<std::collections::BTreeMap<_, _>>();
        for observation in &payload.cases {
            let case = expected.get(observation.case_id.as_str()).ok_or_else(|| {
                PulseError::validation(
                    "close_qa_case_unexpected",
                    format!(
                        "QA checkpoint contains unselected case {}",
                        observation.case_id
                    ),
                )
            })?;
            if observation.case_revision != case.revision
                || observation.outcome != crate::qa::QaCaseOutcome::Passed
            {
                return Err(PulseError::validation(
                    "close_qa_case_not_passed",
                    format!("QA case {} is stale or not passed", observation.case_id),
                ));
            }
            if !covered.insert(observation.case_id.clone()) {
                return Err(PulseError::validation(
                    "close_qa_case_duplicate",
                    format!(
                        "QA case {} is claimed by multiple passed checkpoints",
                        observation.case_id
                    ),
                ));
            }
        }
    }
    if !saw_checkpoint {
        return Err(PulseError::validation(
            "close_qa_checkpoint_missing",
            "QA impact needs a current passed receipt for its required execution scope in acceptance proof",
        ));
    }
    let actual = covered.into_iter().collect::<Vec<_>>();
    let wanted = baseline
        .cases
        .iter()
        .map(|case| case.id.clone())
        .collect::<Vec<_>>();
    if actual != wanted {
        return Err(PulseError::validation(
            "close_qa_coverage_incomplete",
            format!("QA checkpoint coverage must exactly match selected cases: expected={wanted:?}, actual={actual:?}"),
        ));
    }
    Ok(())
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
        NodeStatus::Verifying => "verifying",
        NodeStatus::Rework => "rework",
        NodeStatus::Blocked => "blocked",
        _ => "invalid",
    }
}
