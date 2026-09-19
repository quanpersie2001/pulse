//! `pulse metrics` (plan 0025 E6): the numbers plan 0022 measured by hand,
//! computed from the repository's own records — event log, receipts, issue
//! store and learnings — instead of a person counting.
//!
//! Purpose: one deterministic read over the truth layer. `--since` narrows
//! the event/receipt window; store and learning counts are always the
//! current state (they have no history in the log beyond events this report
//! counts separately).
//!
//! State touched: none. Read-only over `.pulse/` — no lock, no mutation —
//! and deterministic for the same tree.
//!
//! Invariants:
//!
//! * every number has its definition written on the field, in rustdoc;
//! * a number that cannot be derived from the log is reported as
//!   `not_derivable` with the reason, never invented (plan 0022's seven
//!   hand-written rows map only partially to log data);
//! * `--since` applies to event-derived and receipt-derived counts only.
//!
//! Allowed dependencies: `event`, `evidence::receipt`, `store::issues`,
//! `learn` — the same reads the other kernel modules make. Never the CLI.

use std::collections::BTreeMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::error::Result;
use crate::event::read_events;
use crate::evidence::receipt::list_receipts;
use crate::store::issues::read_all;

/// One metric the plan hand-writes but the log cannot answer, with the
/// reason it stays manual (plan 0025 E6: "số nào không tính được từ log thì
/// KHÔNG bịa").
#[derive(Debug, Clone, Serialize)]
pub struct NotDerivable {
    pub metric: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct LaneVerdicts {
    /// Receipts `kind: "lane"` whose `payload.verdict` is `pass`.
    pub pass: usize,
    /// Same, `fail`.
    pub fail: usize,
    /// Same, `inconclusive` (or any other verdict value).
    pub inconclusive: usize,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct VerifyRuns {
    /// Events `verify.recorded` with `payload.passed == true`.
    pub passed: usize,
    /// Events `verify.recorded` with `payload.passed != true`.
    pub failed: usize,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct PanelCounts {
    /// Events `run.started` with `payload.phase == "reconcile"` — panels
    /// prepared for a round 2.
    pub reconciles: usize,
    /// Findings in reconciled lane receipts, by post-reconciliation status.
    pub findings: FindingStatuses,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct FindingStatuses {
    /// Confirmed by the arbitration — rework.
    pub open: usize,
    /// Panel round 2 lowered it — a doubt already rejected.
    pub unconfirmed: usize,
    /// Machine check or quorum resolved it.
    pub resolved: usize,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct LearningCounts {
    /// Current `status` of every learning file.
    pub candidate: usize,
    pub active: usize,
    pub retired: usize,
    /// Active-or-not learnings reported misleading more often than helpful
    /// (plan 0025 E3) — recall already excludes them.
    pub suspect: usize,
    /// Cites whose pinned hash no longer matches the file (plan 0025 E4),
    /// across all learnings.
    pub stale_cites: usize,
    /// Active learnings with a `check_argv` — what `pulse verify` enforces
    /// (plan 0025 E2).
    pub enforced: usize,
    /// Usage totals across every learning, whatever the status.
    pub usage: UsageTotals,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct UsageTotals {
    pub helpful: u32,
    pub not_needed: u32,
    pub misleading: u32,
}

/// Every number, each with its definition in rustdoc.
#[derive(Debug, Clone, Serialize)]
pub struct MetricsReport {
    /// Tickets whose current status is `done` (store state; `--since` does
    /// not apply).
    pub tickets_done: usize,

    /// Friction notes recorded (since `--since`) on tickets that are `done`
    /// now, divided by [`Self::tickets_done`] — plan 0022's
    /// "friction per done Ticket" row, minus the human judgement of whether
    /// each friction is a Pulse bug. `0` when no ticket is done.
    pub friction_per_ticket_done: f64,

    /// Frictions no learning cites and no dismissal explains
    /// (current state; blocks `close-story`, plan 0025 E1).
    pub friction_unclassified: usize,

    /// Tickets that ever bounced `verifying -> active` (events
    /// `issue.transitioned` with `reason: "rework"`, since `--since`), over
    /// [`Self::tickets_done`]. `0` when no ticket is done.
    pub rework_rate: f64,

    /// Lane receipt verdicts (since `--since`).
    pub lane_verdicts: LaneVerdicts,

    /// Events `receipt.recorded` with `lane_verdict_corrected: true` — a
    /// seal-time rule lowered a lane's `pass` (since `--since`).
    pub lane_verdict_corrected: usize,

    /// `pulse verify` runs (since `--since`).
    pub verify_runs: VerifyRuns,

    /// Review-panel activity (since `--since`).
    pub panel: PanelCounts,

    /// Median minutes from a ticket's first `run.started` (its claim) to its
    /// `issue.transitioned {to: done}`, over tickets with both milestones in
    /// the window. `None` when no ticket has both — absent data is absent,
    /// not zero.
    pub median_claim_to_done_minutes: Option<f64>,

    /// Learning stock and loop health (current state).
    pub learnings: LearningCounts,

    /// Numbers the log cannot answer, with reasons — always present so a
    /// reader knows the report is exhaustive, not silent.
    pub not_derivable: Vec<NotDerivable>,
}

/// # Errors
/// Propagates event-log, receipt, store and learnings read failures: a
/// metrics report is about real data, and an unreadable store is an error
/// rather than a zero.
pub fn compute(repo_root: &Path, since: Option<DateTime<Utc>>) -> Result<MetricsReport> {
    let events = read_events(repo_root)?;
    let events: Vec<_> = match since {
        Some(cut) => events
            .into_iter()
            .filter(|event| event.occurred_at >= cut)
            .collect(),
        None => events,
    };
    let records = read_all(repo_root)?;
    let receipts = list_receipts(repo_root)?.receipts;
    let receipts: Vec<_> = match since {
        Some(cut) => receipts
            .into_iter()
            .filter(|receipt| receipt.recorded_at >= cut)
            .collect(),
        None => receipts,
    };

    // --- tickets ---
    let done_tickets: Vec<&str> = records
        .iter()
        .filter(|record| {
            record.get("kind").and_then(|v| v.as_str()) == Some("ticket")
                && record.get("status").and_then(|v| v.as_str()) == Some("done")
        })
        .filter_map(|record| record.get("id").and_then(|v| v.as_str()))
        .collect();
    let tickets_done = done_tickets.len();
    let done: std::collections::HashSet<&str> = done_tickets.into_iter().collect();

    // --- friction ---
    let friction_events: Vec<&crate::event::EventEnvelope> = events
        .iter()
        .filter(|event| {
            event.event_type == "note.recorded"
                && event.payload.get("kind").and_then(|v| v.as_str()) == Some("friction")
        })
        .collect();
    let friction_on_done = friction_events
        .iter()
        .filter(|event| done.contains(event.subject.id.as_str()))
        .count();
    let friction_unclassified = crate::learn::friction::list(repo_root, None)?
        .into_iter()
        .filter(|friction| friction.state == crate::learn::friction::FrictionState::Unclassified)
        .count();

    // --- rework ---
    let reworked: std::collections::HashSet<&str> = events
        .iter()
        .filter(|event| {
            event.event_type == "issue.transitioned"
                && event.payload.get("reason").and_then(|v| v.as_str()) == Some("rework")
        })
        .map(|event| event.subject.id.as_str())
        .collect();

    // --- lane verdicts (receipts are the verdict store) ---
    let mut lane_verdicts = LaneVerdicts::default();
    let mut panel_findings = FindingStatuses::default();
    for receipt in &receipts {
        if receipt.kind != "lane" {
            continue;
        }
        match receipt.payload.get("verdict").and_then(|v| v.as_str()) {
            Some("pass") => lane_verdicts.pass += 1,
            Some("fail") => lane_verdicts.fail += 1,
            _ => lane_verdicts.inconclusive += 1,
        }
        // Decision 0027: only the reconciled receipt's findings carry the
        // post-arbitration statuses.
        if receipt.payload.get("reconciled").and_then(|v| v.as_bool()) == Some(true) {
            for finding in receipt
                .payload
                .get("findings")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
            {
                match finding.get("status").and_then(|v| v.as_str()) {
                    Some("open") => panel_findings.open += 1,
                    Some("unconfirmed") => panel_findings.unconfirmed += 1,
                    Some("resolved") => panel_findings.resolved += 1,
                    _ => {}
                }
            }
        }
    }
    let lane_verdict_corrected = events
        .iter()
        .filter(|event| {
            event.event_type == "receipt.recorded"
                && event
                    .payload
                    .get("lane_verdict_corrected")
                    .and_then(|v| v.as_bool())
                    == Some(true)
        })
        .count();

    // --- verify runs ---
    let mut verify_runs = VerifyRuns::default();
    for event in events.iter().filter(|e| e.event_type == "verify.recorded") {
        if event.payload.get("passed").and_then(|v| v.as_bool()) == Some(true) {
            verify_runs.passed += 1;
        } else {
            verify_runs.failed += 1;
        }
    }

    // --- panel ---
    let reconciles = events
        .iter()
        .filter(|event| {
            event.event_type == "run.started"
                && event.payload.get("phase").and_then(|v| v.as_str()) == Some("reconcile")
        })
        .count();

    // --- claim -> done cycle time ---
    // First claim per subject: a `run.started` carrying a `run_id` (lane
    // inputs and reconcile preparations emit `run.started` too, with role
    // and input/phase instead).
    let mut first_claim: BTreeMap<&str, DateTime<Utc>> = BTreeMap::new();
    let mut done_at: BTreeMap<&str, DateTime<Utc>> = BTreeMap::new();
    for event in &events {
        if event.event_type == "run.started"
            && event
                .payload
                .get("run_id")
                .and_then(|v| v.as_str())
                .is_some()
        {
            first_claim
                .entry(event.subject.id.as_str())
                .or_insert(event.occurred_at);
        }
        if event.event_type == "issue.transitioned"
            && event.payload.get("to").and_then(|v| v.as_str()) == Some("done")
        {
            done_at.insert(event.subject.id.as_str(), event.occurred_at);
        }
    }
    let mut minutes: Vec<f64> = first_claim
        .iter()
        .filter_map(|(subject, started)| {
            done_at
                .get(*subject)
                .map(|finished| (*finished - *started).num_seconds().max(0) as f64 / 60.0)
        })
        .collect();
    minutes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_claim_to_done_minutes = if minutes.is_empty() {
        None
    } else {
        let mid = minutes.len() / 2;
        Some(if minutes.len() % 2 == 1 {
            minutes[mid]
        } else {
            (minutes[mid - 1] + minutes[mid]) / 2.0
        })
    };

    // --- learnings ---
    let learnings = crate::learn::store::list(repo_root)?;
    let mut counts = LearningCounts::default();
    let mut usage = UsageTotals::default();
    for learning in &learnings {
        match learning.frontmatter.status.as_str() {
            "candidate" => counts.candidate += 1,
            "active" => counts.active += 1,
            "retired" => counts.retired += 1,
            _ => {}
        }
        if crate::learn::recall::is_suspect(learning) {
            counts.suspect += 1;
        }
        counts.stale_cites += crate::learn::stale_cites(repo_root, learning).len();
        if learning.frontmatter.status == "active" && !learning.frontmatter.check_argv.is_empty() {
            counts.enforced += 1;
        }
        usage.helpful += learning.frontmatter.usage.helpful;
        usage.not_needed += learning.frontmatter.usage.not_needed;
        usage.misleading += learning.frontmatter.usage.misleading;
    }
    counts.usage = usage;

    // --- what the log cannot answer (plan 0022's hand-measured rows) ---
    let not_derivable = vec![
        NotDerivable {
            metric: "claim_conflicts".to_string(),
            reason: "a refused claim writes no event, so conflicts leave no trace; counting \
                     them needs a failure event, which this session deliberately does not add"
                .to_string(),
        },
        NotDerivable {
            metric: "rust_lines_src".to_string(),
            reason: "counts the development repo's own source tree (plan 0022's `find src | wc \
                     -l`); no target-repo record encodes it"
                .to_string(),
        },
        NotDerivable {
            metric: "error_codes_distinct".to_string(),
            reason: "a static property of the Pulse binary (grep over source), not log data"
                .to_string(),
        },
        NotDerivable {
            metric: "cli_leaf_commands".to_string(),
            reason: "a static property of the Pulse binary (recurse --help), not log data"
                .to_string(),
        },
        NotDerivable {
            metric: "hand_typed_commands_to_close_ticket".to_string(),
            reason: "plan 0022 measures this qualitatively against a golden path; the log has \
                     no event for 'an operator typed a command'"
                .to_string(),
        },
        NotDerivable {
            metric: "required_flags_on_close_path".to_string(),
            reason: "plan 0022 measures this qualitatively against a golden path; not log data"
                .to_string(),
        },
        NotDerivable {
            metric: "repos_running_pulse".to_string(),
            reason: "the registry (~/.pulse/projects.json) is cross-project and outside any \
                     single repo's event log"
                .to_string(),
        },
    ];

    Ok(MetricsReport {
        tickets_done,
        friction_per_ticket_done: divide(friction_on_done, tickets_done),
        friction_unclassified,
        rework_rate: divide(reworked.len(), tickets_done),
        lane_verdicts,
        lane_verdict_corrected,
        verify_runs,
        panel: PanelCounts {
            reconciles,
            findings: panel_findings,
        },
        median_claim_to_done_minutes,
        learnings: counts,
        not_derivable,
    })
}

fn divide(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::emit_event;
    use crate::evidence::receipt::{record_receipt, NewReceipt, ReceiptSource, ReceiptSubject};
    use serde_json::{json, Value};
    use std::path::Path;

    /// One done ticket, one still-active ticket, all events at fixed times.
    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::UNIX_EPOCH + chrono::Duration::seconds(secs)
    }

    fn seed(path: &Path) {
        crate::store::issues::mutate(path, |mut records| {
            records.push(json!({
                "schema": 3, "id": "TK-a3f9", "kind": "ticket", "title": "t",
                "status": "done", "revision": 3,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "role": "implementation",
            }));
            records.push(json!({
                "schema": 3, "id": "TK-b7c2", "kind": "ticket", "title": "t",
                "status": "done", "revision": 3,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "role": "implementation",
            }));
            records.push(json!({
                "schema": 3, "id": "TK-4e11", "kind": "ticket", "title": "t",
                "status": "active", "revision": 1,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "role": "implementation",
            }));
            Ok(records)
        })
        .unwrap();
    }

    fn friction_note(path: &Path, subject: &str, when: DateTime<Utc>) {
        emit_event(
            path,
            "note.recorded",
            "agent:worker",
            subject,
            json!({"kind": "friction", "text": "f"}),
            when,
        )
        .unwrap();
    }

    fn transition(path: &Path, subject: &str, to: &str, reason: Option<&str>, when: DateTime<Utc>) {
        let mut payload = json!({"to": to});
        if let Some(reason) = reason {
            payload["reason"] = json!(reason);
        }
        emit_event(
            path,
            "issue.transitioned",
            "agent:worker",
            subject,
            payload,
            when,
        )
        .unwrap();
    }

    fn lane_receipt(path: &Path, verdict: &str, reconciled: bool, findings: Value) {
        record_receipt(
            path,
            None,
            NewReceipt {
                kind: "lane".to_string(),
                subject: ReceiptSubject {
                    id: "ST-1111".to_string(),
                    revision: None,
                },
                actor: "agent:review-correctness".to_string(),
                source: ReceiptSource {
                    commit: "c".to_string(),
                    dirty_hash: "d".to_string(),
                },
                run_id: None,
                payload: json!({
                    "verdict": verdict,
                    "reconciled": reconciled,
                    "findings": findings,
                }),
                artifact_paths: Vec::new(),
            },
        )
        .unwrap();
    }

    #[test]
    fn tickets_done_and_friction_per_ticket_done_follow_their_definitions() {
        let repo = tempfile::tempdir().unwrap();
        seed(repo.path());
        // Three frictions on done tickets, one on a ticket that is not done:
        // the ratio is 3 / 2, not 4 / 3.
        friction_note(repo.path(), "TK-a3f9", at(100));
        friction_note(repo.path(), "TK-a3f9", at(101));
        friction_note(repo.path(), "TK-b7c2", at(102));
        friction_note(repo.path(), "TK-4e11", at(103));
        let report = compute(repo.path(), None).unwrap();
        assert_eq!(report.tickets_done, 2);
        assert_eq!(report.friction_per_ticket_done, 1.5);
    }

    #[test]
    fn friction_unclassified_counts_what_no_learning_cites_and_no_dismissal_explains() {
        let repo = tempfile::tempdir().unwrap();
        seed(repo.path());
        friction_note(repo.path(), "TK-a3f9", at(100));
        friction_note(repo.path(), "TK-b7c2", at(101));
        assert_eq!(compute(repo.path(), None).unwrap().friction_unclassified, 2);

        // A dismissal is a classification: the count drops with it.
        crate::learn::dismiss(
            repo.path(),
            &crate::identity::actor::ActorRef {
                kind: crate::identity::actor::ActorKind::Human,
                id: "quan".to_string(),
            },
            "TK-a3f9",
            &[],
            true,
            "ticket-specific",
        )
        .unwrap();
        assert_eq!(compute(repo.path(), None).unwrap().friction_unclassified, 1);
    }

    #[test]
    fn rework_rate_is_reworked_tickets_over_done_tickets() {
        let repo = tempfile::tempdir().unwrap();
        seed(repo.path());
        // Numerator = DISTINCT tickets that ever bounced (the open one
        // bounced twice but counts once): 2. Denominator = done tickets: 2.
        transition(repo.path(), "TK-a3f9", "active", Some("rework"), at(100));
        transition(repo.path(), "TK-4e11", "active", Some("rework"), at(101));
        transition(repo.path(), "TK-4e11", "active", Some("rework"), at(102));
        let report = compute(repo.path(), None).unwrap();
        assert_eq!(report.rework_rate, 1.0);
        assert!(
            report.rework_rate <= 1.0,
            "the rate can touch 1.0 but here stays a ratio of distinct tickets"
        );
    }

    #[test]
    fn lane_verdicts_and_corrections_come_from_receipts_and_events() {
        let repo = tempfile::tempdir().unwrap();
        lane_receipt(repo.path(), "pass", false, json!([]));
        lane_receipt(repo.path(), "fail", false, json!([]));
        lane_receipt(repo.path(), "pass", true, json!([{ "status": "resolved" }]));
        emit_event(
            repo.path(),
            "receipt.recorded",
            "agent:review-correctness",
            "TK-a3f9",
            json!({"lane_verdict_corrected": true, "role": "review-correctness"}),
            at(100),
        )
        .unwrap();
        let report = compute(repo.path(), None).unwrap();
        assert_eq!(report.lane_verdicts.pass, 2);
        assert_eq!(report.lane_verdicts.fail, 1);
        assert_eq!(report.lane_verdicts.inconclusive, 0);
        assert_eq!(report.lane_verdict_corrected, 1);
        // Only the reconciled receipt's findings are counted.
        assert_eq!(report.panel.findings.resolved, 1);
        assert_eq!(report.panel.findings.open, 0);
    }

    #[test]
    fn verify_runs_split_on_the_recorded_pass_flag() {
        let repo = tempfile::tempdir().unwrap();
        for (subject, passed) in [("TK-a3f9", true), ("TK-b7c2", false), ("TK-4e11", false)] {
            emit_event(
                repo.path(),
                "verify.recorded",
                "agent:worker",
                subject,
                json!({"passed": passed}),
                at(100),
            )
            .unwrap();
        }
        let report = compute(repo.path(), None).unwrap();
        assert_eq!(report.verify_runs.passed, 1);
        assert_eq!(report.verify_runs.failed, 2);
    }

    #[test]
    fn panel_counts_reconciles_and_finding_statuses() {
        let repo = tempfile::tempdir().unwrap();
        for _ in 0..2 {
            emit_event(
                repo.path(),
                "run.started",
                "agent:review-correctness",
                "TK-a3f9",
                json!({"role": "review-correctness", "phase": "reconcile"}),
                at(100),
            )
            .unwrap();
        }
        lane_receipt(
            repo.path(),
            "pass",
            true,
            json!([
                {"id": "RF-1", "status": "open"},
                {"id": "RF-2", "status": "unconfirmed"},
                {"id": "RF-3", "status": "unconfirmed"},
                {"id": "RF-4", "status": "resolved"},
            ]),
        );
        let report = compute(repo.path(), None).unwrap();
        assert_eq!(report.panel.reconciles, 2);
        assert_eq!(report.panel.findings.open, 1);
        assert_eq!(report.panel.findings.unconfirmed, 2);
        assert_eq!(report.panel.findings.resolved, 1);
    }

    #[test]
    fn median_claim_to_done_skips_tickets_missing_a_milestone() {
        let repo = tempfile::tempdir().unwrap();
        seed(repo.path());
        // TK-done1: claim at 0, done at 1800s (30 min).
        emit_event(
            repo.path(),
            "run.started",
            "agent:worker",
            "TK-a3f9",
            json!({"run_id": "run_1", "role": "worker"}),
            at(0),
        )
        .unwrap();
        transition(repo.path(), "TK-a3f9", "done", None, at(1800));
        // TK-done2: claim at 600s, done at 2400s (30 min again).
        emit_event(
            repo.path(),
            "run.started",
            "agent:worker",
            "TK-b7c2",
            json!({"run_id": "run_2", "role": "worker"}),
            at(600),
        )
        .unwrap();
        transition(repo.path(), "TK-b7c2", "done", None, at(2400));
        // TK-open has a claim but no done milestone — excluded.
        emit_event(
            repo.path(),
            "run.started",
            "agent:worker",
            "TK-4e11",
            json!({"run_id": "run_3", "role": "worker"}),
            at(700),
        )
        .unwrap();

        let report = compute(repo.path(), None).unwrap();
        assert_eq!(report.median_claim_to_done_minutes, Some(30.0));

        // --since after the first claim leaves exactly one complete pair.
        let report = compute(repo.path(), Some(at(500))).unwrap();
        assert_eq!(report.median_claim_to_done_minutes, Some(30.0));
        // --since after every claim leaves no pair at all: n/a, not zero.
        let report = compute(repo.path(), Some(at(2000))).unwrap();
        assert_eq!(report.median_claim_to_done_minutes, None);
    }

    #[test]
    fn learnings_counts_reflect_files_suspects_stales_and_enforcement() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(repo.path().join("code.rs"), "a\nb\nc\n").unwrap();
        for (id, status, helpful, misleading, argv) in [
            ("LRN-0001", "candidate", 0u32, 0u32, Vec::<String>::new()),
            ("LRN-0002", "active", 3, 1, vec!["true".to_string()]),
            ("LRN-0003", "active", 1, 4, Vec::<String>::new()),
            ("LRN-0004", "retired", 0, 0, Vec::<String>::new()),
        ] {
            crate::learn::store::write(
                repo.path(),
                &crate::learn::store::Learning {
                    frontmatter: crate::learn::store::Frontmatter {
                        id: id.to_string(),
                        status: status.to_string(),
                        kind: "failure".to_string(),
                        applies_to: vec![],
                        tags: vec![],
                        from: vec![],
                        expected_signal: String::new(),
                        usage: crate::learn::store::UsageCounts {
                            helpful,
                            misleading,
                            ..Default::default()
                        },
                        check_argv: argv,
                        check_cwd: None,
                        cites: vec![crate::learn::store::Cite {
                            path: "code.rs".to_string(),
                            lines: "1-2".to_string(),
                            sha256: if id == "LRN-0002" {
                                "sha256:mismatch".to_string()
                            } else {
                                crate::canonical_json::hash_bytes(b"a\nb")
                            },
                        }],
                    },
                    body: "## Summary\ns\n".to_string(),
                },
            )
            .unwrap();
        }
        let report = compute(repo.path(), None).unwrap();
        let learnings = &report.learnings;
        assert_eq!(learnings.candidate, 1);
        assert_eq!(learnings.active, 2);
        assert_eq!(learnings.retired, 1);
        assert_eq!(
            learnings.suspect, 1,
            "only LRN-0003 is misleading > helpful"
        );
        assert_eq!(
            learnings.enforced, 1,
            "only the active LRN-0002 has an argv"
        );
        assert_eq!(
            learnings.stale_cites, 1,
            "LRN-0002's hash no longer matches"
        );
        assert_eq!(learnings.usage.helpful, 4);
        assert_eq!(learnings.usage.misleading, 5);
    }

    #[test]
    fn not_derivable_lists_what_the_log_cannot_answer() {
        let repo = tempfile::tempdir().unwrap();
        let report = compute(repo.path(), None).unwrap();
        let names: Vec<&str> = report
            .not_derivable
            .iter()
            .map(|entry| entry.metric.as_str())
            .collect();
        assert!(names.contains(&"claim_conflicts"));
        assert!(names.contains(&"rust_lines_src"));
        for entry in &report.not_derivable {
            assert!(!entry.reason.is_empty());
        }
    }

    #[test]
    fn an_empty_repository_reports_zeros_and_no_median() {
        let repo = tempfile::tempdir().unwrap();
        let report = compute(repo.path(), None).unwrap();
        assert_eq!(report.tickets_done, 0);
        assert_eq!(report.friction_per_ticket_done, 0.0);
        assert_eq!(report.rework_rate, 0.0);
        assert_eq!(report.median_claim_to_done_minutes, None);
    }
}
