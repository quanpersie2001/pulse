//! Public neutral PreparedAssignmentV1 and companion DTOs (P2S2-I1).
//!
//! This module defines the assigned-work contract for Phase 2 prepared
//! assignments. Every type is a pure value DTO with
//! `#[serde(deny_unknown_fields)]` on every struct. There are no graph, docs,
//! source, filesystem or Git imports — only serde, `canonical_json` and error
//! primitives.
//!
//! Ownership: `src/assignment.rs` is the public neutral value owner.
//! Cross-domain composition belongs in `src/kernel/assignment.rs` (future).
//!
//! See `proposals/phase2-slice2-atomic-reservation-workspace-binding.md`.
//!
//! # Stability
//!
//! `PreparedAssignmentV1` is the first wrapper allowed to set
//! `dispatch_authorized=true`. The nested `WorkPacketV1` remains a preview with
//! `dispatch_authorized=false` and unchanged `WorkPacketV1` semantics.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::canonical_json;
use crate::work_packet::WorkPacketV1;
use crate::PulseError;
use crate::PulseResult;

// ---------------------------------------------------------------------------
// Constants / profile
// ---------------------------------------------------------------------------

/// Assignment schema version.
pub const ASSIGNMENT_SCHEMA_VERSION: u32 = 1;

/// Prepared-assignment profile identifier.
pub const PREPARED_ASSIGNMENT_PROFILE: &str = "phase2_prepared_assignment_v1";

/// Lease schema version.
pub const LEASE_SCHEMA_VERSION: u32 = 1;

/// Lease kind for implementation assignments.
pub const LEASE_KIND_IMPLEMENTATION: &str = "implementation_assignment";

/// Workspace record schema version.
pub const WORKSPACE_SCHEMA_VERSION: u32 = 1;

/// Capability inventory schema version.
pub const CAPABILITY_INVENTORY_SCHEMA_VERSION: u32 = 1;

/// Capability match report version.
pub const CAPABILITY_MATCH_REPORT_VERSION: u32 = 1;

/// Default TTL for a prepared assignment lease (1800 seconds).
pub const DEFAULT_TTL_SECONDS: u64 = 1800;

/// Minimum allowed TTL.
pub const MIN_TTL_SECONDS: u64 = 60;

/// Maximum allowed TTL.
pub const MAX_TTL_SECONDS: u64 = 86_400;

/// Workspace mode: in-place (repo root).
pub const WORKSPACE_MODE_IN_PLACE: &str = "in_place";

/// Workspace mode: isolated worktree.
pub const WORKSPACE_MODE_ISOLATED: &str = "isolated_worktree";

/// Workspace binding state: bound.
pub const WORKSPACE_STATE_BOUND: &str = "bound";

/// Workspace binding state: released.
pub const WORKSPACE_STATE_RELEASED: &str = "released";

/// Workspace binding state: stale_needs_operator.
pub const WORKSPACE_STATE_STALE: &str = "stale_needs_operator";

/// Lease state: prepared.
pub const LEASE_STATE_PREPARED: &str = "prepared";

/// Lease state: released (tombstone terminal).
pub const LEASE_STATE_RELEASED: &str = "released";

/// Lease state: expired (tombstone terminal).
pub const LEASE_STATE_EXPIRED: &str = "expired";

/// Lease state: stale_needs_operator (tombstone terminal).
pub const LEASE_STATE_STALE: &str = "stale_needs_operator";

/// Capability match status: matched.
pub const CAP_MATCH_MATCHED: &str = "matched";

/// Capability match status: failed.
pub const CAP_MATCH_FAILED: &str = "failed";

/// Dispatch authorization status for prepared assignments.
pub const DISPATCH_AUTHORIZED_STATUS: &str = "prepared_assignment";

/// Runner status: not_started.
pub const RUNNER_STATUS_NOT_STARTED: &str = "not_started";

/// Runner status: not_installed.
pub const RUNNER_STATUS_NOT_INSTALLED: &str = "not_installed";

/// Gate status: passed.
pub const GATE_STATUS_PASSED: &str = "passed";

/// Gate status: not_evaluated.
pub const GATE_STATUS_NOT_EVALUATED: &str = "not_evaluated";

/// Gate status: not_installed.
pub const GATE_STATUS_NOT_INSTALLED: &str = "not_installed";

/// Lifecycle transition ready -> active.
pub const LIFECYCLE_READY_TO_ACTIVE: &str = "ready_to_active";

/// Lifecycle gate profile.
pub const LIFECYCLE_GATE_PROFILE: &str = "phase2_prepared_assignment_v1";

/// JSON Schema for `PreparedAssignmentV1`.
pub const PREPARED_ASSIGNMENT_SCHEMA: &str = include_str!("schema/prepared-assignment.schema.json");

/// JSON Schema for `PreparedAssignmentRecordV1`.
pub const PREPARED_ASSIGNMENT_RECORD_SCHEMA: &str =
    include_str!("schema/prepared-assignment-record.schema.json");

/// JSON Schema for `AssignmentLeaseRecordV1`.
pub const ASSIGNMENT_LEASE_SCHEMA: &str = include_str!("schema/assignment-lease.schema.json");

/// JSON Schema for `AssignmentWorkspaceRecordV1`.
pub const ASSIGNMENT_WORKSPACE_SCHEMA: &str =
    include_str!("schema/assignment-workspace.schema.json");

/// JSON Schema for `CapabilityInventoryV1`.
pub const CAPABILITY_INVENTORY_SCHEMA: &str =
    include_str!("schema/capability-inventory.schema.json");

/// JSON Schema for `CapabilityMatchReport`.
pub const CAPABILITY_MATCH_SCHEMA: &str = include_str!("schema/capability-match.schema.json");

/// Tombstone schema version.
pub const TOMBSTONE_SCHEMA_VERSION: u32 = 1;

/// JSON Schema for `AssignmentTombstoneV1`.
pub const ASSIGNMENT_TOMBSTONE_SCHEMA: &str =
    include_str!("schema/assignment-tombstone.schema.json");

/// Tombstone state: released (terminal).
pub const TOMBSTONE_STATE_RELEASED: &str = "released";

/// Tombstone state: expired (terminal).
pub const TOMBSTONE_STATE_EXPIRED: &str = "expired";

/// Tombstone state: stale_needs_operator (terminal).
pub const TOMBSTONE_STATE_STALE: &str = "stale_needs_operator";

// ---------------------------------------------------------------------------
// Top-level PreparedAssignmentV1
// ---------------------------------------------------------------------------

/// Complete Phase 2 prepared assignment (P2S2-D1).
///
/// This is a wrapper over `WorkPacketV1`. The nested packet retains preview
/// semantics (`dispatch_authorized=false`). The outer wrapper sets
/// `dispatch.dispatch_authorized=true` only after lease, workspace,
/// capability, source and lifecycle gates pass.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PreparedAssignmentV1 {
    pub schema_version: u32,
    pub profile: String,
    pub code: String,
    pub prepared_assignment_id: String,
    pub subject: AssignmentSubjectSnapshot,
    /// Embedded preview packet (unchanged WorkPacketV1).
    pub packet: WorkPacketV1,
    pub packet_fingerprint: String,
    pub revalidated_snapshot: RevalidatedSnapshot,
    pub lease: AssignmentLeaseSummary,
    pub workspace: AssignmentWorkspaceSummary,
    pub capability_match: CapabilityMatchReport,
    pub lifecycle: AssignmentLifecycle,
    pub dispatch: AssignmentDispatch,
    pub transaction: AssignmentTransaction,
    /// sha256 fingerprint of the canonical fingerprint projection.
    pub prepared_assignment_fingerprint: String,
    #[serde(default)]
    pub reason_codes: Vec<String>,
}

// ---------------------------------------------------------------------------
// Subject snapshot (assignment-specific)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentSubjectSnapshot {
    pub id: String,
    pub kind: String,
    pub revision_before: u64,
    pub revision_after: u64,
    pub contract_revision: u64,
    pub status_before: String,
    pub status_after: String,
}

// ---------------------------------------------------------------------------
// Revalidated snapshot
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RevalidatedSnapshot {
    pub graph_fingerprint: String,
    pub readiness_profile: String,
    pub readiness_fingerprint: String,
    pub authority_policy_fingerprint: String,
    pub docs_registry_fingerprint: String,
    pub docs_index_fingerprint: String,
    pub source_commit: String,
    pub source_cleanliness: String,
    pub repository_id: String,
}

// ---------------------------------------------------------------------------
// Lease summary (embedded in PreparedAssignmentV1.dispatch context)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentLeaseSummary {
    pub lease_id: String,
    pub state: String,
    pub assignee: String,
    pub issued_by: String,
    pub issued_at: String,
    pub expires_at: String,
    pub ttl_seconds: u64,
    pub exclusive: bool,
}

// ---------------------------------------------------------------------------
// Workspace summary (embedded in PreparedAssignmentV1.dispatch context)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentWorkspaceSummary {
    pub workspace_id: String,
    pub binding_status: String,
    pub mode: String,
    pub path: String,
    pub repository_id: String,
    pub base_commit: String,
    pub cleanliness: String,
    pub owner_lease_id: String,
}

// ---------------------------------------------------------------------------
// Capability match report
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CapabilityMatchReport {
    pub inventory_identity: String,
    pub principal: String,
    pub status: String,
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub matched: Vec<String>,
    #[serde(default)]
    pub missing: Vec<String>,
    #[serde(default)]
    pub extra: Vec<String>,
    #[serde(default)]
    pub reason_codes: Vec<String>,
}

// ---------------------------------------------------------------------------
// Capability inventory (input DTO)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CapabilityInventoryV1 {
    pub schema_version: u32,
    pub principal: String,
    pub inventory_id: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentLifecycle {
    pub transition: String,
    pub gate_profile: String,
    pub gate_status: String,
    pub expected_revision: u64,
    pub new_revision: u64,
    pub event_id: String,
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentDispatch {
    pub dispatch_authorized: bool,
    pub authorization_status: String,
    pub runner_status: String,
    #[serde(default)]
    pub gate_families: Vec<AssignmentGateFamily>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentGateFamily {
    pub family: String,
    pub status: String,
    #[serde(default)]
    pub reason_codes: Vec<String>,
}

// ---------------------------------------------------------------------------
// Transaction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentTransaction {
    pub transaction_id: String,
    #[serde(default)]
    pub committed_targets: Vec<String>,
    pub event_path: String,
    pub recovery_state: String,
}

// ---------------------------------------------------------------------------
// Assignment lease record (persisted runtime state)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentLeaseRecordV1 {
    pub schema_version: u32,
    pub lease_id: String,
    pub kind: String,
    pub subject: AssignmentLeaseSubject,
    pub assignee: AssignmentLeaseAssignee,
    pub issued_by: String,
    pub issued_at: String,
    pub expires_at: String,
    pub ttl_seconds: u64,
    pub state: String,
    pub packet_fingerprint: String,
    pub readiness_fingerprint: String,
    pub workspace_id: String,
    pub prepared_assignment_id: String,
    pub capability_inventory_identity: String,
    pub source: AssignmentLeaseSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentLeaseSubject {
    pub kind: String,
    pub id: String,
    pub revision: u64,
    pub contract_revision: u64,
    pub status_at_claim: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentLeaseAssignee {
    pub principal: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentLeaseSource {
    pub repository_id: String,
    pub base_commit: String,
}

// ---------------------------------------------------------------------------
// Workspace record (persisted runtime state)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentWorkspaceRecordV1 {
    pub schema_version: u32,
    pub workspace_id: String,
    pub lease_id: String,
    pub prepared_assignment_id: String,
    pub subject: WorkspaceSubjectRef,
    pub mode: String,
    pub path: String,
    pub repository_id: String,
    pub base_commit: String,
    pub head_commit_at_bind: String,
    pub cleanliness_at_bind: String,
    pub state: String,
    pub created_at: String,
    pub released_at: Option<String>,
    pub cleanup: WorkspaceCleanupPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceSubjectRef {
    pub kind: String,
    pub id: String,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceCleanupPolicy {
    pub policy: String,
    pub status: String,
}

// ---------------------------------------------------------------------------
// Prepared assignment record (persisted runtime state)
// ---------------------------------------------------------------------------

/// Persisted prepared-assignment record that mirrors `PreparedAssignmentV1`
/// for local recovery and transaction tracking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PreparedAssignmentRecordV1 {
    pub schema_version: u32,
    pub profile: String,
    pub code: String,
    pub prepared_assignment_id: String,
    pub subject: AssignmentSubjectSnapshot,
    pub packet_fingerprint: String,
    pub revalidated_snapshot: RevalidatedSnapshot,
    pub lease: AssignmentLeaseSummary,
    pub workspace: AssignmentWorkspaceSummary,
    pub capability_match: CapabilityMatchReport,
    pub lifecycle: AssignmentLifecycle,
    pub dispatch: AssignmentDispatch,
    pub transaction: AssignmentTransaction,
    pub prepared_assignment_fingerprint: String,
    #[serde(default)]
    pub reason_codes: Vec<String>,
}

// ---------------------------------------------------------------------------
// Tombstone record (persisted terminal lease state)
// ---------------------------------------------------------------------------

/// Terminal tombstone record for a prepared-assignment lease.
///
/// Written atomically with live-lease removal during release/recovery.
/// A tombstone prevents duplicate claims by proving the lease ID is no
/// longer live, even if the live lease file was already removed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentTombstoneV1 {
    pub schema_version: u32,
    pub lease_id: String,
    pub subject_id: String,
    pub state: String,
    pub recorded_at: String,
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default)]
    pub reason_codes: Vec<String>,
}

impl AssignmentTombstoneV1 {
    /// Normalize (no set-like collections currently).
    pub fn normalize(&mut self) {
        sort_strings(&mut self.reason_codes);
    }
}

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

impl PreparedAssignmentV1 {
    /// Normalize every set-like collection for deterministic ordering.
    pub fn normalize(&mut self) {
        sort_strings(&mut self.reason_codes);
        self.packet.normalize();
        self.capability_match.normalize();
        self.dispatch.normalize();
    }
}

impl PreparedAssignmentRecordV1 {
    /// Normalize every set-like collection for deterministic ordering.
    pub fn normalize(&mut self) {
        sort_strings(&mut self.reason_codes);
        self.capability_match.normalize();
        self.dispatch.normalize();
    }
}

impl CapabilityMatchReport {
    pub fn normalize(&mut self) {
        sort_strings(&mut self.required);
        sort_strings(&mut self.matched);
        sort_strings(&mut self.missing);
        sort_strings(&mut self.extra);
        sort_strings(&mut self.reason_codes);
    }
}

impl AssignmentDispatch {
    pub fn normalize(&mut self) {
        for family in &mut self.gate_families {
            sort_strings(&mut family.reason_codes);
        }
        self.gate_families.sort_by(|a, b| a.family.cmp(&b.family));
    }
}

impl CapabilityInventoryV1 {
    /// Normalize capabilities.
    pub fn normalize(&mut self) {
        sort_strings(&mut self.capabilities);
    }
}

impl AssignmentLeaseRecordV1 {
    /// Normalize (no set-like collections currently).
    pub fn normalize(&mut self) {
        // No set-like fields.
    }
}

impl AssignmentWorkspaceRecordV1 {
    /// Normalize (no set-like collections currently).
    pub fn normalize(&mut self) {
        // No set-like fields.
    }
}

// ---------------------------------------------------------------------------
// Fingerprint projections
// ---------------------------------------------------------------------------

impl PreparedAssignmentV1 {
    /// Compute the canonical prepared-assignment fingerprint.
    ///
    /// The fingerprint projection excludes:
    ///   - `prepared_assignment_fingerprint` (self-reference)
    ///
    /// It includes: schema, profile, prepared_assignment_id, subject,
    /// packet_fingerprint, revalidated_snapshot, lease summary,
    /// workspace summary, capability_match, lifecycle, dispatch gate
    /// statuses, transaction ID and event path, and reason_codes.
    pub fn compute_fingerprint(&self) -> PulseResult<String> {
        let mut owned = self.clone();
        owned.normalize();
        let value = serde_json::to_value(&owned)?;
        let projection = strip_prepared_assignment_fingerprint_projection(&value);
        let canonical = canonical_json::to_canonical_value(&projection)?;
        let bytes = canonical_json::canonical_value_bytes(&canonical)?;
        Ok(canonical_json::hash_bytes(&bytes))
    }
}

impl PreparedAssignmentRecordV1 {
    /// Compute the canonical prepared-assignment record fingerprint.
    ///
    /// The runtime record is a lossless projection of the prepared assignment
    /// without embedded packet bytes. It therefore uses the same fingerprint
    /// projection as `PreparedAssignmentV1`: exclude the self-reference and the
    /// convenience `packet` field, and rely on `packet_fingerprint` as the packet
    /// identity boundary.
    pub fn compute_fingerprint(&self) -> PulseResult<String> {
        let mut owned = self.clone();
        owned.normalize();
        let value = serde_json::to_value(&owned)?;
        let projection = strip_prepared_assignment_fingerprint_projection(&value);
        let canonical = canonical_json::to_canonical_value(&projection)?;
        let bytes = canonical_json::canonical_value_bytes(&canonical)?;
        Ok(canonical_json::hash_bytes(&bytes))
    }
}

/// Strip self-referential/non-semantic fields from the prepared-assignment
/// fingerprint projection.
fn strip_prepared_assignment_fingerprint_projection(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, child) in map {
                if key == "prepared_assignment_fingerprint" || key == "packet" {
                    continue;
                }
                out.insert(
                    key.clone(),
                    strip_prepared_assignment_fingerprint_projection(child),
                );
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(strip_prepared_assignment_fingerprint_projection)
                .collect(),
        ),
        other => other.clone(),
    }
}

impl CapabilityInventoryV1 {
    /// Compute the canonical inventory identity hash.
    pub fn compute_inventory_identity(&self) -> PulseResult<String> {
        let mut owned = self.clone();
        owned.normalize();
        let value = serde_json::to_value(&owned)?;
        let canonical = canonical_json::to_canonical_value(&value)?;
        let bytes = canonical_json::canonical_value_bytes(&canonical)?;
        Ok(canonical_json::hash_bytes(&bytes))
    }
}

impl AssignmentLeaseRecordV1 {
    /// Compute the canonical lease record fingerprint.
    pub fn compute_fingerprint(&self) -> PulseResult<String> {
        let value = serde_json::to_value(self)?;
        let canonical = canonical_json::to_canonical_value(&value)?;
        let bytes = canonical_json::canonical_value_bytes(&canonical)?;
        Ok(canonical_json::hash_bytes(&bytes))
    }
}

impl AssignmentWorkspaceRecordV1 {
    /// Compute the canonical workspace record fingerprint.
    pub fn compute_fingerprint(&self) -> PulseResult<String> {
        let value = serde_json::to_value(self)?;
        let canonical = canonical_json::to_canonical_value(&value)?;
        let bytes = canonical_json::canonical_value_bytes(&canonical)?;
        Ok(canonical_json::hash_bytes(&bytes))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sort_strings(v: &mut Vec<String>) {
    v.sort();
    v.dedup();
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

impl Default for AssignmentDispatch {
    fn default() -> Self {
        Self {
            dispatch_authorized: true,
            authorization_status: DISPATCH_AUTHORIZED_STATUS.to_string(),
            runner_status: RUNNER_STATUS_NOT_STARTED.to_string(),
            gate_families: vec![
                AssignmentGateFamily {
                    family: "packet_revalidation".to_string(),
                    status: GATE_STATUS_PASSED.to_string(),
                    reason_codes: vec![],
                },
                AssignmentGateFamily {
                    family: "lease".to_string(),
                    status: GATE_STATUS_PASSED.to_string(),
                    reason_codes: vec![],
                },
                AssignmentGateFamily {
                    family: "workspace_binding".to_string(),
                    status: GATE_STATUS_PASSED.to_string(),
                    reason_codes: vec![],
                },
                AssignmentGateFamily {
                    family: "capability_match".to_string(),
                    status: GATE_STATUS_PASSED.to_string(),
                    reason_codes: vec![],
                },
                AssignmentGateFamily {
                    family: "lifecycle".to_string(),
                    status: GATE_STATUS_PASSED.to_string(),
                    reason_codes: vec![],
                },
                AssignmentGateFamily {
                    family: "runner".to_string(),
                    status: GATE_STATUS_NOT_INSTALLED.to_string(),
                    reason_codes: vec!["runner_not_started_by_slice2".to_string()],
                },
                AssignmentGateFamily {
                    family: "handoff".to_string(),
                    status: GATE_STATUS_NOT_INSTALLED.to_string(),
                    reason_codes: vec!["handoff_gate_not_installed".to_string()],
                },
                AssignmentGateFamily {
                    family: "verification".to_string(),
                    status: GATE_STATUS_NOT_INSTALLED.to_string(),
                    reason_codes: vec!["verification_runner_not_installed".to_string()],
                },
            ],
        }
    }
}

impl Default for AssignmentTransaction {
    fn default() -> Self {
        Self {
            transaction_id: String::new(),
            committed_targets: vec![],
            event_path: String::new(),
            recovery_state: "complete".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Error code constants for capability matching
// ---------------------------------------------------------------------------

/// Capability inventory file is missing or unreadable.
pub const ERR_CAP_INVENTORY_MISSING: &str = "assignment_capability_inventory_missing";

/// Capability inventory has an invalid schema.
pub const ERR_CAP_INVENTORY_INVALID: &str = "assignment_capability_inventory_invalid";

/// Capability inventory principal does not match the claim assignee.
pub const ERR_CAP_PRINCIPAL_MISMATCH: &str = "assignment_capability_principal_mismatch";

/// One or more required capabilities are missing from the inventory.
pub const ERR_CAP_MISSING: &str = "assignment_capability_missing";

// ---------------------------------------------------------------------------
// Workspace-induced capability requirement
// ---------------------------------------------------------------------------

/// Capability string for managing isolated worktrees.
pub const CAP_WORKSPACE_WORKTREE: &str = "workspace.worktree";

// ---------------------------------------------------------------------------
// Capability matching
// ---------------------------------------------------------------------------

/// Build the final required-capability set from packet requirements and
/// workspace-mode-induced requirements.
///
/// When `workspace_mode` is `"isolated_worktree"`, the capability
/// `"workspace.worktree"` is implicitly required, even when the packet
/// only allowed in-place.
pub fn capability_required_set(required: &[String], workspace_mode: Option<&str>) -> Vec<String> {
    let mut set: Vec<String> = required.to_vec();
    if let Some(mode) = workspace_mode {
        if mode == WORKSPACE_MODE_ISOLATED && !set.iter().any(|c| c == CAP_WORKSPACE_WORKTREE) {
            set.push(CAP_WORKSPACE_WORKTREE.to_string());
        }
    }
    set.sort();
    set.dedup();
    set
}

impl CapabilityInventoryV1 {
    /// Parse and validate capability inventory from serialized JSON bytes.
    ///
    /// Returns:
    /// - `Err(assignment_capability_inventory_invalid)` if the JSON does not
    ///   match the `CapabilityInventoryV1` schema or if `schema_version` is
    ///   not 1.
    pub fn from_json_bytes(bytes: &[u8]) -> PulseResult<Self> {
        let inv: Self = serde_json::from_slice(bytes).map_err(|e| {
            PulseError::validation(
                ERR_CAP_INVENTORY_INVALID,
                format!("capability inventory JSON is invalid: {e}"),
            )
        })?;

        if inv.schema_version != CAPABILITY_INVENTORY_SCHEMA_VERSION {
            return Err(PulseError::validation(
                ERR_CAP_INVENTORY_INVALID,
                format!(
                    "capability inventory schema_version {} != expected {}",
                    inv.schema_version, CAPABILITY_INVENTORY_SCHEMA_VERSION
                ),
            ));
        }

        Ok(inv)
    }

    /// Validate that the inventory principal matches the expected assignee.
    pub fn validate_principal(&self, expected: &str) -> PulseResult<()> {
        if self.principal != expected {
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

    /// Match this capability inventory against the given required set and
    /// optional workspace mode. Returns a complete `CapabilityMatchReport`.
    ///
    /// This method:
    /// 1. Validates the principal matches `assignee`.
    /// 2. Builds the final required set from packet requirements and
    ///    workspace-induced requirements.
    /// 3. Sorts/dedupes the inventory capabilities.
    /// 4. Computes matched, missing, and extra sets.
    /// 5. Computes the canonical inventory identity.
    /// 6. Returns the report with status `matched` or `failed`.
    pub fn match_required(
        &self,
        assignee: &str,
        required: &[String],
        workspace_mode: Option<&str>,
    ) -> PulseResult<CapabilityMatchReport> {
        self.validate_principal(assignee)?;

        let final_required = capability_required_set(required, workspace_mode);
        let inventory_id = self.compute_inventory_identity()?;

        // Normalize inventory capabilities for deterministic matching.
        let mut inventory_caps = self.capabilities.clone();
        sort_strings(&mut inventory_caps);

        let matched: Vec<String> = final_required
            .iter()
            .filter(|r| inventory_caps.iter().any(|c| c == *r))
            .cloned()
            .collect();

        let missing: Vec<String> = final_required
            .iter()
            .filter(|r| !inventory_caps.iter().any(|c| c == *r))
            .cloned()
            .collect();

        let extra: Vec<String> = inventory_caps
            .iter()
            .filter(|c| !final_required.iter().any(|r| r == *c))
            .cloned()
            .collect();

        let status = if missing.is_empty() {
            CAP_MATCH_MATCHED.to_string()
        } else {
            CAP_MATCH_FAILED.to_string()
        };

        let mut report = CapabilityMatchReport {
            inventory_identity: inventory_id,
            principal: assignee.to_string(),
            status,
            required: final_required.clone(),
            matched,
            missing,
            extra,
            reason_codes: vec![],
        };
        report.normalize();
        Ok(report)
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    // -----------------------------------------------------------------------
    // Helper: build a minimal prepared assignment for tests
    // -----------------------------------------------------------------------

    fn dummy_packet() -> WorkPacketV1 {
        // Build a minimal WorkPacketV1 using the existing test helper
        // pattern. We construct inline to avoid importing test-only helpers.
        use crate::work_packet::{
            PacketAssurance, PacketBudget, PacketCapabilities, PacketDispatch, PacketKnowledge,
            PacketScope, PacketScopeHints, PacketSource, PacketWorkspace, PACKET_PROFILE,
            PACKET_SCHEMA_VERSION,
        };
        use crate::work_packet::{
            PacketContext, PacketContractScope, PacketDecisionFrontier, PacketDocsApplicability,
            PacketDocsIndex, PacketDocumentation, PacketDocumentationImpact, PacketEffortMetadata,
            PacketFutureGate, PacketGraph, PacketImplementationContractV1, PacketQaStatus,
            PacketReadBudget, PacketRelationBundle, PacketShaping, PacketShapingWorkBinding,
            PacketSuggestionQuery, SnapshotReport, SubjectSnapshot,
        };
        WorkPacketV1 {
            schema_version: PACKET_SCHEMA_VERSION,
            profile: PACKET_PROFILE.to_string(),
            code: "reservation_candidate".to_string(),
            subject: SubjectSnapshot {
                id: "TK-001".to_string(),
                kind: "ticket".to_string(),
                role: "implementation".to_string(),
                title: "Test ticket".to_string(),
                revision: 1,
                contract_revision: 1,
                status: "ready".to_string(),
                risk: "low".to_string(),
                materialization: "R1".to_string(),
                content_dir: "works/TK-001".to_string(),
            },
            snapshot: SnapshotReport {
                graph_fingerprint:
                    "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                        .to_string(),
                readiness_profile: "phase1_contract_readiness_v1".to_string(),
                readiness_fingerprint:
                    "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                        .to_string(),
                readiness_status: "ready".to_string(),
                authority_policy_revision: 1,
                authority_policy_fingerprint:
                    "sha256:2222222222222222222222222222222222222222222222222222222222222222"
                        .to_string(),
                docs_registry_revision: 1,
                docs_registry_fingerprint:
                    "sha256:3333333333333333333333333333333333333333333333333333333333333333"
                        .to_string(),
                docs_index_fingerprint:
                    "sha256:4444444444444444444444444444444444444444444444444444444444444444"
                        .to_string(),
                source_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
            },
            contract: PacketImplementationContractV1 {
                mode: "guided".to_string(),
                work_surface: "code".to_string(),
                plan_policy: "worker_optional".to_string(),
                semantic_impact: "behavior_or_public_risk_change".to_string(),
                effort: PacketEffortMetadata::default(),
                verification_profile: "service-change".to_string(),
                brief: None,
                objective: "Enable token rotation".to_string(),
                current_behavior: "No rotation".to_string(),
                target_behavior: "Atomic rotation".to_string(),
                code_anchors: vec![],
                documentation_anchors: vec![],
                configuration_anchors: vec![],
                data_anchors: vec![],
                research_refs: vec![],
                required_changes: vec![],
                invariants: vec![],
                acceptance: vec![],
                scope: PacketContractScope::default(),
                implementation_freedom: vec![],
                required_decisions: vec![],
                shared_approach_refs: vec![],
                expected_evidence: vec![],
                expected_handoff: vec![],
            },
            context: PacketContext {
                parents: vec![],
                decisions: vec![],
            },
            shaping: PacketShaping {
                status: "current".to_string(),
                receipt_id: "rcpt_00000000000000000000000000".to_string(),
                receipt_hash:
                    "sha256:5555555555555555555555555555555555555555555555555555555555555555"
                        .to_string(),
                owning_work: PacketShapingWorkBinding {
                    id: "ST-001".to_string(),
                    revision_observed: 3,
                    contract_revision: 2,
                },
                shape_mode: "focused_branches".to_string(),
                destination: None,
                map: None,
                critical_branches: vec![],
                bounded_fog: vec![],
                remaining_uncertainty: vec![],
                decision_frontier: PacketDecisionFrontier {
                    status: "evaluated".to_string(),
                    items: vec![],
                },
            },
            graph: PacketGraph {
                structural_state: "executable".to_string(),
                hard_blockers: vec![],
                soft_preferences: vec![],
                supersession: None,
                relations: PacketRelationBundle::default(),
            },
            documentation: PacketDocumentation {
                applicability: PacketDocsApplicability {
                    status: "complete".to_string(),
                    required: vec![],
                    optional: vec![],
                    write_candidates: vec![],
                    excluded: vec![],
                },
                suggestion_query: PacketSuggestionQuery {
                    text: "Enable token rotation".to_string(),
                    normalized_terms: vec![
                        "enable".to_string(),
                        "token".to_string(),
                        "rotation".to_string(),
                    ],
                },
                suggested_sections: vec![],
                read_budget: PacketReadBudget {
                    required_sections: 0,
                    recommended_initial_sections: 4,
                    max_initial_lines: 240,
                    suggestion_limit: 8,
                    snippet_max_bytes_each: 500,
                },
                index: PacketDocsIndex {
                    state: "current".to_string(),
                    fingerprint:
                        "sha256:6666666666666666666666666666666666666666666666666666666666666666"
                            .to_string(),
                    mode: "lexical".to_string(),
                },
            },
            knowledge: PacketKnowledge {
                status: "not_installed".to_string(),
                owner_phase: 4,
                knowledge_fingerprint: None,
                required: vec![],
                recommended: vec![],
                suggested: vec![],
                excluded: vec![],
            },
            source: PacketSource {
                repository_id: "repo_test".to_string(),
                kind: "git_commit".to_string(),
                commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
                head_ref: Some("refs/heads/main".to_string()),
                worktree_root_kind: "primary_or_existing_worktree".to_string(),
                cleanliness: "clean".to_string(),
                operation_state: "normal".to_string(),
                currentness: "current".to_string(),
            },
            workspace: PacketWorkspace {
                binding_status: "not_allocated".to_string(),
                workspace_id: None,
                required_strategy: "isolated_worktree_required".to_string(),
                base_repository_id: "repo_test".to_string(),
                base_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
                requirements: vec![],
            },
            capabilities: PacketCapabilities {
                evaluation_status: "not_evaluated".to_string(),
                required: vec!["source.read".to_string(), "source.write".to_string()],
                optional: vec![],
                missing: vec![],
                inventory_identity: None,
            },
            scope: PacketScope {
                scope_hints: PacketScopeHints::default(),
                implementation_freedom: vec![],
                hard_stops: vec![],
                enforcement: crate::work_packet::PacketScopeEnforcement {
                    status: "not_installed".to_string(),
                    owner_phase: 2,
                },
            },
            assurance: PacketAssurance {
                verification_profile: "service-change".to_string(),
                expected_evidence: vec![],
                expected_handoff: vec![],
                documentation_impact: PacketDocumentationImpact::default(),
                qa: PacketQaStatus {
                    posture: "none".to_string(),
                    status: "not_applicable".to_string(),
                    affected_case_ids: vec![],
                },
                promotion_policy: PacketFutureGate {
                    status: "not_installed".to_string(),
                    owner_phase: 3,
                },
                close_gate: PacketFutureGate {
                    status: "not_installed".to_string(),
                    owner_phase: 3,
                },
            },
            dispatch: PacketDispatch {
                reservation_candidate: true,
                dispatch_authorized: false,
                authorization_status: "not_reserved".to_string(),
                gate_families: vec![],
                revalidation_preconditions: vec![],
            },
            budget: PacketBudget::default(),
            packet_fingerprint: String::new(),
            reason_codes: vec![],
        }
    }

    fn minimal_prepared_assignment(id: &str) -> PreparedAssignmentV1 {
        let packet = dummy_packet();
        let lease = AssignmentLeaseSummary {
            lease_id: "lease_01J00000000000000000000000".to_string(),
            state: LEASE_STATE_PREPARED.to_string(),
            assignee: "agent:codex-local".to_string(),
            issued_by: "human:test".to_string(),
            issued_at: "2026-07-28T10:00:00Z".to_string(),
            expires_at: "2026-07-28T10:30:00Z".to_string(),
            ttl_seconds: DEFAULT_TTL_SECONDS,
            exclusive: true,
        };
        let workspace = AssignmentWorkspaceSummary {
            workspace_id: "wt_TK-001_01J00000000000000000000000".to_string(),
            binding_status: "bound".to_string(),
            mode: WORKSPACE_MODE_ISOLATED.to_string(),
            path: ".pulse/runtime/workspaces/wt_TK-001_01J00000000000000000000000".to_string(),
            repository_id: "repo_test".to_string(),
            base_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
            cleanliness: "clean".to_string(),
            owner_lease_id: "lease_01J00000000000000000000000".to_string(),
        };
        let cap_match = CapabilityMatchReport {
            inventory_identity:
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
            principal: "agent:codex-local".to_string(),
            status: CAP_MATCH_MATCHED.to_string(),
            required: vec!["source.read".to_string(), "source.write".to_string()],
            matched: vec!["source.read".to_string(), "source.write".to_string()],
            missing: vec![],
            extra: vec!["test.run".to_string()],
            reason_codes: vec![],
        };
        let lifecycle = AssignmentLifecycle {
            transition: LIFECYCLE_READY_TO_ACTIVE.to_string(),
            gate_profile: LIFECYCLE_GATE_PROFILE.to_string(),
            gate_status: GATE_STATUS_PASSED.to_string(),
            expected_revision: 1,
            new_revision: 2,
            event_id: "evt_01J00000000000000000000000".to_string(),
        };
        let snapshot = RevalidatedSnapshot {
            graph_fingerprint:
                "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                    .to_string(),
            readiness_profile: "phase1_contract_readiness_v1".to_string(),
            readiness_fingerprint:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                    .to_string(),
            authority_policy_fingerprint:
                "sha256:2222222222222222222222222222222222222222222222222222222222222222"
                    .to_string(),
            docs_registry_fingerprint:
                "sha256:3333333333333333333333333333333333333333333333333333333333333333"
                    .to_string(),
            docs_index_fingerprint:
                "sha256:4444444444444444444444444444444444444444444444444444444444444444"
                    .to_string(),
            source_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
            source_cleanliness: "clean".to_string(),
            repository_id: "repo_test".to_string(),
        };
        PreparedAssignmentV1 {
            schema_version: ASSIGNMENT_SCHEMA_VERSION,
            profile: PREPARED_ASSIGNMENT_PROFILE.to_string(),
            code: "prepared_assignment".to_string(),
            prepared_assignment_id: id.to_string(),
            subject: AssignmentSubjectSnapshot {
                id: "TK-001".to_string(),
                kind: "ticket".to_string(),
                revision_before: 1,
                revision_after: 2,
                contract_revision: 1,
                status_before: "ready".to_string(),
                status_after: "active".to_string(),
            },
            packet,
            packet_fingerprint:
                "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    .to_string(),
            revalidated_snapshot: snapshot,
            lease,
            workspace,
            capability_match: cap_match,
            lifecycle,
            dispatch: AssignmentDispatch::default(),
            transaction: AssignmentTransaction::default(),
            prepared_assignment_fingerprint: String::new(),
            reason_codes: vec![],
        }
    }

    fn schema_validator(schema: &str) -> jsonschema::JSONSchema {
        let schema_value: Value =
            serde_json::from_str(schema).expect("test fixture should be valid");
        jsonschema::JSONSchema::compile(&schema_value).expect("test fixture should be valid")
    }

    fn assert_schema_accepts(schema: &str, value: &Value) {
        let compiled = schema_validator(schema);
        let errors: Vec<String> = compiled
            .validate(value)
            .err()
            .map(|errs| errs.map(|err| err.to_string()).collect())
            .unwrap_or_default();
        assert!(errors.is_empty(), "schema validation failed: {errors:?}");
    }

    fn assert_schema_rejects(schema: &str, value: &Value) {
        let compiled = schema_validator(schema);
        assert!(
            compiled.validate(value).is_err(),
            "schema unexpectedly accepted {value}"
        );
    }

    // -----------------------------------------------------------------------
    // Deny-unknown round-trip tests
    // -----------------------------------------------------------------------

    #[test]
    fn public_assignment_schemas_compile_and_enforce_boundaries() {
        for schema in [
            PREPARED_ASSIGNMENT_SCHEMA,
            PREPARED_ASSIGNMENT_RECORD_SCHEMA,
            ASSIGNMENT_LEASE_SCHEMA,
            ASSIGNMENT_WORKSPACE_SCHEMA,
            CAPABILITY_INVENTORY_SCHEMA,
            CAPABILITY_MATCH_SCHEMA,
        ] {
            schema_validator(schema);
        }

        let mut pa = minimal_prepared_assignment("pa_01JSCHEMATEST");
        pa.transaction.transaction_id = "txn_01JSCHEMATEST".to_string();
        pa.transaction.event_path = ".pulse/events/2026-07-28/evt_01JSCHEMATEST.json".to_string();
        pa.prepared_assignment_fingerprint = pa
            .compute_fingerprint()
            .expect("test fixture should be valid");
        let pa_value = serde_json::to_value(&pa).expect("test fixture should be valid");
        assert_schema_accepts(PREPARED_ASSIGNMENT_SCHEMA, &pa_value);

        let mut authorized_nested_packet = pa_value.clone();
        authorized_nested_packet["packet"]["dispatch"]["dispatch_authorized"] = Value::Bool(true);
        assert_schema_rejects(PREPARED_ASSIGNMENT_SCHEMA, &authorized_nested_packet);

        let mut missing_capability = pa_value.clone();
        missing_capability["capability_match"]["status"] =
            Value::String(CAP_MATCH_FAILED.to_string());
        missing_capability["capability_match"]["missing"] =
            Value::Array(vec![Value::String("source.write".to_string())]);
        assert_schema_rejects(PREPARED_ASSIGNMENT_SCHEMA, &missing_capability);

        let record = PreparedAssignmentRecordV1 {
            schema_version: pa.schema_version,
            profile: pa.profile.clone(),
            code: pa.code.clone(),
            prepared_assignment_id: pa.prepared_assignment_id.clone(),
            subject: pa.subject.clone(),
            packet_fingerprint: pa.packet_fingerprint.clone(),
            revalidated_snapshot: pa.revalidated_snapshot.clone(),
            lease: pa.lease.clone(),
            workspace: pa.workspace.clone(),
            capability_match: pa.capability_match.clone(),
            lifecycle: pa.lifecycle.clone(),
            dispatch: pa.dispatch.clone(),
            transaction: pa.transaction.clone(),
            prepared_assignment_fingerprint: pa.prepared_assignment_fingerprint.clone(),
            reason_codes: pa.reason_codes.clone(),
        };
        let record_value = serde_json::to_value(&record).expect("test fixture should be valid");
        assert_schema_accepts(PREPARED_ASSIGNMENT_RECORD_SCHEMA, &record_value);

        let inventory = CapabilityInventoryV1 {
            schema_version: CAPABILITY_INVENTORY_SCHEMA_VERSION,
            principal: "agent:codex-local".to_string(),
            inventory_id: "local-codex-default".to_string(),
            capabilities: vec!["source.read".to_string()],
        };
        assert_schema_accepts(
            CAPABILITY_INVENTORY_SCHEMA,
            &serde_json::to_value(&inventory).expect("test fixture should be valid"),
        );
        assert_schema_accepts(
            CAPABILITY_MATCH_SCHEMA,
            &serde_json::to_value(&pa.capability_match).expect("test fixture should be valid"),
        );
    }

    #[test]
    fn prepared_assignment_deny_unknown_fields() {
        let mut pa = minimal_prepared_assignment("pa_01J00000000000000000000000");
        pa.prepared_assignment_fingerprint = pa
            .compute_fingerprint()
            .expect("test fixture should be valid");
        let json = serde_json::to_value(&pa).expect("test fixture should be valid");

        // Round-trip succeeds.
        let round_trip: PreparedAssignmentV1 =
            serde_json::from_value(json.clone()).expect("test fixture should be valid");
        assert_eq!(round_trip, pa);

        // Unknown field rejected.
        let mut tampered = json.clone();
        if let Value::Object(ref mut map) = tampered {
            map.insert(
                "runner_field_unknown".to_string(),
                Value::String("unrecognized".to_string()),
            );
        }
        let err = serde_json::from_value::<PreparedAssignmentV1>(tampered);
        assert!(
            err.is_err(),
            "deny_unknown_fields must reject unknown top-level fields"
        );
    }

    #[test]
    fn lease_summary_deny_unknown_fields() {
        let val = serde_json::json!({
            "lease_id": "lease_test",
            "state": "prepared",
            "assignee": "agent:test",
            "issued_by": "human:test",
            "issued_at": "2026-07-28T10:00:00Z",
            "expires_at": "2026-07-28T10:30:00Z",
            "ttl_seconds": 1800,
            "exclusive": true
        });
        let parsed: AssignmentLeaseSummary =
            serde_json::from_value(val.clone()).expect("test fixture should be valid");
        assert_eq!(parsed.ttl_seconds, 1800);

        let mut tampered = val.clone();
        if let Value::Object(ref mut map) = tampered {
            map.insert(
                "unknown_runner_field".to_string(),
                Value::String("future".to_string()),
            );
        }
        let err = serde_json::from_value::<AssignmentLeaseSummary>(tampered);
        assert!(
            err.is_err(),
            "deny_unknown_fields must reject unknown lease summary fields"
        );
    }

    #[test]
    fn workspace_summary_deny_unknown_fields() {
        let val = serde_json::json!({
            "workspace_id": "wt_test",
            "binding_status": "bound",
            "mode": "isolated_worktree",
            "path": ".pulse/runtime/workspaces/wt_test",
            "repository_id": "repo_test",
            "base_commit": "0123456789abcdef0123456789abcdef01234567",
            "cleanliness": "clean",
            "owner_lease_id": "lease_test"
        });
        let parsed: AssignmentWorkspaceSummary =
            serde_json::from_value(val.clone()).expect("test fixture should be valid");
        assert_eq!(parsed.mode, "isolated_worktree");

        let mut tampered = val;
        if let Value::Object(ref mut map) = tampered {
            map.insert(
                "unknown_proof_field".to_string(),
                Value::String("future".to_string()),
            );
        }
        let err = serde_json::from_value::<AssignmentWorkspaceSummary>(tampered);
        assert!(
            err.is_err(),
            "deny_unknown_fields must reject unknown workspace summary fields"
        );
    }

    #[test]
    fn capability_match_report_deny_unknown_fields() {
        let val = serde_json::json!({
            "inventory_identity": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "principal": "agent:test",
            "status": "matched",
            "required": ["source.read"],
            "matched": ["source.read"],
            "missing": [],
            "extra": [],
            "reason_codes": []
        });
        let parsed: CapabilityMatchReport =
            serde_json::from_value(val.clone()).expect("test fixture should be valid");
        assert_eq!(parsed.status, "matched");

        let mut tampered = val;
        if let Value::Object(ref mut map) = tampered {
            map.insert(
                "unknown_runner_field".to_string(),
                Value::String("future".to_string()),
            );
        }
        let err = serde_json::from_value::<CapabilityMatchReport>(tampered);
        assert!(
            err.is_err(),
            "deny_unknown_fields must reject unknown capability match fields"
        );
    }

    #[test]
    fn capability_inventory_deny_unknown_fields() {
        let val = serde_json::json!({
            "schema_version": 1,
            "principal": "agent:test",
            "inventory_id": "test-inventory",
            "capabilities": ["source.read", "source.write"]
        });
        let parsed: CapabilityInventoryV1 =
            serde_json::from_value(val.clone()).expect("test fixture should be valid");
        assert_eq!(parsed.principal, "agent:test");

        let mut tampered = val;
        if let Value::Object(ref mut map) = tampered {
            map.insert(
                "unknown_runner_field".to_string(),
                Value::String("future".to_string()),
            );
        }
        let err = serde_json::from_value::<CapabilityInventoryV1>(tampered);
        assert!(
            err.is_err(),
            "deny_unknown_fields must reject unknown capability inventory fields"
        );
    }

    // -----------------------------------------------------------------------
    // Reject runner/mailbox/proof fields not explicitly in schema
    // -----------------------------------------------------------------------

    #[test]
    fn prepared_assignment_rejects_unknown_runner_fields_in_nested_objects() {
        let mut pa = minimal_prepared_assignment("pa_deny_runner");
        pa.prepared_assignment_fingerprint = pa
            .compute_fingerprint()
            .expect("test fixture should be valid");
        let mut json = serde_json::to_value(&pa).expect("test fixture should be valid");

        // Add a runner-only field inside dispatch that is not in the DTO.
        if let Some(Value::Object(ref mut map)) = json.get_mut("dispatch") {
            map.insert("delivery_acknowledged".to_string(), Value::Bool(false));
        }
        let err = serde_json::from_value::<PreparedAssignmentV1>(json);
        assert!(
            err.is_err(),
            "PreparedAssignmentV1 must reject unknown dispatch fields"
        );
    }

    // -----------------------------------------------------------------------
    // Normalization tests
    // -----------------------------------------------------------------------

    #[test]
    fn prepared_assignment_normalization_is_deterministic() {
        let mut a = minimal_prepared_assignment("pa_norm");
        let mut b = minimal_prepared_assignment("pa_norm");

        // Add reason codes in different order.
        a.reason_codes = vec!["z_reason".to_string(), "a_reason".to_string()];
        b.reason_codes = vec!["a_reason".to_string(), "z_reason".to_string()];
        assert_ne!(a.reason_codes, b.reason_codes);

        a.normalize();
        b.normalize();
        assert_eq!(a.reason_codes, b.reason_codes);
    }

    #[test]
    fn capability_match_normalization_is_deterministic() {
        let mut a = CapabilityMatchReport {
            inventory_identity: "sha256:test".to_string(),
            principal: "agent:test".to_string(),
            status: CAP_MATCH_MATCHED.to_string(),
            required: vec!["b".to_string(), "a".to_string()],
            matched: vec!["b".to_string(), "a".to_string()],
            missing: vec![],
            extra: vec!["z".to_string(), "m".to_string()],
            reason_codes: vec![],
        };
        let mut b = a.clone();
        a.required = vec!["a".to_string(), "b".to_string()];
        a.matched = vec!["a".to_string(), "b".to_string()];
        a.extra = vec!["m".to_string(), "z".to_string()];

        a.normalize();
        b.normalize();
        assert_eq!(a.required, b.required);
        assert_eq!(a.matched, b.matched);
        assert_eq!(a.extra, b.extra);
    }

    #[test]
    fn dispatch_gate_families_are_sorted() {
        let mut dispatch = AssignmentDispatch {
            dispatch_authorized: true,
            authorization_status: DISPATCH_AUTHORIZED_STATUS.to_string(),
            runner_status: RUNNER_STATUS_NOT_STARTED.to_string(),
            gate_families: vec![
                AssignmentGateFamily {
                    family: "verification".to_string(),
                    status: GATE_STATUS_NOT_INSTALLED.to_string(),
                    reason_codes: vec!["z_code".to_string(), "a_code".to_string()],
                },
                AssignmentGateFamily {
                    family: "lease".to_string(),
                    status: GATE_STATUS_PASSED.to_string(),
                    reason_codes: vec![],
                },
            ],
        };
        dispatch.normalize();
        assert_eq!(dispatch.gate_families[0].family, "lease");
        assert_eq!(dispatch.gate_families[1].family, "verification");
        assert_eq!(
            dispatch.gate_families[1].reason_codes,
            vec!["a_code", "z_code"]
        );
    }

    // -----------------------------------------------------------------------
    // Fingerprint tests
    // -----------------------------------------------------------------------

    #[test]
    fn prepared_assignment_fingerprint_excludes_self() {
        let mut pa = minimal_prepared_assignment("pa_fp_self");
        pa.prepared_assignment_fingerprint = "sha256:placeholder".to_string();
        let fp1 = pa
            .compute_fingerprint()
            .expect("test fixture should be valid");

        // Changing fingerprint field should not change result.
        pa.prepared_assignment_fingerprint = "sha256:other".to_string();
        let fp2 = pa
            .compute_fingerprint()
            .expect("test fixture should be valid");
        assert_eq!(
            fp1, fp2,
            "fingerprint must exclude prepared_assignment_fingerprint field"
        );
    }

    #[test]
    fn prepared_assignment_fingerprint_uses_packet_fingerprint_boundary() {
        let a = minimal_prepared_assignment("pa_fp_packet_boundary");
        let mut b = a.clone();
        b.packet.subject.title = "Different embedded rendering".to_string();
        assert_eq!(
            a.compute_fingerprint().expect("test fixture should be valid"),
            b.compute_fingerprint().expect("test fixture should be valid"),
            "prepared-assignment fingerprint must use packet_fingerprint, not embedded packet bytes"
        );

        b.packet_fingerprint =
            "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string();
        assert_ne!(
            a.compute_fingerprint()
                .expect("test fixture should be valid"),
            b.compute_fingerprint()
                .expect("test fixture should be valid"),
            "packet_fingerprint remains the semantic packet identity boundary"
        );
    }

    #[test]
    fn prepared_assignment_fingerprint_normalizes_set_like_fields() {
        let mut a = minimal_prepared_assignment("pa_fp_norm");
        let mut b = minimal_prepared_assignment("pa_fp_norm");
        a.reason_codes = vec!["z".to_string(), "a".to_string()];
        b.reason_codes = vec!["a".to_string(), "z".to_string()];
        assert_eq!(
            a.compute_fingerprint()
                .expect("test fixture should be valid"),
            b.compute_fingerprint()
                .expect("test fixture should be valid"),
            "fingerprint computation must normalize set-like fields internally"
        );
    }

    #[test]
    fn prepared_assignment_fingerprint_is_deterministic() {
        let mut a = minimal_prepared_assignment("pa_fp_det");
        let mut b = minimal_prepared_assignment("pa_fp_det");
        a.normalize();
        b.normalize();
        a.prepared_assignment_fingerprint = a
            .compute_fingerprint()
            .expect("test fixture should be valid");
        b.prepared_assignment_fingerprint = b
            .compute_fingerprint()
            .expect("test fixture should be valid");
        assert_eq!(a, b, "same inputs produce same fingerprint and assignment");
    }

    #[test]
    fn prepared_assignment_fingerprint_changes_with_content() {
        let mut a = minimal_prepared_assignment("pa_fp_diff1");
        let mut b = minimal_prepared_assignment("pa_fp_diff2");
        a.normalize();
        b.normalize();
        let fp1 = a
            .compute_fingerprint()
            .expect("test fixture should be valid");
        let fp2 = b
            .compute_fingerprint()
            .expect("test fixture should be valid");
        assert_ne!(fp1, fp2, "different id must produce different fingerprint");
    }

    // -----------------------------------------------------------------------
    // No-float guarantee
    // -----------------------------------------------------------------------

    #[test]
    fn prepared_assignment_has_no_floats() {
        let mut pa = minimal_prepared_assignment("pa_nofloat");
        pa.prepared_assignment_fingerprint = pa
            .compute_fingerprint()
            .expect("test fixture should be valid");
        let bytes = serde_json::to_vec_pretty(&pa).expect("test fixture should be valid");
        let text = String::from_utf8(bytes).expect("test fixture should be valid");

        // All numbers must be integers (no decimal point).
        for line in text.lines() {
            if let Some((_, val)) = line.split_once(':') {
                let trimmed = val.trim().trim_end_matches(',');
                // Check for decimal-only numbers
                if let Ok(serde_json::Value::Number(n)) =
                    serde_json::from_str::<serde_json::Value>(trimmed)
                {
                    assert!(
                        n.is_u64() || n.is_i64(),
                        "float found in serialized assignment: {trimmed}"
                    );
                }
            }
        }
    }

    #[test]
    fn capability_inventory_has_no_floats() {
        let inv = CapabilityInventoryV1 {
            schema_version: 1,
            principal: "agent:test".to_string(),
            inventory_id: "test".to_string(),
            capabilities: vec!["source.read".to_string()],
        };
        let _result = serde_json::to_value(&inv).expect("test fixture should be valid");
        let bytes = serde_json::to_vec_pretty(&inv).expect("test fixture should be valid");
        let text = String::from_utf8(bytes).expect("test fixture should be valid");

        // All numbers must be integers (no decimal point).
        for line in text.lines() {
            if let Some((_, val)) = line.split_once(':') {
                let trimmed = val.trim().trim_end_matches(',');
                if let Ok(serde_json::Value::Number(n)) =
                    serde_json::from_str::<serde_json::Value>(trimmed)
                {
                    assert!(
                        n.is_u64() || n.is_i64(),
                        "float found in serialized inventory: {trimmed}"
                    );
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Lease record schema contract
    // -----------------------------------------------------------------------

    #[test]
    fn assignment_lease_record_round_trip() {
        let record = AssignmentLeaseRecordV1 {
            schema_version: LEASE_SCHEMA_VERSION,
            lease_id: "lease_01Jtest".to_string(),
            kind: LEASE_KIND_IMPLEMENTATION.to_string(),
            subject: AssignmentLeaseSubject {
                kind: "ticket".to_string(),
                id: "TK-001".to_string(),
                revision: 8,
                contract_revision: 4,
                status_at_claim: "ready".to_string(),
            },
            assignee: AssignmentLeaseAssignee {
                principal: "agent:codex-local".to_string(),
            },
            issued_by: "human:quannv".to_string(),
            issued_at: "2026-07-28T10:00:00Z".to_string(),
            expires_at: "2026-07-28T10:30:00Z".to_string(),
            ttl_seconds: DEFAULT_TTL_SECONDS,
            state: LEASE_STATE_PREPARED.to_string(),
            packet_fingerprint:
                "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    .to_string(),
            readiness_fingerprint:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                    .to_string(),
            workspace_id: "wt_TK-001_01Jtest".to_string(),
            prepared_assignment_id: "pa_01Jtest".to_string(),
            capability_inventory_identity:
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
            source: AssignmentLeaseSource {
                repository_id: "repo_test".to_string(),
                base_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
            },
        };
        let json = serde_json::to_value(&record).expect("test fixture should be valid");
        let round_trip: AssignmentLeaseRecordV1 =
            serde_json::from_value(json).expect("test fixture should be valid");
        assert_eq!(round_trip, record);
    }

    #[test]
    fn assignment_lease_record_deny_unknown_fields() {
        let val = serde_json::json!({
            "schema_version": 1,
            "lease_id": "lease_test",
            "kind": "implementation_assignment",
            "subject": {
                "kind": "ticket",
                "id": "TK-001",
                "revision": 8,
                "contract_revision": 4,
                "status_at_claim": "ready"
            },
            "assignee": {"principal": "agent:test"},
            "issued_by": "human:test",
            "issued_at": "2026-07-28T10:00:00Z",
            "expires_at": "2026-07-28T10:30:00Z",
            "ttl_seconds": 1800,
            "state": "prepared",
            "packet_fingerprint": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "readiness_fingerprint": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "workspace_id": "wt_test",
            "prepared_assignment_id": "pa_test",
            "capability_inventory_identity": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "source": {
                "repository_id": "repo_test",
                "base_commit": "0123456789abcdef0123456789abcdef01234567"
            }
        });
        let parsed: AssignmentLeaseRecordV1 =
            serde_json::from_value(val.clone()).expect("test fixture should be valid");
        assert_eq!(parsed.kind, "implementation_assignment");

        let mut tampered = val;
        if let Value::Object(ref mut map) = tampered {
            map.insert(
                "runner_state".to_string(),
                Value::String("future".to_string()),
            );
        }
        let err = serde_json::from_value::<AssignmentLeaseRecordV1>(tampered);
        assert!(err.is_err(), "lease record must reject unknown fields");
    }

    // -----------------------------------------------------------------------
    // Workspace record schema contract
    // -----------------------------------------------------------------------

    #[test]
    fn assignment_workspace_record_round_trip() {
        let record = AssignmentWorkspaceRecordV1 {
            schema_version: WORKSPACE_SCHEMA_VERSION,
            workspace_id: "wt_TK-001_01Jtest".to_string(),
            lease_id: "lease_01Jtest".to_string(),
            prepared_assignment_id: "pa_01Jtest".to_string(),
            subject: WorkspaceSubjectRef {
                kind: "ticket".to_string(),
                id: "TK-001".to_string(),
                revision: 8,
            },
            mode: WORKSPACE_MODE_ISOLATED.to_string(),
            path: ".pulse/runtime/workspaces/wt_TK-001_01Jtest".to_string(),
            repository_id: "repo_test".to_string(),
            base_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
            head_commit_at_bind: "0123456789abcdef0123456789abcdef01234567".to_string(),
            cleanliness_at_bind: "clean".to_string(),
            state: WORKSPACE_STATE_BOUND.to_string(),
            created_at: "2026-07-28T10:00:00Z".to_string(),
            released_at: None,
            cleanup: WorkspaceCleanupPolicy {
                policy: "safe_remove_if_clean_at_base".to_string(),
                status: "not_requested".to_string(),
            },
        };
        let json = serde_json::to_value(&record).expect("test fixture should be valid");
        let round_trip: AssignmentWorkspaceRecordV1 =
            serde_json::from_value(json).expect("test fixture should be valid");
        assert_eq!(round_trip, record);
    }

    #[test]
    fn assignment_workspace_record_deny_unknown_fields() {
        let val = serde_json::json!({
            "schema_version": 1,
            "workspace_id": "wt_test",
            "lease_id": "lease_test",
            "prepared_assignment_id": "pa_test",
            "subject": {"kind": "ticket", "id": "TK-001", "revision": 8},
            "mode": "isolated_worktree",
            "path": ".pulse/runtime/workspaces/wt_test",
            "repository_id": "repo_test",
            "base_commit": "0123456789abcdef0123456789abcdef01234567",
            "head_commit_at_bind": "0123456789abcdef0123456789abcdef01234567",
            "cleanliness_at_bind": "clean",
            "state": "bound",
            "created_at": "2026-07-28T10:00:00Z",
            "released_at": null,
            "cleanup": {"policy": "safe_remove_if_clean_at_base", "status": "not_requested"}
        });
        let parsed: AssignmentWorkspaceRecordV1 =
            serde_json::from_value(val.clone()).expect("test fixture should be valid");
        assert_eq!(parsed.mode, "isolated_worktree");

        let mut tampered = val;
        if let Value::Object(ref mut map) = tampered {
            map.insert(
                "unknown_proof_field".to_string(),
                Value::String("future".to_string()),
            );
        }
        let err = serde_json::from_value::<AssignmentWorkspaceRecordV1>(tampered);
        assert!(err.is_err(), "workspace record must reject unknown fields");
    }

    // -----------------------------------------------------------------------
    // Capability inventory identity is deterministic
    // -----------------------------------------------------------------------

    #[test]
    fn capability_inventory_identity_is_deterministic() {
        let inv1 = CapabilityInventoryV1 {
            schema_version: 1,
            principal: "agent:test".to_string(),
            inventory_id: "test".to_string(),
            capabilities: vec!["source.write".to_string(), "source.read".to_string()],
        };
        let inv2 = CapabilityInventoryV1 {
            schema_version: 1,
            principal: "agent:test".to_string(),
            inventory_id: "test".to_string(),
            capabilities: vec!["source.read".to_string(), "source.write".to_string()],
        };
        let id1 = inv1
            .compute_inventory_identity()
            .expect("test fixture should be valid");
        let id2 = inv2
            .compute_inventory_identity()
            .expect("test fixture should be valid");
        assert_eq!(
            id1, id2,
            "capability inventory identity must be order-independent"
        );
    }

    #[test]
    fn capability_inventory_identity_changes_with_content() {
        let inv1 = CapabilityInventoryV1 {
            schema_version: 1,
            principal: "agent:test".to_string(),
            inventory_id: "test".to_string(),
            capabilities: vec!["source.read".to_string()],
        };
        let inv2 = CapabilityInventoryV1 {
            schema_version: 1,
            principal: "agent:test".to_string(),
            inventory_id: "test".to_string(),
            capabilities: vec!["source.write".to_string()],
        };
        let id1 = inv1
            .compute_inventory_identity()
            .expect("test fixture should be valid");
        let id2 = inv2
            .compute_inventory_identity()
            .expect("test fixture should be valid");
        assert_ne!(
            id1, id2,
            "different capabilities must produce different identity"
        );
    }

    // -----------------------------------------------------------------------
    // Default dispatch has all gate families as specified
    // -----------------------------------------------------------------------

    #[test]
    fn default_dispatch_has_correct_gate_families() {
        let dispatch = AssignmentDispatch::default();
        assert!(dispatch.dispatch_authorized);
        assert_eq!(dispatch.authorization_status, DISPATCH_AUTHORIZED_STATUS);
        assert_eq!(dispatch.runner_status, RUNNER_STATUS_NOT_STARTED);
        assert_eq!(dispatch.gate_families.len(), 8);

        let families: Vec<&str> = dispatch
            .gate_families
            .iter()
            .map(|g| g.family.as_str())
            .collect();
        assert!(families.contains(&"packet_revalidation"));
        assert!(families.contains(&"lease"));
        assert!(families.contains(&"workspace_binding"));
        assert!(families.contains(&"capability_match"));
        assert!(families.contains(&"lifecycle"));
        assert!(families.contains(&"runner"));
        assert!(families.contains(&"handoff"));
        assert!(families.contains(&"verification"));
    }

    // -----------------------------------------------------------------------
    // PreparedAssignmentRecordV1 round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn prepared_assignment_record_round_trip() {
        let pa = minimal_prepared_assignment("pa_record_test");
        let record = PreparedAssignmentRecordV1 {
            schema_version: pa.schema_version,
            profile: pa.profile.clone(),
            code: pa.code.clone(),
            prepared_assignment_id: pa.prepared_assignment_id.clone(),
            subject: pa.subject.clone(),
            packet_fingerprint: pa.packet_fingerprint.clone(),
            revalidated_snapshot: pa.revalidated_snapshot.clone(),
            lease: pa.lease.clone(),
            workspace: pa.workspace.clone(),
            capability_match: pa.capability_match.clone(),
            lifecycle: pa.lifecycle.clone(),
            dispatch: pa.dispatch.clone(),
            transaction: pa.transaction.clone(),
            prepared_assignment_fingerprint: pa.prepared_assignment_fingerprint.clone(),
            reason_codes: pa.reason_codes.clone(),
        };
        let json = serde_json::to_value(&record).expect("test fixture should be valid");
        let round_trip: PreparedAssignmentRecordV1 =
            serde_json::from_value(json).expect("test fixture should be valid");
        assert_eq!(round_trip, record);
    }

    // -----------------------------------------------------------------------
    // Constants contract
    // -----------------------------------------------------------------------

    #[test]
    fn assignment_constants_match_spec() {
        assert_eq!(ASSIGNMENT_SCHEMA_VERSION, 1);
        assert_eq!(PREPARED_ASSIGNMENT_PROFILE, "phase2_prepared_assignment_v1");
        assert_eq!(LEASE_SCHEMA_VERSION, 1);
        assert_eq!(LEASE_KIND_IMPLEMENTATION, "implementation_assignment");
        assert_eq!(WORKSPACE_SCHEMA_VERSION, 1);
        assert_eq!(CAPABILITY_INVENTORY_SCHEMA_VERSION, 1);
        assert_eq!(DEFAULT_TTL_SECONDS, 1800);
        assert_eq!(MIN_TTL_SECONDS, 60);
        assert_eq!(MAX_TTL_SECONDS, 86_400);
        assert_eq!(LEASE_STATE_PREPARED, "prepared");
        assert_eq!(CAP_MATCH_MATCHED, "matched");
        assert_eq!(DISPATCH_AUTHORIZED_STATUS, "prepared_assignment");
        assert_eq!(RUNNER_STATUS_NOT_STARTED, "not_started");
        assert_eq!(LIFECYCLE_READY_TO_ACTIVE, "ready_to_active");
        assert_eq!(LIFECYCLE_GATE_PROFILE, "phase2_prepared_assignment_v1");
    }

    // -----------------------------------------------------------------------
    // Error code constants
    // -----------------------------------------------------------------------

    #[test]
    fn capability_error_constants_match_spec() {
        assert_eq!(
            ERR_CAP_INVENTORY_MISSING,
            "assignment_capability_inventory_missing"
        );
        assert_eq!(
            ERR_CAP_INVENTORY_INVALID,
            "assignment_capability_inventory_invalid"
        );
        assert_eq!(
            ERR_CAP_PRINCIPAL_MISMATCH,
            "assignment_capability_principal_mismatch"
        );
        assert_eq!(ERR_CAP_MISSING, "assignment_capability_missing");
    }

    #[test]
    fn cap_workspace_worktree_constant() {
        assert_eq!(CAP_WORKSPACE_WORKTREE, "workspace.worktree");
    }

    // -----------------------------------------------------------------------
    // capability_required_set
    // -----------------------------------------------------------------------

    #[test]
    fn capability_required_set_no_workspace_mode() {
        let required = vec!["source.read".to_string(), "source.write".to_string()];
        let result = capability_required_set(&required, None);
        assert_eq!(result, vec!["source.read", "source.write"]);
    }

    #[test]
    fn capability_required_set_in_place_no_worktree_induced() {
        let required = vec!["source.read".to_string()];
        let result = capability_required_set(&required, Some(WORKSPACE_MODE_IN_PLACE));
        assert_eq!(result, vec!["source.read"]);
    }

    #[test]
    fn capability_required_set_isolated_worktree_induced() {
        let required = vec!["source.read".to_string()];
        let result = capability_required_set(&required, Some(WORKSPACE_MODE_ISOLATED));
        assert_eq!(result, vec!["source.read", "workspace.worktree"]);
    }

    #[test]
    fn capability_required_set_isolated_does_not_dup_existing() {
        let required = vec!["workspace.worktree".to_string(), "source.read".to_string()];
        let result = capability_required_set(&required, Some(WORKSPACE_MODE_ISOLATED));
        assert_eq!(result, vec!["source.read", "workspace.worktree"]);
    }

    #[test]
    fn capability_required_set_sorts_and_dedupes() {
        let required = vec!["b".to_string(), "a".to_string(), "b".to_string()];
        let result = capability_required_set(&required, None);
        assert_eq!(result, vec!["a", "b"]);
    }

    #[test]
    fn capability_required_set_empty() {
        let required: Vec<String> = vec![];
        let result = capability_required_set(&required, None);
        assert!(result.is_empty());
    }

    #[test]
    fn capability_required_set_empty_isolated_has_worktree() {
        let required: Vec<String> = vec![];
        let result = capability_required_set(&required, Some(WORKSPACE_MODE_ISOLATED));
        assert_eq!(result, vec!["workspace.worktree"]);
    }

    // -----------------------------------------------------------------------
    // CapabilityInventoryV1::from_json_bytes
    // -----------------------------------------------------------------------

    #[test]
    fn capability_inventory_parse_valid_json() {
        let json = serde_json::json!({
            "schema_version": 1,
            "principal": "agent:codex-local",
            "inventory_id": "test-id",
            "capabilities": ["source.read", "source.write"]
        });
        let bytes = serde_json::to_vec(&json).expect("test fixture should be valid");
        let inv =
            CapabilityInventoryV1::from_json_bytes(&bytes).expect("valid inventory should parse");
        assert_eq!(inv.principal, "agent:codex-local");
        assert_eq!(inv.inventory_id, "test-id");
        assert_eq!(inv.capabilities.len(), 2);
    }

    #[test]
    fn capability_inventory_parse_rejects_wrong_schema_version() {
        let json = serde_json::json!({
            "schema_version": 999,
            "principal": "agent:codex-local",
            "inventory_id": "test-id",
            "capabilities": []
        });
        let bytes = serde_json::to_vec(&json).expect("test fixture should be valid");
        let err = CapabilityInventoryV1::from_json_bytes(&bytes)
            .expect_err("wrong schema_version should fail");
        assert_eq!(err.code(), ERR_CAP_INVENTORY_INVALID);
    }

    #[test]
    fn capability_inventory_parse_rejects_invalid_json() {
        let bytes = b"not valid json";
        let err =
            CapabilityInventoryV1::from_json_bytes(bytes).expect_err("invalid JSON should fail");
        assert_eq!(err.code(), ERR_CAP_INVENTORY_INVALID);
    }

    #[test]
    fn capability_inventory_parse_rejects_unknown_fields() {
        let json = serde_json::json!({
            "schema_version": 1,
            "principal": "agent:codex-local",
            "inventory_id": "test-id",
            "capabilities": [],
            "unknown_field": "should be rejected"
        });
        let bytes = serde_json::to_vec(&json).expect("test fixture should be valid");
        let err =
            CapabilityInventoryV1::from_json_bytes(&bytes).expect_err("unknown field should fail");
        assert_eq!(err.code(), ERR_CAP_INVENTORY_INVALID);
    }

    // -----------------------------------------------------------------------
    // CapabilityInventoryV1::validate_principal
    // -----------------------------------------------------------------------

    #[test]
    fn validate_principal_accepts_match() {
        let inv = CapabilityInventoryV1 {
            schema_version: 1,
            principal: "agent:test".to_string(),
            inventory_id: "test-id".to_string(),
            capabilities: vec![],
        };
        let result = inv.validate_principal("agent:test");
        assert!(result.is_ok());
    }

    #[test]
    fn validate_principal_rejects_mismatch() {
        let inv = CapabilityInventoryV1 {
            schema_version: 1,
            principal: "agent:foo".to_string(),
            inventory_id: "test-id".to_string(),
            capabilities: vec![],
        };
        let err = inv
            .validate_principal("agent:bar")
            .expect_err("principal mismatch should fail");
        assert_eq!(err.code(), ERR_CAP_PRINCIPAL_MISMATCH);
    }

    // -----------------------------------------------------------------------
    // CapabilityInventoryV1::match_required — success cases
    // -----------------------------------------------------------------------

    #[test]
    fn match_required_exact_match() {
        let inv = CapabilityInventoryV1 {
            schema_version: 1,
            principal: "agent:test".to_string(),
            inventory_id: "test-id".to_string(),
            capabilities: vec!["source.read".to_string(), "source.write".to_string()],
        };
        let required = vec!["source.read".to_string(), "source.write".to_string()];
        let report = inv
            .match_required("agent:test", &required, None)
            .expect("exact match should succeed");
        assert_eq!(report.status, CAP_MATCH_MATCHED);
        assert!(report.missing.is_empty());
        assert!(report.extra.is_empty());
    }

    #[test]
    fn match_required_extra_capabilities_allowed() {
        let inv = CapabilityInventoryV1 {
            schema_version: 1,
            principal: "agent:test".to_string(),
            inventory_id: "test-id".to_string(),
            capabilities: vec![
                "source.read".to_string(),
                "source.write".to_string(),
                "test.run".to_string(),
                "repository.inspect".to_string(),
            ],
        };
        let required = vec!["source.read".to_string(), "source.write".to_string()];
        let report = inv
            .match_required("agent:test", &required, None)
            .expect("extras allowed");
        assert_eq!(report.status, CAP_MATCH_MATCHED);
        assert!(report.missing.is_empty());
        assert_eq!(report.extra.len(), 2);
        assert!(report.extra.iter().any(|s| s == "test.run"));
        assert!(report.extra.iter().any(|s| s == "repository.inspect"));
    }

    // -----------------------------------------------------------------------
    // CapabilityInventoryV1::match_required — failure cases
    // -----------------------------------------------------------------------

    #[test]
    fn match_required_principal_mismatch_rejected() {
        let inv = CapabilityInventoryV1 {
            schema_version: 1,
            principal: "agent:foo".to_string(),
            inventory_id: "test-id".to_string(),
            capabilities: vec!["source.read".to_string()],
        };
        let required = vec!["source.read".to_string()];
        let err = inv
            .match_required("agent:bar", &required, None)
            .expect_err("principal mismatch should fail");
        assert_eq!(err.code(), ERR_CAP_PRINCIPAL_MISMATCH);
    }

    #[test]
    fn match_required_missing_detected() {
        let inv = CapabilityInventoryV1 {
            schema_version: 1,
            principal: "agent:test".to_string(),
            inventory_id: "test-id".to_string(),
            capabilities: vec!["source.read".to_string()],
        };
        let required = vec!["source.read".to_string(), "source.write".to_string()];
        let report = inv
            .match_required("agent:test", &required, None)
            .expect("match returns report even on missing");
        assert_eq!(report.status, CAP_MATCH_FAILED);
        assert_eq!(report.missing, vec!["source.write"]);
        assert_eq!(report.matched, vec!["source.read"]);
        assert!(report.extra.is_empty());
    }

    #[test]
    fn match_required_all_missing() {
        let inv = CapabilityInventoryV1 {
            schema_version: 1,
            principal: "agent:test".to_string(),
            inventory_id: "test-id".to_string(),
            capabilities: vec!["test.run".to_string()],
        };
        let required = vec!["source.read".to_string(), "source.write".to_string()];
        let report = inv
            .match_required("agent:test", &required, None)
            .expect("match returns report even on missing");
        assert_eq!(report.status, CAP_MATCH_FAILED);
        assert_eq!(report.matched.len(), 0);
        assert_eq!(report.missing.len(), 2);
        assert!(report.extra.iter().any(|s| s == "test.run"));
    }

    // -----------------------------------------------------------------------
    // Workspace-induced requirement tests
    // -----------------------------------------------------------------------

    #[test]
    fn match_required_isolated_worktree_induces_workspace_worktree() {
        let inv = CapabilityInventoryV1 {
            schema_version: 1,
            principal: "agent:test".to_string(),
            inventory_id: "test-id".to_string(),
            capabilities: vec!["source.read".to_string(), "workspace.worktree".to_string()],
        };
        let required = vec!["source.read".to_string()];
        let report = inv
            .match_required("agent:test", &required, Some(WORKSPACE_MODE_ISOLATED))
            .expect("workspace.worktree satisfied");
        assert_eq!(report.status, CAP_MATCH_MATCHED);
        assert!(report
            .required
            .contains(&CAP_WORKSPACE_WORKTREE.to_string()));
        assert!(report.matched.contains(&CAP_WORKSPACE_WORKTREE.to_string()));
    }

    #[test]
    fn match_required_isolated_worktree_missing_induced() {
        let inv = CapabilityInventoryV1 {
            schema_version: 1,
            principal: "agent:test".to_string(),
            inventory_id: "test-id".to_string(),
            capabilities: vec!["source.read".to_string()],
        };
        let required = vec!["source.read".to_string()];
        let report = inv
            .match_required("agent:test", &required, Some(WORKSPACE_MODE_ISOLATED))
            .expect("match returns report even on missing");
        assert_eq!(report.status, CAP_MATCH_FAILED);
        assert!(report.missing.contains(&CAP_WORKSPACE_WORKTREE.to_string()));
        assert!(report.matched.contains(&"source.read".to_string()));
    }

    #[test]
    fn match_required_in_place_does_not_induce_worktree() {
        let inv = CapabilityInventoryV1 {
            schema_version: 1,
            principal: "agent:test".to_string(),
            inventory_id: "test-id".to_string(),
            capabilities: vec!["source.read".to_string()],
        };
        let required = vec!["source.read".to_string()];
        let report = inv
            .match_required("agent:test", &required, Some(WORKSPACE_MODE_IN_PLACE))
            .expect("in_place should not induce workspace.worktree");
        assert_eq!(report.status, CAP_MATCH_MATCHED);
        assert!(!report
            .required
            .contains(&CAP_WORKSPACE_WORKTREE.to_string()));
    }

    // -----------------------------------------------------------------------
    // Match report normalization
    // -----------------------------------------------------------------------

    #[test]
    fn match_required_normalizes_report() {
        let inv = CapabilityInventoryV1 {
            schema_version: 1,
            principal: "agent:test".to_string(),
            inventory_id: "test-id".to_string(),
            capabilities: vec!["c".to_string(), "b".to_string(), "a".to_string()],
        };
        let required = vec!["b".to_string(), "c".to_string(), "a".to_string()];
        let report = inv
            .match_required("agent:test", &required, None)
            .expect("match should succeed");
        assert_eq!(report.status, CAP_MATCH_MATCHED);
        assert_eq!(report.required, vec!["a", "b", "c"]);
        assert_eq!(report.matched, vec!["a", "b", "c"]);
        assert!(report.extra.is_empty());
    }

    // -----------------------------------------------------------------------
    // Inventory identity is part of report
    // -----------------------------------------------------------------------

    #[test]
    fn match_required_includes_inventory_identity() {
        let inv = CapabilityInventoryV1 {
            schema_version: 1,
            principal: "agent:test".to_string(),
            inventory_id: "test-id".to_string(),
            capabilities: vec!["source.read".to_string()],
        };
        let expected_id = inv
            .compute_inventory_identity()
            .expect("test fixture should be valid");
        let required = vec!["source.read".to_string()];
        let report = inv
            .match_required("agent:test", &required, None)
            .expect("match should succeed");
        assert_eq!(report.inventory_identity, expected_id);
    }

    // -----------------------------------------------------------------------
    // Sort/dedupe symmetry: same inventory in different orders
    // -----------------------------------------------------------------------

    #[test]
    fn match_required_is_order_independent() {
        let inv_a = CapabilityInventoryV1 {
            schema_version: 1,
            principal: "agent:test".to_string(),
            inventory_id: "test-id".to_string(),
            capabilities: vec!["source.read".to_string(), "source.write".to_string()],
        };
        let inv_b = CapabilityInventoryV1 {
            schema_version: 1,
            principal: "agent:test".to_string(),
            inventory_id: "test-id".to_string(),
            capabilities: vec!["source.write".to_string(), "source.read".to_string()],
        };
        let required = vec!["source.read".to_string(), "source.write".to_string()];
        let report_a = inv_a
            .match_required("agent:test", &required, None)
            .expect("match should succeed");
        let report_b = inv_b
            .match_required("agent:test", &required, None)
            .expect("match should succeed");
        assert_eq!(report_a.matched, report_b.matched);
        assert_eq!(report_a.missing, report_b.missing);
        assert_eq!(report_a.extra, report_b.extra);
        assert_eq!(report_a.required, report_b.required);
    }
}
