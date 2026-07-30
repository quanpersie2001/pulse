//! Core reservation and acknowledgement-gated activation.

use chrono::{DateTime, Utc};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::event::{new_event_id, EventEnvelope};
use crate::graph::node::{Node, NodeStatus};
use crate::graph::store::JsonGraphStore;
use crate::reservation::{
    ActivateReservationArgs, CapabilityInventoryV1, CoreReservationV1, ReservationSource,
    ReservationState, ReservationSubject, ReserveWorkArgs, ReserveWorkOutcome, CAP_MATCH_MATCHED,
    MAX_TTL_SECONDS, MIN_TTL_SECONDS, RESERVATION_SCHEMA_VERSION,
};
use crate::storage::transaction::{
    commit_prepared_multi_target_transaction, new_transaction_id, prepare_multi_target_transaction,
    recover_prepared_transactions, FileState, MultiTargetTransactionIntent, TransactionTarget,
};
use crate::storage::WriteGuard;
use crate::{PulseError, Result};

const RESERVATIONS_DIR: &str = ".pulse/runtime/assignment/reservations";
const PACKETS_DIR: &str = ".pulse/runtime/assignment/packets";

impl JsonGraphStore {
    pub fn reserve_work(&self, args: ReserveWorkArgs) -> Result<ReserveWorkOutcome> {
        check_enrolled(&self.repo_root)?;
        if args.idempotency_key.trim().is_empty() {
            return Err(PulseError::validation(
                "reservation_idempotency_key_required",
                "Core reservation requires an idempotency key",
            ));
        }
        if !(MIN_TTL_SECONDS..=MAX_TTL_SECONDS).contains(&args.ttl_seconds) {
            return Err(PulseError::validation(
                "assignment_ttl_out_of_range",
                format!(
                    "ttl_seconds {} is outside allowed range {}..={}",
                    args.ttl_seconds, MIN_TTL_SECONDS, MAX_TTL_SECONDS
                ),
            ));
        }
        let inventory = CapabilityInventoryV1::from_json_bytes(&args.capability_inventory_bytes)?;
        let lease_id = deterministic_lease_id(&args.idempotency_key);
        for _ in 0..2 {
            let guard = WriteGuard::acquire(&self.repo_root)?;
            recover_prepared_transactions(&self.repo_root)?;
            if let Ok(existing) = load_reservation(&self.repo_root, &lease_id) {
                if existing.idempotency_key_hash != hash_bytes(args.idempotency_key.as_bytes())
                    || existing.subject.ticket_id != args.ticket_id
                {
                    return Err(PulseError::validation(
                        "reservation_idempotency_conflict",
                        "reservation idempotency key is bound to different inputs",
                    ));
                }
                // A Released/Expired/StaleNeedsOperator reservation is not
                // live — the caller (daemon saga recovery) must get a fresh
                // lease rather than a dead one.
                if matches!(
                    existing.state,
                    ReservationState::Reserved
                        | ReservationState::Acknowledged
                        | ReservationState::Active
                ) {
                    let packet = load_packet(&self.repo_root, &lease_id)?;
                    return Ok(ReserveWorkOutcome {
                        reservation: existing,
                        packet,
                    });
                }
            }
            match self.reserve_work_under_fence(&args, &inventory, &lease_id) {
                Ok(outcome) => return Ok(outcome),
                Err(error) if error.code() == "work_packet_docs_cache_needs_refresh" => {
                    drop(guard);
                    crate::docs::build_search_cache(
                        &self.repo_root,
                        crate::docs::IndexOptions::default(),
                    )?;
                }
                Err(error) => return Err(error),
            }
        }
        Err(PulseError::validation(
            "work_packet_docs_index_unavailable",
            "docs cache remained stale after one refresh",
        ))
    }

    fn reserve_work_under_fence(
        &self,
        args: &ReserveWorkArgs,
        inventory: &CapabilityInventoryV1,
        lease_id: &str,
    ) -> Result<ReserveWorkOutcome> {
        authorize_assignment(&self.repo_root, &args.actor, "work.assignment.prepare")?;
        if let Some(existing) = find_live_reservation_for_ticket(&self.repo_root, &args.ticket_id)?
        {
            return Err(PulseError::validation(
                "assignment_live_lease_exists",
                format!(
                    "live exclusive reservation {existing} exists for {}",
                    args.ticket_id
                ),
            ));
        }
        let packet = self.work_packet_under_fence(&args.ticket_id)?;
        if !packet.dispatch.reservation_candidate || packet.dispatch.dispatch_authorized {
            return Err(PulseError::validation(
                "assignment_packet_invalid",
                "packet is not an unassigned reservation candidate",
            ));
        }
        let node_path = self.node_path(&args.ticket_id);
        let node_bytes = fs::read(&node_path).map_err(|error| PulseError::io(&node_path, error))?;
        let node: Node = serde_json::from_slice(&node_bytes)
            .map_err(|error| PulseError::json(&node_path, error))?;
        if node.status != NodeStatus::Ready || node.revision != packet.subject.revision {
            return Err(PulseError::validation(
                "assignment_subject_not_ready",
                "Ticket is no longer the exact ready revision captured by the packet",
            ));
        }
        inventory.validate_principal(&args.assignee)?;
        let capability_match = inventory.match_required(
            &args.assignee,
            &packet.capabilities.required,
            packet.workspace.required_strategy == "isolated_worktree_required",
        )?;
        if capability_match.status != CAP_MATCH_MATCHED {
            return Err(PulseError::validation(
                "assignment_capability_missing",
                format!(
                    "required capabilities missing: {}",
                    capability_match.missing.join(", ")
                ),
            ));
        }
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(args.ttl_seconds as i64);
        let mut reservation = CoreReservationV1 {
            schema_version: RESERVATION_SCHEMA_VERSION,
            reservation_id: lease_id.replacen("lease_", "rsv_", 1),
            lease_id: lease_id.to_string(),
            idempotency_key_hash: hash_bytes(args.idempotency_key.as_bytes()),
            subject: ReservationSubject {
                ticket_id: node.id.clone(),
                ticket_revision: node.revision,
                contract_revision: node.contract_revision,
            },
            assignee: args.assignee.clone(),
            issued_by: args.actor.clone(),
            issued_at: now.to_rfc3339(),
            expires_at: expires_at.to_rfc3339(),
            packet_fingerprint: packet.packet_fingerprint.clone(),
            readiness_fingerprint: packet.snapshot.readiness_fingerprint.clone(),
            source: ReservationSource {
                repository_id: packet.source.repository_id.clone(),
                commit: packet.source.commit.clone(),
            },
            state: ReservationState::Reserved,
            runtime_binding: None,
            acknowledgement: None,
            activated_revision: None,
            released_at: None,
            release_reason: None,
            reservation_fingerprint: String::new(),
        };
        reservation.reservation_fingerprint = reservation.compute_fingerprint()?;
        commit_reservation_change(ReservationChange {
            repo_root: &self.repo_root,
            operation: "work.assignment.reserved",
            actor: &args.actor,
            ticket_id: &args.ticket_id,
            before: None,
            after: &reservation,
            packet: Some(&packet),
            payload: json!({
                "lease_id": lease_id,
                "ticket_revision": node.revision,
                "packet_fingerprint": packet.packet_fingerprint,
                "source_commit": packet.source.commit,
            }),
            failpoint: self.failpoint,
        })?;
        Ok(ReserveWorkOutcome {
            reservation,
            packet,
        })
    }

    pub fn activate_reservation(&self, args: ActivateReservationArgs) -> Result<CoreReservationV1> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        recover_prepared_transactions(&self.repo_root)?;
        authorize_assignment(&self.repo_root, &args.actor, "work.assignment.prepare")?;
        let before = load_reservation(&self.repo_root, &args.lease_id)?;
        if before.state == ReservationState::Active {
            if before.runtime_binding.as_ref() == Some(&args.runtime_binding)
                && before.acknowledgement.as_ref() == Some(&args.acknowledgement)
            {
                return Ok(before);
            }
            return Err(PulseError::validation(
                "reservation_activation_conflict",
                "reservation is already active with a different runtime binding",
            ));
        }
        if before.state != ReservationState::Reserved
            && before.state != ReservationState::Acknowledged
        {
            return Err(PulseError::validation(
                "reservation_not_activatable",
                "reservation is not awaiting acknowledgement/activation",
            ));
        }
        let expires_at = DateTime::parse_from_rfc3339(&before.expires_at)
            .map_err(|_| {
                PulseError::validation(
                    "reservation_record_invalid",
                    "reservation expiry is invalid",
                )
            })?
            .with_timezone(&Utc);
        if expires_at <= Utc::now() {
            return Err(PulseError::validation(
                "reservation_expired",
                "reservation expired before activation",
            ));
        }
        if args.acknowledgement.session_id != args.runtime_binding.session_id
            || args.acknowledgement.packet_fingerprint != before.packet_fingerprint
        {
            return Err(PulseError::validation(
                "assignment_acknowledgement_mismatch",
                "acknowledgement does not bind the exact session and packet",
            ));
        }
        let packet = load_packet(&self.repo_root, &args.lease_id)?;
        if packet.packet_fingerprint != before.packet_fingerprint {
            return Err(PulseError::validation(
                "assignment_packet_invalid",
                "stored packet does not match reservation",
            ));
        }
        if crate::source::head_commit(&self.repo_root)? != before.source.commit {
            return Err(PulseError::validation(
                "work_packet_source_changed",
                "source commit changed before assignment activation",
            ));
        }
        let node_path = self.node_path(&before.subject.ticket_id);
        let node_before_bytes =
            fs::read(&node_path).map_err(|error| PulseError::io(&node_path, error))?;
        let mut node: Node = serde_json::from_slice(&node_before_bytes)
            .map_err(|error| PulseError::json(&node_path, error))?;
        if node.status != NodeStatus::Ready || node.revision != before.subject.ticket_revision {
            return Err(PulseError::validation(
                "assignment_subject_changed",
                "Ticket revision/status changed before activation",
            ));
        }
        node.status = NodeStatus::Active;
        node.status_reason = None;
        node.revision += 1;
        node.updated_at = Utc::now();
        let node_after_bytes = to_canonical_bytes(&node)?;
        let mut after = before.clone();
        after.state = ReservationState::Active;
        after.runtime_binding = Some(args.runtime_binding.clone());
        after.acknowledgement = Some(args.acknowledgement.clone());
        after.activated_revision = Some(node.revision);
        after.reservation_fingerprint = after.compute_fingerprint()?;
        let reservation_before_bytes = to_canonical_bytes(&before)?;
        let reservation_after_bytes = to_canonical_bytes(&after)?;
        let event_id = new_event_id();
        let event_path = event_path(&self.repo_root, &event_id, Utc::now());
        let event = EventEnvelope::new(
            event_id.clone(),
            "work.assignment.activated",
            &args.actor,
            &before.subject.ticket_id,
            json!({
                "lease_id": before.lease_id,
                "ticket_revision_before": before.subject.ticket_revision,
                "ticket_revision_after": node.revision,
                "packet_fingerprint": before.packet_fingerprint,
                "runtime_binding": args.runtime_binding,
                "acknowledgement_id": args.acknowledgement.acknowledgement_id,
            }),
            Utc::now(),
        );
        let event_value = serde_json::to_value(&event)?;
        let targets = vec![
            TransactionTarget::new(
                reservation_path(&self.repo_root, &args.lease_id),
                FileState::Present {
                    hash: hash_bytes(&reservation_before_bytes),
                    revision: 0,
                },
                FileState::Present {
                    hash: hash_bytes(&reservation_after_bytes),
                    revision: 0,
                },
                &reservation_after_bytes,
            ),
            TransactionTarget::new(
                node_path,
                FileState::Present {
                    hash: hash_bytes(&node_before_bytes),
                    revision: before.subject.ticket_revision,
                },
                FileState::Present {
                    hash: hash_bytes(&node_after_bytes),
                    revision: node.revision,
                },
                &node_after_bytes,
            ),
        ];
        let intent = MultiTargetTransactionIntent::prepared_with_transaction_id(
            new_transaction_id(),
            event_id,
            "work.assignment.activated",
            &args.actor,
            targets,
            event_path,
            event_value,
        )?;
        let transaction = prepare_multi_target_transaction(&self.repo_root, intent)?;
        commit_prepared_multi_target_transaction(&transaction, self.failpoint)?;
        Ok(after)
    }

    pub fn release_reservation(
        &self,
        lease_id: &str,
        actor: &str,
        reason: &str,
    ) -> Result<CoreReservationV1> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        recover_prepared_transactions(&self.repo_root)?;
        authorize_assignment(&self.repo_root, actor, "work.assignment.release")?;
        let before = load_reservation(&self.repo_root, lease_id)?;
        if before.state == ReservationState::Released {
            return Ok(before);
        }
        if before.state != ReservationState::Reserved
            && before.state != ReservationState::Acknowledged
        {
            return Err(PulseError::validation(
                "reservation_release_unsafe",
                "only a not-yet-active reservation can be compensated",
            ));
        }
        let mut after = before.clone();
        after.state = ReservationState::Released;
        after.released_at = Some(Utc::now().to_rfc3339());
        after.release_reason = Some(reason.to_string());
        after.reservation_fingerprint = after.compute_fingerprint()?;
        commit_reservation_change(ReservationChange {
            repo_root: &self.repo_root,
            operation: "work.assignment.released",
            actor,
            ticket_id: &before.subject.ticket_id,
            before: Some(&before),
            after: &after,
            packet: None,
            payload: json!({"lease_id": lease_id, "reason": reason}),
            failpoint: self.failpoint,
        })?;
        Ok(after)
    }

    pub fn work_packet_for_reservation(
        &self,
        ticket_id: &str,
        lease_id: &str,
    ) -> Result<crate::work_packet::WorkPacketV1> {
        let reservation = load_reservation(&self.repo_root, lease_id)?;
        if reservation.subject.ticket_id != ticket_id
            || matches!(
                reservation.state,
                ReservationState::Released
                    | ReservationState::Expired
                    | ReservationState::StaleNeedsOperator
            )
        {
            return Err(PulseError::validation(
                "reservation_not_dispatchable",
                "reservation is not live for the requested Ticket",
            ));
        }
        let packet = load_packet(&self.repo_root, lease_id)?;
        if packet.packet_fingerprint != reservation.packet_fingerprint {
            return Err(PulseError::validation(
                "assignment_packet_invalid",
                "stored packet does not match reservation",
            ));
        }
        Ok(packet)
    }
}

struct ReservationChange<'a> {
    repo_root: &'a Path,
    operation: &'a str,
    actor: &'a str,
    ticket_id: &'a str,
    before: Option<&'a CoreReservationV1>,
    after: &'a CoreReservationV1,
    packet: Option<&'a crate::work_packet::WorkPacketV1>,
    payload: serde_json::Value,
    failpoint: Option<crate::storage::transaction::TransactionFailpoint>,
}

fn commit_reservation_change(change: ReservationChange<'_>) -> Result<()> {
    let ReservationChange {
        repo_root,
        operation,
        actor,
        ticket_id,
        before,
        after,
        packet,
        payload,
        failpoint,
    } = change;
    let reservation_bytes = to_canonical_bytes(after)?;
    let path = reservation_path(repo_root, &after.lease_id);
    let before_state = match before {
        Some(value) => {
            let bytes = to_canonical_bytes(value)?;
            FileState::Present {
                hash: hash_bytes(&bytes),
                revision: 0,
            }
        }
        None => FileState::Absent,
    };
    let mut targets = vec![TransactionTarget::new(
        path,
        before_state,
        FileState::Present {
            hash: hash_bytes(&reservation_bytes),
            revision: 0,
        },
        &reservation_bytes,
    )];
    if let Some(packet) = packet {
        let bytes = to_canonical_bytes(packet)?;
        targets.push(TransactionTarget::new(
            packet_path(repo_root, &after.lease_id),
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&bytes),
                revision: 0,
            },
            &bytes,
        ));
    }
    let now = Utc::now();
    let event_id = new_event_id();
    let event = EventEnvelope::new(event_id.clone(), operation, actor, ticket_id, payload, now);
    let intent = MultiTargetTransactionIntent::prepared_with_transaction_id(
        new_transaction_id(),
        event_id.clone(),
        operation,
        actor,
        targets,
        event_path(repo_root, &event_id, now),
        serde_json::to_value(event)?,
    )?;
    let transaction = prepare_multi_target_transaction(repo_root, intent)?;
    commit_prepared_multi_target_transaction(&transaction, failpoint)
}

fn authorize_assignment(repo_root: &Path, actor: &str, grant: &str) -> Result<()> {
    let report = crate::policy::load_authority_policy(repo_root)?;
    let principal = crate::policy::parse_actor(actor);
    crate::policy::authorize(&report, &principal, &[grant])
}

fn check_enrolled(repo_root: &Path) -> Result<()> {
    for relative in [
        ".pulse/workgraph/manifest.json",
        ".pulse/workgraph/schemas/node.schema.json",
    ] {
        if !repo_root.join(relative).is_file() {
            return Err(PulseError::validation(
                "not_enrolled",
                format!(
                    "repository {} is not enrolled: missing {relative}",
                    repo_root.display()
                ),
            ));
        }
    }
    Ok(())
}

fn deterministic_lease_id(key: &str) -> String {
    let digest = hash_bytes(key.as_bytes());
    format!(
        "lease_{}",
        digest
            .trim_start_matches("sha256:")
            .chars()
            .take(26)
            .collect::<String>()
    )
}

fn reservation_path(repo_root: &Path, lease_id: &str) -> PathBuf {
    repo_root
        .join(RESERVATIONS_DIR)
        .join(format!("{lease_id}.json"))
}

fn packet_path(repo_root: &Path, lease_id: &str) -> PathBuf {
    repo_root.join(PACKETS_DIR).join(format!("{lease_id}.json"))
}

fn event_path(repo_root: &Path, event_id: &str, now: DateTime<Utc>) -> PathBuf {
    repo_root
        .join(".pulse/events")
        .join(now.format("%Y-%m-%d").to_string())
        .join(format!("{event_id}.json"))
}

pub fn load_reservation(repo_root: &Path, lease_id: &str) -> Result<CoreReservationV1> {
    let path = reservation_path(repo_root, lease_id);
    let bytes = fs::read(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            PulseError::NotFound {
                subject: format!("reservation {lease_id}"),
            }
        } else {
            PulseError::io(&path, error)
        }
    })?;
    let record = serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))?;
    let record: CoreReservationV1 = record;
    record.validate()?;
    Ok(record)
}

pub fn list_reservations(repo_root: &Path) -> Result<Vec<CoreReservationV1>> {
    let directory = repo_root.join(RESERVATIONS_DIR);
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
            let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
            let record: CoreReservationV1 =
                serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))?;
            record.validate()?;
            Ok(record)
        })
        .collect()
}

fn load_packet(repo_root: &Path, lease_id: &str) -> Result<crate::work_packet::WorkPacketV1> {
    let path = packet_path(repo_root, lease_id);
    let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
    serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))
}

fn find_live_reservation_for_ticket(repo_root: &Path, ticket_id: &str) -> Result<Option<String>> {
    let dir = repo_root.join(RESERVATIONS_DIR);
    if !dir.exists() {
        return Ok(None);
    }
    let mut entries = fs::read_dir(&dir)
        .map_err(|error| PulseError::io(&dir, error))?
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|error| PulseError::io(&dir, error))?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        if entry.path().extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let bytes = fs::read(entry.path()).map_err(|error| PulseError::io(entry.path(), error))?;
        let reservation: CoreReservationV1 = serde_json::from_slice(&bytes)
            .map_err(|error| PulseError::json(entry.path(), error))?;
        if reservation.subject.ticket_id == ticket_id
            && matches!(
                reservation.state,
                ReservationState::Reserved
                    | ReservationState::Acknowledged
                    | ReservationState::Active
            )
        {
            return Ok(Some(reservation.lease_id));
        }
    }
    Ok(None)
}
