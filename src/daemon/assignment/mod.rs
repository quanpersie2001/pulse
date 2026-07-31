//! Durable Core/Daemon provisioning saga state.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentSagaState {
    Reserving,
    Reserved,
    WorkspaceReady,
    SessionReady,
    BootstrapDelivered,
    Acknowledged,
    Activated,
    Verifying,
    Done,
    Rework,
    Blocked,
    Compensating,
    Released,
    Recoverable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentSagaRecord {
    pub schema_version: u32,
    pub saga_id: String,
    pub idempotency_key: String,
    #[serde(default)]
    pub request_fingerprint: String,
    pub project_id: String,
    pub ticket_id: String,
    pub actor: String,
    pub assignee: String,
    pub ticket_revision: u64,
    pub packet_fingerprint: String,
    pub lease_id: Option<String>,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub delivery_id: Option<String>,
    pub acknowledgement_id: Option<String>,
    pub handoff_id: Option<String>,
    pub verification_id: Option<String>,
    pub state: AssignmentSagaState,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
