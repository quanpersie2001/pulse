//! Thin CLI adapter for `pulse run <role> <id>` and `pulse release <id>`
//! (plan 0022 §6, §10).

use std::path::Path;

use serde_json::Value;

use crate::cli::output::render;
use crate::identity::actor::resolve_actor;
use crate::kernel::profile;
use crate::kernel::reservation::release;
use crate::kernel::run::{run_lane, run_worker};
use crate::source;
use crate::store::issues::read_all;
use crate::PulseError;

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_run(
    repo_root: &Path,
    role: &str,
    id: &str,
    ttl_seconds: i64,
    continue_limit: u32,
    force: bool,
    actor: Option<&str>,
    json: bool,
) -> Result<(), PulseError> {
    let actor = resolve_actor(repo_root, actor)?;
    let record = match role {
        "worker" => run_worker(repo_root, &actor, id, ttl_seconds, continue_limit)?,
        "review" => run_review(repo_root, &actor, id, force)?,
        lane => run_lane(repo_root, &actor, id, lane, force)?,
    };
    render(json, &record, format!("run {role} for {id} complete"))
}

/// `pulse run review <id>` (plan §10.2, "role ảo"): every lane in the
/// Ticket's profile that does not already have a `pass` verdict on the
/// current commit, in order.
fn run_review(
    repo_root: &Path,
    actor: &crate::identity::actor::ActorRef,
    id: &str,
    force: bool,
) -> Result<Value, PulseError> {
    let records = read_all(repo_root)?;
    let ticket = records
        .iter()
        .find(|record| record.get("id").and_then(Value::as_str) == Some(id))
        .ok_or_else(|| {
            PulseError::kernel(
                "issue_not_found",
                format!("no record with id {id}"),
                "check the id with `pulse work list`",
            )
        })?;
    let role = ticket
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("implementation");
    let key = profile::profile_key(
        role,
        ticket.get("surface").and_then(Value::as_str),
        ticket.get("risk").and_then(Value::as_str),
    );
    let config = profile::load(repo_root)?;
    let lanes = profile::profile_for(&config, &key)?.lanes.clone();
    let head = source::head_commit(repo_root).unwrap_or_default();

    let mut last = ticket.clone();
    for lane in lanes {
        let already_passing = last
            .pointer(&format!("/verdicts/{lane}/verdict"))
            .and_then(Value::as_str)
            == Some("pass")
            && last
                .pointer(&format!("/verdicts/{lane}/commit"))
                .and_then(Value::as_str)
                == Some(head.as_str());
        if already_passing {
            continue;
        }
        last = run_lane(repo_root, actor, id, &lane, force)?;
    }
    Ok(last)
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
