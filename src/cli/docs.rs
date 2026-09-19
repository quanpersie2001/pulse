//! `pulse docs applicable|check` (plan 0022 §12.2). Thin renderer: calls
//! `pulse::docs`, renders the result. `check`'s `--write <path>` mode is the
//! one command in this crate that also controls its own exit code directly
//! (plan: "Exit 1 khi có finding") — everywhere else exit codes come only
//! from `Result<(), PulseError>` via `src/bin/pulse.rs`.

use std::io::Write;
use std::path::{Path, PathBuf};

use clap::Subcommand;
use serde_json::json;

use crate::cli::output::render;
use crate::docs;
use crate::PulseError;

#[derive(Subcommand)]
pub(crate) enum DocsCommand {
    /// Docs matching `<id>`'s anchors/tags (plan §12.2), same rule as
    /// `pulse learn applicable`.
    Applicable {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Broken internal links, missing `docs/README.md` paths, and stale
    /// `generated_by.check_argv`. Exits 1 if the verdict is `fail`, unless
    /// `--write` is given (a lane role never signals failure by exit code —
    /// only through its output's `verdict`). With `--ticket <id>`, the
    /// report also carries one low-severity finding per doc the ticket's
    /// edits may have staled (plan 0025 F3); low findings never flip the
    /// verdict, so they never flip the exit code either.
    Check {
        #[arg(long)]
        json: bool,
        /// Advise for this ticket: which docs describing changed code the
        /// ticket did not update. Unknown ids are refused.
        #[arg(long)]
        ticket: Option<String>,
        /// Write the lane-shape report here instead of stdout, and print
        /// `{"status":"done"}` — this IS the `check-docs` lane, with no
        /// agent and no wrapper script: `pulse docs check --ticket <id>
        /// --write .pulse/evidence/<id>/check-docs.json`, then `pulse lane
        /// seal <id> check-docs`.
        #[arg(long)]
        write: Option<PathBuf>,
    },
}

pub(crate) fn handle(repo_root: &Path, command: DocsCommand) -> Result<(), PulseError> {
    match command {
        DocsCommand::Applicable { id, json } => {
            let matched = docs::applicable::applicable(repo_root, &id)?;
            let values: Vec<serde_json::Value> = matched
                .iter()
                .map(|m| json!({"path": m.path, "why": m.why, "lines": m.lines}))
                .collect();
            let human = matched
                .iter()
                .map(|m| format!("{} ({} lines) — {}", m.path, m.lines, m.why))
                .collect::<Vec<_>>()
                .join("\n");
            render(json, &values, human)
        }
        DocsCommand::Check {
            json,
            ticket,
            write,
        } => handle_check(repo_root, json, ticket.as_deref(), write.as_deref()),
    }
}

fn handle_check(
    repo_root: &Path,
    json: bool,
    ticket: Option<&str>,
    write: Option<&Path>,
) -> Result<(), PulseError> {
    // Plan 0025 F3: `--ticket` computes the stale-doc advisory (which docs
    // describing changed code the ticket did not update) and rides it into
    // the report as low findings. `pulse docs check --ticket <id> --write
    // <path>` is the whole check-docs lane for one ticket.
    let advisory = match ticket {
        None => Vec::new(),
        Some(id) => {
            let records = crate::store::issues::read_all(repo_root)?;
            let record = crate::kernel::issues::require(&records, id)?;
            crate::docs::stale::for_ticket(repo_root, record)?
        }
    };
    let report = docs::check::check_with(repo_root, ticket, &advisory)?;

    if let Some(path) = write {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
        }
        let bytes = serde_json::to_vec_pretty(&report)?;
        std::fs::write(path, bytes).map_err(|error| PulseError::io(path, error))?;
        println!("{}", json!({"status": "done"}));
        return Ok(());
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|e| PulseError::validation("json_serialize_error", e.to_string()))?
        );
    } else {
        println!("verdict: {}", report.verdict);
        for finding in &report.findings {
            println!("  [{}] {}: {}", finding.id, finding.owner, finding.summary);
        }
    }
    std::io::stdout().flush().ok();

    // Exit 1 exactly when the verdict is fail (plan 0025 F3): for the plain
    // report that is the same "any finding" rule as before — every
    // structural finding is medium — while an advisory-only run (low
    // findings from `--ticket`) stays exit 0, like the verdict it cannot
    // flip.
    if report.verdict == "fail" {
        std::process::exit(1);
    }
    Ok(())
}
