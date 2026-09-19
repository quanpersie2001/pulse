//! Thin CLI adapter for `pulse metrics` (plan 0025 E6): parse `--since`,
//! call the kernel's read-only report, render it. The definitions of the
//! numbers live on the fields of [`pulse::kernel::metrics::MetricsReport`],
//! not here.

use std::path::Path;

use crate::cli::output::render;
use crate::kernel::metrics;
use crate::PulseError;

pub(crate) fn handle(repo_root: &Path, since: Option<&str>, json: bool) -> Result<(), PulseError> {
    let cut = match since {
        None => None,
        Some(raw) => {
            if let Ok(at) = chrono::DateTime::parse_from_rfc3339(raw) {
                Some(at.with_timezone(&chrono::Utc))
            } else if let Ok(day) = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
                Some(day.and_hms_opt(0, 0, 0).unwrap_or_default().and_utc())
            } else {
                return Err(PulseError::kernel(
                    "metrics_since_invalid",
                    format!("--since {raw:?} is not an RFC3339 timestamp or a YYYY-MM-DD date"),
                    "pass e.g. 2026-09-18 or 2026-09-18T12:00:00Z",
                ));
            }
        }
    };
    let report = metrics::compute(repo_root, cut)?;
    render(json, &report, human(&report))
}

/// The two-column text table: `name  value`, one line per top-level number.
fn human(report: &metrics::MetricsReport) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let line = |out: &mut String, name: &str, value: String| {
        let _ = writeln!(out, "{name:<34} {value}");
    };
    line(&mut out, "tickets_done", report.tickets_done.to_string());
    line(
        &mut out,
        "friction_per_ticket_done",
        format!("{:.2}", report.friction_per_ticket_done),
    );
    line(
        &mut out,
        "friction_unclassified",
        report.friction_unclassified.to_string(),
    );
    line(
        &mut out,
        "rework_rate",
        format!("{:.2}", report.rework_rate),
    );
    line(
        &mut out,
        "lane_verdicts pass/fail/inconclusive",
        format!(
            "{}/{}/{}",
            report.lane_verdicts.pass, report.lane_verdicts.fail, report.lane_verdicts.inconclusive
        ),
    );
    line(
        &mut out,
        "lane_verdict_corrected",
        report.lane_verdict_corrected.to_string(),
    );
    line(
        &mut out,
        "verify_runs passed/failed",
        format!(
            "{}/{}",
            report.verify_runs.passed, report.verify_runs.failed
        ),
    );
    line(
        &mut out,
        "panel_reconciles",
        report.panel.reconciles.to_string(),
    );
    line(
        &mut out,
        "panel_findings open/unconfirmed/resolved",
        format!(
            "{}/{}/{}",
            report.panel.findings.open,
            report.panel.findings.unconfirmed,
            report.panel.findings.resolved
        ),
    );
    line(
        &mut out,
        "median_claim_to_done_minutes",
        report
            .median_claim_to_done_minutes
            .map(|minutes| format!("{minutes:.1}"))
            .unwrap_or_else(|| "n/a".to_string()),
    );
    line(
        &mut out,
        "learnings candidate/active/retired",
        format!(
            "{}/{}/{}",
            report.learnings.candidate, report.learnings.active, report.learnings.retired
        ),
    );
    line(
        &mut out,
        "learnings suspect",
        report.learnings.suspect.to_string(),
    );
    line(
        &mut out,
        "learnings stale_cites",
        report.learnings.stale_cites.to_string(),
    );
    line(
        &mut out,
        "learnings enforced",
        report.learnings.enforced.to_string(),
    );
    line(
        &mut out,
        "usage helpful/not_needed/misleading",
        format!(
            "{}/{}/{}",
            report.learnings.usage.helpful,
            report.learnings.usage.not_needed,
            report.learnings.usage.misleading
        ),
    );
    for entry in &report.not_derivable {
        let _ = writeln!(out, "not_derivable: {} — {}", entry.metric, entry.reason);
    }
    out
}
