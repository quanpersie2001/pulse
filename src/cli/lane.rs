//! Thin CLI adapter for `pulse lane input|seal|reconcile` (plan 0022
//! §8.3-8.4, decision 0027).
//!
//! Dispatch lives with the host (Claude Code's Task tool, Codex's
//! `spawn_agent`, or a human in a second terminal); these three commands are
//! the boundary Pulse owns on either side of it: prepare a lane (or one seat
//! of a panel), seal its output, and — when the profile declares a panel —
//! reconcile the seats into one verdict.

use std::path::Path;
use std::time::Duration;

use serde_json::json;

use crate::cli::output::render;
use crate::identity::actor::resolve_actor;
use crate::kernel::lane;
use crate::PulseError;

pub(crate) fn handle_input(
    repo_root: &Path,
    id: &str,
    role: &str,
    force: bool,
    seat: Option<u32>,
    actor: Option<&str>,
    json: bool,
) -> Result<(), PulseError> {
    let actor = resolve_actor(repo_root, actor)?;
    let path = lane::prepare(repo_root, &actor, id, role, force, seat)?;
    let relative = path.strip_prefix(repo_root).unwrap_or(&path);
    let report = json!({"id": id, "role": role, "seat": seat, "input": relative});
    render(
        json,
        &report,
        format!("{role} input for {id}: {}", relative.display()),
    )
}

pub(crate) fn handle_seal(
    repo_root: &Path,
    id: &str,
    role: &str,
    seat: Option<u32>,
    actor: Option<&str>,
    json: bool,
) -> Result<(), PulseError> {
    let actor = resolve_actor(repo_root, actor)?;
    let record = lane::seal(repo_root, &actor, id, role, seat)?;
    let verdict = record
        .pointer(&format!("/verdicts/{role}/verdict"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("recorded")
        .to_string();
    render(json, &record, format!("{role} sealed for {id}: {verdict}"))
}

pub(crate) fn handle_reconcile(
    repo_root: &Path,
    id: &str,
    role: &str,
    prepare: bool,
    timeout_seconds: u64,
    actor: Option<&str>,
    json: bool,
) -> Result<(), PulseError> {
    let actor = resolve_actor(repo_root, actor)?;
    if prepare {
        let path = lane::reconcile_prepare(repo_root, &actor, id, role)?;
        let relative = path.strip_prefix(repo_root).unwrap_or(&path);
        let report = json!({"id": id, "role": role, "phase": "reconcile", "input": relative});
        return render(
            json,
            &report,
            format!("{role} reconcile input for {id}: {}", relative.display()),
        );
    }
    let record = lane::reconcile(
        repo_root,
        &actor,
        id,
        role,
        Duration::from_secs(timeout_seconds),
    )?;
    let verdict = record
        .pointer(&format!("/verdicts/{role}/verdict"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("recorded")
        .to_string();
    render(
        json,
        &record,
        format!("{role} reconciled for {id}: {verdict}"),
    )
}
