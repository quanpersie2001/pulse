//! Thin CLI adapter for `pulse run <role> --ticket <id>`.

use crate::cli::output::render;
use crate::qa::QaExecutionScope;
use crate::{JsonGraphStore, PulseError};

/// Aggregated `pulse run` invocation options.
pub(crate) struct RunOptions<'a> {
    pub(crate) role: &'a str,
    pub(crate) ticket: Option<&'a str>,
    pub(crate) ttl_seconds: u64,
    pub(crate) idempotency_key: &'a str,
    pub(crate) forced_worktree: bool,
    pub(crate) scope: QaExecutionScope,
    pub(crate) story: Option<&'a str>,
    pub(crate) acknowledge_drift: bool,
}

pub(crate) fn handle(
    store: &JsonGraphStore,
    options: &RunOptions<'_>,
    json: bool,
) -> Result<(), PulseError> {
    validate(options)?;
    let request = crate::kernel::run::RunRequest {
        role: options.role,
        ticket_id: options.ticket,
        story_id: options.story,
        scope: options.scope,
        ttl_seconds: options.ttl_seconds,
        idempotency_key: options.idempotency_key,
        forced_worktree: options.forced_worktree,
        acknowledge_drift: options.acknowledge_drift,
    };
    let outcome = store.run_role(&request)?;
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

/// Transport-level argument validation: which roles need which subject ids.
fn validate(options: &RunOptions<'_>) -> Result<(), PulseError> {
    match options.scope {
        QaExecutionScope::TicketCheckpoint => {
            if options.ticket.is_none() {
                return Err(PulseError::validation(
                    "run_ticket_required",
                    format!("pulse run {} requires --ticket <id>", options.role),
                ));
            }
            if options.story.is_some() {
                return Err(PulseError::validation(
                    "run_scope_args_invalid",
                    "--story only applies to qa --scope story_close",
                ));
            }
            Ok(())
        }
        QaExecutionScope::StoryClose => {
            if options.role != "qa" {
                return Err(PulseError::validation(
                    "run_scope_role_invalid",
                    format!(
                        "--scope story_close is a qa qualification; role {} has no story_close scope",
                        options.role
                    ),
                ));
            }
            if options.story.is_none() {
                return Err(PulseError::validation(
                    "run_story_required",
                    "qa --scope story_close requires --story <id>",
                ));
            }
            Ok(())
        }
    }
}
