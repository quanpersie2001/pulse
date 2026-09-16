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
    /// `generated_by.check_argv`. Exits 1 if any finding is found, unless
    /// `--write` is given (a lane role never signals failure by exit code —
    /// only through its output's `verdict`).
    Check {
        #[arg(long)]
        json: bool,
        /// Write the lane-shape report here instead of stdout, and print
        /// `{"status":"done"}` — wires directly as `runners.json`'s
        /// `check-docs` role with no wrapper script, e.g.
        /// `pulse docs check --write {artifact_dir}/check-docs.json`.
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
        DocsCommand::Check { json, write } => handle_check(repo_root, json, write.as_deref()),
    }
}

fn handle_check(repo_root: &Path, json: bool, write: Option<&Path>) -> Result<(), PulseError> {
    let report = docs::check::check(repo_root)?;

    if let Some(path) = write {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
        }
        let bytes = serde_json::to_vec_pretty(&report)?;
        std::fs::write(path, bytes).map_err(|error| PulseError::io(path, error))?;
        println!("{}", json!({"status": "done"}));
        return Ok(());
    }

    let has_findings = !report.findings.is_empty();
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

    if has_findings {
        std::process::exit(1);
    }
    Ok(())
}
