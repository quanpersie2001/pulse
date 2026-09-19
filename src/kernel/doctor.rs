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
//! * a lane prepared but never sealed — `pulse lane input` recorded a
//!   pre-run snapshot and no `pulse lane seal` consumed it, so the host
//!   either never dispatched the lane or the lane died silently (dogfood
//!   ST-1, F8: an unsealed lane used to leave no trace at all).
//!
//! The doctor owns no mutation and no lock: it reads snapshots.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use chrono::Utc;
use serde::Serialize;
use serde_json::Value;

use crate::error::{PulseError, Result};
use crate::evidence::receipt::list_receipts;
use crate::kernel::profile::fence_ignore;
use crate::kernel::reservation::touches_of;
use crate::kernel::scope;
use crate::store::issues::{self, validate_record};

/// One lane whose pre-run snapshot is still on disk, so nothing sealed it.
#[derive(Debug, Clone, Serialize)]
pub struct StaleLanePreparation {
    pub id: String,
    pub role: String,
    pub prepared_at: Option<String>,
    pub prepared_by: Option<String>,
}

/// One Ticket in `active` whose lease expired.
#[derive(Debug, Clone, Serialize)]
pub struct ExpiredLease {
    pub id: String,
    pub actor: Option<String>,
    pub run_id: Option<String>,
    pub expires_at: Option<String>,
}

/// A dirty path whose every covering ticket is `done` (plan 0025 B6): the
/// work was accepted but its files were never committed. The host's rule
/// is "commit right after `pulse close`"; a path still dirty afterwards is
/// an operator reminder, not a gate.
#[derive(Debug, Clone, Serialize)]
pub struct AwaitingCommit {
    pub path: String,
    /// Every ticket whose `touches` cover the path (all `done`).
    pub held_by: Vec<String>,
}

/// One learning whose `misleading` count has outgrown its `helpful` count
/// (plan 0025 E3): recall already hides it, so it silently stops being
/// useful — the human decides whether to retire it or trust it again.
#[derive(Debug, Clone, Serialize)]
pub struct LearningSuspect {
    pub id: String,
    pub status: String,
    pub helpful: u32,
    pub not_needed: u32,
    pub misleading: u32,
}

/// One learning cite whose hash no longer matches the file on disk
/// (plan 0025 E4).
#[derive(Debug, Clone, Serialize)]
pub struct StaleCite {
    pub learning: String,
    pub path: String,
    pub lines: String,
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
    /// Lanes prepared by `pulse lane input` that no `pulse lane seal`
    /// consumed.
    pub stale_lane_preparations: Vec<StaleLanePreparation>,
    /// Dirty paths only `done` tickets claim (plan 0025 B6).
    pub awaiting_commit: Vec<AwaitingCommit>,
    /// Learnings reported misleading more often than helpful (plan 0025
    /// E3) — recall already excludes them; retirement is the human's call.
    pub learning_suspects: Vec<LearningSuspect>,
    /// Learning code citations whose hash no longer matches the file
    /// (plan 0025 E4) — the learning needs a human re-read, not a retire.
    pub stale_cites: Vec<StaleCite>,
}

impl DoctorReport {
    /// Number of findings a human should look at; `pulse doctor` exits
    /// non-zero when this is > 0, so it can gate a script.
    pub fn warning_count(&self) -> usize {
        self.store_torn_lines.len()
            + self.unreadable_receipts.len()
            + self.expired_leases.len()
            + self.orphan_evidence.len()
            + self.stale_lane_preparations.len()
            + self.awaiting_commit.len()
            + self.learning_suspects.len()
            + self.stale_cites.len()
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

    let stale_lane_preparations = stale_lane_preparations(repo_root);
    let awaiting_commit = awaiting_commit(repo_root, &records);
    // Plan 0025 E3: recall has already silenced a suspect learning — without
    // this check it would vanish without a trace. Never auto-retired: a
    // human decides, with the counts in front of them.
    let learning_suspects = crate::learn::store::list(repo_root)?
        .into_iter()
        .filter(crate::learn::recall::is_suspect)
        .map(|learning| LearningSuspect {
            id: learning.frontmatter.id,
            status: learning.frontmatter.status,
            helpful: learning.frontmatter.usage.helpful,
            not_needed: learning.frontmatter.usage.not_needed,
            misleading: learning.frontmatter.usage.misleading,
        })
        .collect();
    // Plan 0025 E4: a cite whose bytes moved is a learning pointing at code
    // that no longer says what it said. Detection only — recall still
    // includes it (code can change back; the lesson is not automatically
    // wrong), and nothing is retired by machine.
    let stale_cites = crate::learn::store::list(repo_root)?
        .into_iter()
        .flat_map(|learning| {
            let id = learning.frontmatter.id.clone();
            crate::learn::stale_cites(repo_root, &learning)
                .into_iter()
                .map(move |cite| StaleCite {
                    learning: id.clone(),
                    path: cite.path.clone(),
                    lines: cite.lines.clone(),
                })
                .collect::<Vec<StaleCite>>()
        })
        .collect();

    Ok(DoctorReport {
        store_torn_lines: torn,
        unreadable_receipts: unreadable,
        expired_leases,
        orphan_evidence,
        stale_lane_preparations,
        awaiting_commit,
        learning_suspects,
        stale_cites,
    })
}

/// Dirty paths (after `fence_ignore`) that only `done` tickets claim
/// (plan 0025 B6): the ticket closed but nobody committed its files. A
/// path no ticket claims is not a finding — mid-run dirt is normal — and
/// a path an open ticket claims is that ticket's work in progress. A
/// missing or failing git (no repo, no commits) means nothing to report:
/// the doctor reports Pulse state, not environment problems.
fn awaiting_commit(repo_root: &Path, records: &[Value]) -> Vec<AwaitingCommit> {
    let Ok(state) = crate::source::snapshot(repo_root, &fence_ignore(repo_root)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for path in state.dirty_paths {
        let covering: Vec<&Value> = records
            .iter()
            .filter(|record| scope::covers(&touches_of(record), &path))
            .collect();
        let all_done = !covering.is_empty()
            && covering
                .iter()
                .all(|record| record.get("status").and_then(Value::as_str) == Some("done"));
        if !all_done {
            continue;
        }
        out.push(AwaitingCommit {
            path,
            held_by: covering
                .iter()
                .filter_map(|record| record.get("id").and_then(Value::as_str))
                .map(str::to_string)
                .collect(),
        });
    }
    out
}

/// Every `<id>/<role>.snapshot.json` still under `.pulse/runtime/lane/`.
/// A missing or unreadable directory is not a finding: the tree only exists
/// once a lane has been prepared at least once.
fn stale_lane_preparations(repo_root: &Path) -> Vec<StaleLanePreparation> {
    let root = repo_root.join(".pulse/runtime/lane");
    let mut stale = Vec::new();
    let Ok(ids) = fs::read_dir(&root) else {
        return stale;
    };
    for id_entry in ids.flatten() {
        let Some(id) = id_entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Ok(files) = fs::read_dir(id_entry.path()) else {
            continue;
        };
        for file in files.flatten() {
            let name = file.file_name();
            let Some(role) = name
                .to_str()
                .and_then(|name| name.strip_suffix(".snapshot.json"))
            else {
                continue;
            };
            let snapshot: Option<serde_json::Value> = fs::read(file.path())
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok());
            let field = |key: &str| {
                snapshot
                    .as_ref()
                    .and_then(|value| value.get(key))
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
            };
            stale.push(StaleLanePreparation {
                id: id.clone(),
                role: role.to_string(),
                prepared_at: field("at"),
                prepared_by: field("actor"),
            });
        }
    }
    stale.sort_by(|left, right| (&left.id, &left.role).cmp(&(&right.id, &right.role)));
    stale
}
