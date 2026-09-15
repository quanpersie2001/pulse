//! Repository enrollment (plan 0022 §14 P1.3, interim minimal version).
//!
//! Full assets (`assets/agents-block.md`, `PULSE.md` profile seed,
//! `docs/README.md` seed, host detector files, prompts) are P1.10's job.
//! Until then this creates just enough for the rest of the CLI to have
//! somewhere to write: the `.pulse/` tree, an empty `issues.jsonl`, an empty
//! `runners.json`, a placeholder `PULSE.md`, and the two non-anchored
//! `.gitignore` entries plan §3 requires (`**/.pulse/runtime/`,
//! `**/.pulse/cache/` — anchored patterns broke in a nested repo, per the
//! ROADMAP note plan §3 references).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::storage::WriteGuard;
use crate::PulseError;

const DIRS: [&str; 6] = [
    ".pulse",
    ".pulse/receipts",
    ".pulse/evidence",
    ".pulse/events",
    ".pulse/learnings",
    ".pulse/runtime",
];

const GITIGNORE_ENTRIES: [&str; 2] = ["**/.pulse/runtime/", "**/.pulse/cache/"];

const PULSE_MD_SEED: &str = "\
# PULSE.md - seeded by `pulse init`.
# Profile and lane defaults land in plan 0022 P1.9; this is a placeholder.
fence_ignore: []
profiles: {}
";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryInitStatus {
    Initialized,
    Unchanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryInitReport {
    pub schema_version: u32,
    pub status: RepositoryInitStatus,
    pub created: Vec<String>,
}

/// Create `.pulse/`, the store files it needs, `PULSE.md` and the runtime/
/// cache `.gitignore` entries, if they are not already present. Idempotent:
/// a second run reports `Unchanged` and creates nothing.
///
/// # Errors
/// Propagates an I/O error creating a directory or file, or a lock timeout
/// acquiring the repository write lock.
pub fn initialize_repository(repo_root: &Path) -> Result<RepositoryInitReport> {
    let _guard = WriteGuard::acquire(repo_root)?;
    let mut created = Vec::new();

    for dir in DIRS {
        let path = repo_root.join(dir);
        if !path.exists() {
            fs::create_dir_all(&path).map_err(|error| PulseError::io(&path, error))?;
            created.push(dir.to_string());
        }
    }

    let issues_path = crate::store::issues::issues_path(repo_root);
    if !issues_path.exists() {
        fs::write(&issues_path, b"").map_err(|error| PulseError::io(&issues_path, error))?;
        created.push(".pulse/issues.jsonl".to_string());
    }

    let runners_path = repo_root.join(".pulse/runners.json");
    if !runners_path.exists() {
        fs::write(&runners_path, b"{}\n").map_err(|error| PulseError::io(&runners_path, error))?;
        created.push(".pulse/runners.json".to_string());
    }

    let pulse_md_path = repo_root.join("PULSE.md");
    if !pulse_md_path.exists() {
        fs::write(&pulse_md_path, PULSE_MD_SEED)
            .map_err(|error| PulseError::io(&pulse_md_path, error))?;
        created.push("PULSE.md".to_string());
    }

    created.extend(ensure_gitignore_entries(repo_root)?);

    let status = if created.is_empty() {
        RepositoryInitStatus::Unchanged
    } else {
        RepositoryInitStatus::Initialized
    };
    Ok(RepositoryInitReport {
        schema_version: 1,
        status,
        created,
    })
}

fn ensure_gitignore_entries(repo_root: &Path) -> Result<Vec<String>> {
    let path = repo_root.join(".gitignore");
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let missing: Vec<&str> = GITIGNORE_ENTRIES
        .into_iter()
        .filter(|entry| !existing.lines().any(|line| line.trim() == *entry))
        .collect();
    if missing.is_empty() {
        return Ok(Vec::new());
    }

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    for entry in &missing {
        updated.push_str(entry);
        updated.push('\n');
    }
    fs::write(&path, updated).map_err(|error| PulseError::io(&path, error))?;
    Ok(missing
        .into_iter()
        .map(|entry| format!(".gitignore: {entry}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_run_creates_everything_and_reports_initialized() {
        let repo = tempfile::tempdir().unwrap();
        let report = initialize_repository(repo.path()).unwrap();
        assert_eq!(report.status, RepositoryInitStatus::Initialized);
        assert!(repo.path().join(".pulse/issues.jsonl").exists());
        assert!(repo.path().join(".pulse/runners.json").exists());
        assert!(repo.path().join("PULSE.md").exists());
        let gitignore = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
        for entry in GITIGNORE_ENTRIES {
            assert!(gitignore.contains(entry));
        }
    }

    #[test]
    fn second_run_is_idempotent_and_reports_unchanged() {
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path()).unwrap();
        let report = initialize_repository(repo.path()).unwrap();
        assert_eq!(report.status, RepositoryInitStatus::Unchanged);
        assert!(report.created.is_empty());
    }

    #[test]
    fn preserves_a_hand_edited_gitignore_and_only_appends_missing_entries() {
        let repo = tempfile::tempdir().unwrap();
        fs::write(repo.path().join(".gitignore"), "node_modules/\n").unwrap();
        initialize_repository(repo.path()).unwrap();
        let gitignore = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
        assert!(gitignore.contains("node_modules/"));
        for entry in GITIGNORE_ENTRIES {
            assert!(gitignore.contains(entry));
        }
    }
}
