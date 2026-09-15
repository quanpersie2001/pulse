//! Source fence (plan 0022 §5.2, pulled forward from P1.5 into this commit
//! because the old 2179-line source.rs depended on `work_packet`,
//! `evidence::manifest`, `docs::manifest` and `knowledge::manifest`, all
//! deleted alongside `graph/*` — it could not compile standalone).
//!
//! A cheap snapshot of "what does the tree look like right now": HEAD plus a
//! hash of every dirty path's content/diff, filtered to drop `.pulse/**` and
//! any path matching a caller-supplied `fence_ignore` list (plan §5.2 —
//! editing a note file must never stale a close; Track B hit exactly that
//! false positive with `close_source_stale`).
//!
//! No worktree mirroring in v3 (plan §10.6): [`state_repo_root`] is the
//! identity function.

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::error::{PulseError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub commit: String,
    pub dirty_hash: String,
    pub dirty_paths: Vec<String>,
}

/// Snapshot HEAD and every dirty path not fenced out.
///
/// # Errors
/// Propagates a git invocation failure (not a repository, git not on PATH,
/// no commits yet).
pub fn snapshot(repo_root: &Path, fence_ignore: &[String]) -> Result<Source> {
    let commit = head_commit(repo_root)?;
    let dirty_paths = dirty_paths(repo_root, fence_ignore)?;
    let dirty_hash = hash_dirty_state(repo_root, &dirty_paths)?;
    Ok(Source {
        commit,
        dirty_hash,
        dirty_paths,
    })
}

/// Whether two snapshots describe the same tree state.
pub fn same(a: &Source, b: &Source) -> bool {
    a.commit == b.commit && a.dirty_hash == b.dirty_hash
}

/// # Errors
/// Propagates a git invocation failure.
pub fn head_commit(repo_root: &Path) -> Result<String> {
    Ok(git(repo_root, &["rev-parse", "HEAD"])?.trim().to_string())
}

fn dirty_paths(repo_root: &Path, fence_ignore: &[String]) -> Result<Vec<String>> {
    let status = git(repo_root, &["status", "--porcelain=v1"])?;
    let mut paths: Vec<String> = status
        .lines()
        .filter_map(|line| line.get(3..))
        .map(|path| path.trim_matches('"').to_string())
        .filter(|path| !is_fenced_out(path, fence_ignore))
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn is_fenced_out(path: &str, fence_ignore: &[String]) -> bool {
    if path.starts_with(".pulse/") {
        return true;
    }
    fence_ignore.iter().any(|pattern| glob_match(pattern, path))
}

/// Minimal glob: an exact match, a `dir/` prefix, or a `dir/**` prefix.
/// `PULSE.md fence_ignore` entries are simple prefixes, not a full glob
/// engine — good enough until a real need for `*`/`?` mid-pattern appears.
fn glob_match(pattern: &str, path: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return path == prefix || path.starts_with(&format!("{prefix}/"));
    }
    if let Some(prefix) = pattern.strip_suffix('/') {
        return path.starts_with(&format!("{prefix}/"));
    }
    path == pattern
}

fn hash_dirty_state(repo_root: &Path, dirty_paths: &[String]) -> Result<String> {
    if dirty_paths.is_empty() {
        return Ok(crate::canonical_json::hash_bytes(b""));
    }
    let mut hasher = Sha256::new();
    for path in dirty_paths {
        hasher.update(path.as_bytes());
        hasher.update(b"\0");
        let diff = git(repo_root, &["diff", "--binary", "HEAD", "--", path]).unwrap_or_default();
        if diff.is_empty() {
            // Untracked (or deleted-with-no-diff-text) path: `git diff`
            // shows nothing for a file git has never tracked.
            if let Ok(bytes) = std::fs::read(repo_root.join(path)) {
                hasher.update(&bytes);
            }
        } else {
            hasher.update(diff.as_bytes());
        }
        hasher.update(b"\0");
    }
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

fn git(repo_root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .map_err(|error| {
            PulseError::kernel(
                "git_invocation_failed",
                format!("failed to run git {args:?}: {error}"),
                "check that git is on PATH",
            )
        })?;
    if !output.status.success() {
        return Err(PulseError::kernel(
            "git_invocation_failed",
            format!(
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
            "check that repo_root is a git repository with at least one commit",
        ));
    }
    String::from_utf8(output.stdout).map_err(|error| {
        PulseError::kernel(
            "git_invocation_failed",
            format!("git {args:?} produced non-UTF-8 output: {error}"),
            "this repository's paths must be valid UTF-8",
        )
    })
}

/// v3 has no worktree mirroring (plan §10.6): the workspace root is always
/// the state root.
///
/// # Errors
/// Never actually fails; the `Result` matches every other path-resolving
/// function in this module and keeps callers stable if that changes.
pub fn state_repo_root(workspace_root: &Path) -> Result<PathBuf> {
    Ok(workspace_root.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            let status = StdCommand::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .status()
                .unwrap();
            assert!(status.success());
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "test"]);
        std::fs::write(dir.path().join("README.md"), "hello\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "init"]);
        dir
    }

    #[test]
    fn clean_tree_has_no_dirty_paths() {
        let repo = init_repo();
        let snap = snapshot(repo.path(), &[]).unwrap();
        assert!(snap.dirty_paths.is_empty());
    }

    #[test]
    fn fence_ignores_dot_pulse() {
        let repo = init_repo();
        std::fs::create_dir_all(repo.path().join(".pulse")).unwrap();
        std::fs::write(repo.path().join(".pulse/issues.jsonl"), "{}\n").unwrap();
        let snap = snapshot(repo.path(), &[]).unwrap();
        assert!(snap.dirty_paths.is_empty());
    }

    #[test]
    fn same_is_true_when_only_a_fence_ignored_file_changes() {
        let repo = init_repo();
        let fence_ignore = vec!["works/friction.md".to_string()];
        std::fs::write(repo.path().join("works").join("friction.md"), "a\n").unwrap_or_else(|_| {
            std::fs::create_dir_all(repo.path().join("works")).unwrap();
            std::fs::write(repo.path().join("works/friction.md"), "a\n").unwrap();
        });
        let before = snapshot(repo.path(), &fence_ignore).unwrap();
        std::fs::write(repo.path().join("works/friction.md"), "b\n").unwrap();
        let after = snapshot(repo.path(), &fence_ignore).unwrap();
        assert!(same(&before, &after), "{before:?} != {after:?}");
    }

    #[test]
    fn same_is_false_when_a_non_ignored_file_changes() {
        let repo = init_repo();
        let before = snapshot(repo.path(), &[]).unwrap();
        std::fs::write(repo.path().join("README.md"), "changed\n").unwrap();
        let after = snapshot(repo.path(), &[]).unwrap();
        assert!(!same(&before, &after));
    }

    #[test]
    fn an_untracked_new_file_changes_the_dirty_hash() {
        let repo = init_repo();
        let before = snapshot(repo.path(), &[]).unwrap();
        std::fs::write(repo.path().join("new.txt"), "new\n").unwrap();
        let after = snapshot(repo.path(), &[]).unwrap();
        assert!(!same(&before, &after));
        assert!(after.dirty_paths.contains(&"new.txt".to_string()));
    }

    #[test]
    fn state_repo_root_is_the_identity_function() {
        let repo = init_repo();
        assert_eq!(state_repo_root(repo.path()).unwrap(), repo.path());
    }
}
