//! Thin CLI adapter for `pulse run <role> --ticket <id>`.

use crate::cli::output::render;
use crate::{JsonGraphStore, PulseError};

/// Aggregated `pulse run` invocation options.
pub(crate) struct RunOptions<'a> {
    pub(crate) role: &'a str,
    pub(crate) ticket: &'a str,
    pub(crate) ttl_seconds: u64,
    pub(crate) idempotency_key: &'a str,
    pub(crate) forced_worktree: bool,
    pub(crate) acknowledge_drift: bool,
}

pub(crate) fn handle(
    store: &JsonGraphStore,
    options: &RunOptions<'_>,
    json: bool,
) -> Result<(), PulseError> {
    let outcome = store.run_role(
        options.role,
        options.ticket,
        options.ttl_seconds,
        options.idempotency_key,
        options.forced_worktree,
        options.acknowledge_drift,
    )?;
    let human = match outcome.inconclusive_reason.as_deref() {
        Some(reason) => format!(
            "run {}: {} ({})\nrecord: {}",
            outcome.role, outcome.status, reason, outcome.run_record_path
        ),
        None => format!(
            "run {}: {}\nrecord: {}",
            outcome.role, outcome.status, outcome.run_record_path
        ),
    };
    render(json, &outcome, human)
}
