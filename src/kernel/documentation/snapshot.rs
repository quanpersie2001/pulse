//! Bounded validation-input snapshots for documentation receipts.
//!
//! This module reads current documents, declared generated inputs/outputs and
//! navigation projections. It rejects symlink traversal and excludes Pulse's
//! mutable receipt/runtime planes so recording cannot invalidate its own proof.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::canonical_json::hash_bytes;
use crate::docs::{DocsRegistry, DocumentStatus};
use crate::evidence::model::ContentBinding;
use crate::{PulseError, PulseResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DocumentSnapshot {
    pub(super) document_id: String,
    pub(super) document_revision: u64,
    pub(super) path: String,
    pub(super) content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ValidationSnapshot {
    pub(super) documents: Vec<DocumentSnapshot>,
    pub(super) content: Vec<ContentBinding>,
}

#[derive(Debug, Default)]
struct SnapshotContent {
    hashes: BTreeMap<String, String>,
    total_bytes: u64,
}

const MAX_SNAPSHOT_FILES: usize = 10_000;
const MAX_SNAPSHOT_BYTES: u64 = 128 * 1024 * 1024;

pub(super) fn snapshot_validation_inputs(
    repo_root: &Path,
    registry: &DocsRegistry,
) -> PulseResult<ValidationSnapshot> {
    let repo_root = crate::storage::paths::canonicalize_existing_dir(repo_root)?;
    let mut documents = Vec::new();
    let mut content = SnapshotContent::default();
    snapshot_path(
        &repo_root,
        Path::new(".pulse/docs/registry.json"),
        &mut content,
    )?;
    for document in registry.documents.iter().filter(|document| {
        document.status != DocumentStatus::Retired && document.superseded_by.is_none()
    }) {
        let relative = Path::new(&document.path);
        snapshot_path(&repo_root, relative, &mut content).map_err(|error| {
            if error.code() == "io_error" {
                PulseError::validation(
                    "docs_validation_receipt_content_missing",
                    format!("current document content is missing: {}", document.path),
                )
            } else {
                error
            }
        })?;
        documents.push(DocumentSnapshot {
            document_id: document.id.clone(),
            document_revision: document.revision,
            path: document.path.clone(),
            content_hash: content
                .hashes
                .get(&document.path)
                .expect("document path was inserted into snapshot")
                .clone(),
        });
    }
    for target in crate::docs::projection_targets(registry) {
        let relative = Path::new(&target.path);
        let resolved = crate::storage::paths::resolve_repo_relative(&repo_root, relative)?;
        if resolved.is_file() {
            snapshot_path(&repo_root, relative, &mut content)?;
        }
    }
    let content = content
        .hashes
        .into_iter()
        .map(|(path, sha256)| ContentBinding { path, sha256 })
        .collect();
    Ok(ValidationSnapshot { documents, content })
}

#[allow(dead_code)]
fn snapshot_pattern(
    repo_root: &Path,
    pattern: &str,
    content: &mut SnapshotContent,
) -> PulseResult<()> {
    if !pattern.contains('*') {
        let relative = Path::new(pattern);
        let resolved = crate::storage::paths::resolve_repo_relative(repo_root, relative)?;
        if resolved.is_file() {
            return snapshot_path(repo_root, relative, content);
        }
        if resolved.is_dir() {
            let tree_pattern = format!("{}/**", pattern.trim_end_matches('/'));
            return snapshot_matching_tree(repo_root, &resolved, &tree_pattern, content);
        }
        return Ok(());
    }
    let wildcard = pattern.find('*');
    let root = wildcard.map_or(pattern, |index| &pattern[..index]);
    let root = root
        .rsplit_once('/')
        .map_or("", |(directory, _)| directory)
        .trim_matches('/');
    let relative_root = Path::new(root);
    let resolved = if root.is_empty() {
        repo_root.to_path_buf()
    } else {
        crate::storage::paths::resolve_repo_relative(repo_root, relative_root)?
    };
    if resolved.is_file() {
        if generated_pattern_matches(pattern, root) {
            snapshot_path(repo_root, relative_root, content)?;
        }
        return Ok(());
    }
    if !resolved.exists() {
        return Ok(());
    }
    snapshot_matching_tree(repo_root, &resolved, pattern, content)
}

#[allow(dead_code)]
fn snapshot_matching_tree(
    repo_root: &Path,
    directory: &Path,
    pattern: &str,
    content: &mut SnapshotContent,
) -> PulseResult<()> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| PulseError::io(directory, error))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| PulseError::io(directory, error))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let file_type = entry
            .file_type()
            .map_err(|error| PulseError::io(entry.path(), error))?;
        let path = entry.path();
        let relative = path.strip_prefix(repo_root).map_err(|_| {
            PulseError::validation(
                "docs_validation_snapshot_unsafe",
                "generated validation path escaped repository",
            )
        })?;
        let normalized = relative.to_string_lossy().replace('\\', "/");
        if excluded_snapshot_path(&normalized) {
            continue;
        }
        if file_type.is_symlink() {
            return Err(PulseError::validation(
                "docs_validation_snapshot_unsafe",
                format!("generated validation patterns may not traverse symlink {normalized}"),
            ));
        }
        if file_type.is_dir() {
            snapshot_matching_tree(repo_root, &path, pattern, content)?;
        } else if file_type.is_file() && generated_pattern_matches(pattern, &normalized) {
            snapshot_path(repo_root, relative, content)?;
        }
    }
    Ok(())
}

fn snapshot_path(
    repo_root: &Path,
    relative: &Path,
    content: &mut SnapshotContent,
) -> PulseResult<()> {
    let path = crate::storage::paths::resolve_repo_relative(repo_root, relative)?;
    let metadata = fs::metadata(&path).map_err(|error| PulseError::io(&path, error))?;
    if !metadata.is_file() {
        return Err(PulseError::validation(
            "docs_validation_receipt_content_missing",
            format!(
                "validation content is not a regular file: {}",
                relative.display()
            ),
        ));
    }
    let key = path_string(relative);
    if content.hashes.contains_key(&key) {
        return Ok(());
    }
    if content.hashes.len() >= MAX_SNAPSHOT_FILES {
        return Err(PulseError::validation(
            "docs_validation_snapshot_bounded",
            "documentation validation snapshot exceeds file-count bound",
        ));
    }
    if content.total_bytes.saturating_add(metadata.len()) > MAX_SNAPSHOT_BYTES {
        return Err(PulseError::validation(
            "docs_validation_snapshot_bounded",
            "documentation validation snapshot exceeds byte bound",
        ));
    }
    let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
    content.total_bytes += metadata.len();
    content.hashes.insert(key, hash_bytes(&bytes));
    Ok(())
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[allow(dead_code)]
fn generated_pattern_matches(pattern: &str, path: &str) -> bool {
    if pattern == "**" || pattern == path {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return path == prefix || path.starts_with(&format!("{prefix}/"));
    }
    pattern
        .strip_suffix('*')
        .is_some_and(|prefix| path.starts_with(prefix))
}

#[allow(dead_code)]
fn excluded_snapshot_path(path: &str) -> bool {
    path == ".git"
        || path.starts_with(".git/")
        || path.starts_with(".pulse/runtime/")
        || path.starts_with(".pulse/cache/")
        || path.starts_with(".pulse/evidence/receipts/")
        || path.starts_with(".pulse/events/")
}
