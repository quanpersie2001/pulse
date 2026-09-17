//! Thin CLI adapter for `pulse doctor`: render the kernel report, exit
//! non-zero when there is anything to look at.

use std::path::Path;

use crate::kernel::doctor::{self, DetectorStatus, DoctorReport};
use crate::PulseError;

pub(crate) fn handle(repo_root: &Path, json: bool) -> Result<(), PulseError> {
    let report = doctor::run(repo_root)?;
    let warnings = report.warning_count();
    if json {
        println!(
            "{}",
            serde_json::json!({
                "report": report,
                "warnings": warnings,
            })
        );
    } else {
        render(&report);
        println!();
        if warnings == 0 {
            println!("doctor: clean");
        } else {
            println!("doctor: {warnings} finding(s)");
        }
    }
    if warnings > 0 {
        return Err(PulseError::kernel(
            "doctor_findings",
            format!("{warnings} finding(s) need a look"),
            "each finding above is operator-recoverable state; fix or explicitly ignore it",
        ));
    }
    Ok(())
}

fn render(report: &DoctorReport) {
    let DoctorReport {
        store_torn_lines,
        unreadable_receipts,
        expired_leases,
        orphan_evidence,
        detector_context_threshold,
        host_hooks_installed,
    } = report;

    println!("store");
    if store_torn_lines.is_empty() {
        println!("  clean (every line parses and validates)");
    } else {
        println!("  TORN lines: {store_torn_lines:?} — fix or drop them by hand (the strict store refuses to read until then)");
    }

    println!("receipts");
    if unreadable_receipts.is_empty() {
        println!("  all readable");
    } else {
        println!("  UNREADABLE (reported, never erased — Decision 0017):");
        for path in unreadable_receipts {
            println!("    {path}");
        }
    }

    println!("leases");
    if expired_leases.is_empty() {
        println!("  no active ticket holds an expired lease");
    } else {
        println!("  EXPIRED on active tickets — resume with `pulse run worker <id>` or `pulse release <id>`:");
        for lease in expired_leases {
            println!(
                "    {} actor={} run={} expired_at={}",
                lease.id,
                lease.actor.as_deref().unwrap_or("?"),
                lease.run_id.as_deref().unwrap_or("?"),
                lease.expires_at.as_deref().unwrap_or("?"),
            );
        }
    }

    println!("evidence");
    if orphan_evidence.is_empty() {
        println!("  every directory is named by a receipt");
    } else {
        println!("  ORPHAN (no receipt points here) — keep or delete by hand:");
        for id in orphan_evidence {
            println!("    .pulse/evidence/{id}");
        }
    }

    println!("detector context-threshold (plan 10.4)");
    if !*host_hooks_installed {
        println!("  host hooks not installed — nothing to warn about");
        return;
    }
    match detector_context_threshold {
        DetectorStatus::Exercised => {
            println!("  exercised (a continue round-trip is in the event log)")
        }
        DetectorStatus::MarkerPending => {
            println!("  WARNING: marker present but unconsumed — the host loop stopped mid-cycle; checkpoint and continue");
        }
        DetectorStatus::Unexercised => {
            println!("  WARNING: never exercised — no continue round-trip recorded; the 70% context handoff is untested in this repo");
        }
    }
}
