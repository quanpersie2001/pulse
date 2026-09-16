//! JSON payload builders behind `pulse serve`'s read-only API (Decision
//! 0023 §6). Infallible by design: read failures degrade to empty lists
//! plus a note in the payload, never an error page — the server is a
//! viewer, and a half-written repo is still worth looking at.
//!
//! These functions are the test surface; `http.rs` only routes to them.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use super::discovery;
use super::lenient_read_issues;

/// `GET /api/projects` — rescan the workspace fresh on every call.
pub fn projects_payload(workspace: &Path) -> Value {
    json!({ "projects": discovery::discover(workspace) })
}

/// The repo root behind a project id, resolved by rescanning.
pub fn resolve_project(workspace: &Path, pid: &str) -> Option<PathBuf> {
    discovery::resolve(workspace, pid)
}

/// `GET /api/p/<pid>/board` — everything the kanban needs: all records,
/// learnings, and the receipt ledger's health (counts only; receipts ride
/// on the per-issue payload).
pub fn board_payload(repo_root: &Path) -> Value {
    let (records, skipped) = lenient_read_issues(repo_root);
    let (learnings, learnings_note) = match crate::learn::store::list(repo_root) {
        Ok(items) => (items, Value::Null),
        Err(error) => (Vec::new(), json!(error.to_string())),
    };
    let (receipt_count, unreadable_receipts) =
        match crate::evidence::receipt::list_receipts(repo_root) {
            Ok(list) => (list.receipts.len(), list.unreadable.len()),
            Err(_) => (0, 0),
        };
    json!({
        "issues": records,
        "skipped_lines": skipped,
        "learnings": learnings,
        "learnings_note": learnings_note,
        "receipt_count": receipt_count,
        "unreadable_receipts": unreadable_receipts,
    })
}

/// `GET /api/p/<pid>/issue/<id>` — the full drawer: the record itself,
/// every receipt naming it, its event trace, and the evidence manifest.
pub fn issue_payload(repo_root: &Path, id: &str) -> Option<Value> {
    let (records, skipped) = lenient_read_issues(repo_root);
    let issue = records
        .iter()
        .find(|record| record.get("id").and_then(Value::as_str) == Some(id))?;
    let receipts_list = crate::evidence::receipt::list_receipts(repo_root).ok();
    let receipts: Vec<&crate::evidence::receipt::ReceiptEnvelope> = receipts_list
        .as_ref()
        .map(|list| {
            list.receipts
                .iter()
                .filter(|receipt| receipt.subject.id == id)
                .collect()
        })
        .unwrap_or_default();
    let events = crate::event::read_events(repo_root).unwrap_or_default();
    let trace: Vec<&crate::event::EventEnvelope> = events
        .iter()
        .filter(|event| event.subject.id == id)
        .collect();
    Some(json!({
        "issue": issue,
        "receipts": receipts,
        "events": trace,
        "evidence": evidence_manifest(repo_root, id),
        "skipped_lines": skipped,
    }))
}

/// Every file under `.pulse/evidence/<id>/`, as `{path, size}` objects
/// sorted by path. Paths are repo-relative to the evidence dir and are the
/// only strings the browser may echo back to `/p/<pid>/evidence/…`.
pub fn evidence_manifest(repo_root: &Path, id: &str) -> Vec<Value> {
    let mut out = Vec::new();
    let base = repo_root.join(".pulse").join("evidence").join(id);
    walk_evidence(&base, &base, &mut out);
    out.sort_by(|a, b| {
        a.get("path")
            .and_then(Value::as_str)
            .cmp(&b.get("path").and_then(Value::as_str))
    });
    out
}

fn walk_evidence(base: &Path, dir: &Path, out: &mut Vec<Value>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_evidence(base, &path, out);
            continue;
        }
        let Ok(relative) = path.strip_prefix(base) else {
            continue;
        };
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        out.push(json!({
            "path": relative.to_string_lossy(),
            "size": size,
        }));
    }
}

/// Read one evidence file for `GET /p/<pid>/evidence/<rel>`, refusing any
/// path that escapes the evidence dir (canonicalize + prefix check).
/// Returns the bytes and a best-effort content type.
pub fn evidence_file(
    repo_root: &Path,
    id: &str,
    relative: &str,
) -> Option<(Vec<u8>, &'static str)> {
    if relative.is_empty() {
        return None;
    }
    let base = repo_root.join(".pulse").join("evidence").join(id);
    let candidate = base.join(relative);
    let canonical_base = base.canonicalize().ok()?;
    let canonical = candidate.canonicalize().ok()?;
    if !canonical.starts_with(&canonical_base) || !canonical.is_file() {
        return None;
    }
    let bytes = std::fs::read(&canonical).ok()?;
    let kind = content_type(&canonical.to_string_lossy());
    Some((bytes, kind))
}

/// Convenience for tests and the http layer: map a project id to its
/// payload context in one call.
pub fn with_project<T>(workspace: &Path, pid: &str, f: impl FnOnce(&Path) -> T) -> Option<T> {
    resolve_project(workspace, pid).map(|root| f(&root))
}

fn content_type(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if lower.ends_with(".json") {
        "application/json"
    } else {
        // logs, txt, py, rs, toml, mjs… all text to a human eye
        "text/plain; charset=utf-8"
    }
}
