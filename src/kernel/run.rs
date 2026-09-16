//! `pulse run worker <id>` (plan 0022 §10.1) and `pulse run <lane> <id>`
//! (plan 0022 §10.2).
//!
//! Placeholders: the plan's runners.json example uses `{issue}`/
//! `{evidence_dir}` and an `"argv":[...]` array; the kept `runner::` module
//! (plan §14: "giữ spawn/timeout/bounded") already fixes a different, real
//! contract — one `"command"` string split with `runner::split_argv`, and
//! placeholders `{input}`/`{ticket}`/`{repo}`/`{artifact_dir}`
//! (`runner::PLACEHOLDERS`). This uses the contract that already exists
//! rather than rewriting the kept module to match the plan's illustration.
//!
//! No worktree mirroring/routing here (plan §10.6): the worker always runs
//! in `repo_root`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use serde_json::Value;

use crate::error::{PulseError, Result};
use crate::event::emit_event;
use crate::identity::actor::ActorRef;
use crate::kernel::issues::{append_note, apply_to_record, bump, find, require, NoteKind};
use crate::kernel::{lane, packet, profile, reservation};
use crate::runner::{self, CommandSpec};
use crate::source;
use crate::store::issues;

pub const DEFAULT_CONTINUE_LIMIT: u32 = 5;

fn evidence_dir(repo_root: &Path, id: &str) -> PathBuf {
    repo_root.join(".pulse/evidence").join(id)
}

fn run_dir(repo_root: &Path, id: &str) -> PathBuf {
    repo_root.join(".pulse/runtime/run").join(id)
}

fn load_runner(repo_root: &Path, role: &str) -> Result<CommandSpec> {
    let path = repo_root.join(".pulse/runners.json");
    let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
    let value: Value = serde_json::from_slice(&bytes).map_err(PulseError::from)?;
    let entry = value.get(role).ok_or_else(|| {
        PulseError::kernel(
            "runner_role_missing",
            format!("no role {role} in .pulse/runners.json"),
            "add a `{role}` entry ({{\"command\": \"...\", \"timeout_seconds\": N}}) to .pulse/runners.json",
        )
    })?;
    CommandSpec::from_value(entry)
}

fn write_worker_input(repo_root: &Path, id: &str, run_id: &str) -> Result<PathBuf> {
    let mut packet_value = packet::build_packet(repo_root, id)?;
    let dir = run_dir(repo_root, id);
    fs::create_dir_all(&dir).map_err(|error| PulseError::io(&dir, error))?;
    let evidence = evidence_dir(repo_root, id);
    fs::create_dir_all(&evidence).map_err(|error| PulseError::io(&evidence, error))?;
    let path = dir.join("worker-input.json");
    // The runner's own run id, so a worker echoing it back in cp.json/handoff
    // lines up with run.started/run.completed events (dogfood ST-1, F11 —
    // two id spaces used to coexist in the same event field).
    packet_value["protocol"]["run_id"] = serde_json::json!(run_id);
    let bytes = serde_json::to_vec_pretty(&packet_value)?;
    fs::write(&path, bytes).map_err(|error| PulseError::io(&path, error))?;
    Ok(path)
}

fn checkpoint_count(repo_root: &Path, id: &str) -> Result<usize> {
    let records = issues::read_all(repo_root)?;
    let ticket = require(&records, id)?;
    Ok(ticket
        .get("checkpoints")
        .and_then(Value::as_array)
        .map_or(0, Vec::len))
}

enum RoleOutcome {
    HandedOff,
    Blocked { reason: String },
    Continue,
    Inconclusive { reason: String },
}

/// Spawn `role` and classify its outcome (plan §10.1 step 4). Never
/// returns `Err` for a bad/timed-out/nonzero-exit run — those become
/// `RoleOutcome::Inconclusive`, matching "khác/timeout/exit≠0 -> run record
/// inconclusive" (a spawn failure to launch at all is the one case that
/// still surfaces as an `Err`).
fn spawn_and_classify(
    repo_root: &Path,
    role: &str,
    actor: &ActorRef,
    id: &str,
    input_path: &Path,
) -> Result<RoleOutcome> {
    let spec = load_runner(repo_root, role)?;
    let argv = runner::split_argv(&spec.command)?;
    let mut values = BTreeMap::new();
    values.insert("input".to_string(), input_path.display().to_string());
    values.insert("ticket".to_string(), id.to_string());
    values.insert("repo".to_string(), repo_root.display().to_string());
    values.insert(
        "artifact_dir".to_string(),
        evidence_dir(repo_root, id).display().to_string(),
    );
    let argv = runner::materialize_argv(&argv, &values)?;

    let outcome = runner::execute(
        repo_root,
        &argv,
        &[("PULSE_ACTOR", actor.as_kind_id())],
        Duration::from_secs(spec.timeout_seconds),
        spec.max_output_bytes,
        None,
    )?;

    if !outcome.exited_cleanly() {
        return Ok(RoleOutcome::Inconclusive {
            reason: if outcome.timed_out {
                "timed out".to_string()
            } else {
                format!("exit {:?}", outcome.exit_code)
            },
        });
    }
    let Ok(value) = runner::parse_output_json(&outcome) else {
        return Ok(RoleOutcome::Inconclusive {
            reason: "final stdout line was not valid JSON".to_string(),
        });
    };
    match value.get("status").and_then(Value::as_str) {
        Some("handed_off") => Ok(RoleOutcome::HandedOff),
        Some("blocked") => Ok(RoleOutcome::Blocked {
            reason: value
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("blocked")
                .to_string(),
        }),
        Some("continue") => Ok(RoleOutcome::Continue),
        other => Ok(RoleOutcome::Inconclusive {
            reason: format!("unexpected status {other:?}"),
        }),
    }
}

/// System-level status change the run loop makes on the worker's behalf
/// (not a human `pulse transition`, so it does not go through
/// `kernel::issues::transition`'s human-only authorization).
fn set_status(repo_root: &Path, id: &str, status: &str) -> Result<Value> {
    let saved = issues::mutate(repo_root, |records| {
        apply_to_record(records, id, |record| {
            let object = record.as_object_mut().expect("records are always objects");
            object.insert("status".to_string(), Value::String(status.to_string()));
            bump(object);
            Ok(())
        })
    })?;
    Ok(require(&saved, id)?.clone())
}

/// # Errors
/// Propagates lease-acquisition errors (`run_not_ready_or_active`,
/// `run_lease_held`, `run_another_active`); `run_continue_without_checkpoint`
/// if a `continue` exit is not backed by a new checkpoint (treated as a
/// crash); `run_inconclusive` if the final run attempt did not end in
/// `handed_off`/`blocked`/a still-retriable `continue`.
pub fn run_worker(
    repo_root: &Path,
    actor: &ActorRef,
    id: &str,
    ttl_seconds: i64,
    continue_limit: u32,
) -> Result<Value> {
    let run_id = format!("run_{}", ulid::Ulid::new());
    reservation::acquire_lease(repo_root, actor, id, "worker", &run_id, ttl_seconds)?;

    let mut attempt: u32 = 0;
    loop {
        let role = if attempt == 0 {
            "worker"
        } else if load_runner(repo_root, "worker-continue").is_ok() {
            "worker-continue"
        } else {
            "worker"
        };
        let checkpoints_before = checkpoint_count(repo_root, id)?;
        let input_path = write_worker_input(repo_root, id, &run_id)?;
        let outcome = spawn_and_classify(repo_root, role, actor, id, &input_path)?;

        match outcome {
            RoleOutcome::HandedOff => {
                let records = issues::read_all(repo_root)?;
                let ticket = require(&records, id)?.clone();
                emit_event(
                    repo_root,
                    "run.completed",
                    actor.as_kind_id(),
                    id,
                    serde_json::json!({"attempt": attempt, "outcome": "handed_off", "run_id": run_id}),
                    Utc::now(),
                )?;
                return Ok(ticket);
            }
            RoleOutcome::Blocked { reason } => {
                append_note(repo_root, actor, id, &reason, NoteKind::Note)?;
                let updated = set_status(repo_root, id, "blocked")?;
                emit_event(
                    repo_root,
                    "run.completed",
                    actor.as_kind_id(),
                    id,
                    serde_json::json!({"attempt": attempt, "outcome": "blocked", "reason": reason, "run_id": run_id}),
                    Utc::now(),
                )?;
                return Ok(updated);
            }
            RoleOutcome::Continue => {
                let checkpoints_after = checkpoint_count(repo_root, id)?;
                if checkpoints_after <= checkpoints_before {
                    return Err(PulseError::kernel(
                        "run_continue_without_checkpoint",
                        format!("{id} exited continue without recording a new checkpoint"),
                        "the worker must checkpoint before exiting {\"status\":\"continue\"}; treat this as a crash and resume with `pulse run worker`",
                    ));
                }
                attempt += 1;
                if attempt > continue_limit {
                    append_note(repo_root, actor, id, "needs_split", NoteKind::Note)?;
                    let updated = set_status(repo_root, id, "blocked")?;
                    emit_event(
                        repo_root,
                        "run.completed",
                        actor.as_kind_id(),
                        id,
                        serde_json::json!({"attempt": attempt, "outcome": "blocked", "reason": "needs_split", "run_id": run_id}),
                        Utc::now(),
                    )?;
                    return Ok(updated);
                }
            }
            RoleOutcome::Inconclusive { reason } => {
                emit_event(
                    repo_root,
                    "run.completed",
                    actor.as_kind_id(),
                    id,
                    serde_json::json!({"attempt": attempt, "outcome": "inconclusive", "reason": reason, "run_id": run_id}),
                    Utc::now(),
                )?;
                return Err(PulseError::kernel(
                    "run_inconclusive",
                    format!("{id} run attempt {attempt} ended inconclusively: {reason}"),
                    "the lease is kept; inspect logs and either resume with `pulse run worker` or `pulse release`",
                ));
            }
        }
    }
}

/// `pulse run <lane> <id>` (plan §10.2): spawn the lane, then validate and
/// seal via `kernel::lane`. A `fail` verdict reworks the Ticket
/// (`verifying -> active`).
///
/// # Errors
/// `lane_not_in_profile` unless `force`. Propagates `kernel::lane`'s
/// validation errors.
pub fn run_lane(
    repo_root: &Path,
    actor: &ActorRef,
    id: &str,
    role: &str,
    force: bool,
) -> Result<Value> {
    let records = issues::read_all(repo_root)?;
    let ticket = require(&records, id)?;
    let kind = ticket.get("kind").and_then(Value::as_str).unwrap_or("");
    let status = ticket.get("status").and_then(Value::as_str).unwrap_or("");
    // A Ticket must have handed off; a Story has no `verifying` status in its
    // lifecycle at all (plan §4.7) — a story-scope qa lane (§10.6, "QA scope
    // story_close là `pulse run qa-<x> <story-id>`") runs against whatever
    // status the Story is currently in, most often `ready`.
    if kind == "ticket" && status != "verifying" {
        return Err(PulseError::kernel(
            "lane_not_verifying",
            format!("{id} is {status}, not verifying"),
            "a lane only runs against a Ticket that has handed off, or a Story for a story-scope qa lane",
        ));
    }

    // A Story's own `surface`/`risk` (plan §4.5) resolve its profile for a
    // story-scope lane run. Unlike a Ticket, the ready gate never requires
    // these on a Story, so a Story can reach `ready` without them — that is
    // a data problem `--force` must not paper over, so this check runs
    // whether or not `force` was passed.
    let surface = ticket.get("surface").and_then(Value::as_str);
    let risk = ticket.get("risk").and_then(Value::as_str);
    if kind == "story" && (surface.is_none() || risk.is_none()) {
        return Err(PulseError::kernel(
            "profile_missing",
            format!("{id} has no surface/risk set; a story-scope lane run needs both to resolve a profile"),
            "set both first: `pulse work update <id> --set surface=<cli|api|ui|lib|docs> --set risk=<low|medium|high>`",
        ));
    }

    if !force {
        let ticket_role = ticket
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("implementation");
        let key = profile::profile_key(ticket_role, surface, risk);
        let config = profile::load(repo_root)?;
        let own_profile = profile::profile_for(&config, &key)?;
        // A story's own surface routes its own profile, and the surface of
        // each qa_case routes too (dogfood ST-1, F18): a multi-surface story
        // classified api must still run its ui cases' qa-ui lane without
        // --force. Tickets keep the single-surface rule.
        let mut allowed = own_profile.lanes.iter().any(|lane_name| lane_name == role);
        if !allowed && kind == "story" {
            let case_surfaces: Vec<&str> = ticket
                .get("qa_cases")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|case| case.get("surface").and_then(Value::as_str))
                .collect();
            for case_surface in case_surfaces {
                let case_key = profile::profile_key(ticket_role, Some(case_surface), risk);
                if let Ok(case_profile) = profile::profile_for(&config, &case_key) {
                    if case_profile.lanes.iter().any(|lane_name| lane_name == role) {
                        allowed = true;
                        break;
                    }
                }
            }
        }
        if !allowed {
            return Err(PulseError::kernel(
                "lane_not_in_profile",
                format!("{role} is not in the {key} profile"),
                "pass --force to run a lane outside the Ticket's profile",
            ));
        }
    }

    let fence_ignore = profile::load(repo_root)
        .map(|c| c.fence_ignore)
        .unwrap_or_default();
    let before = source::snapshot(repo_root, &fence_ignore)?;
    let dir = run_dir(repo_root, id);
    fs::create_dir_all(&dir).map_err(|error| PulseError::io(&dir, error))?;
    let evidence = evidence_dir(repo_root, id);
    fs::create_dir_all(&evidence).map_err(|error| PulseError::io(&evidence, error))?;
    let story = ticket
        .get("story")
        .and_then(Value::as_str)
        .and_then(|story_id| find(&records, story_id));
    let input_value = lane::lane_input(repo_root, ticket, story, role)?;
    let input_path = dir.join(format!("{role}-input.json"));
    fs::write(&input_path, serde_json::to_vec_pretty(&input_value)?)
        .map_err(|error| PulseError::io(&input_path, error))?;

    let spec = load_runner(repo_root, role)?;
    let argv = runner::split_argv(&spec.command)?;
    let mut values = BTreeMap::new();
    values.insert("input".to_string(), input_path.display().to_string());
    values.insert("ticket".to_string(), id.to_string());
    values.insert("repo".to_string(), repo_root.display().to_string());
    values.insert(
        "artifact_dir".to_string(),
        evidence_dir(repo_root, id).display().to_string(),
    );
    let argv = runner::materialize_argv(&argv, &values)?;
    // Lane runs emit the same run.started/run.completed pair as the worker
    // loop — a lane that dies without sealing used to be invisible in
    // `events tail` (dogfood ST-1, F8).
    emit_event(
        repo_root,
        "run.started",
        actor.as_kind_id(),
        id,
        serde_json::json!({"role": role}),
        Utc::now(),
    )?;
    let outcome = runner::execute(
        repo_root,
        &argv,
        &[("PULSE_ACTOR", format!("agent:{role}"))],
        Duration::from_secs(spec.timeout_seconds),
        spec.max_output_bytes,
        None,
    )?;
    if !outcome.exited_cleanly() {
        emit_event(
            repo_root,
            "run.completed",
            actor.as_kind_id(),
            id,
            serde_json::json!({"role": role, "outcome": "inconclusive", "reason": "nonzero exit or timeout"}),
            Utc::now(),
        )?;
        return Err(PulseError::kernel(
            "run_inconclusive",
            format!("{role} did not exit cleanly"),
            "inspect the lane's logs and rerun `pulse run {role} {id}`",
        ));
    }

    let receipt = match lane::validate_and_seal(repo_root, id, role, &before, &fence_ignore) {
        Ok(receipt) => receipt,
        Err(error) => {
            emit_event(
                repo_root,
                "run.completed",
                actor.as_kind_id(),
                id,
                serde_json::json!({"role": role, "outcome": "inconclusive", "reason": "seal failed"}),
                Utc::now(),
            )?;
            return Err(error);
        }
    };
    let verdict = receipt
        .payload
        .get("verdict")
        .and_then(Value::as_str)
        .unwrap_or("inconclusive");
    emit_event(
        repo_root,
        "run.completed",
        actor.as_kind_id(),
        id,
        serde_json::json!({"role": role, "outcome": "sealed", "verdict": verdict}),
        Utc::now(),
    )?;
    if verdict == "fail" {
        let updated = set_status(repo_root, id, "active")?;
        emit_event(
            repo_root,
            "issue.transitioned",
            actor.as_kind_id(),
            id,
            serde_json::json!({"to": "active", "reason": "rework", "lane": role}),
            Utc::now(),
        )?;
        return Ok(updated);
    }
    Ok(require(&issues::read_all(repo_root)?, id)?.clone())
}
