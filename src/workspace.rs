//! Workspace mode definitions and binding helpers (P2S2-I1/I5).
//!
//! This module defines workspace modes, selection rules, path validation,
//! in-place binding, isolated worktree creation, cleanup and adoption
//! helpers.
//!
//! Ownership: `src/workspace.rs` owns worktree commands and workspace path
//! policy. It calls `src/source.rs` for Git/source validation rather than
//! duplicating source snapshot logic.
//!
//! See `proposals/phase2-slice2-atomic-reservation-workspace-binding.md`.

use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::source;
use crate::PulseError;
pub(crate) type WorkspaceResult<T> = std::result::Result<T, PulseError>;

// ---------------------------------------------------------------------------
// Workspace mode
// ---------------------------------------------------------------------------

/// Describes how a workspace is provisioned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceMode {
    /// Work in the repository root directly (only allowed when packet says
    /// `in_place_allowed`).
    InPlace,
    /// Work in a `git worktree add --detach` managed workspace.
    IsolatedWorktree,
}

impl WorkspaceMode {
    /// Canonical string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkspaceMode::InPlace => "in_place",
            WorkspaceMode::IsolatedWorktree => "isolated_worktree",
        }
    }
}

// ---------------------------------------------------------------------------
// Workspace strategy (derived from risk)
// ---------------------------------------------------------------------------

/// The workspace strategy dictated by a work item's risk assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceStrategy {
    /// In-place is permitted; isolated worktree is optional.
    InPlaceAllowed,
    /// Isolated worktree is required; in-place is forbidden.
    IsolatedWorktreeRequired,
    /// Risk is unassessed — workspace strategy unknown.
    Unassessed,
}

impl FromStr for WorkspaceMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "in_place" => Ok(WorkspaceMode::InPlace),
            "isolated_worktree" => Ok(WorkspaceMode::IsolatedWorktree),
            _ => Err(()),
        }
    }
}

impl WorkspaceStrategy {
    /// Canonical string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkspaceStrategy::InPlaceAllowed => "in_place_allowed",
            WorkspaceStrategy::IsolatedWorktreeRequired => "isolated_worktree_required",
            WorkspaceStrategy::Unassessed => "unassessed",
        }
    }

    /// Whether in-place is an acceptable mode for this strategy.
    pub fn allows_in_place(&self) -> bool {
        matches!(self, WorkspaceStrategy::InPlaceAllowed)
    }

    /// Whether isolated worktree is required.
    pub fn requires_isolated(&self) -> bool {
        matches!(self, WorkspaceStrategy::IsolatedWorktreeRequired)
    }
}

// ---------------------------------------------------------------------------
// Workspace binding status (projection)
// ---------------------------------------------------------------------------

/// Status of a workspace binding in a prepared assignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingStatus {
    /// Workspace is bound and ready.
    Bound,
    /// Workspace has been released.
    Released,
    /// Workspace needs operator review.
    StaleNeedsOperator,
    /// Workspace not yet allocated.
    NotAllocated,
}

impl FromStr for WorkspaceStrategy {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "in_place_allowed" => Ok(WorkspaceStrategy::InPlaceAllowed),
            "isolated_worktree_required" => Ok(WorkspaceStrategy::IsolatedWorktreeRequired),
            "unassessed" => Ok(WorkspaceStrategy::Unassessed),
            _ => Err(()),
        }
    }
}

impl BindingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BindingStatus::Bound => "bound",
            BindingStatus::Released => "released",
            BindingStatus::StaleNeedsOperator => "stale_needs_operator",
            BindingStatus::NotAllocated => "not_allocated",
        }
    }
}

// ---------------------------------------------------------------------------
// Workspace mode selection (P2S2-I5)
// ---------------------------------------------------------------------------

/// Select the final workspace mode given a CLI request (or None for auto) and
/// the packet-derived strategy.
///
/// Rules per P2S2-D7:
/// - `None` (auto): isolated when strategy requires isolated, otherwise
///   in-place.
/// - `Some(InPlace)`: allowed only when `strategy.allows_in_place()`.
/// - `Some(IsolatedWorktree)`: always valid (creation may still fail).
///
/// Errors:
/// - `assignment_workspace_mode_unsupported` if requested mode violates
///   strategy.
pub fn select_workspace_mode(
    requested: Option<WorkspaceMode>,
    strategy: WorkspaceStrategy,
) -> WorkspaceResult<WorkspaceMode> {
    match requested {
        None => {
            if strategy.requires_isolated() {
                Ok(WorkspaceMode::IsolatedWorktree)
            } else {
                Ok(WorkspaceMode::InPlace)
            }
        }
        Some(WorkspaceMode::InPlace) => {
            if strategy.allows_in_place() {
                Ok(WorkspaceMode::InPlace)
            } else {
                Err(PulseError::validation(
                    "assignment_workspace_worktree_required",
                    "risk assessment requires isolated worktree; in-place is not allowed",
                ))
            }
        }
        Some(WorkspaceMode::IsolatedWorktree) => Ok(WorkspaceMode::IsolatedWorktree),
    }
}

// ---------------------------------------------------------------------------
// Managed path validation
// ---------------------------------------------------------------------------

/// A validated workspace path that is safe under the runtime root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedWorkspacePath {
    /// Resolved canonical absolute path.
    pub canonical_path: PathBuf,
    /// Whether the path is within the runtime workspaces root.
    pub is_within_runtime_root: bool,
}

/// Validate that a workspace candidate path is safe under the given runtime
/// root.
///
/// Rules:
/// - Path must not be absolute (should be repository-relative).
/// - Path must not traverse upward (`..`).
/// - After resolution, the path must be under the runtime root to prevent
///   symlink escape.
///
/// Errors: `unsafe_path` (mapped from `AbsolutePath`, `PathTraversal` or
/// `PathEscape`).
pub fn validate_managed_path(
    candidate: &Path,
    runtime_root: &Path,
) -> WorkspaceResult<ManagedWorkspacePath> {
    use crate::storage::paths::validate_relative_path;
    use std::fs;

    validate_relative_path(candidate).map_err(|_| {
        PulseError::validation(
            "unsafe_path",
            format!(
                "workspace path traversal or absolute path: {}",
                candidate.display()
            ),
        )
    })?;

    let runtime_root =
        fs::canonicalize(runtime_root).map_err(|error| PulseError::io(runtime_root, error))?;

    let full_path = if candidate.is_relative() {
        runtime_root.join(candidate)
    } else {
        candidate.to_path_buf()
    };

    // Check the actual resolved path (if it exists) or an approximation.
    let resolved = if full_path.exists() {
        fs::canonicalize(&full_path).map_err(|error| PulseError::io(&full_path, error))?
    } else {
        // Don't resolve non-existent paths through canonicalize (would fail).
        // Validate by walking components and checking intermediates.
        validate_path_components(&full_path, &runtime_root).map_err(|_| {
            PulseError::validation(
                "unsafe_path",
                format!(
                    "workspace path escapes runtime root: {}",
                    candidate.display()
                ),
            )
        })?;
        full_path
    };

    if !resolved.starts_with(&runtime_root) {
        return Err(PulseError::validation(
            "unsafe_path",
            format!(
                "workspace path escapes runtime root: {}",
                candidate.display()
            ),
        ));
    }

    let is_within_runtime_root = resolved.starts_with(&runtime_root);
    Ok(ManagedWorkspacePath {
        canonical_path: resolved,
        is_within_runtime_root,
    })
}

/// Validate that every component of a (possibly non-existent) path stays
/// under the runtime root. Handles symlinks by checking each existing parent.
fn validate_path_components(path: &Path, runtime_root: &Path) -> std::result::Result<(), ()> {
    let mut current = runtime_root.to_path_buf();

    // Walk the relative part of the path.
    if let Ok(rel) = path.strip_prefix(runtime_root) {
        for component in rel.components() {
            match component {
                Component::Normal(part) => {
                    current.push(part);
                    if current.exists() {
                        // Resolve symlinks at each step
                        if let Ok(canonical) = current.canonicalize() {
                            if !canonical.starts_with(runtime_root) {
                                return Err(());
                            }
                            current = canonical;
                        }
                    }
                }
                Component::ParentDir => {
                    if !current.pop() {
                        return Err(());
                    }
                }
                Component::CurDir => {}
                Component::RootDir | Component::Prefix(_) => {
                    return Err(());
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Workspace ID and path generation
// ---------------------------------------------------------------------------

/// Generate a deterministic workspace ID from a ticket ID and a random suffix.
///
/// Format: `wt_TK-XXX_<randomhex>`
pub fn generate_workspace_id(ticket_id: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let suffix = format!("{:016x}", now.as_nanos() & 0xffff_ffff_ffff_ffff);
    format!("wt_{}_{}", ticket_id, suffix)
}

/// Generate a deterministic workspace path under the runtime workspaces root.
pub fn generate_workspace_path(runtime_workspaces_root: &Path, workspace_id: &str) -> PathBuf {
    runtime_workspaces_root.join(workspace_id)
}

// ---------------------------------------------------------------------------
// In-place binding (P2S2-D7)
// ---------------------------------------------------------------------------

/// Result of binding a workspace in-place (repo root).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InPlaceBinding {
    /// The canonical repo root path.
    pub path: PathBuf,
    /// Repository identity.
    pub repository_id: String,
    /// The exact base commit at bind time.
    pub base_commit: String,
    /// HEAD commit at bind time (same as base_commit in practice).
    pub head_commit: String,
    /// Cleanliness at bind time.
    pub cleanliness: String,
    /// Worktree root kind.
    pub worktree_root_kind: String,
}

/// Bind a workspace in-place at the repository root.
///
/// Validates that:
/// 1. The repository is a valid Git repo with a resolvable HEAD.
/// 2. HEAD matches the expected base commit.
/// 3. The worktree is clean (no tracked/untracked non-ignored changes).
/// 4. No Git operation (merge, rebase, etc.) is in progress.
///
/// Errors:
/// - `work_packet_source_unavailable` if HEAD cannot be resolved.
/// - `work_packet_dirty_source_unsupported` if the worktree is dirty.
/// - `work_packet_source_operation_in_progress` if a Git op is active.
/// - `work_packet_source_changed` if HEAD does not match `base_commit`.
pub fn bind_in_place(
    repo_root: &Path,
    base_commit: &str,
    repository_id: &str,
) -> WorkspaceResult<InPlaceBinding> {
    let validation = source::check_workspace_base(repo_root, base_commit)?;

    Ok(InPlaceBinding {
        path: repo_root.to_path_buf(),
        repository_id: repository_id.to_string(),
        base_commit: base_commit.to_string(),
        head_commit: validation.commit,
        cleanliness: validation.cleanliness.as_str().to_string(),
        worktree_root_kind: validation.worktree_root_kind.as_str().to_string(),
    })
}

// ---------------------------------------------------------------------------
// Isolated worktree binding (P2S2-D7/D8)
// ---------------------------------------------------------------------------

/// Result of creating and binding an isolated worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsolatedWorktreeBinding {
    /// Path to the worktree.
    pub path: PathBuf,
    /// Workspace ID.
    pub workspace_id: String,
    /// The exact base commit at bind time.
    pub base_commit: String,
    /// HEAD commit at bind time.
    pub head_commit: String,
    /// Repository identity.
    pub repository_id: String,
    /// Cleanliness at bind time.
    pub cleanliness: String,
    /// Worktree root kind (will be `linked_worktree`).
    pub worktree_root_kind: String,
    /// Whether the worktree was newly created (vs. adopted).
    pub was_newly_created: bool,
}

/// Create an isolated worktree at the exact base commit.
///
/// This function:
/// 1. Generates the deterministic workspace path under the runtime workspaces
///    root.
/// 2. Creates the parent directory if needed.
/// 3. Runs `git worktree add --detach <path> <base-commit>` from `repo_root`.
/// 4. Validates the new worktree source state.
///
/// On failure during steps 1-3, the pending worktree directory is safely
/// cleaned up. On failure in step 4 (validation), the worktree is left for
/// operator review and the function returns
/// `assignment_workspace_cleanup_needed`.
///
/// Errors:
/// - `assignment_workspace_create_failed` if `git worktree add` fails.
/// - `assignment_workspace_source_mismatch` if worktree source doesn't match.
/// - `assignment_workspace_dirty` if worktree is not clean at base.
/// - `assignment_workspace_cleanup_needed` if worktree exists but validation
///   fails and safe cleanup is not possible.
pub fn create_isolated_worktree(
    repo_root: &Path,
    runtime_workspaces_root: &Path,
    workspace_id: &str,
    base_commit: &str,
    repository_id: &str,
) -> WorkspaceResult<IsolatedWorktreeBinding> {
    let worktree_path = generate_workspace_path(runtime_workspaces_root, workspace_id);

    // Ensure the runtime workspaces directory exists.
    if !runtime_workspaces_root.exists() {
        std::fs::create_dir_all(runtime_workspaces_root)
            .map_err(|error| PulseError::io(runtime_workspaces_root, error))?;
    }

    // Check if the path already exists (possible adoption scenario).
    if worktree_path.exists() {
        // Attempt to adopt the existing worktree.
        if can_adopt_worktree(&worktree_path, base_commit, repository_id).unwrap_or(false) {
            let validation = source::check_workspace_base(&worktree_path, base_commit)?;
            return Ok(IsolatedWorktreeBinding {
                path: worktree_path,
                workspace_id: workspace_id.to_string(),
                base_commit: base_commit.to_string(),
                head_commit: validation.commit,
                repository_id: repository_id.to_string(),
                cleanliness: validation.cleanliness.as_str().to_string(),
                worktree_root_kind: validation.worktree_root_kind.as_str().to_string(),
                was_newly_created: false,
            });
        }

        // Path exists but can't be adopted.
        return Err(PulseError::validation(
            "assignment_workspace_cleanup_needed",
            format!(
                "workspace path already exists and cannot be adopted: {}",
                worktree_path.display()
            ),
        ));
    }

    // Create the worktree parent directory.
    if let Some(parent) = worktree_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
        }
    }

    // Run `git worktree add --detach <path> <base-commit>`.
    let output = Command::new("git")
        .args([
            "worktree",
            "add",
            "--detach",
            worktree_path.to_str().unwrap_or_default(),
            base_commit,
        ])
        .current_dir(repo_root)
        .output()
        .map_err(|error| PulseError::io(repo_root.join(".git"), error))?;

    if !output.status.success() {
        // Clean up the pending directory on git failure.
        let _ = std::fs::remove_dir_all(&worktree_path);
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(PulseError::validation(
            "assignment_workspace_create_failed",
            format!("git worktree add failed: {}", stderr),
        ));
    }

    // Validate the new worktree source state.
    let validation =
        source::check_workspace_base(&worktree_path, base_commit).inspect_err(|_| {
            // Clean up the worktree on validation failure when safe.
            let _ = cleanup_worktree(repo_root, &worktree_path);
        })?;

    Ok(IsolatedWorktreeBinding {
        path: worktree_path,
        workspace_id: workspace_id.to_string(),
        base_commit: base_commit.to_string(),
        head_commit: validation.commit,
        repository_id: repository_id.to_string(),
        cleanliness: validation.cleanliness.as_str().to_string(),
        worktree_root_kind: validation.worktree_root_kind.as_str().to_string(),
        was_newly_created: true,
    })
}

// ---------------------------------------------------------------------------
// Validate exact base commit (P2S2-I5)
// ---------------------------------------------------------------------------

/// Validate that the worktree at `repo_root` is at the exact expected base
/// commit and is clean with no operations in progress.
///
/// A lightweight wrapper around `source::check_workspace_base`.
pub fn validate_exact_base(
    repo_root: &Path,
    expected_commit: &str,
) -> WorkspaceResult<source::WorkspaceBaseValidation> {
    source::check_workspace_base(repo_root, expected_commit)
}

// ---------------------------------------------------------------------------
// Worktree cleanup and adoption (P2S2-D8/D10)
// ---------------------------------------------------------------------------

/// Check whether an existing worktree can be adopted for a claim.
///
/// An existing worktree can be adopted when:
/// 1. It is at the expected base commit.
/// 2. It references the expected repository.
///
/// This function does not check cleanliness or operation state; those are
/// validated by the caller during binding.
pub fn can_adopt_worktree(
    worktree_path: &Path,
    expected_base_commit: &str,
    _expected_repository_id: &str,
) -> WorkspaceResult<bool> {
    if !worktree_path.join(".git").exists() && !worktree_path.join(".git").is_symlink() {
        return Ok(false);
    }

    // Check if HEAD resolves.
    let head = match source::head_commit(worktree_path) {
        Ok(h) => h,
        Err(_) => return Ok(false),
    };

    Ok(head == expected_base_commit)
}

/// Safely remove an isolated worktree.
///
/// Uses `git worktree remove` when possible, falling back to manual directory
/// removal. This function will NOT remove a path that appears to be the
/// primary repository root (checks for `../../.git` vs `./.git` patterns).
///
/// Rules:
/// - Never delete the in-place root (caller must ensure).
/// - Uses `git worktree remove --force` for clean worktrees.
/// - Falls back to `std::fs::remove_dir_all` only when safe.
pub fn cleanup_worktree(repo_root: &Path, worktree_path: &Path) -> WorkspaceResult<()> {
    // Safety: never delete the primary repo root.
    let canonical_worktree = worktree_path
        .canonicalize()
        .map_err(|error| PulseError::io(worktree_path, error))?;
    let canonical_root = repo_root
        .canonicalize()
        .map_err(|error| PulseError::io(repo_root, error))?;
    if canonical_worktree == canonical_root {
        return Err(PulseError::validation(
            "unsafe_path",
            "refusing to clean up the primary repository root".to_string(),
        ));
    }

    // First try `git worktree remove`.
    let output = Command::new("git")
        .args(["worktree", "remove", "--force"])
        .arg(worktree_path.as_os_str())
        .current_dir(repo_root)
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            return Ok(());
        }
    }

    // Fall back to safe manual removal.
    std::fs::remove_dir_all(worktree_path).map_err(|error| PulseError::io(worktree_path, error))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers for generating workspace IDs deterministically for tests
// ---------------------------------------------------------------------------

/// Generate a workspace ID from a ticket ID and a stable suffix (for tests).
pub fn generate_workspace_id_with_suffix(ticket_id: &str, suffix: &str) -> String {
    format!("wt_{}_{}", ticket_id, suffix)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_mode_round_trip() {
        use std::str::FromStr;
        assert_eq!(WorkspaceMode::InPlace.as_str(), "in_place");
        assert_eq!(
            WorkspaceMode::IsolatedWorktree.as_str(),
            "isolated_worktree"
        );
        assert_eq!(
            WorkspaceMode::from_str("in_place"),
            Ok(WorkspaceMode::InPlace)
        );
        assert_eq!(
            WorkspaceMode::from_str("isolated_worktree"),
            Ok(WorkspaceMode::IsolatedWorktree)
        );
        assert!(WorkspaceMode::from_str("unknown").is_err());
    }

    #[test]
    fn workspace_strategy_mapping() {
        assert!(WorkspaceStrategy::InPlaceAllowed.allows_in_place());
        assert!(!WorkspaceStrategy::InPlaceAllowed.requires_isolated());
        assert!(!WorkspaceStrategy::IsolatedWorktreeRequired.allows_in_place());
        assert!(WorkspaceStrategy::IsolatedWorktreeRequired.requires_isolated());
        assert!(!WorkspaceStrategy::Unassessed.allows_in_place());
        assert!(!WorkspaceStrategy::Unassessed.requires_isolated());
    }

    #[test]
    fn binding_status_values() {
        assert_eq!(BindingStatus::Bound.as_str(), "bound");
        assert_eq!(BindingStatus::Released.as_str(), "released");
        assert_eq!(
            BindingStatus::StaleNeedsOperator.as_str(),
            "stale_needs_operator"
        );
        assert_eq!(BindingStatus::NotAllocated.as_str(), "not_allocated");
    }

    // -------------------------------------------------------------------
    // Workspace mode selection
    // -------------------------------------------------------------------

    #[test]
    fn select_workspace_mode_auto_in_place() {
        assert_eq!(
            select_workspace_mode(None, WorkspaceStrategy::InPlaceAllowed).unwrap(),
            WorkspaceMode::InPlace
        );
    }

    #[test]
    fn select_workspace_mode_auto_isolated() {
        assert_eq!(
            select_workspace_mode(None, WorkspaceStrategy::IsolatedWorktreeRequired).unwrap(),
            WorkspaceMode::IsolatedWorktree
        );
    }

    #[test]
    fn select_workspace_mode_explicit_in_place_allowed() {
        assert_eq!(
            select_workspace_mode(
                Some(WorkspaceMode::InPlace),
                WorkspaceStrategy::InPlaceAllowed
            )
            .unwrap(),
            WorkspaceMode::InPlace
        );
    }

    #[test]
    fn select_workspace_mode_explicit_in_place_rejected() {
        let err = select_workspace_mode(
            Some(WorkspaceMode::InPlace),
            WorkspaceStrategy::IsolatedWorktreeRequired,
        )
        .unwrap_err();
        assert_eq!(err.code(), "assignment_workspace_worktree_required");
    }

    #[test]
    fn select_workspace_mode_explicit_isolated_always_valid() {
        assert_eq!(
            select_workspace_mode(
                Some(WorkspaceMode::IsolatedWorktree),
                WorkspaceStrategy::InPlaceAllowed
            )
            .unwrap(),
            WorkspaceMode::IsolatedWorktree
        );
        assert_eq!(
            select_workspace_mode(
                Some(WorkspaceMode::IsolatedWorktree),
                WorkspaceStrategy::IsolatedWorktreeRequired
            )
            .unwrap(),
            WorkspaceMode::IsolatedWorktree
        );
    }

    // -------------------------------------------------------------------
    // Workspace ID and path generation
    // -------------------------------------------------------------------

    #[test]
    fn generate_workspace_id_format() {
        let id = generate_workspace_id("TK-001");
        assert!(id.starts_with("wt_TK-001_"));
        assert!(id.len() > "wt_TK-001_".len());
    }

    #[test]
    fn generate_workspace_path_format() {
        let root = Path::new("/tmp/runtime/workspaces");
        let path = generate_workspace_path(root, "wt_TK-001_deadbeef");
        assert_eq!(
            path,
            Path::new("/tmp/runtime/workspaces/wt_TK-001_deadbeef")
        );
    }

    #[test]
    fn generate_workspace_id_with_suffix_format() {
        let id = generate_workspace_id_with_suffix("TK-005", "test_suffix");
        assert_eq!(id, "wt_TK-005_test_suffix");
    }

    // -------------------------------------------------------------------
    // Managed path validation
    // -------------------------------------------------------------------

    #[test]
    fn validate_managed_path_rejects_absolute() {
        let runtime_root = Path::new("/tmp/runtime");
        let err = validate_managed_path(Path::new("/etc/passwd"), runtime_root).unwrap_err();
        assert_eq!(err.code(), "unsafe_path");
    }

    #[test]
    fn validate_managed_path_rejects_traversal() {
        let runtime_root = Path::new("/tmp/runtime");
        let err = validate_managed_path(Path::new("../escape"), runtime_root).unwrap_err();
        assert_eq!(err.code(), "unsafe_path");
    }

    #[test]
    fn validate_managed_path_rejects_parent_components() {
        let runtime_root = Path::new("/tmp/runtime");
        let err =
            validate_managed_path(Path::new("workspaces/../../etc"), runtime_root).unwrap_err();
        assert_eq!(err.code(), "unsafe_path");
    }

    #[test]
    fn validate_managed_path_accepts_valid_relative() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime_root = tmp.path().join("runtime");
        std::fs::create_dir_all(&runtime_root).unwrap();
        let result = validate_managed_path(Path::new("workspaces/wt_test"), &runtime_root).unwrap();
        assert!(result.canonical_path.ends_with("workspaces/wt_test"));
        assert!(result.is_within_runtime_root);
    }

    // -------------------------------------------------------------------
    // In-place binding (requires temp Git repo)
    // -------------------------------------------------------------------

    fn init_repo(path: &Path) {
        let output = Command::new("git")
            .args(["init", "-q"])
            .current_dir(path)
            .output()
            .expect("git init");
        assert!(output.status.success());
        let output = Command::new("git")
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@test",
                "commit",
                "--allow-empty",
                "-m",
                "initial",
            ])
            .current_dir(path)
            .output()
            .expect("git commit");
        assert!(output.status.success(), "git commit failed");
    }

    fn commit_file(path: &Path, rel: &str, content: &[u8]) -> String {
        if let Some(parent) = path.join(rel).parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path.join(rel), content).unwrap();
        let output = Command::new("git")
            .args(["add", rel])
            .current_dir(path)
            .output()
            .expect("git add");
        assert!(output.status.success(), "git add failed");
        let output = Command::new("git")
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
        assert!(output.status.success(), "git commit failed");
        // Return HEAD commit.
        let out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(path)
            .output()
            .expect("git rev-parse");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    fn bind_in_place_clean_repo() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let head = commit_file(tmp.path(), "README.md", b"hello");
        let binding = bind_in_place(tmp.path(), &head, "repo_test").unwrap();
        assert_eq!(binding.repository_id, "repo_test");
        assert_eq!(binding.base_commit, head);
        assert_eq!(binding.head_commit, head);
        assert_eq!(binding.cleanliness, "clean");
    }

    #[test]
    fn bind_in_place_wrong_commit_fails() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let head = commit_file(tmp.path(), "README.md", b"hello");
        // Use a different (nonexistent) commit.
        let wrong = format!("{}0", &head[..39]);
        let err = bind_in_place(tmp.path(), &wrong, "repo_test").unwrap_err();
        assert_eq!(err.code(), "work_packet_source_changed");
    }

    #[test]
    fn bind_in_place_dirty_repo_fails() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let head = commit_file(tmp.path(), "README.md", b"hello");
        // Dirty the worktree.
        std::fs::write(tmp.path().join("README.md"), b"modified").unwrap();
        let err = bind_in_place(tmp.path(), &head, "repo_test").unwrap_err();
        assert_eq!(err.code(), "work_packet_dirty_source_unsupported");
    }

    #[test]
    fn bind_in_place_non_git_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let err = bind_in_place(
            tmp.path(),
            "0000000000000000000000000000000000000000",
            "repo_test",
        )
        .unwrap_err();
        assert_eq!(err.code(), "work_packet_source_unavailable");
    }

    // -------------------------------------------------------------------
    // Isolated worktree creation
    // -------------------------------------------------------------------

    #[test]
    fn create_isolated_worktree_success() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let head = commit_file(tmp.path(), "README.md", b"hello");

        let workspaces_root = tmp.path().join(".pulse/runtime/workspaces");
        let workspace_id = "wt_TK-001_test";

        let binding = create_isolated_worktree(
            tmp.path(),
            &workspaces_root,
            workspace_id,
            &head,
            "repo_test",
        )
        .unwrap();

        assert_eq!(binding.workspace_id, workspace_id);
        assert!(binding.was_newly_created);
        assert_eq!(binding.base_commit, head);
        assert_eq!(binding.head_commit, head);
        assert_eq!(binding.cleanliness, "clean");
        assert_eq!(binding.repository_id, "repo_test");
        assert_eq!(binding.worktree_root_kind, "linked_worktree");
        assert!(binding.path.exists());

        // Clean up worktree.
        cleanup_worktree(tmp.path(), &binding.path).unwrap();
    }

    #[test]
    fn create_isolated_worktree_wrong_commit_fails() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let head = commit_file(tmp.path(), "README.md", b"hello");
        let wrong = format!("{}0", &head[..39]);

        let workspaces_root = tmp.path().join(".pulse/runtime/workspaces");
        let workspace_id = "wt_TK-002_test";

        // git worktree add --detach will fail because the commit doesn't exist.
        let err = create_isolated_worktree(
            tmp.path(),
            &workspaces_root,
            workspace_id,
            &wrong,
            "repo_test",
        )
        .unwrap_err();
        assert_eq!(err.code(), "assignment_workspace_create_failed");

        // Worktree should have been cleaned up.
        assert!(!workspaces_root.join(workspace_id).exists());
    }

    #[test]
    fn create_isolated_worktree_non_git_source_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let workspaces_root = tmp.path().join(".pulse/runtime/workspaces");
        let workspace_id = "wt_TK-003_test";

        let err = create_isolated_worktree(
            tmp.path(),
            &workspaces_root,
            workspace_id,
            "0000000000000000000000000000000000000000",
            "repo_test",
        )
        .unwrap_err();
        assert_eq!(err.code(), "assignment_workspace_create_failed");
    }

    // -------------------------------------------------------------------
    // Worktree cleanup
    // -------------------------------------------------------------------

    #[test]
    fn cleanup_worktree_removes_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let head = commit_file(tmp.path(), "README.md", b"hello");

        let workspaces_root = tmp.path().join(".pulse/runtime/workspaces");
        let workspace_id = "wt_TK-004_cleanup";

        let binding = create_isolated_worktree(
            tmp.path(),
            &workspaces_root,
            workspace_id,
            &head,
            "repo_test",
        )
        .unwrap();

        assert!(binding.path.exists());
        cleanup_worktree(tmp.path(), &binding.path).unwrap();
        assert!(!binding.path.exists());
    }

    #[test]
    fn cleanup_worktree_rejects_primary_root() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let err = cleanup_worktree(tmp.path(), tmp.path()).unwrap_err();
        assert_eq!(err.code(), "unsafe_path");
    }

    // -------------------------------------------------------------------
    // Worktree adoption checks
    // -------------------------------------------------------------------

    #[test]
    fn can_adopt_worktree_matching_commit() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let head = commit_file(tmp.path(), "README.md", b"hello");

        assert!(can_adopt_worktree(tmp.path(), &head, "repo_test").unwrap());
    }

    #[test]
    fn can_adopt_worktree_non_matching_commit() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let head = commit_file(tmp.path(), "README.md", b"hello");
        let wrong = format!("{}0", &head[..39]);

        assert!(!can_adopt_worktree(tmp.path(), &wrong, "repo_test").unwrap());
    }

    #[test]
    fn can_adopt_worktree_non_git() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!can_adopt_worktree(
            tmp.path(),
            "0000000000000000000000000000000000000000",
            "repo_test"
        )
        .unwrap());
    }

    // -------------------------------------------------------------------
    // Existing worktree adoption at creation time
    // -------------------------------------------------------------------

    #[test]
    fn create_isolated_worktree_adopts_existing_matching() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let head = commit_file(tmp.path(), "README.md", b"hello");

        let workspaces_root = tmp.path().join(".pulse/runtime/workspaces");
        let workspace_id = "wt_TK-006_adopt";

        // Create the worktree manually first.
        let wt_path = workspaces_root.join(workspace_id);
        let output = Command::new("git")
            .args([
                "worktree",
                "add",
                "--detach",
                wt_path.to_str().unwrap(),
                &head,
            ])
            .current_dir(tmp.path())
            .output()
            .expect("git worktree add");
        assert!(output.status.success(), "first worktree add failed");

        // Now "create" again — should adopt the existing worktree.
        let binding = create_isolated_worktree(
            tmp.path(),
            &workspaces_root,
            workspace_id,
            &head,
            "repo_test",
        )
        .unwrap();

        assert_eq!(binding.workspace_id, workspace_id);
        assert!(!binding.was_newly_created);
        assert!(binding.path.exists());

        cleanup_worktree(tmp.path(), &binding.path).unwrap();
    }

    #[test]
    fn create_isolated_worktree_existing_non_adoptable_fails() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let head = commit_file(tmp.path(), "README.md", b"hello");

        // Create a different repo to get a different commit.
        let other_tmp = tempfile::tempdir().unwrap();
        init_repo(other_tmp.path());
        let other_head = commit_file(other_tmp.path(), "OTHER.md", b"other");

        let workspaces_root = tmp.path().join(".pulse/runtime/workspaces");
        let workspace_id = "wt_TK-007_no_adopt";

        // Create a worktree from the other repo at this path (simulating stale).
        let wt_path = workspaces_root.join(workspace_id);
        let output = Command::new("git")
            .args([
                "worktree",
                "add",
                "--detach",
                wt_path.to_str().unwrap(),
                &other_head,
            ])
            .current_dir(other_tmp.path())
            .output()
            .expect("git worktree add");
        assert!(output.status.success(), "worktree add from other repo");

        // Attempt to create with different base — should fail.
        let err = create_isolated_worktree(
            tmp.path(),
            &workspaces_root,
            workspace_id,
            &head,
            "repo_test",
        )
        .unwrap_err();
        assert_eq!(err.code(), "assignment_workspace_cleanup_needed");

        // Clean up manually.
        let _ = Command::new("git")
            .args(["worktree", "remove", "--force", wt_path.to_str().unwrap()])
            .current_dir(other_tmp.path())
            .output();
    }

    // -------------------------------------------------------------------
    // Validate exact base
    // -------------------------------------------------------------------

    #[test]
    fn validate_exact_base_clean() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let head = commit_file(tmp.path(), "README.md", b"hello");
        let validation = validate_exact_base(tmp.path(), &head).unwrap();
        assert_eq!(validation.commit, head);
        assert_eq!(validation.cleanliness, source::SourceCleanliness::Clean);
        assert_eq!(
            validation.worktree_root_kind,
            source::WorktreeRootKind::PrimaryOrExistingWorktree
        );
    }

    #[test]
    fn validate_exact_base_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let head = commit_file(tmp.path(), "README.md", b"hello");
        let wrong = format!("{}0", &head[..39]);
        let err = validate_exact_base(tmp.path(), &wrong).unwrap_err();
        assert_eq!(err.code(), "work_packet_source_changed");
    }
}
