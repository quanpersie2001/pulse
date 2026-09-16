//! Ticket lease (plan 0022 §10.1 steps 1-2, §4.4).
//!
//! Rewritten small for v3: a lease is just `{role, actor, run_id,
//! expires_at}` on the record itself, no separate lease/workspace store.

use std::path::Path;

use chrono::{Duration, Utc};
use serde_json::{json, Value};

use crate::error::{PulseError, Result};
use crate::event::emit_event;
use crate::identity::actor::ActorRef;
use crate::kernel::issues::{apply_to_record, bump, require};
use crate::store::issues;

fn lease_expires_at(ticket: &Value) -> Option<chrono::DateTime<Utc>> {
    ticket
        .pointer("/lease/expires_at")
        .and_then(Value::as_str)
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

fn lease_actor(ticket: &Value) -> Option<&str> {
    ticket.pointer("/lease/actor").and_then(Value::as_str)
}

fn lease_is_live(ticket: &Value, now: chrono::DateTime<Utc>) -> bool {
    lease_actor(ticket).is_some() && lease_expires_at(ticket).is_some_and(|expires| expires > now)
}

/// Acquire (or extend, if the caller already holds it) the lease on `id` and
/// transition `ready -> active`. Refuses if another actor's lease is still
/// live on this Ticket, or another Ticket in the repo is `active` with a
/// still-live lease.
///
/// # Errors
/// `run_not_ready_or_active` if the Ticket is neither `ready`, `active` nor
/// `verifying` (a `verifying` Ticket is re-verified: fresh lease, back to
/// `active`, and its next handoff refreshes the source snapshot — the ST-1
/// dogfood F10 recovery for a post-handoff source fix).
/// `run_lease_held` if another actor's lease on this Ticket is still live.
/// `run_another_active` if a different Ticket is active with a live lease.
pub fn acquire_lease(
    repo_root: &Path,
    actor: &ActorRef,
    id: &str,
    role: &str,
    run_id: &str,
    ttl_seconds: i64,
) -> Result<Value> {
    let now = Utc::now();
    let records = issues::read_all(repo_root)?;
    let ticket = require(&records, id)?;
    let status = ticket.get("status").and_then(Value::as_str).unwrap_or("");
    if !matches!(status, "ready" | "active" | "verifying") {
        return Err(PulseError::kernel(
            "run_not_ready_or_active",
            format!("{id} is {status}, not ready, active or verifying"),
            "a worker run picks up a ready ticket, resumes an active one, or \
             re-verifies a verifying one (fresh lease, next handoff refreshes \
             the source snapshot)",
        ));
    }
    if status == "active"
        && lease_is_live(ticket, now)
        && lease_actor(ticket) != Some(actor.as_kind_id().as_str())
    {
        return Err(PulseError::kernel(
            "run_lease_held",
            format!(
                "{id}'s lease is held by {}",
                lease_actor(ticket).unwrap_or("?")
            ),
            "wait for the lease to expire or ask its holder to release it",
        ));
    }
    if let Some(other) = records.iter().find(|record| {
        record.get("id").and_then(Value::as_str) != Some(id)
            && record.get("status").and_then(Value::as_str) == Some("active")
            && lease_is_live(record, now)
    }) {
        let other_id = other.get("id").and_then(Value::as_str).unwrap_or("?");
        return Err(PulseError::kernel(
            "run_another_active",
            format!("{other_id} is already active with a live lease"),
            "close or release the other active ticket first; only one ticket runs at a time",
        ));
    }

    let expires_at = (now + Duration::seconds(ttl_seconds)).to_rfc3339();
    let saved = issues::mutate(repo_root, |records| {
        apply_to_record(records, id, |record| {
            let object = record.as_object_mut().expect("records are always objects");
            object.insert(
                "lease".to_string(),
                json!({"role": role, "actor": actor.as_kind_id(), "run_id": run_id, "expires_at": expires_at}),
            );
            object.insert("status".to_string(), Value::String("active".to_string()));
            bump(object);
            Ok(())
        })
    })?;
    emit_event(
        repo_root,
        "run.started",
        actor.as_kind_id(),
        id,
        json!({"run_id": run_id, "role": role}),
        now,
    )?;
    Ok(require(&saved, id)?.clone())
}

/// `pulse release <id>`: drop a stuck or expired lease and return the
/// Ticket to `ready`.
///
/// # Errors
/// `issue_not_found` if `id` does not exist.
pub fn release(repo_root: &Path, actor: &ActorRef, id: &str) -> Result<Value> {
    let saved = issues::mutate(repo_root, |records| {
        apply_to_record(records, id, |record| {
            let object = record.as_object_mut().expect("records are always objects");
            object.insert("lease".to_string(), Value::Null);
            if object.get("status").and_then(Value::as_str) == Some("active") {
                object.insert("status".to_string(), Value::String("ready".to_string()));
            }
            bump(object);
            Ok(())
        })
    })?;
    emit_event(
        repo_root,
        "issue.transitioned",
        actor.as_kind_id(),
        id,
        json!({"released": true}),
        Utc::now(),
    )?;
    Ok(require(&saved, id)?.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn agent(id: &str) -> ActorRef {
        ActorRef {
            kind: crate::identity::actor::ActorKind::Agent,
            id: id.to_string(),
        }
    }

    fn ready_ticket(id: &str) -> Value {
        json!({
            "schema": 3, "id": id, "kind": "ticket", "title": "t",
            "status": "ready", "revision": 1,
            "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
            "role": "implementation",
        })
    }

    #[test]
    fn acquiring_a_ready_ticket_makes_it_active_with_a_lease() {
        let repo = tempfile::tempdir().unwrap();
        issues::mutate(repo.path(), |mut r| {
            r.push(ready_ticket("TK-a3f9"));
            Ok(r)
        })
        .unwrap();
        let updated = acquire_lease(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            "worker",
            "run_1",
            3600,
        )
        .unwrap();
        assert_eq!(updated["status"], "active");
        assert_eq!(updated["lease"]["actor"], "agent:worker");
    }

    #[test]
    fn a_live_lease_held_by_another_actor_is_refused() {
        let repo = tempfile::tempdir().unwrap();
        issues::mutate(repo.path(), |mut r| {
            r.push(ready_ticket("TK-a3f9"));
            Ok(r)
        })
        .unwrap();
        acquire_lease(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            "worker",
            "run_1",
            3600,
        )
        .unwrap();
        let err = acquire_lease(
            repo.path(),
            &agent("someone-else"),
            "TK-a3f9",
            "worker",
            "run_2",
            3600,
        )
        .unwrap_err();
        assert_eq!(err.code(), "run_lease_held");
    }

    #[test]
    fn the_same_actor_may_re_acquire_extending_the_lease() {
        let repo = tempfile::tempdir().unwrap();
        issues::mutate(repo.path(), |mut r| {
            r.push(ready_ticket("TK-a3f9"));
            Ok(r)
        })
        .unwrap();
        acquire_lease(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            "worker",
            "run_1",
            3600,
        )
        .unwrap();
        let updated = acquire_lease(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            "worker",
            "run_1",
            7200,
        )
        .unwrap();
        assert_eq!(updated["status"], "active");
    }

    #[test]
    fn a_different_live_active_ticket_blocks_acquisition() {
        let repo = tempfile::tempdir().unwrap();
        issues::mutate(repo.path(), |mut r| {
            r.push(ready_ticket("TK-a3f9"));
            r.push(ready_ticket("TK-bbbb"));
            Ok(r)
        })
        .unwrap();
        acquire_lease(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            "worker",
            "run_1",
            3600,
        )
        .unwrap();
        let err = acquire_lease(
            repo.path(),
            &agent("worker"),
            "TK-bbbb",
            "worker",
            "run_2",
            3600,
        )
        .unwrap_err();
        assert_eq!(err.code(), "run_another_active");
    }

    #[test]
    fn release_drops_the_lease_and_returns_the_ticket_to_ready() {
        let repo = tempfile::tempdir().unwrap();
        issues::mutate(repo.path(), |mut r| {
            r.push(ready_ticket("TK-a3f9"));
            Ok(r)
        })
        .unwrap();
        acquire_lease(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            "worker",
            "run_1",
            3600,
        )
        .unwrap();
        let released = release(repo.path(), &agent("worker"), "TK-a3f9").unwrap();
        assert_eq!(released["status"], "ready");
        assert!(released["lease"].is_null());
    }
}
