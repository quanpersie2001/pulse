//! Source fence (plan 0022 §5.2, pulled forward from P1.5 into this commit
//! because the old 2179-line source.rs depended on `work_packet`,
//! `evidence::manifest`, `docs::manifest` and `knowledge::manifest`, all
//! deleted alongside `graph/*` — it could not compile standalone).
//!
//! A cheap snapshot of "what does the tree look like right now": HEAD plus a
//! hash of every dirty path's content/diff, filtered to drop `.pulse/**`,
//! the root `PULSE.md`/`AGENTS.md` harness config, and any path matching a
//! caller-supplied `fence_ignore` list (plan §5.2 — editing a note file must
//! never stale a close; Track B hit exactly that false positive with
//! `close_source_stale`).
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

/// Content fence for one ticket (decision 0025 B6): every file matching
/// `touches`, tracked or not, hashed by path + bytes; independent of HEAD
/// and of every path outside `touches`.
///
/// Unlike [`snapshot`], the hash covers the whole scope whether or not
/// git calls it dirty — committing an in-scope file with unchanged content
/// leaves this fence unmoved, which is the property that lets one ticket's
/// close survive another ticket's commit landing first. `commit` is
/// carried for display only; the fence comparison
/// (`kernel::profile::same_fence`) never reads it for a scoped ticket.
///
/// # Errors
/// Propagates a git invocation failure (not a repository, git not on
/// PATH, no commits yet).
pub fn scoped_snapshot(
    repo_root: &Path,
    touches: &[String],
    fence_ignore: &[String],
) -> Result<Source> {
    let commit = head_commit(repo_root)?;
    // Every path git can see — tracked plus untracked-but-not-ignored —
    // narrowed to the ticket's scope. Fence filtering runs first, so a
    // touch naming `.pulse/**` or a `fence_ignore` glob holds nothing.
    let tracked = git(repo_root, &["ls-files"])?;
    let others = git(repo_root, &["ls-files", "--others", "--exclude-standard"])?;
    let mut scope: Vec<String> = tracked
        .lines()
        .chain(others.lines())
        .map(|path| path.trim_matches('"').to_string())
        .filter(|path| !is_fenced_out(path, fence_ignore))
        .filter(|path| touches.iter().any(|pattern| glob_match(pattern, path)))
        .collect();
    scope.sort();
    scope.dedup();
    let dirty_hash = hash_scope(repo_root, &scope)?;
    // The dirty list stays a display aid: which in-scope paths git
    // currently calls dirty. The fence itself is `dirty_hash`.
    let dirty_paths: Vec<String> = dirty_paths(repo_root, fence_ignore)?
        .into_iter()
        .filter(|path| touches.iter().any(|pattern| glob_match(pattern, path)))
        .collect();
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

/// Whether `path` sits outside every source fence: `.pulse/**`, the root
/// harness configs, and the target's own `fence_ignore` list. `snapshot`
/// uses it to build `dirty_paths`; `kernel::lane::changed_files_since`
/// (plan 0025 F3's shared changed-files source) applies the same rule, so
/// a file that cannot stale a fence also cannot stale a doc.
pub(crate) fn is_fenced_out(path: &str, fence_ignore: &[String]) -> bool {
    if path.starts_with(".pulse/") {
        return true;
    }
    // Harness config written by `pulse init` is not product source: editing
    // it must never stale a close or brick one (dogfood ST-1, F14 — tuning
    // the qa-lane seed's own profiles left every ticket uncloseable until
    // the repo opted out via fence_ignore). Root-level only: a nested
    // AGENTS.md belongs to the repo's own tree.
    if path == "PULSE.md" || path == "AGENTS.md" {
        return true;
    }
    fence_ignore.iter().any(|pattern| glob_match(pattern, path))
}

/// Minimal glob, shared by `fence_ignore` (this module) and learnings/docs
/// `applies_to` matching (plan §11.2/§12.2 — "dùng lại `source::glob_match`,
/// mở rộng cho `*` một cấp nếu cần"): an exact match, a `dir/` prefix, a
/// `dir/**` prefix (both matching `dir` itself), or — the one-level
/// extension — a single `*` within one path segment (`docs/*.md`,
/// `src/*/handler.rs`). No `**` in the middle of a pattern, no character
/// classes: neither caller needs them, and a full glob engine is a crate
/// this plan intentionally avoids.
pub(crate) fn glob_match(pattern: &str, path: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix("/**") {
        if path == prefix || path.starts_with(&format!("{prefix}/")) {
            return true;
        }
    } else if let Some(prefix) = pattern.strip_suffix('/') {
        if path.starts_with(&format!("{prefix}/")) {
            return true;
        }
    } else if pattern == path {
        return true;
    }
    pattern.contains('*') && segments_match(&split_segments(pattern), &split_segments(path))
}

fn split_segments(value: &str) -> Vec<&str> {
    value.split('/').collect()
}

fn segments_match(pattern: &[&str], path: &[&str]) -> bool {
    match (pattern.first(), path.first()) {
        (None, None) => true,
        (Some(p), Some(s)) => segment_glob(p, s) && segments_match(&pattern[1..], &path[1..]),
        _ => false,
    }
}

/// One path segment against one pattern segment, with at most one `*`
/// wildcard in the pattern segment (`*.md`, `handler-*`, `a*b`).
fn segment_glob(pattern: &str, segment: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == segment,
        Some((prefix, suffix)) => {
            segment.len() >= prefix.len() + suffix.len()
                && segment.starts_with(prefix)
                && segment.ends_with(suffix)
        }
    }
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

/// The scope fence hash: every path in `scope` contributes
/// `path\0<bytes>\0`; a tracked-but-deleted file contributes the
/// tombstone `path\0<deleted>\0`, so a deletion moves the fence like any
/// edit. The `scope:sha256:` prefix is a different namespace from
/// [`hash_dirty_state`]'s `sha256:` — the two fence kinds can never
/// compare equal by accident (decision 0025 B6).
fn hash_scope(repo_root: &Path, scope: &[String]) -> Result<String> {
    let mut hasher = Sha256::new();
    for path in scope {
        hasher.update(path.as_bytes());
        hasher.update(b"\0");
        match std::fs::read(repo_root.join(path)) {
            Ok(bytes) => hasher.update(&bytes),
            // Tracked but gone from disk — read errors mean "not these
            // bytes", which is exactly what the tombstone encodes.
            Err(_) => hasher.update(b"<deleted>"),
        }
        hasher.update(b"\0");
    }
    Ok(format!("scope:sha256:{}", hex::encode(hasher.finalize())))
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
    fn fence_ignores_the_root_harness_config_files() {
        // Dogfood ST-1 F14: tuning PULSE.md (the qa-lane profiles) after a
        // handoff bricked every close with close_source_stale. Harness
        // config is not product source — the root files are always fenced;
        // a nested AGENTS.md still counts.
        let repo = init_repo();
        // A tracked file under web/ so git reports web/AGENTS.md as its own
        // untracked path instead of collapsing it into `?? web/`.
        std::fs::create_dir_all(repo.path().join("web")).unwrap();
        std::fs::write(repo.path().join("web/tracked.md"), "x\n").unwrap();
        assert!(StdCommand::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["add", "."])
            .status()
            .unwrap()
            .success());
        assert!(StdCommand::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["commit", "-qm", "web"])
            .status()
            .unwrap()
            .success());
        std::fs::write(repo.path().join("PULSE.md"), "profiles: {}\n").unwrap();
        std::fs::write(repo.path().join("AGENTS.md"), "agent rules\n").unwrap();
        std::fs::write(repo.path().join("web/AGENTS.md"), "nested\n").unwrap();
        let snap = snapshot(repo.path(), &[]).unwrap();
        assert!(!snap.dirty_paths.contains(&"PULSE.md".to_string()));
        assert!(!snap.dirty_paths.contains(&"AGENTS.md".to_string()));
        assert!(snap.dirty_paths.contains(&"web/AGENTS.md".to_string()));
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

    #[test]
    fn glob_match_keeps_the_original_prefix_and_exact_semantics() {
        assert!(glob_match("works/friction.md", "works/friction.md"));
        assert!(!glob_match("works/friction.md", "works/other.md"));
        assert!(glob_match("works/**", "works"));
        assert!(glob_match("works/**", "works/nested/file.md"));
        assert!(!glob_match("works/", "works"));
        assert!(glob_match("works/", "works/file.md"));
    }

    #[test]
    fn glob_match_supports_a_one_level_wildcard() {
        assert!(glob_match("docs/*.md", "docs/readme.md"));
        assert!(!glob_match("docs/*.md", "docs/sub/readme.md"));
        assert!(glob_match("src/*/handler.rs", "src/auth/handler.rs"));
        assert!(!glob_match(
            "src/*/handler.rs",
            "src/auth/nested/handler.rs"
        ));
    }

    // --- scoped fence (decision 0025 B6) ---

    fn repo_with_src_and_web() -> tempfile::TempDir {
        let dir = init_repo();
        let run = |args: &[&str]| {
            assert!(StdCommand::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .status()
                .unwrap()
                .success());
        };
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::create_dir_all(dir.path().join("web")).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "fn a() {}\n").unwrap();
        std::fs::write(dir.path().join("web/app.js"), "// app\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "src and web"]);
        dir
    }

    #[test]
    fn the_scope_hash_ignores_everything_outside_the_touches() {
        let repo = repo_with_src_and_web();
        let before = scoped_snapshot(repo.path(), &["src/**".to_string()], &[]).unwrap();
        assert!(before.dirty_hash.starts_with("scope:sha256:"));
        // Worker-2's file, a different ticket's scope entirely.
        std::fs::write(repo.path().join("web/app.js"), "// mutated\n").unwrap();
        let after = scoped_snapshot(repo.path(), &["src/**".to_string()], &[]).unwrap();
        assert_eq!(before.dirty_hash, after.dirty_hash);
    }

    #[test]
    fn committing_an_in_scope_file_with_unchanged_content_does_not_move_the_fence() {
        // The core property (decision 0025): HEAD is not the fence. The
        // ticket's file landing as a commit moves HEAD but not the bytes
        // this ticket owns.
        let repo = repo_with_src_and_web();
        std::fs::write(repo.path().join("src/new.rs"), "fn b() {}\n").unwrap();
        let before = scoped_snapshot(repo.path(), &["src/**".to_string()], &[]).unwrap();
        let run = |args: &[&str]| {
            assert!(StdCommand::new("git")
                .arg("-C")
                .arg(repo.path())
                .args(args)
                .status()
                .unwrap()
                .success());
        };
        run(&["add", "src/new.rs"]);
        run(&["commit", "-q", "-m", "land src"]);
        let after = scoped_snapshot(repo.path(), &["src/**".to_string()], &[]).unwrap();
        assert_ne!(before.commit, after.commit, "HEAD did move");
        assert_eq!(before.dirty_hash, after.dirty_hash, "the fence did not");
    }

    #[test]
    fn editing_a_scoped_file_moves_the_fence() {
        let repo = repo_with_src_and_web();
        let before = scoped_snapshot(repo.path(), &["src/**".to_string()], &[]).unwrap();
        std::fs::write(repo.path().join("src/lib.rs"), "fn a() { changed }\n").unwrap();
        let after = scoped_snapshot(repo.path(), &["src/**".to_string()], &[]).unwrap();
        assert_ne!(before.dirty_hash, after.dirty_hash);
        assert_eq!(after.dirty_paths, vec!["src/lib.rs".to_string()]);
    }

    #[test]
    fn a_new_untracked_file_matching_the_glob_moves_the_fence() {
        let repo = repo_with_src_and_web();
        let before = scoped_snapshot(repo.path(), &["src/*".to_string()], &[]).unwrap();
        std::fs::write(repo.path().join("src/extra.rs"), "fn c() {}\n").unwrap();
        let after = scoped_snapshot(repo.path(), &["src/*".to_string()], &[]).unwrap();
        assert_ne!(before.dirty_hash, after.dirty_hash);
    }

    #[test]
    fn deleting_a_scoped_file_moves_the_fence() {
        let repo = repo_with_src_and_web();
        let before = scoped_snapshot(repo.path(), &["src/**".to_string()], &[]).unwrap();
        std::fs::remove_file(repo.path().join("src/lib.rs")).unwrap();
        let after = scoped_snapshot(repo.path(), &["src/**".to_string()], &[]).unwrap();
        assert_ne!(before.dirty_hash, after.dirty_hash);
        assert_eq!(after.dirty_paths, vec!["src/lib.rs".to_string()]);
    }

    #[test]
    fn dot_pulse_never_enters_the_scope_even_when_touched() {
        let repo = repo_with_src_and_web();
        let before = scoped_snapshot(repo.path(), &[".pulse/**".to_string()], &[]).unwrap();
        std::fs::create_dir_all(repo.path().join(".pulse")).unwrap();
        std::fs::write(repo.path().join(".pulse/issues.jsonl"), "{}\n").unwrap();
        let after = scoped_snapshot(repo.path(), &[".pulse/**".to_string()], &[]).unwrap();
        assert_eq!(before.dirty_hash, after.dirty_hash);
        assert!(after.dirty_paths.is_empty());
    }
}
