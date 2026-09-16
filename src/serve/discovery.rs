//! Workspace discovery for `pulse serve` (Decision 0023 §5): find every
//! directory under a workspace root that holds a `.pulse/issues.jsonl`.
//!
//! A directory is a project if and only if that file exists; the walk is
//! depth-limited and skips the usual heavy/generated directories. The id
//! is the first 12 hex chars of the sha256 of the canonical path, so the
//! path itself never appears in a URL and ids are stable across rescans.

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 4;
const SKIPPED_DIRS: [&str; 9] = [
    "node_modules",
    ".git",
    "target",
    ".venv",
    "venv",
    "__pycache__",
    ".next",
    "dist",
    "build",
];

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProjectEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    pub counts: ProjectCounts,
    pub last_activity_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct ProjectCounts {
    pub epic: u64,
    pub story: u64,
    pub ticket: u64,
    pub decision: u64,
    /// Tickets not yet `done`/`cancelled` (draft, ready, blocked, active,
    /// verifying).
    pub tickets_open: u64,
    pub tickets_done: u64,
}

pub fn issues_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".pulse").join("issues.jsonl")
}

/// Stable short id for a repo root: 12 hex chars of the sha256 of its
/// canonical path.
///
/// # Panics
/// Never — hashing infallible over the lossy path string.
pub fn project_id(canonical: &Path) -> String {
    let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
    hex::encode(&digest[..6])
}

/// Rescan the workspace. Cheap by design (directory walk + small JSONL
/// reads); callers may run it per request.
pub fn discover(workspace: &Path) -> Vec<ProjectEntry> {
    let mut out = Vec::new();
    walk(workspace, 0, &mut out);
    out.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    out
}

/// The canonical repo root whose id matches `pid`, if discovered.
pub fn resolve(workspace: &Path, pid: &str) -> Option<PathBuf> {
    discover(workspace)
        .into_iter()
        .find(|entry| entry.id == pid)
        .map(|entry| PathBuf::from(entry.path))
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<ProjectEntry>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if SKIPPED_DIRS.contains(&name) {
            continue;
        }
        if issues_path(&path).exists() {
            out.push(project_entry(&path));
        } else {
            walk(&path, depth + 1, out);
        }
    }
}

fn project_entry(repo_root: &Path) -> ProjectEntry {
    let canonical = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let mut counts = ProjectCounts::default();
    let mut last: Option<String> = None;
    let (records, _) = crate::serve::lenient_read_issues(repo_root);
    for record in &records {
        match record.get("kind").and_then(Value::as_str) {
            Some("epic") => counts.epic += 1,
            Some("story") => counts.story += 1,
            Some("decision") => counts.decision += 1,
            Some("ticket") => {
                counts.ticket += 1;
                match record.get("status").and_then(Value::as_str) {
                    Some("done") | Some("cancelled") => counts.tickets_done += 1,
                    _ => counts.tickets_open += 1,
                }
            }
            _ => {}
        }
        if let Some(ts) = record.get("updated_at").and_then(Value::as_str) {
            if last.as_deref().map_or(true, |seen| ts > seen) {
                last = Some(ts.to_string());
            }
        }
    }
    ProjectEntry {
        id: project_id(&canonical),
        name: repo_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| repo_root.display().to_string()),
        path: canonical.display().to_string(),
        counts,
        last_activity_at: last,
    }
}
