//! `pulse serve` (Decision 0023): a read-only local HTTP server over one
//! workspace's Pulse projects.
//!
//! Invariants: the server never writes, never locks and never mutates —
//! every request re-reads files from disk and rescans the workspace, so a
//! concurrent CLI mutation is just the next request's data. JSONL reads
//! are lenient: a malformed line (a torn tail mid-append, say) is skipped
//! and counted, never a 500. Project ids are sha256 prefixes of canonical
//! paths, so paths never appear in URLs.
//!
//! Allowed dependencies: `store`, `event`, `evidence`, `learn`, `storage`
//! (reads only). Never `kernel` or `cli`.

pub mod api;
pub mod discovery;
pub mod http;

use serde_json::Value;
use std::path::Path;

/// Read `.pulse/issues.jsonl` leniently: valid lines are parsed, empty and
/// `#` lines are ignored, and any malformed line is counted in `skipped`
/// instead of failing the read (the CLI's strict reader stays strict).
pub fn lenient_read_issues(repo_root: &Path) -> (Vec<Value>, usize) {
    let path = repo_root.join(".pulse").join("issues.jsonl");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return (Vec::new(), 0);
    };
    let mut records = Vec::new();
    let mut skipped = 0usize;
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match serde_json::from_str(line) {
            Ok(value) => records.push(value),
            Err(_) => skipped += 1,
        }
    }
    (records, skipped)
}
