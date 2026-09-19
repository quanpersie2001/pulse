//! Thin CLI adapter for `pulse claim <id>` and `pulse release <id>`
//! (plan 0022 §10.1, §4.4).
//!
//! The lease is what `checkpoint`/`handoff` authorize against, so a session
//! that intends to work a Ticket claims it first. Pulse dispatches nothing
//! here: claiming is a statement of intent by whoever is about to work.

use std::path::Path;

use crate::cli::output::render;
use crate::identity::actor::resolve_actor;
use crate::kernel::reservation::{acquire_lease, release, reserve};
use crate::PulseError;

pub(crate) fn handle_claim(
    repo_root: &Path,
    id: &str,
    ttl_seconds: i64,
    actor: Option<&str>,
    json: bool,
) -> Result<(), PulseError> {
    let actor = resolve_actor(repo_root, actor)?;
    let run_id = format!("run_{}", ulid::Ulid::new());
    let record = acquire_lease(repo_root, &actor, id, "worker", &run_id, ttl_seconds)?;
    render(
        json,
        &record,
        format!("{id} claimed by {} ({run_id})", actor.as_kind_id()),
    )
}

pub(crate) fn handle_release(
    repo_root: &Path,
    id: &str,
    actor: Option<&str>,
    json: bool,
) -> Result<(), PulseError> {
    let actor = resolve_actor(repo_root, actor)?;
    let record = release(repo_root, &actor, id)?;
    render(json, &record, format!("{id} released"))
}

pub(crate) fn handle_reserve(
    repo_root: &Path,
    id: &str,
    paths: &[String],
    actor: Option<&str>,
    json: bool,
) -> Result<(), PulseError> {
    let actor = resolve_actor(repo_root, actor)?;
    let record = reserve(repo_root, &actor, id, paths)?;
    render(
        json,
        &record,
        format!("{id} now touches {}", paths.join(", ")),
    )
}
