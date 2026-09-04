//! Core-owned exact work reservation contract.
//!
//! Runtime provisioning is deliberately absent. Project, Workspace, Session
//! and provider process identities enter only as opaque activation bindings
//! supplied by the daemon after explicit acknowledgement.

use serde::{Deserialize, Serialize};

use crate::canonical_json::hash_serializable;
use crate::{PulseError, Result};

pub const RESERVATION_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_TTL_SECONDS: u64 = 1800;
pub const MIN_TTL_SECONDS: u64 = 60;
pub const MAX_TTL_SECONDS: u64 = 86_400;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReservationState {
    Reserved,
    Acknowledged,
    Active,
    Released,
    Expired,
    StaleNeedsOperator,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReservationSubject {
    pub ticket_id: String,
    pub ticket_revision: u64,
    pub contract_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReservationSource {
    pub repository_id: String,
    pub commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeBinding {
    pub project_id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub provider_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentAcknowledgement {
    pub acknowledgement_id: String,
    pub delivery_id: String,
    pub session_id: String,
    pub packet_fingerprint: String,
    pub acknowledged_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CoreReservation {
    pub schema_version: u32,
    pub reservation_id: String,
    pub lease_id: String,
    pub idempotency_key_hash: String,
    pub subject: ReservationSubject,
    pub assignee: String,
    pub issued_by: String,
    pub issued_at: String,
    pub expires_at: String,
    pub packet_fingerprint: String,
    pub readiness_fingerprint: String,
    pub source: ReservationSource,
    pub state: ReservationState,
    pub runtime_binding: Option<RuntimeBinding>,
    pub acknowledgement: Option<AssignmentAcknowledgement>,
    pub activated_revision: Option<u64>,
    pub released_at: Option<String>,
    pub release_reason: Option<String>,
    pub reservation_fingerprint: String,
}

impl CoreReservation {
    pub fn compute_fingerprint(&self) -> Result<String> {
        let mut projection = self.clone();
        projection.reservation_fingerprint.clear();
        hash_serializable(&projection)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != RESERVATION_SCHEMA_VERSION
            || !self.reservation_id.starts_with("rsv_")
            || !self.lease_id.starts_with("lease_")
            || self.subject.ticket_id.trim().is_empty()
            || self.packet_fingerprint.trim().is_empty()
            || self.readiness_fingerprint.trim().is_empty()
        {
            return Err(PulseError::validation(
                "reservation_record_invalid",
                "reservation record is structurally invalid",
            ));
        }
        if self.compute_fingerprint()? != self.reservation_fingerprint {
            return Err(PulseError::validation(
                "reservation_fingerprint_mismatch",
                "reservation fingerprint does not match canonical contents",
            ));
        }
        match self.state {
            ReservationState::Reserved => {
                if self.runtime_binding.is_some() || self.acknowledgement.is_some() {
                    return Err(PulseError::validation(
                        "reservation_record_invalid",
                        "reserved state must not carry runtime activation fields",
                    ));
                }
            }
            ReservationState::Acknowledged | ReservationState::Active => {
                if self.runtime_binding.is_none() || self.acknowledgement.is_none() {
                    return Err(PulseError::validation(
                        "reservation_record_invalid",
                        "acknowledged/active state requires runtime binding and acknowledgement",
                    ));
                }
            }
            ReservationState::Released
            | ReservationState::Expired
            | ReservationState::StaleNeedsOperator => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ReserveWorkArgs {
    pub ticket_id: String,
    pub actor: String,
    pub assignee: String,
    pub ttl_seconds: u64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReserveWorkOutcome {
    pub reservation: CoreReservation,
    pub packet: crate::work_packet::WorkPacket,
}

#[derive(Debug, Clone)]
pub struct ActivateReservationArgs {
    pub lease_id: String,
    pub actor: String,
    pub runtime_binding: RuntimeBinding,
    pub acknowledgement: AssignmentAcknowledgement,
}

#[derive(Debug, Clone)]
pub struct AcknowledgeReservationArgs {
    pub lease_id: String,
    pub actor: String,
    pub runtime_binding: RuntimeBinding,
    pub acknowledgement: AssignmentAcknowledgement,
}
