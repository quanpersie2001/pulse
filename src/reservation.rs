//! Core-owned exact work reservation contract.
//!
//! Runtime provisioning is deliberately absent. Project, Workspace, Session
//! and provider process identities enter only as opaque activation bindings
//! supplied by the daemon after explicit acknowledgement.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::canonical_json::hash_serializable;
use crate::{PulseError, Result};

pub const RESERVATION_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_TTL_SECONDS: u64 = 1800;
pub const MIN_TTL_SECONDS: u64 = 60;
pub const MAX_TTL_SECONDS: u64 = 86_400;
pub const CAP_MATCH_MATCHED: &str = "matched";
pub const CAP_MATCH_FAILED: &str = "failed";
pub const ERR_CAP_INVENTORY_INVALID: &str = "assignment_capability_inventory_invalid";
pub const ERR_CAP_PRINCIPAL_MISMATCH: &str = "assignment_capability_principal_mismatch";
pub const ERR_CAP_MISSING: &str = "assignment_capability_missing";
pub const CAPABILITY_INVENTORY_SCHEMA: &str =
    include_str!("schema/capability-inventory.schema.json");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CapabilityInventoryV1 {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub principal: String,
    pub inventory_id: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CapabilityMatchReport {
    pub inventory_identity: String,
    pub principal: String,
    pub status: String,
    pub required: Vec<String>,
    pub matched: Vec<String>,
    pub missing: Vec<String>,
    pub extra: Vec<String>,
    pub reason_codes: Vec<String>,
}

impl CapabilityInventoryV1 {
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self> {
        let value: Value = serde_json::from_slice(bytes).map_err(|error| {
            PulseError::validation(
                ERR_CAP_INVENTORY_INVALID,
                format!("capability inventory JSON is invalid: {error}"),
            )
        })?;
        let schema: Value = serde_json::from_str(CAPABILITY_INVENTORY_SCHEMA)?;
        let validator = jsonschema::JSONSchema::compile(&schema).map_err(|error| {
            PulseError::validation(
                ERR_CAP_INVENTORY_INVALID,
                format!("capability inventory schema is invalid: {error}"),
            )
        })?;
        if let Err(errors) = validator.validate(&value) {
            return Err(PulseError::validation(
                ERR_CAP_INVENTORY_INVALID,
                errors
                    .map(|error| error.to_string())
                    .collect::<Vec<_>>()
                    .join("; "),
            ));
        }
        serde_json::from_value(value).map_err(|error| {
            PulseError::validation(
                ERR_CAP_INVENTORY_INVALID,
                format!("capability inventory JSON is invalid: {error}"),
            )
        })
    }

    pub fn validate_principal(&self, expected: &str) -> Result<()> {
        if !self.principal.is_empty() && self.principal != expected {
            return Err(PulseError::validation(
                ERR_CAP_PRINCIPAL_MISMATCH,
                format!(
                    "capability inventory principal '{}' does not match assignee '{expected}'",
                    self.principal
                ),
            ));
        }
        Ok(())
    }

    pub fn match_required(
        &self,
        assignee: &str,
        required: &[String],
        isolated_worktree: bool,
    ) -> Result<CapabilityMatchReport> {
        self.validate_principal(assignee)?;
        let mut final_required = required.to_vec();
        if isolated_worktree {
            final_required.push("workspace.worktree".to_string());
        }
        normalize(&mut final_required);
        let mut inventory = self.capabilities.clone();
        normalize(&mut inventory);
        let matched = final_required
            .iter()
            .filter(|item| inventory.contains(item))
            .cloned()
            .collect::<Vec<_>>();
        let missing = final_required
            .iter()
            .filter(|item| !inventory.contains(item))
            .cloned()
            .collect::<Vec<_>>();
        let extra = inventory
            .iter()
            .filter(|item| !final_required.contains(item))
            .cloned()
            .collect::<Vec<_>>();
        let mut normalized = self.clone();
        normalize(&mut normalized.capabilities);
        let inventory_identity = hash_serializable(&normalized)?;
        Ok(CapabilityMatchReport {
            inventory_identity,
            principal: assignee.to_string(),
            status: if missing.is_empty() {
                CAP_MATCH_MATCHED
            } else {
                CAP_MATCH_FAILED
            }
            .to_string(),
            required: final_required,
            matched,
            missing: missing.clone(),
            extra,
            reason_codes: if missing.is_empty() {
                vec![]
            } else {
                vec![ERR_CAP_MISSING.to_string()]
            },
        })
    }
}

fn normalize(values: &mut Vec<String>) {
    values.retain(|value| !value.trim().is_empty());
    values.sort();
    values.dedup();
}

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
pub struct CoreReservationV1 {
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

impl CoreReservationV1 {
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
    pub capability_inventory_bytes: Vec<u8>,
    pub ttl_seconds: u64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReserveWorkOutcome {
    pub reservation: CoreReservationV1,
    pub packet: crate::work_packet::WorkPacketV1,
}

#[derive(Debug, Clone)]
pub struct ActivateReservationArgs {
    pub lease_id: String,
    pub actor: String,
    pub runtime_binding: RuntimeBinding,
    pub acknowledgement: AssignmentAcknowledgement,
}
