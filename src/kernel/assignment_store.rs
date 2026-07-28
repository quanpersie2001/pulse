//! Runtime assignment record IO and read-only recovery classification
//! (P2S2-I2).
//!
//! This module owns filesystem IO for runtime assignment records, tombstones
//! and safe repository-relative path validation. It does **not** evaluate
//! lifecycle, authority, capabilities or Git, and it does **not** call the
//! work graph store.
//!
//! Ownership boundaries (per proposal P2S2):
//!
//! - `src/assignment.rs` owns value types, normalization and fingerprint
//!   projections. This store consumes those types.
//! - `src/kernel/assignment.rs` (future) owns cross-domain composition,
//!   authority checks, packet revalidation, lifecycle gate evaluation and
//!   mutation ordering. This store provides the IO substrate for that layer.
//! - `src/graph/store` owns canonical workgraph mutation. This store never
//!   touches `.pulse/workgraph/nodes/` or `.pulse/events/`.
//!
//! # Runtime filesystem layout
//!
//! All paths are under `.pulse/runtime/assignment/`:
//!
//! ```text
//! .pulse/runtime/assignment/
//!   leases/       live lease records
//!   workspaces/   workspace binding records
//!   prepared/     prepared-assignment records
//!   tombstones/   terminal lease-summary tombstones
//! ```
//!
//! # Enrollment safety
//!
//! Every public entry point validates the target repository is enrolled
//! (`.pulse/workgraph/manifest.json` exists) **before** creating any runtime
//! directories or acquiring the repository lock. A non-enrolled repository
//! always fails with `not_enrolled` and never gains `.pulse/runtime/` paths
//! as a side effect.
//!
//! See `proposals/phase2-slice2-atomic-reservation-workspace-binding.md`.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::assignment::{
    AssignmentLeaseRecordV1, AssignmentTombstoneV1, AssignmentWorkspaceRecordV1,
    PreparedAssignmentRecordV1, LEASE_KIND_IMPLEMENTATION, LEASE_SCHEMA_VERSION,
    LEASE_STATE_EXPIRED, LEASE_STATE_PREPARED, LEASE_STATE_RELEASED, LEASE_STATE_STALE,
    PREPARED_ASSIGNMENT_PROFILE, TOMBSTONE_SCHEMA_VERSION, TOMBSTONE_STATE_EXPIRED,
    TOMBSTONE_STATE_RELEASED, TOMBSTONE_STATE_STALE, WORKSPACE_MODE_IN_PLACE,
    WORKSPACE_MODE_ISOLATED, WORKSPACE_SCHEMA_VERSION, WORKSPACE_STATE_BOUND,
    WORKSPACE_STATE_RELEASED, WORKSPACE_STATE_STALE,
};
use crate::storage;
use crate::PulseError;
use crate::PulseResult;

// ---------------------------------------------------------------------------
// Runtime path constants
// ---------------------------------------------------------------------------

/// The runtime assignment root directory relative to a target repository root.
pub const ASSIGNMENT_RUNTIME_ROOT: &str = ".pulse/runtime/assignment";

/// Live lease records directory name.
pub const LEASES_DIR: &str = "leases";

/// Workspace binding records directory name.
pub const WORKSPACES_DIR: &str = "workspaces";

/// Prepared-assignment records directory name.
pub const PREPARED_DIR: &str = "prepared";

/// Terminal tombstone records directory name.
pub const TOMBSTONES_DIR: &str = "tombstones";

/// The `.json` file extension used for all runtime assignment records.
pub const RECORD_EXTENSION: &str = "json";

// ---------------------------------------------------------------------------
// Enrollment check (preserve / no-bootstrap)
// ---------------------------------------------------------------------------

/// Check that the target repository is enrolled with a valid workgraph.
///
/// This is a pure read-only inspection: it never creates `.pulse/runtime/`
/// directories or `.pulse/runtime/assignment/` paths. Call this before any
/// runtime assignment IO to enforce the preserve/no-bootstrap contract.
///
/// Returns `Ok(())` when the repository is enrolled, or a `not_enrolled`
/// validation error otherwise.
pub fn check_enrolled(repo_root: &Path) -> PulseResult<()> {
    let manifest = repo_root.join(".pulse/workgraph/manifest.json");
    if !manifest.exists() {
        return Err(PulseError::validation(
            "not_enrolled",
            format!(
                "repository {} is not enrolled: no workgraph manifest found",
                repo_root.display()
            ),
        ));
    }
    let node_schema = repo_root.join(".pulse/workgraph/schemas/node.schema.json");
    if !node_schema.exists() {
        return Err(PulseError::validation(
            "not_enrolled",
            format!(
                "repository {} is not enrolled: no workgraph node schema found",
                repo_root.display()
            ),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Build the absolute path to the runtime assignment root directory.
pub fn assignment_root(repo_root: &Path) -> PathBuf {
    repo_root.join(ASSIGNMENT_RUNTIME_ROOT)
}

/// Build the absolute path to the leases subdirectory.
pub fn leases_dir(repo_root: &Path) -> PathBuf {
    assignment_root(repo_root).join(LEASES_DIR)
}

/// Build the absolute path to the workspaces subdirectory.
pub fn workspaces_dir(repo_root: &Path) -> PathBuf {
    assignment_root(repo_root).join(WORKSPACES_DIR)
}

/// Build the absolute path to the prepared-assignment records subdirectory.
pub fn prepared_dir(repo_root: &Path) -> PathBuf {
    assignment_root(repo_root).join(PREPARED_DIR)
}

/// Build the absolute path to the tombstones subdirectory.
pub fn tombstones_dir(repo_root: &Path) -> PathBuf {
    assignment_root(repo_root).join(TOMBSTONES_DIR)
}

fn validate_record_id(kind: &str, id: &str, expected_prefix: &str) -> PulseResult<()> {
    if id.is_empty()
        || !id.starts_with(expected_prefix)
        || id.len() == expected_prefix.len()
        || id.contains('/')
        || id.contains('\\')
        || id.contains('.')
        || Path::new(id)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(PulseError::validation(
            "invalid_assignment_record_id",
            format!(
                "{kind} id {id:?} is not filesystem-safe or does not start with {expected_prefix:?}"
            ),
        ));
    }
    Ok(())
}

fn validate_lease_id(lease_id: &str) -> PulseResult<()> {
    validate_record_id("lease", lease_id, "lease_")
}

fn validate_workspace_id(workspace_id: &str) -> PulseResult<()> {
    validate_record_id("workspace", workspace_id, "wt_")
}

fn validate_prepared_assignment_id(pa_id: &str) -> PulseResult<()> {
    validate_record_id("prepared assignment", pa_id, "pa_")
}

fn allowed_value(kind: &str, field: &str, value: &str, allowed: &[&str]) -> PulseResult<()> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(PulseError::validation(
            "invalid_assignment_record",
            format!("{kind}.{field} has unsupported value {value:?}"),
        ))
    }
}

fn require_version(kind: &str, actual: u32, expected: u32) -> PulseResult<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(PulseError::validation(
            "invalid_assignment_record",
            format!("{kind}.schema_version {actual} is unsupported; expected {expected}"),
        ))
    }
}

fn require_rfc3339(kind: &str, field: &str, value: &str) -> PulseResult<()> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|_| ())
        .map_err(|error| {
            PulseError::validation(
                "invalid_assignment_record",
                format!("{kind}.{field} is not RFC 3339: {error}"),
            )
        })
}

fn require_safe_relative_path(kind: &str, field: &str, value: &str) -> PulseResult<()> {
    if value == "." {
        return Ok(());
    }
    crate::storage::safe_repo_relative(value)
        .map(|_| ())
        .map_err(|error| {
            PulseError::validation(
                "invalid_assignment_record",
                format!("{kind}.{field} must be safe repository-relative path: {error}"),
            )
        })
}

fn validate_lease_record(record: &AssignmentLeaseRecordV1) -> PulseResult<()> {
    require_version("lease", record.schema_version, LEASE_SCHEMA_VERSION)?;
    validate_lease_id(&record.lease_id)?;
    validate_workspace_id(&record.workspace_id)?;
    validate_prepared_assignment_id(&record.prepared_assignment_id)?;
    allowed_value("lease", "kind", &record.kind, &[LEASE_KIND_IMPLEMENTATION])?;
    allowed_value(
        "lease",
        "state",
        &record.state,
        &[
            LEASE_STATE_PREPARED,
            LEASE_STATE_RELEASED,
            LEASE_STATE_EXPIRED,
            LEASE_STATE_STALE,
        ],
    )?;
    require_rfc3339("lease", "issued_at", &record.issued_at)?;
    require_rfc3339("lease", "expires_at", &record.expires_at)?;
    Ok(())
}

fn validate_workspace_record(record: &AssignmentWorkspaceRecordV1) -> PulseResult<()> {
    require_version("workspace", record.schema_version, WORKSPACE_SCHEMA_VERSION)?;
    validate_workspace_id(&record.workspace_id)?;
    validate_lease_id(&record.lease_id)?;
    validate_prepared_assignment_id(&record.prepared_assignment_id)?;
    allowed_value(
        "workspace",
        "mode",
        &record.mode,
        &[WORKSPACE_MODE_IN_PLACE, WORKSPACE_MODE_ISOLATED],
    )?;
    allowed_value(
        "workspace",
        "state",
        &record.state,
        &[
            WORKSPACE_STATE_BOUND,
            WORKSPACE_STATE_RELEASED,
            WORKSPACE_STATE_STALE,
        ],
    )?;
    require_safe_relative_path("workspace", "path", &record.path)?;
    require_rfc3339("workspace", "created_at", &record.created_at)?;
    if let Some(released_at) = &record.released_at {
        require_rfc3339("workspace", "released_at", released_at)?;
    }
    Ok(())
}

fn validate_prepared_record(record: &PreparedAssignmentRecordV1) -> PulseResult<()> {
    require_version(
        "prepared_assignment",
        record.schema_version,
        crate::assignment::ASSIGNMENT_SCHEMA_VERSION,
    )?;
    validate_prepared_assignment_id(&record.prepared_assignment_id)?;
    if record.profile != PREPARED_ASSIGNMENT_PROFILE {
        return Err(PulseError::validation(
            "invalid_assignment_record",
            format!(
                "prepared_assignment.profile has unsupported value {:?}",
                record.profile
            ),
        ));
    }
    validate_lease_id(&record.lease.lease_id)?;
    validate_workspace_id(&record.workspace.workspace_id)?;
    require_safe_relative_path(
        "prepared_assignment",
        "workspace.path",
        &record.workspace.path,
    )?;
    Ok(())
}

fn validate_tombstone_record(record: &AssignmentTombstoneV1) -> PulseResult<()> {
    require_version("tombstone", record.schema_version, TOMBSTONE_SCHEMA_VERSION)?;
    validate_lease_id(&record.lease_id)?;
    allowed_value(
        "tombstone",
        "state",
        &record.state,
        &[
            TOMBSTONE_STATE_RELEASED,
            TOMBSTONE_STATE_EXPIRED,
            TOMBSTONE_STATE_STALE,
        ],
    )?;
    require_rfc3339("tombstone", "recorded_at", &record.recorded_at)?;
    Ok(())
}

fn record_path(dir: PathBuf, kind: &str, id: &str, expected_prefix: &str) -> PulseResult<PathBuf> {
    validate_record_id(kind, id, expected_prefix)?;
    Ok(dir.join(format!("{id}.{ext}", ext = RECORD_EXTENSION)))
}

/// Build the absolute path to a specific lease record file.
pub fn lease_path(repo_root: &Path, lease_id: &str) -> PulseResult<PathBuf> {
    record_path(leases_dir(repo_root), "lease", lease_id, "lease_")
}

/// Build the absolute path to a specific workspace record file.
pub fn workspace_path(repo_root: &Path, workspace_id: &str) -> PulseResult<PathBuf> {
    record_path(workspaces_dir(repo_root), "workspace", workspace_id, "wt_")
}

/// Build the absolute path to a specific prepared-assignment record file.
pub fn prepared_assignment_path(repo_root: &Path, pa_id: &str) -> PulseResult<PathBuf> {
    record_path(prepared_dir(repo_root), "prepared assignment", pa_id, "pa_")
}

/// Build the absolute path to a specific tombstone record file.
pub fn tombstone_path(repo_root: &Path, lease_id: &str) -> PulseResult<PathBuf> {
    record_path(tombstones_dir(repo_root), "lease", lease_id, "lease_")
}

// ---------------------------------------------------------------------------
// Directory existence helpers (safe: never create)
// ---------------------------------------------------------------------------

fn leases_dir_exists(repo_root: &Path) -> bool {
    leases_dir(repo_root).is_dir()
}

fn tombstones_dir_exists(repo_root: &Path) -> bool {
    tombstones_dir(repo_root).is_dir()
}

// ---------------------------------------------------------------------------
// List helpers
// ---------------------------------------------------------------------------

/// Extract the file stem (ID) from a record filename.
fn id_from_filename(path: &Path) -> Option<String> {
    if path.extension().and_then(|s| s.to_str()) != Some(RECORD_EXTENSION) {
        return None;
    }
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
}

/// List record IDs from a directory, returning only `.json` file stems.
fn list_ids_from_dir(dir: &Path) -> PulseResult<Vec<String>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut ids: Vec<String> = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| PulseError::io(dir, e))? {
        let entry = entry.map_err(|e| PulseError::io(dir, e))?;
        let file_type = entry.file_type().map_err(|e| PulseError::io(dir, e))?;
        if !file_type.is_file() {
            continue;
        }
        if let Some(id) = id_from_filename(&entry.path()) {
            ids.push(id);
        }
    }
    ids.sort();
    Ok(ids)
}

// ---------------------------------------------------------------------------
// Public list functions
// ---------------------------------------------------------------------------

/// List all live lease record IDs.
///
/// Returns an empty vector when the leases directory does not exist (no
/// runtime assignment state has been created yet). Never creates the
/// directory as a side effect.
pub fn list_lease_ids(repo_root: &Path) -> PulseResult<Vec<String>> {
    check_enrolled(repo_root)?;
    list_ids_from_dir(&leases_dir(repo_root))
}

/// List all workspace record IDs.
pub fn list_workspace_ids(repo_root: &Path) -> PulseResult<Vec<String>> {
    check_enrolled(repo_root)?;
    list_ids_from_dir(&workspaces_dir(repo_root))
}

/// List all prepared-assignment record IDs.
pub fn list_prepared_ids(repo_root: &Path) -> PulseResult<Vec<String>> {
    check_enrolled(repo_root)?;
    list_ids_from_dir(&prepared_dir(repo_root))
}

/// List all tombstone record IDs (leases with terminal state).
pub fn list_tombstone_ids(repo_root: &Path) -> PulseResult<Vec<String>> {
    check_enrolled(repo_root)?;
    list_ids_from_dir(&tombstones_dir(repo_root))
}

// ---------------------------------------------------------------------------
// Public load functions
// ---------------------------------------------------------------------------

/// Load a live lease record by ID.
pub fn load_lease(repo_root: &Path, lease_id: &str) -> PulseResult<AssignmentLeaseRecordV1> {
    check_enrolled(repo_root)?;
    let path = lease_path(repo_root, lease_id)?;
    let record = storage::read_json(&path)?;
    validate_lease_record(&record)?;
    Ok(record)
}

/// Load a workspace record by ID.
pub fn load_workspace(
    repo_root: &Path,
    workspace_id: &str,
) -> PulseResult<AssignmentWorkspaceRecordV1> {
    check_enrolled(repo_root)?;
    let path = workspace_path(repo_root, workspace_id)?;
    let record = storage::read_json(&path)?;
    validate_workspace_record(&record)?;
    Ok(record)
}

/// Load a prepared-assignment record by ID.
pub fn load_prepared(repo_root: &Path, pa_id: &str) -> PulseResult<PreparedAssignmentRecordV1> {
    check_enrolled(repo_root)?;
    let path = prepared_assignment_path(repo_root, pa_id)?;
    let record = storage::read_json(&path)?;
    validate_prepared_record(&record)?;
    Ok(record)
}

/// Load a tombstone record by lease ID.
pub fn load_tombstone(repo_root: &Path, lease_id: &str) -> PulseResult<AssignmentTombstoneV1> {
    check_enrolled(repo_root)?;
    let path = tombstone_path(repo_root, lease_id)?;
    let record = storage::read_json(&path)?;
    validate_tombstone_record(&record)?;
    Ok(record)
}

// ---------------------------------------------------------------------------
// Public write functions (create-new)
// ---------------------------------------------------------------------------

/// Ensure a directory exists (internal helper for write operations).
fn ensure_dir(dir: &Path) -> PulseResult<()> {
    fs::create_dir_all(dir).map_err(|e| PulseError::io(dir, e))
}

/// Write a lease record as a new file (create-new semantics).
///
/// The leases directory is created if it does not already exist.
/// Errors if the file already exists.
pub fn write_lease(repo_root: &Path, record: &AssignmentLeaseRecordV1) -> PulseResult<()> {
    check_enrolled(repo_root)?;
    validate_lease_record(record)?;
    let dir = leases_dir(repo_root);
    ensure_dir(&dir)?;
    let bytes = crate::canonical_json::to_canonical_bytes(record)?;
    storage::create_new(&lease_path(repo_root, &record.lease_id)?, &bytes)
}

/// Write a workspace record as a new file (create-new semantics).
pub fn write_workspace(repo_root: &Path, record: &AssignmentWorkspaceRecordV1) -> PulseResult<()> {
    check_enrolled(repo_root)?;
    validate_workspace_record(record)?;
    let dir = workspaces_dir(repo_root);
    ensure_dir(&dir)?;
    let bytes = crate::canonical_json::to_canonical_bytes(record)?;
    storage::create_new(&workspace_path(repo_root, &record.workspace_id)?, &bytes)
}

/// Write a prepared-assignment record as a new file (create-new semantics).
pub fn write_prepared(repo_root: &Path, record: &PreparedAssignmentRecordV1) -> PulseResult<()> {
    check_enrolled(repo_root)?;
    validate_prepared_record(record)?;
    let dir = prepared_dir(repo_root);
    ensure_dir(&dir)?;
    let bytes = crate::canonical_json::to_canonical_bytes(record)?;
    storage::create_new(
        &prepared_assignment_path(repo_root, &record.prepared_assignment_id)?,
        &bytes,
    )
}

/// Write a tombstone record as a new file (create-new semantics).
pub fn write_tombstone(repo_root: &Path, tombstone: &AssignmentTombstoneV1) -> PulseResult<()> {
    check_enrolled(repo_root)?;
    validate_tombstone_record(tombstone)?;
    let dir = tombstones_dir(repo_root);
    ensure_dir(&dir)?;
    let bytes = crate::canonical_json::to_canonical_bytes(tombstone)?;
    storage::create_new(&tombstone_path(repo_root, &tombstone.lease_id)?, &bytes)
}

// ---------------------------------------------------------------------------
// Public remove functions
// ---------------------------------------------------------------------------

/// Remove a live lease record file.
///
/// Returns `Ok(())` if the file was removed, or an error if the file
/// does not exist or removal fails.
pub fn remove_lease(repo_root: &Path, lease_id: &str) -> PulseResult<()> {
    check_enrolled(repo_root)?;
    let path = lease_path(repo_root, lease_id)?;
    fs::remove_file(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            PulseError::NotFound {
                subject: format!("lease record {lease_id}"),
            }
        } else {
            PulseError::io(&path, e)
        }
    })
}

/// Remove a workspace record file.
pub fn remove_workspace(repo_root: &Path, workspace_id: &str) -> PulseResult<()> {
    check_enrolled(repo_root)?;
    let path = workspace_path(repo_root, workspace_id)?;
    fs::remove_file(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            PulseError::NotFound {
                subject: format!("workspace record {workspace_id}"),
            }
        } else {
            PulseError::io(&path, e)
        }
    })
}

// ---------------------------------------------------------------------------
// Live lease predicate
// ---------------------------------------------------------------------------

/// Find a live exclusive lease ID for the given subject (Ticket ID).
///
/// A lease is considered live when:
/// - A live lease record file exists for that lease ID.
/// - No terminal tombstone exists for that lease ID.
/// - The lease `state` is `"prepared"`.
/// - The lease has not expired (`expires_at > now`).
///
/// Returns `Some(lease_id)` if a live lease is found, `None` otherwise.
///
/// This checks only the runtime store conditions. The kernel composition
/// layer additionally verifies that the graph node revision matches the
/// lease's expected revision before performing mutations.
pub fn find_live_lease_for_subject(
    repo_root: &Path,
    subject_id: &str,
) -> PulseResult<Option<String>> {
    check_enrolled(repo_root)?;

    if !leases_dir_exists(repo_root) {
        return Ok(None);
    }
    let tombstones_present = tombstones_dir_exists(repo_root);

    let lease_ids = list_lease_ids(repo_root)?;
    for lease_id in &lease_ids {
        // Skip leases with a terminal tombstone.
        if tombstones_present && tombstone_path(repo_root, lease_id)?.exists() {
            continue;
        }
        // Load and validate the lease record. Corrupt or schema-invalid records
        // are not live; recovery classification surfaces them separately.
        let record = match load_lease(repo_root, lease_id).and_then(|record| {
            validate_lease_record(&record)?;
            Ok(record)
        }) {
            Ok(r) => r,
            Err(_) => continue,
        };
        // Check subject match.
        if record.subject.id != subject_id {
            continue;
        }
        // Check state is "prepared" (live).
        if record.state != crate::assignment::LEASE_STATE_PREPARED {
            continue;
        }
        // Check not expired.
        let Ok(expires) = chrono::DateTime::parse_from_rfc3339(&record.expires_at) else {
            continue;
        };
        if chrono::Utc::now() > expires {
            continue;
        }
        return Ok(Some(lease_id.clone()));
    }
    Ok(None)
}

/// Check whether a specific lease ID has a terminal tombstone.
pub fn has_tombstone(repo_root: &Path, lease_id: &str) -> PulseResult<bool> {
    check_enrolled(repo_root)?;
    validate_lease_id(lease_id)?;
    if !tombstones_dir_exists(repo_root) {
        return Ok(false);
    }
    Ok(tombstone_path(repo_root, lease_id)?.exists())
}

// ---------------------------------------------------------------------------
// Read-only recovery classification
// ---------------------------------------------------------------------------

/// Classification of a single runtime assignment record during recovery.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LeaseClassification {
    /// Live lease that is current and not expired.
    Live,
    /// Lease that has expired (expires_at is in the past).
    Expired,
    /// Lease that has a terminal tombstone (released/expired/stale).
    Tombstoned,
    /// Runtime state is internally inconsistent and must not be repaired silently.
    Ambiguous(String),
    /// Lease record that could not be loaded, parsed or validated.
    Invalid(String),
}

/// A summary entry in the read-only recovery report.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecoveryEntry {
    /// The lease ID.
    pub lease_id: String,
    /// The subject (Ticket) ID this lease belongs to.
    pub subject_id: String,
    /// The lease classification.
    pub classification: LeaseClassification,
    /// The lease state string.
    pub state: String,
    /// The workspace ID bound to this lease, if any.
    pub workspace_id: String,
}

/// Read-only recovery state report for runtime assignments.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AssignmentRecoveryReport {
    /// All lease records found in the runtime store.
    pub entries: Vec<RecoveryEntry>,
    /// Lease IDs that are live (state=prepared, not expired, no tombstone).
    pub live_count: usize,
    /// Lease IDs that have expired (state=prepared, expires_at is past).
    pub expired_count: usize,
    /// Lease IDs that have a terminal tombstone.
    pub tombstoned_count: usize,
    /// Runtime entries that are internally inconsistent and require operator repair.
    pub ambiguous_count: usize,
    /// Lease IDs that could not be loaded, parsed or validated.
    pub invalid_count: usize,
    /// Orphan workspace IDs that have no matching live/tombstoned lease.
    pub orphan_workspace_ids: Vec<String>,
}

/// Classify runtime assignment state for read-only recovery inspection.
///
/// This function never mutates runtime or graph state. It lists lease records
/// and classifies each one based on its state, expiry and tombstone presence.
/// Orphan workspaces (no matching lease record) are also reported.
///
/// When the runtime assignment directory does not exist, returns an empty
/// report (no error).
pub fn classify_assignment_recovery_state(
    repo_root: &Path,
) -> PulseResult<AssignmentRecoveryReport> {
    check_enrolled(repo_root)?;

    let mut entries: Vec<RecoveryEntry> = Vec::new();
    let mut live_count = 0usize;
    let mut expired_count = 0usize;
    let mut tombstoned_count = 0usize;
    let mut ambiguous_count = 0usize;
    let mut invalid_count = 0usize;

    // Collect all known lease IDs (live leases + tombstones).
    let lease_ids = list_lease_ids(repo_root)?;
    let tombstone_ids = list_tombstone_ids(repo_root)?;

    // Merge lease IDs, deduping: a tombstoned lease might still have a live
    // file if recovery/release left it in an inconsistent state. The
    // classification handles this.
    let mut all_ids: Vec<String> = Vec::new();
    all_ids.extend(lease_ids.iter().cloned());
    for tid in &tombstone_ids {
        if !all_ids.contains(tid) {
            all_ids.push(tid.clone());
        }
    }
    all_ids.sort();
    all_ids.dedup();

    for lease_id in &all_ids {
        let has_live_file = lease_path(repo_root, lease_id)?.exists();
        let has_tombstone_file = tombstone_path(repo_root, lease_id)?.exists();

        if !has_live_file && has_tombstone_file {
            // Tombstone-only entry: lease was released/expired.
            match load_tombstone(repo_root, lease_id) {
                Ok(tombstone) => match validate_tombstone_record(&tombstone) {
                    Ok(()) => {
                        tombstoned_count += 1;
                        entries.push(RecoveryEntry {
                            lease_id: lease_id.clone(),
                            subject_id: tombstone.subject_id,
                            classification: LeaseClassification::Tombstoned,
                            state: tombstone.state,
                            workspace_id: String::new(),
                        });
                    }
                    Err(error) => {
                        invalid_count += 1;
                        entries.push(RecoveryEntry {
                            lease_id: lease_id.clone(),
                            subject_id: tombstone.subject_id,
                            classification: LeaseClassification::Invalid(format!(
                                "tombstone invalid: {error}"
                            )),
                            state: tombstone.state,
                            workspace_id: String::new(),
                        });
                    }
                },
                Err(_) => {
                    invalid_count += 1;
                    entries.push(RecoveryEntry {
                        lease_id: lease_id.clone(),
                        subject_id: String::new(),
                        classification: LeaseClassification::Invalid(
                            "tombstone unreadable".to_string(),
                        ),
                        state: String::new(),
                        workspace_id: String::new(),
                    });
                }
            }
            continue;
        }

        if !has_live_file {
            // Neither live file nor tombstone (shouldn't happen via
            // dedup, but handle gracefully).
            invalid_count += 1;
            entries.push(RecoveryEntry {
                lease_id: lease_id.clone(),
                subject_id: String::new(),
                classification: LeaseClassification::Invalid("missing".to_string()),
                state: String::new(),
                workspace_id: String::new(),
            });
            continue;
        }

        // Live lease file exists.
        match load_lease(repo_root, lease_id) {
            Ok(record) => {
                if let Err(error) = validate_lease_record(&record) {
                    invalid_count += 1;
                    entries.push(RecoveryEntry {
                        lease_id: lease_id.clone(),
                        subject_id: record.subject.id,
                        classification: LeaseClassification::Invalid(format!(
                            "lease invalid: {error}"
                        )),
                        state: record.state,
                        workspace_id: record.workspace_id,
                    });
                } else if has_tombstone_file {
                    // Both live file and tombstone: ambiguous ownership until
                    // a later mutating recovery command proves whether release
                    // completed or the live record should be preserved.
                    ambiguous_count += 1;
                    entries.push(RecoveryEntry {
                        lease_id: lease_id.clone(),
                        subject_id: record.subject.id,
                        classification: LeaseClassification::Ambiguous(
                            "live lease and terminal tombstone both exist".to_string(),
                        ),
                        state: record.state,
                        workspace_id: record.workspace_id,
                    });
                } else if record.state != LEASE_STATE_PREPARED {
                    // Non-prepared lease without a matching tombstone:
                    // ambiguous terminal state.
                    ambiguous_count += 1;
                    entries.push(RecoveryEntry {
                        lease_id: lease_id.clone(),
                        subject_id: record.subject.id,
                        classification: LeaseClassification::Ambiguous(
                            "non-prepared live lease without tombstone".to_string(),
                        ),
                        state: record.state,
                        workspace_id: record.workspace_id,
                    });
                } else if is_expired(&record.expires_at) {
                    expired_count += 1;
                    entries.push(RecoveryEntry {
                        lease_id: lease_id.clone(),
                        subject_id: record.subject.id,
                        classification: LeaseClassification::Expired,
                        state: record.state,
                        workspace_id: record.workspace_id,
                    });
                } else {
                    live_count += 1;
                    entries.push(RecoveryEntry {
                        lease_id: lease_id.clone(),
                        subject_id: record.subject.id,
                        classification: LeaseClassification::Live,
                        state: record.state,
                        workspace_id: record.workspace_id,
                    });
                }
            }
            Err(e) => {
                invalid_count += 1;
                entries.push(RecoveryEntry {
                    lease_id: lease_id.clone(),
                    subject_id: String::new(),
                    classification: LeaseClassification::Invalid(format!("load failed: {e}")),
                    state: String::new(),
                    workspace_id: String::new(),
                });
            }
        }
    }

    // Detect orphan workspaces: workspaces whose lease_id doesn't match
    // any known lease record (live or tombstoned).
    let all_known_lease_ids: Vec<String> = {
        let mut known: Vec<String> = Vec::new();
        // Collect lease IDs from live leases.
        for lid in &lease_ids {
            if !known.contains(lid) {
                known.push(lid.clone());
            }
        }
        // Also from tombstones.
        for tid in &tombstone_ids {
            if !known.contains(tid) {
                known.push(tid.clone());
            }
        }
        // Plus from the recovery entries (for entries loaded from
        // tombstones whose lease_id isn't in either live or tombstone
        // lists — unlikely but safe).
        for entry in &entries {
            if !known.contains(&entry.lease_id) {
                known.push(entry.lease_id.clone());
            }
        }
        known
    };

    let workspace_ids = list_workspace_ids(repo_root)?;
    let mut orphan_workspace_ids: Vec<String> = Vec::new();
    for ws_id in &workspace_ids {
        match load_workspace(repo_root, ws_id) {
            Ok(ws) => {
                if validate_workspace_record(&ws).is_err()
                    || !all_known_lease_ids.contains(&ws.lease_id)
                {
                    orphan_workspace_ids.push(ws_id.clone());
                }
            }
            Err(_) => {
                orphan_workspace_ids.push(ws_id.clone());
            }
        }
    }
    orphan_workspace_ids.sort();

    Ok(AssignmentRecoveryReport {
        entries,
        live_count,
        expired_count,
        tombstoned_count,
        ambiguous_count,
        invalid_count,
        orphan_workspace_ids,
    })
}

/// Check whether an RFC 3339 timestamp is expired (in the past).
fn is_expired(timestamp: &str) -> bool {
    match chrono::DateTime::parse_from_rfc3339(timestamp) {
        Ok(dt) => {
            let now: chrono::DateTime<chrono::Utc> = chrono::Utc::now();
            now > dt
        }
        Err(_) => {
            // Unparsable timestamp: treat as expired.
            true
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assignment::{
        AssignmentLeaseAssignee, AssignmentLeaseRecordV1, AssignmentLeaseSource,
        AssignmentLeaseSubject, AssignmentLeaseSummary, AssignmentLifecycle,
        AssignmentSubjectSnapshot, AssignmentTombstoneV1, AssignmentWorkspaceRecordV1,
        AssignmentWorkspaceSummary, CapabilityMatchReport, PreparedAssignmentRecordV1,
        RevalidatedSnapshot, WorkspaceCleanupPolicy, WorkspaceSubjectRef, WORKSPACE_MODE_ISOLATED,
        WORKSPACE_STATE_BOUND,
    };
    use tempfile::TempDir;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn enrolled_repo() -> PulseResult<TempDir> {
        let dir = tempfile::tempdir().map_err(|error| PulseError::io("<tempdir>", error))?;
        // Create minimal enrollment markers.
        let manifest = dir.path().join(".pulse/workgraph/manifest.json");
        let manifest_parent = manifest.parent().ok_or_else(|| {
            PulseError::validation("test_setup_invalid", "manifest path has no parent")
        })?;
        fs::create_dir_all(manifest_parent)
            .map_err(|error| PulseError::io(manifest_parent, error))?;
        fs::write(
            &manifest,
            r#"{"schema_version":1,"code":"pulse-main","content_root":"../../works","id_pattern":"^(EP|ST|TK|DEC)-[0-9]{3,}$","node_schema":".pulse/workgraph/schemas/node.schema.json","edge_schema":".pulse/workgraph/schemas/edge.schema.json","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}"#,
        )
        .map_err(|error| PulseError::io(&manifest, error))?;

        let node_schema = dir.path().join(".pulse/workgraph/schemas/node.schema.json");
        let schema_parent = node_schema.parent().ok_or_else(|| {
            PulseError::validation("test_setup_invalid", "node schema path has no parent")
        })?;
        fs::create_dir_all(schema_parent).map_err(|error| PulseError::io(schema_parent, error))?;
        fs::write(&node_schema, "{}").map_err(|error| PulseError::io(&node_schema, error))?;
        Ok(dir)
    }

    fn enrolled_repo_or_panic() -> TempDir {
        enrolled_repo().unwrap_or_else(|error| panic!("create enrolled repo: {error}"))
    }

    fn non_enrolled_repo() -> TempDir {
        let dir = tempfile::tempdir().expect("create temp dir");
        // Create a .pulse directory but no workgraph — simulates partial state.
        fs::create_dir_all(dir.path().join(".pulse")).expect("create .pulse dir");
        dir
    }

    fn dummy_lease(id: &str, subject_id: &str) -> AssignmentLeaseRecordV1 {
        AssignmentLeaseRecordV1 {
            schema_version: crate::assignment::LEASE_SCHEMA_VERSION,
            lease_id: id.to_string(),
            kind: crate::assignment::LEASE_KIND_IMPLEMENTATION.to_string(),
            subject: AssignmentLeaseSubject {
                kind: "ticket".to_string(),
                id: subject_id.to_string(),
                revision: 5,
                contract_revision: 2,
                status_at_claim: "ready".to_string(),
            },
            assignee: AssignmentLeaseAssignee {
                principal: "agent:test".to_string(),
            },
            issued_by: "human:test".to_string(),
            issued_at: "2026-07-28T10:00:00Z".to_string(),
            expires_at: "2030-07-28T10:30:00Z".to_string(),
            ttl_seconds: 1800,
            state: crate::assignment::LEASE_STATE_PREPARED.to_string(),
            packet_fingerprint:
                "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    .to_string(),
            readiness_fingerprint:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                    .to_string(),
            workspace_id: format!("wt_{id}"),
            prepared_assignment_id: format!("pa_{id}"),
            capability_inventory_identity:
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
            source: AssignmentLeaseSource {
                repository_id: "repo_test".to_string(),
                base_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
            },
        }
    }

    fn dummy_workspace(id: &str, lease_id: &str) -> AssignmentWorkspaceRecordV1 {
        AssignmentWorkspaceRecordV1 {
            schema_version: crate::assignment::WORKSPACE_SCHEMA_VERSION,
            workspace_id: id.to_string(),
            lease_id: lease_id.to_string(),
            prepared_assignment_id: format!("pa_{lease_id}"),
            subject: WorkspaceSubjectRef {
                kind: "ticket".to_string(),
                id: "TK-001".to_string(),
                revision: 5,
            },
            mode: WORKSPACE_MODE_ISOLATED.to_string(),
            path: format!(".pulse/runtime/workspaces/{id}"),
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
        }
    }

    fn dummy_tombstone(lease_id: &str, subject_id: &str, state: &str) -> AssignmentTombstoneV1 {
        AssignmentTombstoneV1 {
            schema_version: crate::assignment::TOMBSTONE_SCHEMA_VERSION,
            lease_id: lease_id.to_string(),
            subject_id: subject_id.to_string(),
            state: state.to_string(),
            recorded_at: "2026-07-28T11:00:00Z".to_string(),
            actor: "human:test".to_string(),
            reason: None,
            reason_codes: vec![],
        }
    }

    // -----------------------------------------------------------------------
    // Enrollment checks
    // -----------------------------------------------------------------------

    #[test]
    fn check_enrolled_succeeds_for_enrolled_repo() {
        let repo = enrolled_repo_or_panic();
        check_enrolled(repo.path()).expect("enrolled repo should pass");
    }

    #[test]
    fn check_enrolled_fails_for_non_enrolled_repo() {
        let repo = non_enrolled_repo();
        let err = check_enrolled(repo.path()).expect_err("non-enrolled repo should fail");
        assert_eq!(err.code(), "not_enrolled");
    }

    #[test]
    fn store_functions_reject_non_enrolled_repo() {
        let repo = non_enrolled_repo();

        let e = list_lease_ids(repo.path()).expect_err("expected error");
        assert_eq!(e.code(), "not_enrolled");

        let e = list_workspace_ids(repo.path()).expect_err("expected error");
        assert_eq!(e.code(), "not_enrolled");

        let e = list_prepared_ids(repo.path()).expect_err("expected error");
        assert_eq!(e.code(), "not_enrolled");

        let e = list_tombstone_ids(repo.path()).expect_err("expected error");
        assert_eq!(e.code(), "not_enrolled");
    }

    #[test]
    fn non_enrolled_write_rejects_before_runtime_creation_even_with_bad_id() {
        let repo = non_enrolled_repo();
        let lease = dummy_lease("../escape", "TK-001");
        let err = write_lease(repo.path(), &lease).expect_err("expected non-enrolled error");
        assert_eq!(err.code(), "not_enrolled");
        assert!(
            !repo.path().join(".pulse/runtime").exists(),
            "non-enrolled writes must not bootstrap runtime paths"
        );
    }

    // -----------------------------------------------------------------------
    // Write + List + Load round-trips
    // -----------------------------------------------------------------------

    #[test]
    fn write_and_list_lease_records() {
        let repo = enrolled_repo_or_panic();
        let lease = dummy_lease("lease_01JTEST", "TK-001");
        write_lease(repo.path(), &lease).expect("write lease record");

        let ids = list_lease_ids(repo.path()).expect("list leases");
        assert_eq!(ids, vec!["lease_01JTEST"]);

        let loaded = load_lease(repo.path(), "lease_01JTEST").expect("load lease");
        assert_eq!(loaded, lease);

        let lease_record_path = lease_path(repo.path(), "lease_01JTEST")
            .unwrap_or_else(|error| panic!("lease path should be valid: {error}"));
        let bytes = fs::read_to_string(lease_record_path).expect("read canonical lease bytes");
        let expected = String::from_utf8(
            crate::canonical_json::to_canonical_bytes(&lease)
                .unwrap_or_else(|error| panic!("serialize lease canonically: {error}")),
        )
        .expect("canonical JSON is UTF-8");
        assert_eq!(bytes, expected);
    }

    #[test]
    fn write_rejects_path_traversal_ids_before_directory_creation() {
        let repo = enrolled_repo_or_panic();
        let lease = dummy_lease("lease_../escape", "TK-001");
        let err = write_lease(repo.path(), &lease).expect_err("unsafe id should fail");
        assert_eq!(err.code(), "invalid_assignment_record_id");
        assert!(
            !leases_dir(repo.path()).exists(),
            "unsafe record IDs must reject before creating runtime dirs"
        );
    }

    #[test]
    fn load_remove_and_tombstone_reject_path_traversal_ids() {
        let repo = enrolled_repo_or_panic();
        let err = load_lease(repo.path(), "../escape").unwrap_err();
        assert_eq!(err.code(), "invalid_assignment_record_id");
        let err = remove_lease(repo.path(), "lease_/escape").unwrap_err();
        assert_eq!(err.code(), "invalid_assignment_record_id");
        let err = has_tombstone(repo.path(), "lease_..escape").unwrap_err();
        assert_eq!(err.code(), "invalid_assignment_record_id");
    }

    #[test]
    fn write_and_list_workspace_records() {
        let repo = enrolled_repo_or_panic();
        let ws = dummy_workspace("wt_TK-001_01JTEST", "lease_01JTEST");
        write_workspace(repo.path(), &ws).expect("write workspace record");

        let ids = list_workspace_ids(repo.path()).expect("list workspaces");
        assert_eq!(ids, vec!["wt_TK-001_01JTEST"]);

        let loaded = load_workspace(repo.path(), "wt_TK-001_01JTEST").expect("load workspace");
        assert_eq!(loaded, ws);
    }

    #[test]
    fn write_workspace_rejects_unsafe_workspace_path_before_directory_creation() {
        let repo = enrolled_repo_or_panic();
        let mut ws = dummy_workspace("wt_UNSAFE", "lease_01JTEST");
        ws.path = "../outside".to_string();
        let err = write_workspace(repo.path(), &ws).expect_err("unsafe workspace path should fail");
        assert_eq!(err.code(), "invalid_assignment_record");
        assert!(
            !workspaces_dir(repo.path()).exists(),
            "unsafe workspace paths must reject before creating runtime dirs"
        );
    }

    #[test]
    fn write_and_list_tombstones() {
        let repo = enrolled_repo_or_panic();
        let tombstone = dummy_tombstone("lease_01JTEST", "TK-001", "released");
        write_tombstone(repo.path(), &tombstone).expect("write tombstone");

        let ids = list_tombstone_ids(repo.path()).expect("list tombstones");
        assert_eq!(ids, vec!["lease_01JTEST"]);

        let loaded = load_tombstone(repo.path(), "lease_01JTEST").expect("load tombstone");
        assert_eq!(loaded, tombstone);
    }

    #[test]
    fn write_and_list_prepared_records() {
        let repo = enrolled_repo_or_panic();
        // Build a minimal prepared-assignment record.
        let pa = dummy_prepared_record("pa_01JTEST");
        write_prepared(repo.path(), &pa).expect("write prepared record");

        let ids = list_prepared_ids(repo.path()).expect("list prepared records");
        assert_eq!(ids, vec!["pa_01JTEST"]);

        let loaded = load_prepared(repo.path(), "pa_01JTEST").expect("load prepared record");
        assert_eq!(loaded, pa);
    }

    fn dummy_prepared_record(id: &str) -> PreparedAssignmentRecordV1 {
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
        let lease_summary = AssignmentLeaseSummary {
            lease_id: "lease_01JTEST".to_string(),
            state: crate::assignment::LEASE_STATE_PREPARED.to_string(),
            assignee: "agent:test".to_string(),
            issued_by: "human:test".to_string(),
            issued_at: "2026-07-28T10:00:00Z".to_string(),
            expires_at: "2030-07-28T10:30:00Z".to_string(),
            ttl_seconds: 1800,
            exclusive: true,
        };
        let ws_summary = AssignmentWorkspaceSummary {
            workspace_id: "wt_TEST".to_string(),
            binding_status: WORKSPACE_STATE_BOUND.to_string(),
            mode: WORKSPACE_MODE_ISOLATED.to_string(),
            path: ".pulse/runtime/workspaces/wt_TEST".to_string(),
            repository_id: "repo_test".to_string(),
            base_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
            cleanliness: "clean".to_string(),
            owner_lease_id: "lease_01JTEST".to_string(),
        };
        let cap_match = CapabilityMatchReport {
            inventory_identity:
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
            principal: "agent:test".to_string(),
            status: crate::assignment::CAP_MATCH_MATCHED.to_string(),
            required: vec!["source.read".to_string()],
            matched: vec!["source.read".to_string()],
            missing: vec![],
            extra: vec![],
            reason_codes: vec![],
        };
        let lifecycle = AssignmentLifecycle {
            transition: crate::assignment::LIFECYCLE_READY_TO_ACTIVE.to_string(),
            gate_profile: crate::assignment::LIFECYCLE_GATE_PROFILE.to_string(),
            gate_status: crate::assignment::GATE_STATUS_PASSED.to_string(),
            expected_revision: 5,
            new_revision: 6,
            event_id: "evt_01JTEST".to_string(),
        };
        PreparedAssignmentRecordV1 {
            schema_version: crate::assignment::ASSIGNMENT_SCHEMA_VERSION,
            profile: crate::assignment::PREPARED_ASSIGNMENT_PROFILE.to_string(),
            code: "prepared_assignment".to_string(),
            prepared_assignment_id: id.to_string(),
            subject: AssignmentSubjectSnapshot {
                id: "TK-001".to_string(),
                kind: "ticket".to_string(),
                revision_before: 5,
                revision_after: 6,
                contract_revision: 2,
                status_before: "ready".to_string(),
                status_after: "active".to_string(),
            },
            packet_fingerprint:
                "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    .to_string(),
            revalidated_snapshot: snapshot,
            lease: lease_summary,
            workspace: ws_summary,
            capability_match: cap_match,
            lifecycle,
            dispatch: crate::assignment::AssignmentDispatch::default(),
            transaction: crate::assignment::AssignmentTransaction::default(),
            prepared_assignment_fingerprint: String::new(),
            reason_codes: vec![],
        }
    }

    // -----------------------------------------------------------------------
    // Remove functions
    // -----------------------------------------------------------------------

    #[test]
    fn remove_lease_works() {
        let repo = enrolled_repo_or_panic();
        let lease = dummy_lease("lease_01JREMOVE", "TK-002");
        write_lease(repo.path(), &lease).expect("write lease");
        assert_eq!(list_lease_ids(repo.path()).unwrap().len(), 1);

        remove_lease(repo.path(), "lease_01JREMOVE").expect("remove lease");
        assert_eq!(list_lease_ids(repo.path()).unwrap().len(), 0);
    }

    #[test]
    fn remove_lease_not_found() {
        let repo = enrolled_repo_or_panic();
        let err = remove_lease(repo.path(), "lease_NONEXISTENT").expect_err("should fail");
        assert!(matches!(err, PulseError::NotFound { .. }));
    }

    // -----------------------------------------------------------------------
    // Live lease check
    // -----------------------------------------------------------------------

    #[test]
    fn live_lease_found_for_subject() {
        let repo = enrolled_repo_or_panic();
        let lease = dummy_lease("lease_01JLIVE", "TK-LIVE");
        write_lease(repo.path(), &lease).expect("write lease");

        let found = find_live_lease_for_subject(repo.path(), "TK-LIVE").expect("find live lease");
        assert_eq!(found, Some("lease_01JLIVE".to_string()));
    }

    #[test]
    fn no_live_lease_when_tombstoned() {
        let repo = enrolled_repo_or_panic();
        let lease = dummy_lease("lease_01JTOMB", "TK-TOMB");
        write_lease(repo.path(), &lease).expect("write lease");
        let tombstone = dummy_tombstone("lease_01JTOMB", "TK-TOMB", "released");
        write_tombstone(repo.path(), &tombstone).expect("write tombstone");

        let found = find_live_lease_for_subject(repo.path(), "TK-TOMB").expect("find live lease");
        assert_eq!(found, None);
    }

    #[test]
    fn no_live_lease_when_expired() {
        let repo = enrolled_repo_or_panic();
        let mut lease = dummy_lease("lease_01JEXP", "TK-EXP");
        lease.expires_at = "2020-01-01T00:00:00Z".to_string(); // past expiry.
        write_lease(repo.path(), &lease).expect("write lease");

        let found = find_live_lease_for_subject(repo.path(), "TK-EXP").expect("find live lease");
        assert_eq!(found, None);
    }

    #[test]
    fn no_live_lease_when_wrong_state() {
        let repo = enrolled_repo_or_panic();
        let mut lease = dummy_lease("lease_01JSTATE", "TK-STATE");
        lease.state = "expired".to_string();
        write_lease(repo.path(), &lease).expect("write lease");

        let found = find_live_lease_for_subject(repo.path(), "TK-STATE").expect("find live lease");
        assert_eq!(found, None);
    }

    #[test]
    fn no_live_lease_for_non_matching_subject() {
        let repo = enrolled_repo_or_panic();
        let lease = dummy_lease("lease_01JOTHER", "TK-OTHER");
        write_lease(repo.path(), &lease).expect("write lease");

        let found =
            find_live_lease_for_subject(repo.path(), "TK-DIFFERENT").expect("find live lease");
        assert_eq!(found, None);
    }

    #[test]
    fn no_live_lease_when_no_leases_exist() {
        let repo = enrolled_repo_or_panic();
        let found = find_live_lease_for_subject(repo.path(), "TK-EMPTY").expect("find live lease");
        assert_eq!(found, None);
    }

    // -----------------------------------------------------------------------
    // Tombstone check
    // -----------------------------------------------------------------------

    #[test]
    fn has_tombstone_returns_false_when_none() {
        let repo = enrolled_repo_or_panic();
        assert!(!has_tombstone(repo.path(), "lease_NONE").expect("has tombstone"));
    }

    #[test]
    fn has_tombstone_returns_true_when_tombstone_exists() {
        let repo = enrolled_repo_or_panic();
        let tombstone = dummy_tombstone("lease_HAS_TMB", "TK-001", "released");
        write_tombstone(repo.path(), &tombstone).expect("write tombstone");
        assert!(has_tombstone(repo.path(), "lease_HAS_TMB").expect("has tombstone"));
    }

    // -----------------------------------------------------------------------
    // Recovery classification (read-only)
    // -----------------------------------------------------------------------

    #[test]
    fn recovery_report_empty_when_no_runtime_state() {
        let repo = enrolled_repo_or_panic();
        let report =
            classify_assignment_recovery_state(repo.path()).expect("classify recovery state");
        assert_eq!(report.entries.len(), 0);
        assert_eq!(report.live_count, 0);
        assert_eq!(report.expired_count, 0);
        assert_eq!(report.tombstoned_count, 0);
        assert_eq!(report.ambiguous_count, 0);
        assert_eq!(report.invalid_count, 0);
        assert!(report.orphan_workspace_ids.is_empty());
    }

    #[test]
    fn recovery_report_classifies_live_leases() {
        let repo = enrolled_repo_or_panic();
        let lease = dummy_lease("lease_01JCLS", "TK-CLS");
        write_lease(repo.path(), &lease).expect("write lease");

        let report =
            classify_assignment_recovery_state(repo.path()).expect("classify recovery state");
        assert_eq!(report.live_count, 1);
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].classification, LeaseClassification::Live);
    }

    #[test]
    fn recovery_report_classifies_expired_leases() {
        let repo = enrolled_repo_or_panic();
        let mut lease = dummy_lease("lease_01JXPR", "TK-XPR");
        lease.expires_at = "2020-01-01T00:00:00Z".to_string();
        write_lease(repo.path(), &lease).expect("write lease");

        let report =
            classify_assignment_recovery_state(repo.path()).expect("classify recovery state");
        assert_eq!(report.expired_count, 1);
        assert_eq!(
            report.entries[0].classification,
            LeaseClassification::Expired
        );
    }

    #[test]
    fn recovery_report_classifies_tombstoned_leases() {
        let repo = enrolled_repo_or_panic();
        let tombstone = dummy_tombstone("lease_01JTMB", "TK-TMB", "released");
        write_tombstone(repo.path(), &tombstone).expect("write tombstone");

        let report =
            classify_assignment_recovery_state(repo.path()).expect("classify recovery state");
        assert_eq!(report.tombstoned_count, 1);
        assert_eq!(
            report.entries[0].classification,
            LeaseClassification::Tombstoned
        );
    }

    #[test]
    fn recovery_report_classifies_live_plus_tombstone_as_ambiguous() {
        let repo = enrolled_repo_or_panic();
        let lease = dummy_lease("lease_01JAMB", "TK-AMB");
        write_lease(repo.path(), &lease).expect("write lease");
        let tombstone = dummy_tombstone("lease_01JAMB", "TK-AMB", "released");
        write_tombstone(repo.path(), &tombstone).expect("write tombstone");

        let report =
            classify_assignment_recovery_state(repo.path()).expect("classify recovery state");
        assert_eq!(report.ambiguous_count, 1);
        assert_eq!(report.live_count, 0);
        assert!(matches!(
            report.entries[0].classification,
            LeaseClassification::Ambiguous(_)
        ));
    }

    #[test]
    fn recovery_report_classifies_invalid_runtime_records() {
        let repo = enrolled_repo_or_panic();
        let dir = leases_dir(repo.path());
        fs::create_dir_all(&dir).expect("create lease dir");
        fs::write(dir.join("lease_BAD.json"), b"{not json").expect("write corrupt lease");

        let report =
            classify_assignment_recovery_state(repo.path()).expect("classify recovery state");
        assert_eq!(report.invalid_count, 1);
        assert!(matches!(
            report.entries[0].classification,
            LeaseClassification::Invalid(_)
        ));
    }

    #[test]
    fn recovery_report_finds_orphan_workspaces() {
        let repo = enrolled_repo_or_panic();
        // Write a workspace referencing a lease that doesn't exist.
        let ws = dummy_workspace("wt_ORPHAN", "lease_NOBODY");
        write_workspace(repo.path(), &ws).expect("write workspace");

        let report =
            classify_assignment_recovery_state(repo.path()).expect("classify recovery state");
        assert_eq!(report.orphan_workspace_ids, vec!["wt_ORPHAN"]);
    }

    #[test]
    fn recovery_report_live_and_tombstoned_mixed() {
        let repo = enrolled_repo_or_panic();

        // Live lease.
        let live = dummy_lease("lease_01JLIVE2", "TK-LIVE2");
        write_lease(repo.path(), &live).expect("write live");

        // Expired lease.
        let mut expired = dummy_lease("lease_01JXPR2", "TK-XPR2");
        expired.expires_at = "2020-01-01T00:00:00Z".to_string();
        write_lease(repo.path(), &expired).expect("write expired");

        // Tombstoned lease.
        let tombstone = dummy_tombstone("lease_01JTMB2", "TK-TMB2", "expired");
        write_tombstone(repo.path(), &tombstone).expect("write tombstone");

        let report =
            classify_assignment_recovery_state(repo.path()).expect("classify recovery state");
        assert_eq!(report.live_count, 1);
        assert_eq!(report.expired_count, 1);
        assert_eq!(report.tombstoned_count, 1);
        assert_eq!(report.orphan_workspace_ids, Vec::<String>::new());
    }

    #[test]
    fn recovery_report_non_enrolled_repo_fails() {
        let repo = non_enrolled_repo();
        let err = classify_assignment_recovery_state(repo.path()).expect_err("should fail");
        assert_eq!(err.code(), "not_enrolled");
    }
}
