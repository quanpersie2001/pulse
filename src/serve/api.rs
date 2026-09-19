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

/// `GET /api/projects` — registry first, optional workspace scan, fresh on
/// every call.
pub fn projects_payload(registry: Option<&Path>, workspace: Option<&Path>) -> Value {
    json!({ "projects": discovery::discover_merged(registry, workspace) })
}

/// The repo root behind a project id, resolved fresh.
pub fn resolve_project(
    registry: Option<&Path>,
    workspace: Option<&Path>,
    pid: &str,
) -> Option<PathBuf> {
    discovery::discover_merged(registry, workspace)
        .into_iter()
        .find(|entry| entry.id == pid)
        .map(|entry| PathBuf::from(entry.path))
}

/// How many of the newest events the board payload carries for the
/// project-wide activity feed; the full trace rides on the issue payload.
const RECENT_EVENTS: usize = 200;

/// `GET /api/p/<pid>/board` — everything the kanban needs: all records,
/// learnings, the newest events, and the receipt ledger's health (counts
/// only; receipts ride on the per-issue payload).
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
    // `read_events` sorts by ULID id, i.e. chronologically: the tail is
    // the newest.
    let events = crate::event::read_events(repo_root).unwrap_or_default();
    let recent_events = &events[events.len().saturating_sub(RECENT_EVENTS)..];
    json!({
        "issues": records,
        "recent_events": recent_events,
        "skipped_lines": skipped,
        "learnings": learnings,
        "learnings_note": learnings_note,
        "receipt_count": receipt_count,
        "unreadable_receipts": unreadable_receipts,
    })
}

/// `GET /api/p/<pid>/issue/<id>` — the full drawer: the record itself,
/// every receipt naming it, its event trace, its evidence manifest, and
/// the evidence of its hierarchy neighbours (`related_evidence`): a
/// ticket's parent story (where story-scope QA lanes leave screenshots)
/// and a story's tickets. Each related entry names its owning `issue`,
/// the id the browser must use in `/p/<pid>/evidence/<issue>/…`.
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
        "related_evidence": related_evidence(repo_root, &records, issue),
        "skipped_lines": skipped,
    }))
}

fn related_evidence(repo_root: &Path, records: &[Value], issue: &Value) -> Vec<Value> {
    let str_field =
        |record: &Value, key: &str| record.get(key).and_then(Value::as_str).map(str::to_string);
    let related: Vec<String> = match str_field(issue, "kind").as_deref() {
        Some("ticket") => str_field(issue, "story").into_iter().collect(),
        Some("story") => {
            let story = str_field(issue, "id");
            records
                .iter()
                .filter(|record| {
                    str_field(record, "kind").as_deref() == Some("ticket")
                        && str_field(record, "story") == story
                })
                .filter_map(|record| str_field(record, "id"))
                .collect()
        }
        _ => Vec::new(),
    };
    let mut out = Vec::new();
    for owner in related {
        // Ids come from the store, but only well-formed ones may name a
        // directory: never let a record smuggle `..` into a path.
        if owner.is_empty() || !owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            continue;
        }
        for mut entry in evidence_manifest(repo_root, &owner) {
            if let Some(object) = entry.as_object_mut() {
                object.insert("issue".to_string(), Value::String(owner.clone()));
            }
            out.push(entry);
        }
    }
    out
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
        // The manifest path is a URL path segment for the browser
        // (`/p/<pid>/evidence/<rel>`), so it is spelled with `/` on every
        // platform, never the Windows separator.
        out.push(json!({
            "path": relative.to_string_lossy().replace('\\', "/"),
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
pub fn with_project<T>(
    registry: Option<&Path>,
    workspace: Option<&Path>,
    pid: &str,
    f: impl FnOnce(&Path) -> T,
) -> Option<T> {
    resolve_project(registry, workspace, pid).map(|root| f(&root))
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
