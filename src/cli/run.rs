//! Thin CLI adapter for `pulse run <role> --ticket <id>`.

use crate::cli::output::render;
use crate::{JsonGraphStore, PulseError};

pub(crate) fn handle(
    store: &JsonGraphStore,
    role: &str,
    ticket: &str,
    ttl_seconds: u64,
    idempotency_key: &str,
    json: bool,
) -> Result<(), PulseError> {
    let outcome = store.run_role(role, ticket, ttl_seconds, idempotency_key)?;
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
