//! `pulse doctor` (plan 0022 §11.3 minimum + store errors) — one read-only
//! pass over a repo's Pulse state that reports what a human would otherwise
//! discover by opening files by hand.
//!
//! Checks (Decision: plan §11.3 evidence ladder, minimum surface):
//!
//! * the store parses clean — torn/duplicated lines are counted, not fatal
//!   (the store itself stays strict; the doctor is the lenient reader);
//! * receipts unreadable — reported, never erased (Decision 0017);
//! * a Ticket in `active` whose lease expired — the run loop keeps the
//!   lease on `run_inconclusive`, so a stale one is operator-recoverable
//!   state (`pulse release`), but invisible without this check;
//! * evidence directories no receipt points at (orphans — likely leftovers
//!   of a manual experiment; reported for a human decision, never deleted);
//! * the context-threshold detector (plan §10.4): its marker is transient
//!   (fired → consumed → deleted), so "exercised" is inferred from a
//!   completed `continue` round-trip in the event log; a fired-but-
//!   unconsumed marker and an installed-but-never-exercised pair are the
//!   two warnings (ST-1 left the mechanism `unexercised`).
//!
//! The doctor owns no mutation and no lock: it reads snapshots.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use chrono::Utc;
use serde::Serialize;

use crate::error::{PulseError, Result};
use crate::event::read_event_log;
use crate::evidence::receipt::list_receipts;
use crate::store::issues::{self, validate_record};

/// Whether the §10.4 context-threshold detector ever completed a cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectorStatus {
    /// A `continue` round-trip exists in the event log.
    Exercised,
    /// The marker exists right now — fired, but the host loop stopped
    /// before consuming it.
    MarkerPending,
    /// Installed, marker absent, and no `continue` round-trip recorded.
    Unexercised,
}

/// One Ticket in `active` whose lease expired.
#[derive(Debug, Clone, Serialize)]
pub struct ExpiredLease {
    pub id: String,
    pub actor: Option<String>,
    pub run_id: Option<String>,
    pub expires_at: Option<String>,
}

/// The full read-only health report.
#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    /// 1-based line numbers of `.pulse/issues.jsonl` that fail parse or
    /// schema validation.
    pub store_torn_lines: Vec<usize>,
    /// Receipt files that could not be read (Decision 0017).
    pub unreadable_receipts: Vec<String>,
    /// `active` Tickets whose lease expired.
    pub expired_leases: Vec<ExpiredLease>,
    /// Evidence directories no receipt names.
    pub orphan_evidence: Vec<String>,
    pub detector_context_threshold: DetectorStatus,
    /// Both §10.4 host hook scripts are installed.
    pub host_hooks_installed: bool,
}

impl DoctorReport {
    /// Number of findings a human should look at; `pulse doctor` exits
    /// non-zero when this is > 0, so it can gate a script.
    pub fn warning_count(&self) -> usize {
        let detector_warns = self.host_hooks_installed
            && self.detector_context_threshold != DetectorStatus::Exercised;
        self.store_torn_lines.len()
            + self.unreadable_receipts.len()
            + self.expired_leases.len()
            + self.orphan_evidence.len()
            + usize::from(detector_warns)
    }
}

/// Run every check against `repo_root`.
///
/// # Errors
/// Propagates I/O errors only for state a doctor cannot report around: a
/// missing store or an unreadable `.pulse/` tree is an environment problem,
/// not a finding.
pub fn run(repo_root: &Path) -> Result<DoctorReport> {
    let store_path = issues::issues_path(repo_root);
    let store_text =
        fs::read_to_string(&store_path).map_err(|error| PulseError::io(store_path, error))?;

    // Lenient pass over the store: collect the records that parse AND
    // validate, count the lines that do not. The strict store refuses the
    // whole read on one bad line; the doctor must see past it.
    let mut torn = Vec::new();
    let mut records = Vec::new();
    for (index, line) in store_text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(line)
            .map_err(|e| e.to_string())
            .and_then(|record| {
                validate_record(&record)
                    .map(|_| record)
                    .map_err(|e| e.to_string())
            }) {
            Ok(record) => records.push(record),
            Err(_) => torn.push(index + 1),
        }
    }

    let receipts = list_receipts(repo_root)?;
    let unreadable = receipts
        .unreadable
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();

    let now = Utc::now();
    let expired_leases = records
        .iter()
        .filter(|record| {
            record.get("kind").and_then(|v| v.as_str()) == Some("ticket")
                && record.get("status").and_then(|v| v.as_str()) == Some("active")
                && record
                    .pointer("/lease/actor")
                    .and_then(|v| v.as_str())
                    .is_some()
        })
        .filter(|record| {
            record
                .pointer("/lease/expires_at")
                .and_then(|v| v.as_str())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .is_some_and(|expires| expires <= now)
        })
        .map(|record| ExpiredLease {
            id: record
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string(),
            actor: record
                .pointer("/lease/actor")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            run_id: record
                .pointer("/lease/run_id")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            expires_at: record
                .pointer("/lease/expires_at")
                .and_then(|v| v.as_str())
                .map(str::to_string),
        })
        .collect::<Vec<_>>();

    let named: HashSet<String> = receipts
        .receipts
        .iter()
        .map(|receipt| receipt.subject.id.clone())
        .collect();
    let evidence_root = repo_root.join(".pulse/evidence");
    let mut orphan_evidence = Vec::new();
    // No evidence directory at all reads as nothing orphaned.
    if let Ok(entries) = fs::read_dir(&evidence_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(id) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !named.contains(id) {
                orphan_evidence.push(id.to_string());
            }
        }
        orphan_evidence.sort();
    }

    let host_hooks_installed = ["statusline.sh", "post-tool-use.sh"].iter().all(|name| {
        repo_root
            .join(".pulse/hosts/claude-code")
            .join(name)
            .is_file()
    });
    let marker = repo_root.join(".pulse/runtime/context-threshold");
    let detector_context_threshold = if marker.exists() {
        DetectorStatus::MarkerPending
    } else {
        let continued = read_event_log(repo_root)?.events.iter().any(|event| {
            event.event_type == "run.completed"
                && event.payload.get("outcome").and_then(|v| v.as_str()) == Some("continue")
        });
        if continued {
            DetectorStatus::Exercised
        } else {
            DetectorStatus::Unexercised
        }
    };

    Ok(DoctorReport {
        store_torn_lines: torn,
        unreadable_receipts: unreadable,
        expired_leases,
        orphan_evidence,
        detector_context_threshold,
        host_hooks_installed,
    })
}
