use std::path::Path;

use crate::cli::output::render;
use crate::kernel::init::{initialize_repository, RepositoryInitStatus};
use crate::PulseError;

pub(crate) fn handle(
    repo_root: &Path,
    refresh: bool,
    no_register: bool,
    with_qa_templates: bool,
    json: bool,
) -> Result<(), PulseError> {
    let report = initialize_repository(repo_root, refresh, with_qa_templates)?;
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
    // One hint line, plan 0025 G1: reservations bind only when a host hook
    // calls the gate. The configuration itself is printed on demand —
    // `pulse init` never writes into a host's settings.
    human.push_str(
        "\n`pulse hook snippet <host>` prints the PreToolUse config that makes reservations binding",
    );
    human.push_str(&register_note);
    render(json, &report, human)
}
