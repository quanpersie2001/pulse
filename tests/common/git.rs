//! Shared git plumbing helpers for target-repository test setup.
//!
//! `git` runs a git command inside a target repo and asserts success; `commit_all`
//! establishes a clean committed baseline (initializing the repo idempotently) and
//! returns `HEAD`. Wired only into crates that drive target-repo git state.

use std::path::Path;
use std::process::Command;

/// Run `git <args>` inside `repo`, asserting success and returning trimmed stdout.
pub fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Initialize (idempotently) and commit all of `repo`, returning `HEAD`.
///
/// The `.git` existence guard makes this safe to call on a fresh tempdir or an
/// already-initialized repo; every caller operates on a fresh tempdir, so the
/// guard is equivalent to unconditional init for all existing call sites.
pub fn commit_all(repo: &Path) -> String {
    if !repo.join(".git").exists() {
        git(repo, &["init"]);
        git(repo, &["config", "user.email", "test@example.com"]);
        git(repo, &["config", "user.name", "Test User"]);
    }
    git(repo, &["add", "."]);
    git(repo, &["commit", "--allow-empty", "-m", "snapshot"]);
    git(repo, &["rev-parse", "HEAD"])
}
