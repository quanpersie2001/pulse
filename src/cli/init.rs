use std::path::Path;

use crate::cli::output::render;
use crate::kernel::init::{initialize_repository, RepositoryInitStatus};
use crate::PulseError;

pub(crate) fn handle(
    repo_root: &Path,
    actor: Option<&str>,
    refresh: bool,
    json: bool,
) -> Result<(), PulseError> {
    let report = initialize_repository(repo_root, actor, refresh)?;
    let status = match report.status {
        RepositoryInitStatus::Initialized => "initialized",
        RepositoryInitStatus::Unchanged => "already initialized",
    };
    let mut human = format!(
        "repository {status}: {}\nreview ignore entries: {}",
        report.repository_id,
        report.proposed_ignore_entries.join(", ")
    );
    // Drift is the one outcome a human has to act on, so it is said plainly
    // rather than left to be inferred from the created/preserved lists.
    if !report.guidance_conflicts.is_empty() {
        human.push_str(&format!(
            "\nhand-edited Pulse block preserved, not refreshed: {}",
            report.guidance_conflicts.join(", ")
        ));
    }
    render(json, &report, human)
}
