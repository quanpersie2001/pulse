//! Safe repository enrollment across Pulse-owned Core domains.
//!
//! This module owns the cross-domain initialization order for a target
//! repository. It touches graph, evidence, documentation, knowledge, authority
//! and their durable source roots under one repository write lock. Every owner
//! preflights existing canonical state before the first canonical write, so a
//! later domain conflict cannot leave earlier domains newly enrolled.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::storage::transaction::recover_prepared_transactions;
use crate::storage::WriteGuard;
use crate::{PulseError, Result};

const PROPOSED_IGNORE_ENTRIES: [&str; 2] = [".pulse/runtime/", ".pulse/cache/"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RepositoryInitStatus {
    Initialized,
    Unchanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RepositoryInitReport {
    pub schema_version: u32,
    pub code: String,
    pub status: RepositoryInitStatus,
    pub repository_id: String,
    pub authority_policy_revision: u64,
    pub created: Vec<String>,
    pub preserved: Vec<String>,
    pub proposed_ignore_entries: Vec<String>,
}

/// Enroll a target repository into all current local-first Core domains.
///
/// The command never edits `.gitignore`; it reports the narrow runtime/cache
/// entries maintainers should review. Existing canonical state is preserved
/// and validated, while drift or a managed-path collision fails closed.
///
/// # Errors
///
/// Returns a typed error when the repository root is invalid, a managed path is
/// unsafe, existing domain state is incompatible, or durable initialization
/// cannot complete.
pub(crate) fn initialize_repository(repo_root: &Path) -> Result<RepositoryInitReport> {
    let repo_root = crate::storage::paths::canonicalize_existing_dir(repo_root)?;
    // The lock owner creates `.pulse/runtime/locks`, so reject managed symlinks
    // and file collisions before acquiring it, then revalidate under the lock.
    preflight_managed_paths(&repo_root)?;
    let _guard = WriteGuard::acquire(&repo_root)?;
    recover_prepared_transactions(&repo_root)?;

    preflight_managed_paths(&repo_root)?;
    crate::graph::store::preflight_bootstrap(&repo_root)?;
    crate::evidence::manifest::preflight_bootstrap(&repo_root)?;
    crate::docs::manifest::preflight_bootstrap(&repo_root)?;
    crate::knowledge::manifest::preflight_bootstrap(&repo_root)?;
    crate::policy::authority::preflight_bootstrap(&repo_root)?;

    let mut created = Vec::new();
    let mut preserved = Vec::new();
    for relative in ["docs", "works", "knowledge/learnings", ".pulse/events"] {
        ensure_directory(&repo_root, relative, &mut created, &mut preserved)?;
    }

    let graph = crate::graph::store::bootstrap(&repo_root)?;
    created.extend(graph.created);
    preserved.extend(graph.preserved);

    let evidence = crate::evidence::manifest::bootstrap(&repo_root)?;
    created.extend(evidence.created);
    preserved.extend(evidence.preserved);

    let docs = crate::docs::manifest::bootstrap_unlocked(&repo_root)?;
    created.extend(docs.created);
    preserved.extend(docs.preserved);

    let knowledge = crate::knowledge::manifest::bootstrap_unlocked(&repo_root)?;
    created.extend(knowledge.created);
    preserved.extend(knowledge.preserved);

    let authority = crate::policy::authority::bootstrap_default_deny(&repo_root)?;
    created.extend(authority.created);
    preserved.extend(authority.preserved);

    let created = stable_relative_paths(&repo_root, created)?;
    let preserved = stable_relative_paths(&repo_root, preserved)?;
    let status = if created.is_empty() {
        RepositoryInitStatus::Unchanged
    } else {
        RepositoryInitStatus::Initialized
    };

    Ok(RepositoryInitReport {
        schema_version: 1,
        code: "repository_initialized".to_string(),
        status,
        repository_id: evidence.manifest.repository_id,
        authority_policy_revision: authority.policy.revision,
        created,
        preserved,
        proposed_ignore_entries: PROPOSED_IGNORE_ENTRIES
            .iter()
            .map(|entry| (*entry).to_string())
            .collect(),
    })
}

fn preflight_managed_paths(repo_root: &Path) -> Result<()> {
    for relative in [
        ".pulse",
        ".pulse/workgraph",
        ".pulse/workgraph/schemas",
        ".pulse/workgraph/nodes",
        ".pulse/workgraph/edges",
        ".pulse/evidence",
        ".pulse/evidence/receipts",
        ".pulse/evidence/artifacts",
        ".pulse/evidence/artifacts/sha256",
        ".pulse/docs",
        ".pulse/knowledge",
        ".pulse/knowledge/entries",
        ".pulse/knowledge/relations",
        ".pulse/policy",
        ".pulse/events",
        ".pulse/runtime",
        ".pulse/runtime/locks",
        ".pulse/runtime/transactions",
        ".pulse/cache",
        "docs",
        "works",
        "knowledge",
        "knowledge/learnings",
    ] {
        let path = repo_root.join(relative);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(PulseError::io(&path, error)),
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(PulseError::validation(
                "repository_init_path_conflict",
                format!(
                    "managed repository path must be a real directory: {}",
                    path.display()
                ),
            ));
        }
    }
    for relative in [
        ".pulse/workgraph/manifest.json",
        ".pulse/workgraph/schemas/node.schema.json",
        ".pulse/workgraph/schemas/edge.schema.json",
        ".pulse/evidence/manifest.json",
        ".pulse/docs/registry.json",
        ".pulse/knowledge/manifest.json",
        ".pulse/policy/authority.json",
        ".pulse/runtime/locks/workgraph.lock",
    ] {
        let path = repo_root.join(relative);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(PulseError::io(&path, error)),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(PulseError::validation(
                "repository_init_path_conflict",
                format!(
                    "managed repository path must be a real file: {}",
                    path.display()
                ),
            ));
        }
    }
    reject_symlinks_recursively(&repo_root.join(".pulse/runtime/transactions"))?;
    Ok(())
}

fn reject_symlinks_recursively(root: &Path) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root).map_err(|error| PulseError::io(root, error))? {
        let entry = entry.map_err(|error| PulseError::io(root, error))?;
        let file_type = entry
            .file_type()
            .map_err(|error| PulseError::io(entry.path(), error))?;
        if file_type.is_symlink() {
            return Err(PulseError::validation(
                "repository_init_path_conflict",
                format!(
                    "managed repository path must not be a symlink: {}",
                    entry.path().display()
                ),
            ));
        }
        if file_type.is_dir() {
            reject_symlinks_recursively(&entry.path())?;
        }
    }
    Ok(())
}

fn ensure_directory(
    repo_root: &Path,
    relative: &str,
    created: &mut Vec<PathBuf>,
    preserved: &mut Vec<PathBuf>,
) -> Result<()> {
    let path = repo_root.join(relative);
    if path.exists() {
        preserved.push(path);
    } else {
        fs::create_dir_all(&path).map_err(|error| PulseError::io(&path, error))?;
        created.push(path);
    }
    Ok(())
}

fn stable_relative_paths(repo_root: &Path, paths: Vec<PathBuf>) -> Result<Vec<String>> {
    let mut relative = BTreeSet::new();
    for path in paths {
        let value = path.strip_prefix(repo_root).map_err(|_| {
            PulseError::validation(
                "repository_init_path_escape",
                format!(
                    "initialized path escaped repository root: {}",
                    path.display()
                ),
            )
        })?;
        relative.insert(value.to_string_lossy().replace('\\', "/"));
    }
    Ok(relative.into_iter().collect())
}
