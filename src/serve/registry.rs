//! The user-level project registry for `pulse serve` (Decision 0023 §5,
//! amended 2026-09-17: registry-primary discovery replaces the raw
//! workspace walk as the default source).
//!
//! One file, `~/.pulse/projects.json` (override with `PULSE_REGISTRY` for
//! tests): an array of `{path, registered_at}`. Written by `pulse init`
//! (the enrollment moment — creating `.pulse/issues.jsonl` *is*
//! registering), read by `pulse serve`. Reads filter dead entries
//! silently: a project whose `.pulse/issues.jsonl` has disappeared is
//! hidden from the board, never an error, and the registry file itself is
//! only rewritten under `pulse init`'s lock.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::discovery;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegisteredProject {
    pub path: PathBuf,
    pub registered_at: String,
}

/// Where the registry lives: `$PULSE_REGISTRY` (tests), else
/// `$HOME/.pulse/projects.json`.
///
/// # Errors
/// Neither `PULSE_REGISTRY` nor `HOME`/`USERPROFILE` is set.
pub fn registry_path() -> Result<PathBuf, crate::PulseError> {
    if let Some(path) = std::env::var_os("PULSE_REGISTRY") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| {
            crate::PulseError::kernel(
                "registry_home_missing",
                "cannot locate the project registry: no PULSE_REGISTRY, HOME or USERPROFILE",
                "set HOME, or point PULSE_REGISTRY at a projects.json path",
            )
        })?;
    Ok(Path::new(&home).join(".pulse").join("projects.json"))
}

/// Read every registered path from `path`, dropping entries whose repo
/// is gone. An unreadable or missing registry file is simply "no
/// registrations". The `_at` form is what tests use — env mutation in
/// parallel tests is a race.
pub fn registered_roots_at(path: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(entries) = serde_json::from_str::<Vec<RegisteredProject>>(&text) else {
        return Vec::new();
    };
    entries
        .into_iter()
        .map(|entry| entry.path)
        .filter(|root| discovery::issues_path(root).exists())
        .collect()
}

/// Read every registered path, dropping entries whose repo is gone. An
/// unreadable or missing registry file is simply "no registrations".
pub fn registered_roots() -> Vec<PathBuf> {
    registry_path()
        .map(|path| registered_roots_at(&path))
        .unwrap_or_default()
}

/// Add `repo_root` to the registry at `path` if not already present.
/// Canonicalizes first so the same repo registered twice (or
/// re-registered after a clone elsewhere) collapses to one entry. The
/// `_into` form is what tests use — env mutation in parallel tests is a
/// race.
///
/// # Errors
/// The read-modify-write fails io-wise.
pub fn register_into(path: &Path, repo_root: &Path) -> Result<(), crate::PulseError> {
    let canonical = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let mut entries: Vec<RegisteredProject> = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    if entries.iter().any(|entry| entry.path == canonical) {
        return Ok(());
    }
    entries.push(RegisteredProject {
        path: canonical,
        registered_at: chrono::Utc::now().to_rfc3339(),
    });
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| crate::PulseError::io(path, error))?;
    }
    let bytes = serde_json::to_vec_pretty(&entries)
        .map_err(|error| crate::PulseError::json(path, error))?;
    crate::storage::atomic_write(path, &bytes)
}

/// Add `repo_root` to the user-level registry if not already present.
///
/// # Errors
/// The registry path cannot be determined, or the read-modify-write fails
/// io-wise. Registration is best-effort by policy: `pulse init` reports
/// the failure but does not fail the init.
pub fn register(repo_root: &Path) -> Result<(), crate::PulseError> {
    let path = registry_path()?;
    register_into(&path, repo_root)
}
