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
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .map_err(|error| PulseError::io(repo_root.join(".git"), error))?;
    if !output.status.success() {
        return Err(PulseError::validation(
            "source_binding_stale",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
