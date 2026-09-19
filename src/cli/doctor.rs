//! Thin CLI adapter for `pulse doctor`: render the kernel report, exit
//! non-zero when there is anything to look at.

use std::path::Path;

use crate::kernel::doctor::{self, DoctorReport};
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
        stale_lane_preparations,
        awaiting_commit,
        learning_suspects,
        stale_cites,
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
        println!("  EXPIRED on active tickets — reclaim with `pulse claim <id>` or drop with `pulse release <id>`:");
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

    println!("lane preparations");
    if stale_lane_preparations.is_empty() {
        println!("  every prepared lane was sealed");
    } else {
        println!(
            "  PREPARED BUT NOT SEALED — the lane never ran, or died without writing its output:"
        );
        for stale in stale_lane_preparations {
            println!(
                "    {} {} prepared_at={} by={}",
                stale.id,
                stale.role,
                stale.prepared_at.as_deref().unwrap_or("?"),
                stale.prepared_by.as_deref().unwrap_or("?"),
            );
        }
    }

    println!("awaiting commit");
    if awaiting_commit.is_empty() {
        println!("  every dirty path belongs to a ticket still working");
    } else {
        println!(
            "  DONE TICKETS WHOSE FILES ARE STILL UNCOMMITTED — commit them \
             (`git add -- <path> && git commit`):"
        );
        for pending in awaiting_commit {
            println!(
                "    {} held by {}",
                pending.path,
                pending.held_by.join(", ")
            );
        }
    }

    println!("learnings");
    if learning_suspects.is_empty() && stale_cites.is_empty() {
        println!("  no suspect learning, no stale citation");
    }
    if !learning_suspects.is_empty() {
        // Plan 0025 E3: recall already excludes these; retirement is a human
        // decision, never automatic.
        println!(
            "  SUSPECT (reported misleading more often than helpful; excluded from packets \
             and `pulse verify`) — retire or re-trust by hand:"
        );
        for suspect in learning_suspects {
            println!(
                "    {} [{}] helpful={} not_needed={} misleading={} — \
                 `pulse learn retire <id> --reason …`",
                suspect.id, suspect.status, suspect.helpful, suspect.not_needed, suspect.misleading
            );
        }
    }
    if !stale_cites.is_empty() {
        // Plan 0025 E4: the code moved; the learning needs a re-read, not a
        // retire — nothing is excluded or retired by machine.
        println!(
            "  STALE CITES (the cited lines changed since the learning was added) — \
             re-read the learning by hand:"
        );
        for cite in stale_cites {
            println!(
                "    {} cites {}:{} — `pulse learn retire <id> --reason …` if it no longer holds",
                cite.learning, cite.path, cite.lines
            );
        }
    }
}
