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
    let human = if report.created.is_empty() {
        format!("repository {status}")
    } else {
        format!("repository {status}: created {}", report.created.join(", "))
    };
    render(json, &report, human)
}
