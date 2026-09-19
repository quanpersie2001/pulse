use std::path::Path;

use crate::cli::output::render;
use crate::kernel::init::{
    initialize_repository_ext, RefreshAction, RefreshTake, RepositoryInitStatus,
};
use crate::PulseError;

pub(crate) fn handle(
    repo_root: &Path,
    refresh: bool,
    no_register: bool,
    take_new: Option<String>,
    keep_mine: Option<String>,
    with_qa_templates: bool,
    json: bool,
) -> Result<(), PulseError> {
    let resolve: Option<(&str, RefreshTake)> = match (&take_new, &keep_mine) {
        (Some(file), _) => Some((file.as_str(), RefreshTake::New)),
        (None, Some(file)) => Some((file.as_str(), RefreshTake::Mine)),
        (None, None) => None,
    };
    let report = initialize_repository_ext(repo_root, refresh, with_qa_templates, resolve)?;
    // Registration is best-effort: a failure to write the user-level
    // registry never fails the repo-local init (Decision 0023).
    let mut register_note = String::new();
    if !no_register {
        match crate::serve::registry::register(repo_root) {
            Ok(()) => register_note = "\nregistered in the user project registry".to_string(),
            Err(error) => {
                register_note = format!("\nregistry registration failed: {error}");
            }
        }
    }
    let status = match report.status {
        RepositoryInitStatus::Initialized => "initialized",
        RepositoryInitStatus::Unchanged => "already initialized",
    };
    let mut human = if report.created.is_empty() {
        format!("repository {status}")
    } else {
        format!("repository {status}: created {}", report.created.join(", "))
    };
    if !report.skipped.is_empty() {
        human.push_str(&format!(
            "\nskipped (already present): {}",
            report.skipped.join(", ")
        ));
    }
    // One line per refreshed file (plan 0025 G2); a conflict still exits 0
    // — it was handled safely — but the last line says how many and where.
    let mut conflicts = 0_usize;
    for file in &report.refreshed {
        human.push_str(&format!("\n{}: {}", file.file, label(file.action)));
        if let Some(note) = &file.note {
            human.push_str(&format!(" — {note}"));
        }
        if file.action == RefreshAction::Conflict {
            conflicts += 1;
        }
    }
    if conflicts > 0 {
        human.push_str(&format!(
            "\n{conflicts} conflict(s); your files are untouched — see \
             .pulse/runtime/refresh/ and resolve with `pulse init --refresh \
             --take-new <file>` or `--keep-mine <file>`"
        ));
    }
    // One hint line, plan 0025 G1: reservations bind only when a host hook
    // calls the gate. The configuration itself is printed on demand —
    // `pulse init` never writes into a host's settings.
    human.push_str(
        "\n`pulse hook snippet <host>` prints the PreToolUse config that makes reservations binding",
    );
    human.push_str(&register_note);
    render(json, &report, human)
}

fn label(action: RefreshAction) -> &'static str {
    match action {
        RefreshAction::Created => "created",
        RefreshAction::Unchanged => "unchanged",
        RefreshAction::Updated => "updated",
        RefreshAction::Merged => "merged",
        RefreshAction::Conflict => "CONFLICT",
        RefreshAction::Kept => "kept",
    }
}
