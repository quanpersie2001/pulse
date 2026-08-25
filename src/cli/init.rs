use std::path::Path;

use crate::cli::output::render;
use crate::kernel::init::{initialize_repository, RepositoryInitStatus};
use crate::PulseError;

pub(crate) fn handle(repo_root: &Path, json: bool) -> Result<(), PulseError> {
    let report = initialize_repository(repo_root)?;
    let status = match report.status {
        RepositoryInitStatus::Initialized => "initialized",
        RepositoryInitStatus::Unchanged => "already initialized",
    };
    let human = format!(
        "repository {status}: {}\nreview ignore entries: {}",
        report.repository_id,
        report.proposed_ignore_entries.join(", ")
    );
    render(json, &report, human)
}
