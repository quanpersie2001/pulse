use std::path::Path;

use crate::cli::output::render;
use crate::kernel::init::{initialize_repository, RepositoryInitStatus};
use crate::PulseError;

pub(crate) fn handle(
    repo_root: &Path,
    refresh: bool,
    host: Option<&str>,
    with_qa_templates: bool,
    json: bool,
) -> Result<(), PulseError> {
    let report = initialize_repository(repo_root, refresh, host, with_qa_templates)?;
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
    if !report.host_settings_snippet.is_empty() {
        human.push_str("\npaste into your host settings:\n");
        human.push_str(&report.host_settings_snippet.join("\n"));
    }
    render(json, &report, human)
}
