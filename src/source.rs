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

use crate::{PulseError, Result};
use std::path::Path;
use std::process::Command;

const EVIDENCE_ONLY_PREFIXES: [&str; 2] = [".pulse/evidence/", ".pulse/events/"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceBindingStatus {
    Current,
    Stale,
    DirtyUnsupported,
    Unsupported,
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

/// Check whether the worktree is clean (no tracked or untracked non-ignored
/// changes).
pub fn check_cleanliness(repo_root: &Path) -> Result<SourceCleanliness> {
    let output = packet_git(
        repo_root,
        ["status", "--porcelain=v1", "--untracked-files=all"],
    )?;
    if output.trim().is_empty() {
        Ok(SourceCleanliness::Clean)
    } else {
        Ok(SourceCleanliness::Dirty)
    }
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
