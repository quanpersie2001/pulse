//! Source snapshot / binding-status ownership.
//!
//! This module answers "is a given source binding still current for this
//! repository?" It deliberately couples git mechanics (`git`, `head_commit`,
//! `resolve_full_commit`) with source-binding status policy (`current_status`,
//! `SourceBindingStatus`) because the status decisions are computed directly
//! from git queries and the two concerns share the same small surface.
//!
//! Ownership review (task #53): a physical move to a `repository/source_snapshot`
//! namespace was considered and **deferred**. It would create a single-file
//! top-level namespace for marginal clarity, and the mechanics/policy split is
//! not safely separable here without duplicating git plumbing. The public
//! `pulse::source::*` path remains the stable contract. Revisit when a second
//! repository-snapshot concern joins this owner.
//!
//! P2S1-I2 additions: `PacketSourceSnapshot`, `packet_base_snapshot`,
//! `revalidate_packet_base`, `check_cleanliness` and `detect_operation_state`
//! for exact clean-HEAD packet source identity.

use crate::canonical_json::{hash_bytes, hash_serializable};
use crate::{PulseError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;

const EVIDENCE_ONLY_PREFIXES: [&str; 2] = [".pulse/evidence/", ".pulse/events/"];
const PULSE_RUNTIME_EXCLUDE_PATHS: [&str; 2] =
    [":(exclude).pulse/runtime/**", ":(exclude).pulse/cache/**"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceBindingStatus {
    Current,
    Stale,
    DirtyUnsupported,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceModeV1 {
    InPlace,
    IsolatedWorktree,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceOperationStateV1 {
    None,
    Merge,
    Rebase,
    CherryPick,
    Revert,
    Bisect,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceCleanlinessV1 {
    Clean,
    Dirty,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceSnapshotStatusV1 {
    Complete,
    Unsupported,
    BoundedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceSnapshotV1 {
    pub schema_version: u32,
    pub repository_id: String,
    pub workspace_id: String,
    pub workspace_mode: WorkspaceModeV1,
    pub base_commit: String,
    pub head_commit: String,
    pub diff_base_commit: String,
    pub operation_state: WorkspaceOperationStateV1,
    pub cleanliness: WorkspaceCleanlinessV1,
    pub tracked_diff_identity: String,
    pub untracked_manifest_identity: String,
    pub status_identity: String,
    pub snapshot_status: WorkspaceSnapshotStatusV1,
    pub captured_at: String,
    pub snapshot_identity: String,
}

impl WorkspaceSnapshotV1 {
    pub fn compute_identity(&self) -> Result<String> {
        let mut projection = serde_json::to_value(self)?;
        let object = projection.as_object_mut().ok_or_else(|| {
            PulseError::validation(
                "workspace_snapshot_invalid",
                "workspace snapshot projection must be an object",
            )
        })?;
        object.remove("snapshot_identity");
        object.remove("captured_at");
        hash_serializable(&projection)
    }
}

// ---------------------------------------------------------------------------
// P2S1-I2: Packet source snapshot types
// ---------------------------------------------------------------------------

/// Complete source snapshot for a work-packet base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PacketSourceSnapshot {
    pub repository_id: String,
    pub kind: String,
    pub commit: String,
    pub head_ref: Option<String>,
    pub worktree_root_kind: WorktreeRootKind,
    pub cleanliness: SourceCleanliness,
    pub operation_state: RepositoryOperationState,
    pub currentness: String,
}

/// Whether the worktree root is the primary or an existing (linked) worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeRootKind {
    PrimaryOrExistingWorktree,
    LinkedWorktree,
}

impl WorktreeRootKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PrimaryOrExistingWorktree => "primary_or_existing_worktree",
            Self::LinkedWorktree => "linked_worktree",
        }
    }
}

/// Whether the source worktree is clean (no tracked or untracked non-ignored
/// changes) or dirty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceCleanliness {
    Clean,
    Dirty,
}

impl SourceCleanliness {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Dirty => "dirty",
        }
    }
}

/// Current repository operation state (merge, rebase, cherry-pick, revert,
/// bisect) or normal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryOperationState {
    Normal,
    MergeInProgress,
    RebaseInProgress,
    CherryPickInProgress,
    RevertInProgress,
    BisectInProgress,
}

impl RepositoryOperationState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::MergeInProgress => "merge_in_progress",
            Self::RebaseInProgress => "rebase_in_progress",
            Self::CherryPickInProgress => "cherry_pick_in_progress",
            Self::RevertInProgress => "revert_in_progress",
            Self::BisectInProgress => "bisect_in_progress",
        }
    }
}

/// P2S3 workspace snapshot result used by I0 feasibility callers. The full
/// contract is [`WorkspaceSnapshotV1`]; this compatibility wrapper
/// omits volatile `captured_at` and `snapshot_identity` fields for older tests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceSnapshotFeasibilityV1 {
    pub schema_version: u64,
    pub repository_id: String,
    pub workspace_id: String,
    pub workspace_mode: String,
    pub base_commit: String,
    pub head_commit: String,
    pub diff_base_commit: String,
    pub operation_state: String,
    pub cleanliness: String,
    pub tracked_diff_identity: String,
    pub untracked_manifest_identity: String,
    pub status_identity: String,
    pub snapshot_status: String,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSnapshotOptions {
    pub repository_id: String,
    pub workspace_id: String,
    pub workspace_mode: String,
    pub base_commit: String,
    pub diff_base_commit: String,
    pub included_paths: Vec<String>,
    pub captured_at: String,
    pub max_tracked_diff_bytes: usize,
    pub max_status_bytes: usize,
    pub max_untracked_entries: usize,
    pub max_untracked_file_bytes: u64,
    pub max_untracked_total_bytes: u64,
}

impl WorkspaceSnapshotOptions {
    pub fn feasibility_defaults(
        repository_id: &str,
        workspace_id: &str,
        workspace_mode: &str,
        base_commit: &str,
    ) -> Self {
        Self {
            repository_id: repository_id.to_string(),
            workspace_id: workspace_id.to_string(),
            workspace_mode: workspace_mode.to_string(),
            base_commit: base_commit.to_string(),
            diff_base_commit: base_commit.to_string(),
            included_paths: Vec::new(),
            captured_at: "1970-01-01T00:00:00Z".to_string(),
            max_tracked_diff_bytes: 1024 * 1024,
            max_status_bytes: 1024 * 1024,
            max_untracked_entries: 4096,
            max_untracked_file_bytes: 1024 * 1024,
            max_untracked_total_bytes: 4 * 1024 * 1024,
        }
    }
}

pub fn workspace_snapshot(
    repo_root: &Path,
    options: &WorkspaceSnapshotOptions,
) -> Result<WorkspaceSnapshotV1> {
    if options.diff_base_commit != options.base_commit {
        return bounded_snapshot(
            repo_root,
            options,
            vec!["diff_base_must_equal_base".to_string()],
        );
    }

    let mut reason_codes = Vec::new();
    let head = head_commit(repo_root)?;
    if let Some(reason) = validate_snapshot_base_relationship(repo_root, options, &head)? {
        reason_codes.push(reason);
    }
    let operation_state = detect_operation_state(repo_root)?;
    if operation_state != RepositoryOperationState::Normal {
        reason_codes.push(format!("git_operation_{}", operation_state.as_str()));
    }

    let tracked = tracked_diff_identity(repo_root, options)?;
    extend_reason(&mut reason_codes, "tracked_diff", &tracked.status);

    let status = status_identity(repo_root, options)?;
    extend_reason(&mut reason_codes, "status", &status.status);

    let untracked = untracked_manifest_identity(repo_root, options)?;
    extend_reason(&mut reason_codes, "untracked_manifest", &untracked.status);

    if has_ignored_source_paths(repo_root, options)? {
        reason_codes.push("ignored_source_paths_unsupported".to_string());
    }

    let snapshot_status = snapshot_status_from_reasons(&reason_codes);
    let cleanliness = if status.dirty || untracked.dirty || tracked.dirty {
        WorkspaceCleanlinessV1::Dirty
    } else {
        WorkspaceCleanlinessV1::Clean
    };

    build_workspace_snapshot(
        options,
        WorkspaceSnapshotBuildParts {
            head,
            operation_state: operation_state_to_v1(&operation_state),
            cleanliness,
            tracked_diff_identity: tracked.identity,
            untracked_manifest_identity: untracked.identity,
            status_identity: status.identity,
            snapshot_status,
        },
    )
}

fn bounded_snapshot(
    repo_root: &Path,
    options: &WorkspaceSnapshotOptions,
    reason_codes: Vec<String>,
) -> Result<WorkspaceSnapshotV1> {
    let head = head_commit(repo_root)?;
    build_workspace_snapshot(
        options,
        WorkspaceSnapshotBuildParts {
            head,
            operation_state: WorkspaceOperationStateV1::Unknown,
            cleanliness: WorkspaceCleanlinessV1::Unknown,
            tracked_diff_identity: hash_bytes(&[]),
            untracked_manifest_identity: hash_bytes(&[]),
            status_identity: hash_bytes(&[]),
            snapshot_status: snapshot_status_from_reasons(&reason_codes),
        },
    )
}

pub fn workspace_snapshot_feasibility(
    repo_root: &Path,
    options: &WorkspaceSnapshotOptions,
) -> Result<WorkspaceSnapshotFeasibilityV1> {
    let snapshot = workspace_snapshot(repo_root, options)?;
    Ok(WorkspaceSnapshotFeasibilityV1 {
        schema_version: snapshot.schema_version as u64,
        repository_id: snapshot.repository_id,
        workspace_id: snapshot.workspace_id,
        workspace_mode: match snapshot.workspace_mode {
            WorkspaceModeV1::InPlace => "in_place".to_string(),
            WorkspaceModeV1::IsolatedWorktree => "isolated_worktree".to_string(),
        },
        base_commit: snapshot.base_commit,
        head_commit: snapshot.head_commit,
        diff_base_commit: snapshot.diff_base_commit,
        operation_state: match snapshot.operation_state {
            WorkspaceOperationStateV1::None => "normal".to_string(),
            WorkspaceOperationStateV1::Merge => "merge_in_progress".to_string(),
            WorkspaceOperationStateV1::Rebase => "rebase_in_progress".to_string(),
            WorkspaceOperationStateV1::CherryPick => "cherry_pick_in_progress".to_string(),
            WorkspaceOperationStateV1::Revert => "revert_in_progress".to_string(),
            WorkspaceOperationStateV1::Bisect => "bisect_in_progress".to_string(),
            WorkspaceOperationStateV1::Unknown => "unknown".to_string(),
        },
        cleanliness: match snapshot.cleanliness {
            WorkspaceCleanlinessV1::Clean => "clean".to_string(),
            WorkspaceCleanlinessV1::Dirty => "dirty".to_string(),
            WorkspaceCleanlinessV1::Unknown => "unknown".to_string(),
        },
        tracked_diff_identity: snapshot.tracked_diff_identity,
        untracked_manifest_identity: snapshot.untracked_manifest_identity,
        status_identity: snapshot.status_identity,
        snapshot_status: match snapshot.snapshot_status {
            WorkspaceSnapshotStatusV1::Complete => "complete".to_string(),
            WorkspaceSnapshotStatusV1::Unsupported => "unsupported".to_string(),
            WorkspaceSnapshotStatusV1::BoundedOut => "bounded_out".to_string(),
        },
        reason_codes: Vec::new(),
    })
}

pub fn resolve_full_commit(repo_root: &Path, commit: &str) -> Result<String> {
    if commit.len() != 40 || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(PulseError::validation(
            "source_binding_stale",
            "source commit must be a full 40 character hex oid",
        ));
    }
    let output = git(
        repo_root,
        ["rev-parse", "--verify", &format!("{commit}^{{commit}}")],
    )?;
    let resolved = output.trim();
    if resolved != commit {
        return Err(PulseError::validation(
            "source_binding_stale",
            "source commit did not resolve to requested oid",
        ));
    }
    Ok(resolved.to_string())
}

pub fn current_status(
    repo_root: &Path,
    commit: &str,
    scoped_paths: &[String],
) -> SourceBindingStatus {
    if resolve_full_commit(repo_root, commit).is_err() {
        return SourceBindingStatus::Stale;
    }
    if scoped_paths.iter().any(|path| path_dirty(repo_root, path)) {
        return SourceBindingStatus::DirtyUnsupported;
    }
    let head = match git(repo_root, ["rev-parse", "HEAD"]) {
        Ok(value) => value.trim().to_string(),
        Err(_) => return SourceBindingStatus::Stale,
    };
    if head == commit {
        return SourceBindingStatus::Current;
    }
    let range = format!("{commit}..HEAD");
    let changed = match git(repo_root, ["diff", "--name-only", &range]) {
        Ok(value) => value,
        Err(_) => return SourceBindingStatus::Stale,
    };
    if changed
        .lines()
        .filter(|line| !line.trim().is_empty())
        .all(is_evidence_only_path)
    {
        SourceBindingStatus::Current
    } else {
        SourceBindingStatus::Stale
    }
}

pub fn head_commit(repo_root: &Path) -> Result<String> {
    Ok(git(repo_root, ["rev-parse", "HEAD"])?.trim().to_string())
}

fn packet_head_commit(repo_root: &Path) -> Result<String> {
    Ok(
        packet_git(repo_root, ["rev-parse", "--verify", "HEAD^{commit}"])?
            .trim()
            .to_string(),
    )
}

fn path_dirty(repo_root: &Path, path: &str) -> bool {
    match git(repo_root, ["status", "--porcelain", "--", path]) {
        Ok(value) => !value.trim().is_empty(),
        Err(_) => true,
    }
}

fn is_evidence_only_path(path: &str) -> bool {
    EVIDENCE_ONLY_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
}

fn git<const N: usize>(repo_root: &Path, args: [&str; N]) -> Result<String> {
    git_with_code(repo_root, args, "source_binding_stale")
}

fn packet_git<const N: usize>(repo_root: &Path, args: [&str; N]) -> Result<String> {
    git_with_code(repo_root, args, "work_packet_source_unavailable")
}

fn git_with_code<const N: usize>(
    repo_root: &Path,
    args: [&str; N],
    error_code: &'static str,
) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .map_err(|error| PulseError::io(repo_root.join(".git"), error))?;
    if !output.status.success() {
        return Err(PulseError::validation(
            error_code,
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Capture the exact packet source snapshot for a repository.
///
/// Returns a `PacketSourceSnapshot` with the current HEAD, head_ref,
/// worktree kind, cleanliness, and operation state. Errors include:
///
/// - `work_packet_source_unavailable` if HEAD cannot be resolved;
/// - `work_packet_dirty_source_unsupported` if the worktree is dirty;
/// - `work_packet_source_operation_in_progress` if a Git operation is active.
pub fn packet_base_snapshot(repo_root: &Path, repository_id: &str) -> Result<PacketSourceSnapshot> {
    let commit = packet_head_commit(repo_root)?;
    if commit.len() != 40 || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(PulseError::validation(
            "work_packet_source_unavailable",
            "HEAD does not resolve to a full 40-character hex commit",
        ));
    }

    let head_ref = resolve_head_ref(repo_root)?;
    let worktree_root_kind = detect_worktree_kind(repo_root)?;

    // P2S1-D4: clean committed source only.
    let cleanliness = check_cleanliness(repo_root)?;
    if cleanliness == SourceCleanliness::Dirty {
        return Err(PulseError::validation(
            "work_packet_dirty_source_unsupported",
            "tracked or untracked non-ignored changes found in worktree",
        ));
    }

    // P2S1-D4: no in-progress Git operations.
    let operation_state = detect_operation_state(repo_root)?;
    if operation_state != RepositoryOperationState::Normal {
        return Err(PulseError::validation(
            "work_packet_source_operation_in_progress",
            format!("Git operation in progress: {}", operation_state.as_str()),
        ));
    }

    Ok(PacketSourceSnapshot {
        repository_id: repository_id.to_string(),
        kind: "git_commit".to_string(),
        commit,
        head_ref,
        worktree_root_kind,
        cleanliness,
        operation_state,
        currentness: "current".to_string(),
    })
}

/// Revalidate a previously captured packet source snapshot against the current
/// repository state.
///
/// Returns `Ok(())` if every field still matches. On mismatch returns a typed
/// `work_packet_source_changed` error. Unlike `packet_base_snapshot`, this
/// function does NOT return dirty/operation errors; those are mapped to the
/// unified `work_packet_source_changed` code when the expected snapshot had a
/// different value.
pub fn revalidate_packet_base(repo_root: &Path, expected: &PacketSourceSnapshot) -> Result<()> {
    let commit = packet_head_commit(repo_root).map_err(|_| {
        PulseError::validation(
            "work_packet_source_changed",
            "source HEAD changed during packet build",
        )
    })?;
    let head_ref = resolve_head_ref(repo_root)?;
    let cleanliness = check_cleanliness(repo_root)?;
    let operation_state = detect_operation_state(repo_root)?;

    let worktree_root_kind = detect_worktree_kind(repo_root).map_err(|_| {
        PulseError::validation(
            "work_packet_source_changed",
            "source worktree state changed during packet build",
        )
    })?;

    if commit != expected.commit
        || head_ref != expected.head_ref
        || worktree_root_kind != expected.worktree_root_kind
        || cleanliness != expected.cleanliness
        || operation_state != expected.operation_state
    {
        return Err(PulseError::validation(
            "work_packet_source_changed",
            "source state changed during packet build",
        ));
    }
    Ok(())
}

/// Check whether the source worktree is clean (no tracked or untracked
/// non-ignored changes outside Pulse-owned metadata).
///
/// Pulse mutates its local graph/events/evidence metadata as repository state.
/// Those files are not source inputs for work-packet base cleanliness; treating
/// them as dirty would make one successful claim block an unrelated concurrent
/// claim after the repository fence serializes their commits. Runtime/cache
/// paths are still required to be ignored separately by
/// `validate_packet_operational_paths`.
pub fn check_cleanliness(repo_root: &Path) -> Result<SourceCleanliness> {
    let output = packet_git(
        repo_root,
        ["status", "--porcelain=v1", "--untracked-files=all"],
    )?;
    let has_source_dirty = output
        .lines()
        .filter_map(status_line_path)
        .any(|path| !is_pulse_metadata_path(path));
    if has_source_dirty {
        Ok(SourceCleanliness::Dirty)
    } else {
        Ok(SourceCleanliness::Clean)
    }
}

fn status_line_path(line: &str) -> Option<&str> {
    if line.len() < 4 {
        return None;
    }
    let path = &line[3..];
    // Porcelain v1 rename/copy lines use `old -> new`; source cleanliness is
    // concerned with the destination path now present in the worktree.
    Some(path.rsplit(" -> ").next().unwrap_or(path).trim_matches('"'))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapshotComponent {
    identity: String,
    status: ComponentStatus,
    dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ComponentStatus {
    Complete,
    Unsupported(String),
    BoundedOut(String),
}

fn tracked_diff_identity(
    repo_root: &Path,
    options: &WorkspaceSnapshotOptions,
) -> Result<SnapshotComponent> {
    let mut args = vec![
        "diff",
        "--binary",
        "--full-index",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "-z",
        &options.diff_base_commit,
        "--",
    ];
    for path in &options.included_paths {
        validate_snapshot_scope_path(path)?;
        args.push(path);
    }
    let bytes = match git_bytes_bounded(repo_root, &args, options.max_tracked_diff_bytes) {
        Ok(bytes) => bytes,
        Err(SnapshotReadError::BoundedOut(bytes)) => {
            return Ok(SnapshotComponent {
                identity: hash_bytes(&bytes),
                status: ComponentStatus::BoundedOut("tracked_diff_bounded_out".to_string()),
                dirty: true,
            });
        }
        Err(SnapshotReadError::Git(message)) => {
            return Ok(SnapshotComponent {
                identity: hash_bytes(message.as_bytes()),
                status: ComponentStatus::Unsupported("tracked_diff_unsupported".to_string()),
                dirty: false,
            });
        }
        Err(SnapshotReadError::Io(error)) => {
            return Err(PulseError::io(repo_root.join(".git"), error))
        }
    };
    if contains_unsupported_diff_marker(&bytes) {
        return Ok(SnapshotComponent {
            identity: hash_bytes(&bytes),
            status: ComponentStatus::Unsupported("tracked_diff_unsupported".to_string()),
            dirty: true,
        });
    }
    Ok(SnapshotComponent {
        dirty: !bytes.is_empty(),
        identity: hash_bytes(&bytes),
        status: ComponentStatus::Complete,
    })
}

fn status_identity(
    repo_root: &Path,
    options: &WorkspaceSnapshotOptions,
) -> Result<SnapshotComponent> {
    let mut args = vec![
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
        "--",
    ];
    for path in &options.included_paths {
        validate_snapshot_scope_path(path)?;
        args.push(path);
    }
    args.extend(PULSE_RUNTIME_EXCLUDE_PATHS);
    let bytes = match git_bytes_bounded(repo_root, &args, options.max_status_bytes) {
        Ok(bytes) => bytes,
        Err(SnapshotReadError::BoundedOut(bytes)) => {
            return Ok(SnapshotComponent {
                identity: hash_bytes(&bytes),
                status: ComponentStatus::BoundedOut("status_bounded_out".to_string()),
                dirty: true,
            });
        }
        Err(SnapshotReadError::Git(message)) => {
            return Ok(SnapshotComponent {
                identity: hash_bytes(message.as_bytes()),
                status: ComponentStatus::Unsupported("status_unsupported".to_string()),
                dirty: false,
            });
        }
        Err(SnapshotReadError::Io(error)) => {
            return Err(PulseError::io(repo_root.join(".git"), error))
        }
    };
    let retained = filter_git_records(&bytes)?;
    Ok(SnapshotComponent {
        dirty: !retained.is_empty(),
        identity: hash_bytes(&retained),
        status: ComponentStatus::Complete,
    })
}

fn filter_git_records(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut retained = Vec::new();
    for entry in bytes
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let text = std::str::from_utf8(entry).map_err(|_| {
            PulseError::validation(
                "run_workspace_snapshot_unsupported",
                "git path is not valid UTF-8",
            )
        })?;
        let path = status_or_diff_path(text);
        validate_managed_relative_path(path)?;
        if !is_pulse_runtime_generated_path(path) {
            retained.extend_from_slice(entry);
            retained.push(0);
        }
    }
    Ok(retained)
}

fn untracked_manifest_identity(
    repo_root: &Path,
    options: &WorkspaceSnapshotOptions,
) -> Result<SnapshotComponent> {
    let mut args = vec!["ls-files", "--others", "-z", "--"];
    for path in &options.included_paths {
        validate_snapshot_scope_path(path)?;
        args.push(path);
    }
    args.extend(PULSE_RUNTIME_EXCLUDE_PATHS);
    let output = match git_nul_records_bounded(
        repo_root,
        &args,
        options.max_status_bytes,
        options.max_untracked_entries,
        Some(is_pulse_runtime_generated_path),
    ) {
        Ok(records) => records,
        Err(SnapshotReadError::BoundedOut(bytes)) => {
            return Ok(SnapshotComponent {
                dirty: true,
                identity: hash_bytes(&bytes),
                status: ComponentStatus::BoundedOut("untracked_entries_bounded_out".to_string()),
            });
        }
        Err(SnapshotReadError::Git(message)) => {
            return Ok(SnapshotComponent {
                dirty: false,
                identity: hash_bytes(message.as_bytes()),
                status: ComponentStatus::Unsupported("untracked_manifest_unsupported".to_string()),
            });
        }
        Err(SnapshotReadError::Io(error)) => {
            return Err(PulseError::io(repo_root.join(".git"), error))
        }
    };
    let mut special_scan_budget = options.max_untracked_entries;
    if let Some(status) = scoped_special_status(repo_root, options, &mut special_scan_budget)? {
        return Ok(SnapshotComponent {
            dirty: true,
            identity: hash_bytes(b"special_file_scan"),
            status,
        });
    }
    let mut entries = Vec::new();
    let mut total = 0_u64;
    let mut count = 0_usize;
    let mut status = ComponentStatus::Complete;
    for raw in output
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let relative = std::str::from_utf8(raw).map_err(|_| {
            PulseError::validation(
                "run_workspace_snapshot_unsupported",
                "untracked path is not valid UTF-8",
            )
        })?;
        validate_managed_relative_path(relative)?;
        if is_pulse_runtime_generated_path(relative) {
            continue;
        }
        count += 1;
        if count > options.max_untracked_entries {
            status = ComponentStatus::BoundedOut("untracked_entries_bounded_out".to_string());
            break;
        }
        let path = repo_root.join(relative);
        let metadata = fs::symlink_metadata(&path).map_err(|error| PulseError::io(&path, error))?;
        let file_type = metadata.file_type();
        if file_type.is_dir() {
            if path.join(".git").exists() {
                return Ok(unsupported_manifest(
                    relative,
                    "nested_repository_unsupported",
                ));
            }
            let mut directory_budget = options.max_untracked_entries;
            if let Some(status) =
                scoped_directory_special_status(repo_root, &path, &[], &mut directory_budget)?
            {
                return Ok(SnapshotComponent {
                    dirty: true,
                    identity: hash_bytes(relative.as_bytes()),
                    status,
                });
            }
            continue;
        }
        #[cfg(unix)]
        let executable = {
            use std::os::unix::fs::{FileTypeExt, PermissionsExt};
            if file_type.is_socket()
                || file_type.is_fifo()
                || file_type.is_block_device()
                || file_type.is_char_device()
            {
                return Ok(unsupported_manifest(relative, "special_file_unsupported"));
            }
            metadata.permissions().mode() & 0o111 != 0
        };
        #[cfg(not(unix))]
        let executable = false;
        let (kind, len, digest) = if file_type.is_file() {
            if metadata.len() > options.max_untracked_file_bytes
                || total.saturating_add(metadata.len()) > options.max_untracked_total_bytes
            {
                status = ComponentStatus::BoundedOut("untracked_manifest_bounded_out".to_string());
                ("regular_bounded_out", metadata.len(), hash_bytes(&[]))
            } else {
                let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
                total += bytes.len() as u64;
                ("regular", metadata.len(), hash_bytes(&bytes))
            }
        } else if file_type.is_symlink() {
            let target = fs::read_link(&path).map_err(|error| PulseError::io(&path, error))?;
            let target_bytes = os_str_bytes(target.as_os_str())?;
            (
                "symlink",
                target_bytes.len() as u64,
                hash_bytes(target_bytes),
            )
        } else {
            return Ok(unsupported_manifest(relative, "special_file_unsupported"));
        };
        entries.push((
            relative.to_string(),
            kind.to_string(),
            executable,
            len,
            digest,
        ));
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(SnapshotComponent {
        dirty: !entries.is_empty(),
        identity: hash_serializable(&entries)?,
        status,
    })
}

fn unsupported_manifest(relative: &str, reason: &str) -> SnapshotComponent {
    SnapshotComponent {
        dirty: true,
        identity: hash_bytes(relative.as_bytes()),
        status: ComponentStatus::Unsupported(reason.to_string()),
    }
}

fn validate_snapshot_base_relationship(
    repo_root: &Path,
    options: &WorkspaceSnapshotOptions,
    head: &str,
) -> Result<Option<String>> {
    if resolve_full_commit(repo_root, &options.base_commit).is_err() {
        return Ok(Some("base_commit_unavailable".to_string()));
    }
    let merge_base = Command::new("git")
        .args(["merge-base", &options.base_commit, head])
        .current_dir(repo_root)
        .output()
        .map_err(|error| PulseError::io(repo_root.join(".git"), error))?;
    if !merge_base.status.success() {
        return Ok(Some("base_commit_not_related_to_head".to_string()));
    }
    let actual = String::from_utf8_lossy(&merge_base.stdout)
        .trim()
        .to_string();
    if actual != options.base_commit {
        return Ok(Some("base_commit_not_head_ancestor".to_string()));
    }
    let is_ancestor = Command::new("git")
        .args(["merge-base", "--is-ancestor", &options.base_commit, head])
        .current_dir(repo_root)
        .status()
        .map_err(|error| PulseError::io(repo_root.join(".git"), error))?;
    if is_ancestor.success() {
        Ok(None)
    } else {
        Ok(Some("base_commit_not_head_ancestor".to_string()))
    }
}

fn scoped_special_status(
    repo_root: &Path,
    options: &WorkspaceSnapshotOptions,
    budget: &mut usize,
) -> Result<Option<ComponentStatus>> {
    if options.included_paths.is_empty() {
        return scoped_directory_special_status(repo_root, repo_root, &[], budget);
    }
    for relative in &options.included_paths {
        validate_snapshot_scope_path(relative)?;
        if is_pulse_runtime_generated_path(relative) {
            continue;
        }
        let scoped = repo_root.join(relative);
        let metadata = match fs::symlink_metadata(&scoped) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(PulseError::io(&scoped, error)),
        };
        let file_type = metadata.file_type();
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileTypeExt;
            if file_type.is_socket()
                || file_type.is_fifo()
                || file_type.is_block_device()
                || file_type.is_char_device()
            {
                return Ok(Some(ComponentStatus::Unsupported(
                    "special_file_unsupported".to_string(),
                )));
            }
        }
        if file_type.is_dir() {
            if scoped.join(".git").exists() {
                return Ok(Some(ComponentStatus::Unsupported(
                    "nested_repository_unsupported".to_string(),
                )));
            }
            if let Some(status) = scoped_directory_special_status(repo_root, &scoped, &[], budget)?
            {
                return Ok(Some(status));
            }
        }
    }
    Ok(None)
}

fn scoped_directory_special_status(
    repo_root: &Path,
    path: &Path,
    excluded_prefixes: &[String],
    budget: &mut usize,
) -> Result<Option<ComponentStatus>> {
    if let Some(relative) = repo_relative_utf8(repo_root, path)? {
        validate_managed_relative_path(&relative)?;
        if is_pulse_runtime_generated_path(&relative) {
            return Ok(None);
        }
    }
    for entry in fs::read_dir(path).map_err(|error| PulseError::io(path, error))? {
        let entry = entry.map_err(|error| PulseError::io(path, error))?;
        if entry.file_name() == ".git" {
            continue;
        }
        let entry_path = entry.path();
        let relative = repo_relative_utf8(repo_root, &entry_path)?;
        if let Some(relative) = relative.as_deref() {
            validate_managed_relative_path(relative)?;
            if is_pulse_runtime_generated_path(relative)
                || excluded_prefixes.iter().any(|prefix| {
                    relative == prefix
                        || relative
                            .strip_prefix(prefix)
                            .is_some_and(|suffix| suffix.starts_with('/'))
                })
            {
                continue;
            }
        }
        let metadata = fs::symlink_metadata(&entry_path)
            .map_err(|error| PulseError::io(&entry_path, error))?;
        let file_type = metadata.file_type();
        if file_type.is_dir() {
            if entry_path.join(".git").exists() {
                return Ok(Some(ComponentStatus::Unsupported(
                    "nested_repository_unsupported".to_string(),
                )));
            }
            if let Some(status) =
                scoped_directory_special_status(repo_root, &entry_path, excluded_prefixes, budget)?
            {
                return Ok(Some(status));
            }
            continue;
        }
        if *budget == 0 {
            return Ok(Some(ComponentStatus::BoundedOut(
                "untracked_entries_bounded_out".to_string(),
            )));
        }
        *budget -= 1;
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileTypeExt;
            if file_type.is_socket()
                || file_type.is_fifo()
                || file_type.is_block_device()
                || file_type.is_char_device()
            {
                return Ok(Some(ComponentStatus::Unsupported(
                    "special_file_unsupported".to_string(),
                )));
            }
        }
    }
    Ok(None)
}

fn repo_relative_utf8(repo_root: &Path, path: &Path) -> Result<Option<String>> {
    let relative = match path.strip_prefix(repo_root) {
        Ok(relative) => relative,
        Err(_) => return Ok(None),
    };
    if relative.as_os_str().is_empty() {
        return Ok(None);
    }
    relative
        .to_str()
        .map(|path| Some(path.replace(std::path::MAIN_SEPARATOR, "/")))
        .ok_or_else(|| {
            PulseError::validation(
                "run_workspace_snapshot_unsupported",
                "workspace path is not valid UTF-8",
            )
        })
}

fn is_pulse_runtime_generated_path(path: &str) -> bool {
    path == ".pulse/runtime"
        || path.starts_with(".pulse/runtime/")
        || path == ".pulse/cache"
        || path.starts_with(".pulse/cache/")
}

fn has_ignored_source_paths(repo_root: &Path, options: &WorkspaceSnapshotOptions) -> Result<bool> {
    let mut args = vec![
        "ls-files",
        "--others",
        "--ignored",
        "--exclude-standard",
        "-z",
        "--",
    ];
    for path in &options.included_paths {
        validate_snapshot_scope_path(path)?;
        args.push(path);
    }
    args.extend(PULSE_RUNTIME_EXCLUDE_PATHS);
    let output = match git_nul_records_bounded(
        repo_root,
        &args,
        options.max_status_bytes,
        options.max_untracked_entries,
        Some(is_pulse_runtime_generated_path),
    ) {
        Ok(records) => records,
        Err(SnapshotReadError::BoundedOut(_)) => return Ok(true),
        Err(SnapshotReadError::Git(_)) => return Ok(true),
        Err(SnapshotReadError::Io(error)) => {
            return Err(PulseError::io(repo_root.join(".git"), error))
        }
    };
    for raw in output
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let relative = std::str::from_utf8(raw).map_err(|_| {
            PulseError::validation(
                "run_workspace_snapshot_unsupported",
                "ignored path is not valid UTF-8",
            )
        })?;
        validate_managed_relative_path(relative)?;
        if !is_pulse_runtime_generated_path(relative) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn status_or_diff_path(text: &str) -> &str {
    let path = if text.len() >= 3 { &text[3..] } else { text };
    path.rsplit(" -> ").next().unwrap_or(path).trim_matches('"')
}

fn validate_snapshot_scope_path(path: &str) -> Result<()> {
    validate_managed_relative_path(path)
}

fn validate_managed_relative_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.starts_with('/')
        || path.starts_with("../")
        || path == ".."
        || path.contains("/../")
        || path.contains("//")
        || path.contains('\\')
        || path.as_bytes().contains(&0)
    {
        return Err(PulseError::validation(
            "run_workspace_snapshot_unsupported",
            format!("unsafe repository-relative path: {path}"),
        ));
    }
    Ok(())
}

fn contains_unsupported_diff_marker(bytes: &[u8]) -> bool {
    bytes
        .windows(b"Subproject commit".len())
        .any(|window| window == b"Subproject commit")
}

fn extend_reason(reason_codes: &mut Vec<String>, prefix: &str, status: &ComponentStatus) {
    match status {
        ComponentStatus::Complete => {}
        ComponentStatus::Unsupported(reason) | ComponentStatus::BoundedOut(reason) => {
            if reason.starts_with(prefix) || prefix.is_empty() {
                reason_codes.push(reason.clone());
            } else {
                reason_codes.push(format!("{prefix}_{reason}"));
            }
        }
    }
}

fn snapshot_status_from_reasons(reason_codes: &[String]) -> WorkspaceSnapshotStatusV1 {
    if reason_codes.is_empty() {
        WorkspaceSnapshotStatusV1::Complete
    } else if reason_codes.iter().any(|code| code.contains("bounded_out")) {
        WorkspaceSnapshotStatusV1::BoundedOut
    } else {
        WorkspaceSnapshotStatusV1::Unsupported
    }
}

struct WorkspaceSnapshotBuildParts {
    head: String,
    operation_state: WorkspaceOperationStateV1,
    cleanliness: WorkspaceCleanlinessV1,
    tracked_diff_identity: String,
    untracked_manifest_identity: String,
    status_identity: String,
    snapshot_status: WorkspaceSnapshotStatusV1,
}

fn build_workspace_snapshot(
    options: &WorkspaceSnapshotOptions,
    parts: WorkspaceSnapshotBuildParts,
) -> Result<WorkspaceSnapshotV1> {
    let workspace_mode = match options.workspace_mode.as_str() {
        "in_place" => WorkspaceModeV1::InPlace,
        "isolated_worktree" => WorkspaceModeV1::IsolatedWorktree,
        _ => {
            return Err(PulseError::validation(
                "run_workspace_snapshot_unsupported",
                "workspace mode must be in_place or isolated_worktree",
            ));
        }
    };
    let mut snapshot = WorkspaceSnapshotV1 {
        schema_version: 1,
        repository_id: options.repository_id.clone(),
        workspace_id: options.workspace_id.clone(),
        workspace_mode,
        base_commit: options.base_commit.clone(),
        head_commit: parts.head,
        diff_base_commit: options.diff_base_commit.clone(),
        operation_state: parts.operation_state,
        cleanliness: parts.cleanliness,
        tracked_diff_identity: parts.tracked_diff_identity,
        untracked_manifest_identity: parts.untracked_manifest_identity,
        status_identity: parts.status_identity,
        snapshot_status: parts.snapshot_status,
        captured_at: options.captured_at.clone(),
        snapshot_identity: String::new(),
    };
    snapshot.snapshot_identity = snapshot.compute_identity()?;
    Ok(snapshot)
}

fn operation_state_to_v1(state: &RepositoryOperationState) -> WorkspaceOperationStateV1 {
    match state {
        RepositoryOperationState::Normal => WorkspaceOperationStateV1::None,
        RepositoryOperationState::MergeInProgress => WorkspaceOperationStateV1::Merge,
        RepositoryOperationState::RebaseInProgress => WorkspaceOperationStateV1::Rebase,
        RepositoryOperationState::CherryPickInProgress => WorkspaceOperationStateV1::CherryPick,
        RepositoryOperationState::RevertInProgress => WorkspaceOperationStateV1::Revert,
        RepositoryOperationState::BisectInProgress => WorkspaceOperationStateV1::Bisect,
    }
}

enum SnapshotReadError {
    BoundedOut(Vec<u8>),
    Git(String),
    Io(std::io::Error),
}

fn hardened_git_command(repo_root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command
        .args(["-c", "diff.external=", "-c", "core.pager="])
        .args(args)
        .env("LC_ALL", "C")
        .env_remove("GIT_EXTERNAL_DIFF")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_PAGER", "cat")
        .current_dir(repo_root);
    command
}

fn git_bytes_bounded(
    repo_root: &Path,
    args: &[&str],
    max_bytes: usize,
) -> std::result::Result<Vec<u8>, SnapshotReadError> {
    let output = run_git_bounded(repo_root, args, max_bytes, None)?;
    if !output.complete {
        return Err(SnapshotReadError::BoundedOut(output.stdout));
    }
    if !output.status_success {
        return Err(SnapshotReadError::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(output.stdout)
}

fn git_nul_records_bounded(
    repo_root: &Path,
    args: &[&str],
    max_stdout_bytes: usize,
    max_entries: usize,
    exclude_record: Option<fn(&str) -> bool>,
) -> std::result::Result<Vec<u8>, SnapshotReadError> {
    let mut child = hardened_git_command(repo_root, args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(SnapshotReadError::Io)?;
    let stderr = child.stderr.take().expect("stderr piped");
    let stderr_handle = thread::spawn(move || read_limited(stderr, SNAPSHOT_GIT_STDERR_LIMIT));
    let stdout = child.stdout.take().expect("stdout piped");
    let stdout_read = match read_filtered_nul_stdout_bounded(
        stdout,
        max_stdout_bytes,
        max_entries,
        exclude_record,
    ) {
        Ok(read) => read,
        Err(error) => {
            terminate_child(&mut child);
            let _ = child.wait();
            let _ = stderr_handle.join();
            return Err(SnapshotReadError::Io(error));
        }
    };
    if !stdout_read.complete {
        terminate_child(&mut child);
    }
    let status = child.wait().map_err(SnapshotReadError::Io)?;
    let stderr = stderr_handle
        .join()
        .map_err(|_| SnapshotReadError::Io(std::io::Error::other("stderr reader panicked")))?
        .map_err(SnapshotReadError::Io)?;
    if !stdout_read.complete {
        return Err(SnapshotReadError::BoundedOut(stdout_read.bytes));
    }
    if stdout_read.partial_record {
        return Err(SnapshotReadError::Git(
            "partial NUL-delimited git record".to_string(),
        ));
    }
    if !status.success() {
        return Err(SnapshotReadError::Git(
            String::from_utf8_lossy(&stderr).trim().to_string(),
        ));
    }
    Ok(stdout_read.bytes)
}

struct BoundedGitOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    status_success: bool,
    complete: bool,
}

const SNAPSHOT_GIT_STDERR_LIMIT: usize = 64 * 1024;
const SNAPSHOT_GIT_NUL_RECORD_LIMIT: usize = 64 * 1024;

fn run_git_bounded(
    repo_root: &Path,
    args: &[&str],
    max_stdout_bytes: usize,
    max_nul_records: Option<usize>,
) -> std::result::Result<BoundedGitOutput, SnapshotReadError> {
    let mut child = hardened_git_command(repo_root, args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(SnapshotReadError::Io)?;
    let stderr = child.stderr.take().expect("stderr piped");
    let stderr_handle = thread::spawn(move || read_limited(stderr, SNAPSHOT_GIT_STDERR_LIMIT));
    let stdout = child.stdout.take().expect("stdout piped");
    let stdout_result = read_stdout_bounded(stdout, max_stdout_bytes, max_nul_records);
    let stdout_read = match stdout_result {
        Ok(read) => read,
        Err(error) => {
            terminate_child(&mut child);
            let _ = child.wait();
            let _ = stderr_handle.join();
            return Err(SnapshotReadError::Io(error));
        }
    };
    if !stdout_read.complete {
        terminate_child(&mut child);
    }
    let status = child.wait().map_err(SnapshotReadError::Io)?;
    let stderr = stderr_handle
        .join()
        .map_err(|_| SnapshotReadError::Io(std::io::Error::other("stderr reader panicked")))?
        .map_err(SnapshotReadError::Io)?;
    Ok(BoundedGitOutput {
        stdout: stdout_read.bytes,
        stderr,
        status_success: status.success(),
        complete: stdout_read.complete,
    })
}

struct BoundedStdoutRead {
    bytes: Vec<u8>,
    complete: bool,
}

struct BoundedNulRead {
    bytes: Vec<u8>,
    complete: bool,
    partial_record: bool,
}

fn read_filtered_nul_stdout_bounded<R: Read>(
    mut reader: R,
    max_bytes: usize,
    max_records: usize,
    exclude_record: Option<fn(&str) -> bool>,
) -> std::io::Result<BoundedNulRead> {
    let mut retained = Vec::new();
    let mut record = Vec::new();
    let mut records = 0_usize;
    let raw_record_limit = if exclude_record.is_some() {
        SNAPSHOT_GIT_NUL_RECORD_LIMIT
    } else {
        max_bytes
    };
    let mut buf = [0_u8; 8192];
    loop {
        let read = reader.read(&mut buf)?;
        if read == 0 {
            return Ok(BoundedNulRead {
                bytes: retained,
                complete: true,
                partial_record: !record.is_empty(),
            });
        }
        for byte in &buf[..read] {
            if *byte == 0 {
                if !record.is_empty() {
                    let excluded = match exclude_record {
                        Some(exclude) => match std::str::from_utf8(&record) {
                            Ok(path) => exclude(path),
                            Err(_) => false,
                        },
                        None => false,
                    };
                    if excluded {
                        record.clear();
                        continue;
                    }
                    records += 1;
                    if records > max_records {
                        return Ok(BoundedNulRead {
                            bytes: retained,
                            complete: false,
                            partial_record: false,
                        });
                    }
                    if retained
                        .len()
                        .saturating_add(record.len())
                        .saturating_add(1)
                        > max_bytes
                    {
                        return Ok(BoundedNulRead {
                            bytes: retained,
                            complete: false,
                            partial_record: false,
                        });
                    }
                    retained.extend_from_slice(&record);
                    retained.push(0);
                    record.clear();
                }
            } else {
                record.push(*byte);
                if record.len() > raw_record_limit {
                    return Ok(BoundedNulRead {
                        bytes: retained,
                        complete: false,
                        partial_record: true,
                    });
                }
            }
        }
    }
}

fn read_stdout_bounded<R: Read>(
    mut reader: R,
    max_bytes: usize,
    max_nul_records: Option<usize>,
) -> std::io::Result<BoundedStdoutRead> {
    let mut retained = Vec::new();
    let mut buf = [0_u8; 8192];
    let mut records = 0_usize;
    loop {
        let read = reader.read(&mut buf)?;
        if read == 0 {
            return Ok(BoundedStdoutRead {
                bytes: retained,
                complete: true,
            });
        }
        let mut keep = read;
        if retained.len().saturating_add(read) > max_bytes {
            keep = max_bytes.saturating_sub(retained.len());
        }
        if let Some(max_records) = max_nul_records {
            for (index, byte) in buf[..keep].iter().enumerate() {
                if *byte == 0 {
                    records += 1;
                    if records > max_records {
                        keep = index + 1;
                        break;
                    }
                }
            }
        }
        retained.extend_from_slice(&buf[..keep]);
        if keep < read
            || retained.len() >= max_bytes
            || max_nul_records.is_some_and(|max| records > max)
        {
            return Ok(BoundedStdoutRead {
                bytes: retained,
                complete: false,
            });
        }
    }
}

fn read_limited<R: Read>(mut reader: R, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut retained = Vec::new();
    let mut buf = [0_u8; 8192];
    loop {
        let read = reader.read(&mut buf)?;
        if read == 0 {
            return Ok(retained);
        }
        let keep = limit.saturating_sub(retained.len()).min(read);
        retained.extend_from_slice(&buf[..keep]);
    }
}

fn terminate_child(child: &mut Child) {
    match child.try_wait() {
        Ok(Some(_)) => {}
        Ok(None) => {
            let _ = child.kill();
        }
        Err(_) => {
            let _ = child.kill();
        }
    }
}

#[cfg(unix)]
fn os_str_bytes(value: &std::ffi::OsStr) -> Result<&[u8]> {
    use std::os::unix::ffi::OsStrExt;
    Ok(value.as_bytes())
}

#[cfg(not(unix))]
fn os_str_bytes(value: &std::ffi::OsStr) -> Result<&[u8]> {
    value.to_str().map(|s| s.as_bytes()).ok_or_else(|| {
        PulseError::validation(
            "run_workspace_snapshot_unsupported",
            "path is not valid UTF-8",
        )
    })
}

fn is_pulse_metadata_path(path: &str) -> bool {
    path.starts_with(".pulse/workgraph/")
        || path.starts_with(".pulse/events/")
        || path.starts_with(".pulse/evidence/")
        || path.starts_with(".pulse/docs/")
        || path.starts_with(".pulse/knowledge/")
}

/// Resolve the HEAD symbolic ref, if any.
///
/// Returns `Ok(None)` when HEAD is detached, `Ok(Some(ref))` when on a branch.
fn resolve_head_ref(repo_root: &Path) -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["symbolic-ref", "--quiet", "HEAD"])
        .current_dir(repo_root)
        .output()
        .map_err(|error| PulseError::io(repo_root.join(".git"), error))?;
    if output.status.success() {
        let refname = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if refname.is_empty() {
            return Err(PulseError::validation(
                "work_packet_source_unavailable",
                "HEAD symbolic ref resolved to an empty ref name",
            ));
        }
        return Ok(Some(refname));
    }
    match output.status.code() {
        Some(1) => Ok(None), // detached HEAD
        _ => Err(PulseError::validation(
            "work_packet_source_unavailable",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        )),
    }
}

/// Detect the worktree root kind using `git rev-parse --git-dir`.
fn detect_worktree_kind(repo_root: &Path) -> Result<WorktreeRootKind> {
    let common_dir = packet_git(
        repo_root,
        ["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let git_dir = packet_git(
        repo_root,
        ["rev-parse", "--path-format=absolute", "--git-dir"],
    )?;
    let common_dir = common_dir.trim();
    let git_dir = git_dir.trim();
    if common_dir.is_empty() || git_dir.is_empty() {
        return Err(PulseError::validation(
            "work_packet_source_unavailable",
            "Git directory could not be resolved",
        ));
    }
    if common_dir == git_dir {
        Ok(WorktreeRootKind::PrimaryOrExistingWorktree)
    } else {
        Ok(WorktreeRootKind::LinkedWorktree)
    }
}

/// Detect in-progress Git operations using `git rev-parse --git-path`.
///
/// Uses `--git-path` to work correctly with linked worktrees, where
/// `.git/MERGE_HEAD` may not exist but the actual merge head is at a
/// worktree-specific path.
pub fn detect_operation_state(repo_root: &Path) -> Result<RepositoryOperationState> {
    // MERGE_HEAD
    if git_path_exists(repo_root, "MERGE_HEAD")? {
        return Ok(RepositoryOperationState::MergeInProgress);
    }
    // rebase-merge or rebase-apply directory
    if git_path_exists(repo_root, "rebase-merge")? || git_path_exists(repo_root, "rebase-apply")? {
        return Ok(RepositoryOperationState::RebaseInProgress);
    }
    // CHERRY_PICK_HEAD
    if git_path_exists(repo_root, "CHERRY_PICK_HEAD")? {
        return Ok(RepositoryOperationState::CherryPickInProgress);
    }
    // REVERT_HEAD
    if git_path_exists(repo_root, "REVERT_HEAD")? {
        return Ok(RepositoryOperationState::RevertInProgress);
    }
    // BISECT_LOG
    if git_path_exists(repo_root, "BISECT_LOG")? {
        return Ok(RepositoryOperationState::BisectInProgress);
    }
    Ok(RepositoryOperationState::Normal)
}

/// Check whether a path exists in the Git directory, using `git rev-parse
/// --git-path` for correct linked-worktree resolution.
fn git_path_exists(repo_root: &Path, relative: &str) -> Result<bool> {
    let output = packet_git(
        repo_root,
        [
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            relative,
        ],
    )?;
    let git_path = output.trim().to_string();
    if git_path.is_empty() {
        return Ok(false);
    }
    Ok(Path::new(&git_path).exists())
}

impl From<PacketSourceSnapshot> for crate::work_packet::PacketSource {
    fn from(snapshot: PacketSourceSnapshot) -> Self {
        Self {
            repository_id: snapshot.repository_id,
            kind: snapshot.kind,
            commit: snapshot.commit,
            head_ref: snapshot.head_ref,
            worktree_root_kind: snapshot.worktree_root_kind.as_str().to_string(),
            cleanliness: snapshot.cleanliness.as_str().to_string(),
            operation_state: snapshot.operation_state.as_str().to_string(),
            currentness: snapshot.currentness,
        }
    }
}

/// Validate repository identity consistency across manifests.
///
/// Rules per P2S1-I2:
/// - Evidence manifest must exist (repository identity owner);
/// - If docs manifest/registry exists, its `repository_id` must match;
/// - If knowledge manifest exists, its `repository_id` must match;
/// - Missing knowledge manifest is acceptable (not installed);
/// - Missing docs manifest/registry always blocks packet per P2S1-D14.
///
/// Returns the evidence manifest on success so callers can reuse it.
pub fn check_repository_identity(
    repo_root: &Path,
) -> Result<crate::evidence::manifest::EvidenceManifest> {
    let evidence = map_manifest_error(crate::evidence::manifest::load_existing(repo_root))?
        .ok_or_else(|| {
            PulseError::validation(
                "work_packet_repository_identity_missing",
                "existing repository identity (evidence manifest) not found",
            )
        })?;

    let repo_id = &evidence.repository_id;

    let docs_registry = map_manifest_error(crate::docs::manifest::load_existing(repo_root))?
        .ok_or_else(|| {
            PulseError::validation(
                "work_packet_docs_registry_missing",
                "docs registry not found; packet requires existing docs manifest/registry",
            )
        })?;
    if &docs_registry.repository_id != repo_id {
        return Err(PulseError::validation(
            "work_packet_repository_identity_mismatch",
            format!(
                "docs registry repository_id '{}' does not match evidence manifest '{}'",
                docs_registry.repository_id, repo_id,
            ),
        ));
    }

    if let Some(knowledge) =
        map_manifest_error(crate::knowledge::manifest::load_existing(repo_root))?
    {
        if &knowledge.repository_id != repo_id {
            return Err(PulseError::validation(
                "work_packet_repository_identity_mismatch",
                format!(
                    "knowledge manifest repository_id '{}' does not match evidence manifest '{}'",
                    knowledge.repository_id, repo_id,
                ),
            ));
        }
    }

    Ok(evidence)
}

// ---------------------------------------------------------------------------
// Workspace-specific source validation (P2S2-I5)
// -------------------------------------------------------------------------

/// Result of a workspace base validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceBaseValidation {
    pub commit: String,
    pub head_ref: Option<String>,
    pub worktree_root_kind: WorktreeRootKind,
    pub cleanliness: SourceCleanliness,
    pub operation_state: RepositoryOperationState,
}

/// Validate a workspace path as an exact-base, clean, non-operating Git
/// repository.
///
/// This is a lighter version of `packet_base_snapshot` that does NOT require
/// evidence/docs/knowledge manifest enrollment. It verifies:
/// - HEAD resolves to the expected commit (full 40-char hex)
/// - Worktree is clean (no tracked or untracked non-ignored changes)
/// - No Git operation is in progress
///
/// Returns `WorkspaceBaseValidation` on success, or one of:
/// - `work_packet_source_unavailable` if HEAD cannot be resolved
/// - `work_packet_dirty_source_unsupported` if the worktree is dirty
/// - `work_packet_source_operation_in_progress` if a Git operation is active
/// - `work_packet_source_changed` if HEAD does not match `expected_commit`
pub fn check_workspace_base(
    repo_root: &Path,
    expected_commit: &str,
) -> Result<WorkspaceBaseValidation> {
    let commit = packet_head_commit(repo_root)?;
    if commit.len() != 40 || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(PulseError::validation(
            "work_packet_source_unavailable",
            "HEAD does not resolve to a full 40-character hex commit",
        ));
    }

    if commit != expected_commit {
        return Err(PulseError::validation(
            "work_packet_source_changed",
            format!(
                "HEAD commit {} does not match expected commit {}",
                commit, expected_commit
            ),
        ));
    }

    let head_ref = resolve_head_ref(repo_root)?;
    let worktree_root_kind = detect_worktree_kind(repo_root)?;

    let cleanliness = check_cleanliness(repo_root)?;
    if cleanliness == SourceCleanliness::Dirty {
        return Err(PulseError::validation(
            "work_packet_dirty_source_unsupported",
            "workspace worktree has tracked or untracked non-ignored changes",
        ));
    }

    let operation_state = detect_operation_state(repo_root)?;
    if operation_state != RepositoryOperationState::Normal {
        return Err(PulseError::validation(
            "work_packet_source_operation_in_progress",
            format!(
                "Git operation in progress in workspace: {}",
                operation_state.as_str()
            ),
        ));
    }

    Ok(WorkspaceBaseValidation {
        commit,
        head_ref,
        worktree_root_kind,
        cleanliness,
        operation_state,
    })
}

fn map_manifest_error<T>(result: Result<T>) -> Result<T> {
    result.map_err(|error| match error.code() {
        "work_packet_repository_identity_missing"
        | "work_packet_docs_registry_missing"
        | "work_packet_repository_identity_mismatch" => error,
        code => PulseError::validation(
            "work_packet_manifest_invalid",
            format!("cause_code={code}: {error}"),
        ),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    #[test]
    fn nul_record_reader_detects_partial_final_record() {
        let read =
            read_filtered_nul_stdout_bounded(&b"complete\0partial"[..], 1024, 10, None).unwrap();
        assert!(read.complete);
        assert!(read.partial_record);
        assert_eq!(read.bytes, b"complete\0");
    }

    #[test]
    fn nul_record_reader_excludes_before_retained_caps_and_entry_count() {
        fn exclude_runtime(path: &str) -> bool {
            path.starts_with(".pulse/runtime/")
        }

        let input = b".pulse/runtime/very-long-generated-record\0.pulse/runtime/another-generated-record\0src/main.rs\0";
        let read =
            read_filtered_nul_stdout_bounded(&input[..], 16, 1, Some(exclude_runtime)).unwrap();
        assert!(read.complete);
        assert!(!read.partial_record);
        assert_eq!(read.bytes, b"src/main.rs\0");
    }

    #[test]
    fn nul_record_reader_still_bounds_giant_excluded_record() {
        fn exclude_runtime(path: &str) -> bool {
            path.starts_with(".pulse/runtime/")
        }

        let input = format!(
            ".pulse/runtime/{}\0",
            "x".repeat(SNAPSHOT_GIT_NUL_RECORD_LIMIT)
        );
        let read = read_filtered_nul_stdout_bounded(input.as_bytes(), 8, 1, Some(exclude_runtime))
            .unwrap();
        assert!(!read.complete);
        assert!(read.partial_record);
        assert!(read.bytes.is_empty());
    }

    #[test]
    fn nul_record_reader_caps_before_retaining_long_record() {
        let read =
            read_filtered_nul_stdout_bounded(&b"short\0very-very-long"[..], 8, 10, None).unwrap();
        assert!(!read.complete);
        assert!(read.partial_record);
        assert_eq!(read.bytes, b"short\0");
    }

    #[test]
    fn nul_record_reader_enforces_entry_count_during_read() {
        let read = read_filtered_nul_stdout_bounded(&b"a\0b\0c\0"[..], 1024, 2, None).unwrap();
        assert!(!read.complete);
        assert!(!read.partial_record);
        assert_eq!(read.bytes, b"a\0b\0");
    }

    fn run_git(path: &Path, args: &[&str]) -> std::process::Output {
        Command::new("git")
            .args(args)
            .current_dir(path)
            .output()
            .expect("git command")
    }

    fn init_repo(path: &Path) {
        let git = |args: &[&str]| {
            let output = Command::new("git")
                .args(args)
                .current_dir(path)
                .output()
                .expect("git command");
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
            output
        };
        git(&["init", "-q"]);
        git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@test",
            "commit",
            "--allow-empty",
            "-m",
            "initial",
        ]);
    }

    fn commit_file(path: &Path, rel: &str, content: &[u8]) {
        let parent = path.join(rel).parent().unwrap().to_path_buf();
        fs::create_dir_all(parent).unwrap();
        fs::write(path.join(rel), content).unwrap();
        let output = Command::new("git")
            .args(["add", rel])
            .current_dir(path)
            .output()
            .expect("git add");
        assert!(output.status.success(), "git add failed");
        let commit = Command::new("git")
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@test",
                "commit",
                "-m",
                &format!("add {}", rel),
            ])
            .current_dir(path)
            .output()
            .expect("git commit");
        assert!(
            commit.status.success(),
            "git commit failed: {}",
            String::from_utf8_lossy(&commit.stderr)
        );
    }

    #[test]
    fn clean_full_head_succeeds() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_file(tmp.path(), "README.md", b"hello");
        let snapshot = packet_base_snapshot(tmp.path(), "repo_test").unwrap();
        assert_eq!(snapshot.kind, "git_commit");
        assert_eq!(snapshot.commit.len(), 40);
        assert!(snapshot.head_ref.is_some());
        assert_eq!(snapshot.cleanliness, SourceCleanliness::Clean);
        assert_eq!(snapshot.operation_state, RepositoryOperationState::Normal);
        assert_eq!(
            snapshot.worktree_root_kind,
            WorktreeRootKind::PrimaryOrExistingWorktree
        );
    }

    #[test]
    fn dirty_tracked_file_rejects() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_file(tmp.path(), "README.md", b"original");
        fs::write(tmp.path().join("README.md"), b"modified").unwrap();
        let result = packet_base_snapshot(tmp.path(), "repo_test");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code(),
            "work_packet_dirty_source_unsupported"
        );
    }

    #[test]
    fn untracked_non_ignored_file_rejects() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_file(tmp.path(), "README.md", b"hello");
        fs::write(tmp.path().join("untracked.txt"), b"untracked").unwrap();
        let result = packet_base_snapshot(tmp.path(), "repo_test");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code(),
            "work_packet_dirty_source_unsupported"
        );
    }

    #[test]
    fn ignored_file_does_not_dirty() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_file(tmp.path(), ".gitignore", b"*.log\n");
        fs::write(tmp.path().join("debug.log"), b"log content").unwrap();
        let snapshot = packet_base_snapshot(tmp.path(), "repo_test").unwrap();
        assert_eq!(snapshot.cleanliness, SourceCleanliness::Clean);
    }

    #[test]
    fn detached_head_succeeds_with_null_head_ref() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_file(tmp.path(), "README.md", b"hello");
        let head = head_commit(tmp.path()).unwrap();
        Command::new("git")
            .args(["checkout", "--detach", &head])
            .current_dir(tmp.path())
            .output()
            .expect("git checkout --detach");
        let snapshot = packet_base_snapshot(tmp.path(), "repo_test").unwrap();
        assert_eq!(snapshot.head_ref, None);
        assert_eq!(snapshot.cleanliness, SourceCleanliness::Clean);
    }

    #[test]
    fn merge_in_progress_detected() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_file(tmp.path(), "README.md", b"base\ncontent\n");
        git(tmp.path(), ["checkout", "-b", "feature"]).unwrap();
        commit_file(tmp.path(), "README.md", b"base\nfeature change\n");
        git(tmp.path(), ["checkout", "main"]).unwrap();
        commit_file(tmp.path(), "README.md", b"base\nmain change\n");
        let merge_result = git(tmp.path(), ["merge", "feature"]);
        assert!(merge_result.is_err(), "merge should have conflicted");
        let state = detect_operation_state(tmp.path()).unwrap();
        assert_eq!(state, RepositoryOperationState::MergeInProgress);
        let _ = git(tmp.path(), ["merge", "--abort"]);
    }

    #[test]
    fn rebase_in_progress_detected() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_file(tmp.path(), "README.md", b"base\n");
        git(tmp.path(), ["checkout", "-b", "feature"]).unwrap();
        commit_file(tmp.path(), "README.md", b"feature\n");
        git(tmp.path(), ["checkout", "main"]).unwrap();
        commit_file(tmp.path(), "README.md", b"main\n");
        let rebase_result = git(tmp.path(), ["rebase", "feature"]);
        assert!(rebase_result.is_err(), "rebase should have conflicted");
        let state = detect_operation_state(tmp.path()).unwrap();
        assert_eq!(state, RepositoryOperationState::RebaseInProgress);
        let _ = git(tmp.path(), ["rebase", "--abort"]);
    }

    #[test]
    fn cherry_pick_in_progress_detected() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_file(tmp.path(), "README.md", b"base");
        git(tmp.path(), ["checkout", "-b", "feature"]).unwrap();
        commit_file(tmp.path(), "feature.txt", b"feature work");
        let feature_commit = head_commit(tmp.path()).unwrap();
        git(tmp.path(), ["checkout", "main"]).unwrap();
        commit_file(tmp.path(), "feature.txt", b"main already has this");
        let cherry_pick_result = git(tmp.path(), ["cherry-pick", &feature_commit]);
        assert!(
            cherry_pick_result.is_err(),
            "cherry-pick should have conflicted"
        );
        let state = detect_operation_state(tmp.path()).unwrap();
        assert_eq!(state, RepositoryOperationState::CherryPickInProgress);
        let _ = git(tmp.path(), ["cherry-pick", "--abort"]);
    }

    #[test]
    fn revert_in_progress_detected() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_file(tmp.path(), "README.md", b"first\n");
        let first_commit = head_commit(tmp.path()).unwrap();
        commit_file(tmp.path(), "README.md", b"second\n");
        git(tmp.path(), ["checkout", "-b", "side", &first_commit]).unwrap();
        commit_file(tmp.path(), "README.md", b"side\n");
        git(tmp.path(), ["checkout", "main"]).unwrap();
        let revert_result = git(tmp.path(), ["revert", "--no-commit", &first_commit]);
        assert!(revert_result.is_err(), "revert should have conflicted");
        let state = detect_operation_state(tmp.path()).unwrap();
        assert_eq!(state, RepositoryOperationState::RevertInProgress);
        let _ = git(tmp.path(), ["revert", "--abort"]);
    }

    #[test]
    fn bisect_in_progress_detected() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_file(tmp.path(), "v1.txt", b"v1");
        let bad_commit = head_commit(tmp.path()).unwrap();
        commit_file(tmp.path(), "v2.txt", b"v2");
        git(tmp.path(), ["bisect", "start"]).unwrap();
        git(tmp.path(), ["bisect", "bad", &bad_commit]).unwrap();
        let state = detect_operation_state(tmp.path()).unwrap();
        assert_eq!(state, RepositoryOperationState::BisectInProgress);
        let _ = git(tmp.path(), ["bisect", "reset"]);
    }

    #[test]
    fn revalidate_matching_snapshot_succeeds() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_file(tmp.path(), "README.md", b"hello");
        let snapshot = packet_base_snapshot(tmp.path(), "repo_test").unwrap();
        assert!(revalidate_packet_base(tmp.path(), &snapshot).is_ok());
    }

    #[test]
    fn revalidate_changed_snapshot_fails() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_file(tmp.path(), "README.md", b"hello");
        let snapshot = packet_base_snapshot(tmp.path(), "repo_test").unwrap();
        fs::write(tmp.path().join("README.md"), b"modified").unwrap();
        let result = revalidate_packet_base(tmp.path(), &snapshot);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), "work_packet_source_changed");
    }

    #[test]
    fn check_cleanliness_clean() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_file(tmp.path(), "README.md", b"hello");
        assert_eq!(
            check_cleanliness(tmp.path()).unwrap(),
            SourceCleanliness::Clean
        );
    }

    #[test]
    fn check_cleanliness_dirty() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_file(tmp.path(), "README.md", b"hello");
        fs::write(tmp.path().join("README.md"), b"modified").unwrap();
        assert_eq!(
            check_cleanliness(tmp.path()).unwrap(),
            SourceCleanliness::Dirty
        );
    }

    #[test]
    fn check_cleanliness_ignores_pulse_metadata_changes() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_file(tmp.path(), "README.md", b"hello");
        fs::create_dir_all(tmp.path().join(".pulse/workgraph/nodes")).unwrap();
        fs::write(tmp.path().join(".pulse/workgraph/nodes/TK-001.json"), b"{}").unwrap();
        fs::create_dir_all(tmp.path().join(".pulse/events/2026-01-01")).unwrap();
        fs::write(tmp.path().join(".pulse/events/2026-01-01/evt.json"), b"{}").unwrap();
        fs::create_dir_all(tmp.path().join(".pulse/evidence/receipts")).unwrap();
        fs::write(tmp.path().join(".pulse/evidence/receipts/r.json"), b"{}").unwrap();
        fs::create_dir_all(tmp.path().join(".pulse/docs")).unwrap();
        fs::write(tmp.path().join(".pulse/docs/registry.json"), b"{}").unwrap();
        fs::create_dir_all(tmp.path().join(".pulse/knowledge/records")).unwrap();
        fs::write(tmp.path().join(".pulse/knowledge/records/k.json"), b"{}").unwrap();
        assert_eq!(
            check_cleanliness(tmp.path()).unwrap(),
            SourceCleanliness::Clean
        );
    }

    #[test]
    fn check_cleanliness_still_rejects_pulse_runtime_if_not_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_file(tmp.path(), "README.md", b"hello");
        fs::create_dir_all(tmp.path().join(".pulse/runtime/assignment/leases")).unwrap();
        fs::write(
            tmp.path()
                .join(".pulse/runtime/assignment/leases/lease_TEST.json"),
            b"{}",
        )
        .unwrap();
        assert_eq!(
            check_cleanliness(tmp.path()).unwrap(),
            SourceCleanliness::Dirty
        );
    }

    #[test]
    fn non_git_directory_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let result = packet_base_snapshot(tmp.path(), "repo_test");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), "work_packet_source_unavailable");
    }

    #[test]
    fn linked_worktree_is_classified_and_uses_git_path_operations() {
        let tmp = tempfile::tempdir().unwrap();
        let primary = tmp.path().join("primary");
        let linked = tmp.path().join("linked");
        fs::create_dir_all(&primary).unwrap();
        init_repo(&primary);
        commit_file(&primary, "README.md", b"hello");
        let output = run_git(
            &primary,
            &[
                "worktree",
                "add",
                "--detach",
                linked.to_str().unwrap(),
                "HEAD",
            ],
        );
        assert!(
            output.status.success(),
            "git worktree add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let snapshot = packet_base_snapshot(&linked, "repo_test").unwrap();
        assert_eq!(
            snapshot.worktree_root_kind,
            WorktreeRootKind::LinkedWorktree
        );
        let git_file = fs::read_to_string(linked.join(".git")).expect("linked worktree git file");
        let git_dir: std::path::PathBuf = git_file
            .trim_start_matches("gitdir: ")
            .trim()
            .parse()
            .unwrap();
        fs::write(
            git_dir.join("MERGE_HEAD"),
            b"0123456789abcdef0123456789abcdef01234567\n",
        )
        .unwrap();
        assert_eq!(
            detect_operation_state(&linked).unwrap(),
            RepositoryOperationState::MergeInProgress
        );
    }

    #[test]
    fn preserve_loaders_do_not_bootstrap_missing_planes() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        assert!(crate::evidence::manifest::load_existing(tmp.path())
            .unwrap()
            .is_none());
        assert!(crate::docs::manifest::load_existing(tmp.path())
            .unwrap()
            .is_none());
        assert!(crate::knowledge::manifest::load_existing(tmp.path())
            .unwrap()
            .is_none());
        assert!(!tmp.path().join(".pulse").exists());
    }

    #[test]
    fn docs_registry_without_evidence_does_not_bootstrap_identity() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        fs::create_dir_all(tmp.path().join(".pulse/docs")).unwrap();
        fs::write(tmp.path().join(".pulse/docs/registry.json"), b"{}").unwrap();
        let result = crate::docs::manifest::load_existing(tmp.path());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), "docs_registry_evidence_missing");
        assert!(!tmp.path().join(".pulse/evidence").exists());
    }

    #[test]
    fn malformed_existing_manifest_maps_to_packet_manifest_invalid() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        fs::create_dir_all(tmp.path().join(".pulse/evidence")).unwrap();
        fs::write(tmp.path().join(".pulse/evidence/manifest.json"), b"{}").unwrap();
        let result = check_repository_identity(tmp.path());
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code(), "work_packet_manifest_invalid");
        assert!(error.to_string().contains("cause_code="));
    }

    #[test]
    fn check_repository_identity_missing_evidence_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let result = check_repository_identity(tmp.path());
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code(),
            "work_packet_repository_identity_missing"
        );
    }

    #[test]
    fn repository_identity_consistency_ok() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_file(tmp.path(), "README.md", b"hello");
        let manifest = crate::evidence::bootstrap(tmp.path()).unwrap().manifest;
        let repo_id = manifest.repository_id.clone();
        let docs_out = crate::docs::manifest::bootstrap(tmp.path()).unwrap();
        assert_eq!(docs_out.registry.repository_id, repo_id);
        let result = check_repository_identity(tmp.path()).unwrap();
        assert_eq!(result.repository_id, repo_id);
    }

    #[test]
    fn packet_source_conversion_from_snapshot() {
        let snapshot = PacketSourceSnapshot {
            repository_id: "repo_test_123".to_string(),
            kind: "git_commit".to_string(),
            commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
            head_ref: Some("refs/heads/main".to_string()),
            worktree_root_kind: WorktreeRootKind::PrimaryOrExistingWorktree,
            cleanliness: SourceCleanliness::Clean,
            operation_state: RepositoryOperationState::Normal,
            currentness: "current".to_string(),
        };
        let packet_source: crate::work_packet::PacketSource = snapshot.into();
        assert_eq!(packet_source.repository_id, "repo_test_123");
        assert_eq!(packet_source.kind, "git_commit");
        assert_eq!(
            packet_source.commit,
            "0123456789abcdef0123456789abcdef01234567"
        );
        assert_eq!(packet_source.head_ref, Some("refs/heads/main".to_string()));
        assert_eq!(
            packet_source.worktree_root_kind,
            "primary_or_existing_worktree"
        );
        assert_eq!(packet_source.cleanliness, "clean");
        assert_eq!(packet_source.operation_state, "normal");
        assert_eq!(packet_source.currentness, "current");
    }
}
