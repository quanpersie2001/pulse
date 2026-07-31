//! Durable Core/Daemon provisioning saga state.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentSagaState {
    Reserving,
    Reserved,
    WorkspaceReady,
    SessionReady,
    /// Durable bootstrap delivery intent has been persisted BEFORE provider
    /// I/O. While in this state the provider may or may not have accepted the
    /// bootstrap; the daemon must never blindly re-send and must never release
    /// the lease without proof.
    DeliveryPending,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    /// Intent persisted before provider I/O; provider outcome unknown.
    IntentRecorded,
    /// Transport-level delivery acknowledgement was persisted after I/O.
    Delivered,
    /// Delivery failed in the live daemon and the saga compensated.
    Failed,
    /// Restart found intent without a persisted delivered acknowledgement;
    /// the provider outcome cannot be proven and recovery fails closed.
    Uncertain,
}

/// Durable delivery-intent/outbox record for one assignment bootstrap. Written
/// with a deterministic `delivery_id` and the exact payload BEFORE any
/// provider I/O, then updated with the provider correlation identifiers when
/// the transport-level acknowledgement is persisted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeliveryRecord {
    pub schema_version: u32,
    pub delivery_id: String,
    pub saga_id: String,
    pub session_id: String,
    /// Exact bootstrap payload handed to the provider.
    pub payload: String,
    /// Provider request identifier when the provider protocol exposes one
    /// before I/O (e.g. `pulse-turn-start-<ulid>`); `None` for opaque
    /// transports.
    pub correlation_request_id: Option<String>,
    /// Provider turn identifier from the transport acknowledgement; recorded
    /// only when the delivered state is persisted.
    pub correlation_turn_id: Option<String>,
    pub state: DeliveryState,
    pub created_at: String,
    pub updated_at: String,
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
