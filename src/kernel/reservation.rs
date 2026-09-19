//! Ticket lease (plan 0022 §10.1 steps 1-2, §4.4).
//!
//! Rewritten small for v3: a lease is just `{role, actor, run_id,
//! expires_at}` on the record itself, no separate lease/workspace store.
//!
//! Since decision 0025 a claim is also the file reservation: a Ticket's
//! `touches` (the files it will edit or create) must not overlap what any
//! other held Ticket keeps, so two workers can share one checkout without
//! silently writing the same file.

use std::path::Path;

use chrono::{Duration, Utc};
use serde_json::{json, Value};

use crate::error::{PulseError, Result};
use crate::event::emit_event;
use crate::identity::actor::{ActorKind, ActorRef};
use crate::kernel::issues::{apply_to_record, bump, require};
use crate::kernel::roles::{authorize, Action};
use crate::kernel::scope;
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

/// The `touches` list of a record, or empty when it declares none. Shared
/// by the reservation checks, the scoped fence (`profile::fence_for`) and
/// the frontier — everywhere the files a record holds must be read the
/// same way (decision 0025).
pub(crate) fn touches_of(record: &Value) -> Vec<String> {
    record
        .get("touches")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Every record other than `id` that currently holds its files (decision
/// 0025): `active` with a still-live lease, or `verifying` — files stay held
/// through review, because close compares the handoff fence, and a mid-review
/// edit by another Ticket would stale it forever without a nameable cause.
/// Reused by the reservation checks B4/B5 build on top of this session.
pub(crate) fn held_by_others<'a>(
    records: &'a [Value],
    id: &str,
    now: chrono::DateTime<Utc>,
) -> Vec<&'a Value> {
    records
        .iter()
        .filter(|record| {
            record.get("id").and_then(Value::as_str) != Some(id)
                && match record.get("status").and_then(Value::as_str) {
                    Some("verifying") => true,
                    Some("active") => lease_is_live(record, now),
                    _ => false,
                }
        })
        .collect()
}

/// Acquire (or extend, if the caller already holds it) the lease on `id` and
/// transition `ready -> active`. Refuses if another actor's lease is still
/// live on this Ticket, if the calling actor already holds a live lease on a
/// different Ticket, or if this Ticket's `touches` overlap the files another
/// Ticket currently holds (decision 0025).
///
/// # Errors
/// `run_not_ready_or_active` if the Ticket is neither `ready`, `active` nor
/// `verifying` (a `verifying` Ticket is re-verified: fresh lease, back to
/// `active`, and its next handoff refreshes the source snapshot — the ST-1
/// dogfood F10 recovery for a post-handoff source fix).
/// `run_lease_held` if another actor's lease on this Ticket is still live.
/// `claim_actor_busy` if the calling actor holds a live lease on another
/// Ticket — one actor, one live claim, so parallel workers carry distinct
/// actors (`agent:worker-<n>`).
/// `claim_files_reserved` if this Ticket's `touches` overlap the files held
/// by an `active` (live lease) or `verifying` Ticket; a Ticket without
/// `touches` is exclusive and collides with every held Ticket.
pub fn acquire_lease(
    repo_root: &Path,
    actor: &ActorRef,
    id: &str,
    role: &str,
    run_id: &str,
    ttl_seconds: i64,
) -> Result<Value> {
    let now = Utc::now();
    // One lock for read -> evaluate -> lease write (plan 0025 A4).
    let saved = {
        let guard = crate::storage::WriteGuard::acquire(repo_root)?;
        let records = issues::read_all(repo_root)?;
        let ticket = require(&records, id)?;
        let status = ticket.get("status").and_then(Value::as_str).unwrap_or("");
        if !matches!(status, "ready" | "active" | "verifying") {
            return Err(PulseError::kernel(
                "run_not_ready_or_active",
                format!("{id} is {status}, not ready, active or verifying"),
                "a claim picks up a ready ticket, resumes an active one, or \
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
        // One actor, one live claim (decision 0025 B3): two parallel
        // workers sharing `agent:worker` would interleave edits invisibly,
        // and the close gate reads the actor to tell work from review.
        if records.iter().any(|record| {
            record.get("id").and_then(Value::as_str) != Some(id)
                && lease_is_live(record, now)
                && lease_actor(record) == Some(actor.as_kind_id().as_str())
        }) {
            return Err(PulseError::kernel(
                "claim_actor_busy",
                format!(
                    "{} already holds a live lease on another ticket",
                    actor.as_kind_id()
                ),
                "give each parallel worker its own actor: --actor agent:worker-<n>",
            ));
        }
        // The reservation itself: my `touches` must not intersect what any
        // other Ticket currently holds (decision 0025 B3). A Ticket without
        // `touches` overlaps everything — today's one-at-a-time behavior.
        let my_touches = touches_of(ticket);
        for other in held_by_others(&records, id, now) {
            let Some((_, pattern)) = scope::overlaps(&my_touches, &touches_of(other)) else {
                continue;
            };
            let other_id = other.get("id").and_then(Value::as_str).unwrap_or("?");
            let other_status = other.get("status").and_then(Value::as_str).unwrap_or("?");
            // A held Ticket is reviewed without a lease (handoff drops it),
            // so "in review" is the honest holder name for `verifying`.
            let holder = lease_actor(other).unwrap_or("in review");
            return Err(PulseError::kernel(
                "claim_files_reserved",
                format!("{other_id} ({other_status}, {holder}) holds {pattern}"),
                "wait for the ticket named in the message to close, or add a \
                 blocked_by edge between the two tickets",
            ));
        }

        let expires_at = (now + Duration::seconds(ttl_seconds)).to_rfc3339();
        issues::mutate_locked(&guard, repo_root, |records| {
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
        })?
    };
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

/// `pulse reserve <id> <path>…` (decision 0025 B4): append `paths` to the
/// Ticket's `touches` mid-run. The caller must hold the Ticket's live
/// lease — a reserve widens *its own* claim, so the actor that claimed the
/// Ticket is the one that reserves.
///
/// Note the exclusivity trade: a Ticket *without* `touches` is exclusive
/// (its claim collides with every held Ticket), and reserving turns it
/// into a scoped Ticket — it gives up exclusivity over the whole tree and
/// now holds exactly the files it names. That is the intended behavior per
/// decision 0025; recorded here so nobody is surprised by the widened
/// parallelism after a reserve.
///
/// # Errors
/// `role_forbidden` if `actor` may not checkpoint/handoff.
/// `reserve_paths_invalid` if `paths` is empty or any entry is not a
/// non-empty, repo-relative, `..`-free path — the same rule the ready gate
/// checks, via [`scope::valid_touch`].
/// `reserve_lease_mismatch` if the Ticket is not `active` with a live
/// lease held by `actor`.
/// `claim_files_reserved` (the claim code — the same reservation, made
/// mid-run) if a new path overlaps the files another Ticket currently
/// holds.
pub fn reserve(repo_root: &Path, actor: &ActorRef, id: &str, paths: &[String]) -> Result<Value> {
    authorize(actor, Action::CheckpointOrHandoff)?;
    if paths.is_empty() || paths.iter().any(|path| !scope::valid_touch(path)) {
        return Err(PulseError::kernel(
            "reserve_paths_invalid",
            "reserve needs at least one repo-relative path or glob that stays inside the repo",
            "name the files this ticket edits, e.g. `pulse reserve <id> src/api/handler.rs`",
        ));
    }
    let now = Utc::now();
    // One lock for read -> evaluate -> touches write (plan 0025 A4).
    let saved = {
        let guard = crate::storage::WriteGuard::acquire(repo_root)?;
        let records = issues::read_all(repo_root)?;
        let ticket = require(&records, id)?;
        let status = ticket.get("status").and_then(Value::as_str).unwrap_or("");
        let held_by_caller = status == "active"
            && lease_is_live(ticket, now)
            && lease_actor(ticket) == Some(actor.as_kind_id().as_str());
        if !held_by_caller {
            return Err(PulseError::kernel(
                "reserve_lease_mismatch",
                format!(
                    "{id} is {status} and its live lease is not held by {}",
                    actor.as_kind_id()
                ),
                "only the actor holding a live lease may reserve; `pulse claim <id>` first",
            ));
        }
        // The mid-run version of the claim check: the newly requested
        // paths must not intersect what any other Ticket currently holds.
        for other in held_by_others(&records, id, now) {
            let Some((_, pattern)) = scope::overlaps(paths, &touches_of(other)) else {
                continue;
            };
            let other_id = other.get("id").and_then(Value::as_str).unwrap_or("?");
            let other_status = other.get("status").and_then(Value::as_str).unwrap_or("?");
            let holder = lease_actor(other).unwrap_or("in review");
            return Err(PulseError::kernel(
                "claim_files_reserved",
                format!(
                    "{other_id} ({other_status}, {holder}) holds {pattern}; checkpoint and \
                     stop, then reclaim after {other_id} closes"
                ),
                "checkpoint and stop; watch `pulse events tail --follow` for the blocking \
                 ticket's transition to done, then reclaim the ticket and reserve again",
            ));
        }
        issues::mutate_locked(&guard, repo_root, |records| {
            apply_to_record(records, id, |record| {
                let mut merged = touches_of(record);
                for path in paths {
                    if !merged.contains(path) {
                        merged.push(path.clone());
                    }
                }
                let object = record.as_object_mut().expect("records are always objects");
                object.insert("touches".to_string(), json!(merged));
                bump(object);
                Ok(())
            })
        })?
    };
    emit_event(
        repo_root,
        "lease.reserved",
        actor.as_kind_id(),
        id,
        json!({"paths": paths}),
        now,
    )?;
    Ok(require(&saved, id)?.clone())
}

/// `pulse release <id>`: drop a stuck or expired lease and return the
/// Ticket to `ready`. A live lease belongs to whoever holds it: releasing
/// someone else's live lease is refused (`release_not_holder`) unless the
/// caller is a `human:` actor, who may always step in.
///
/// # Errors
/// `issue_not_found` if `id` does not exist; `release_not_holder` if a live
/// lease is held by another actor and the caller is not human.
pub fn release(repo_root: &Path, actor: &ActorRef, id: &str) -> Result<Value> {
    // One lock for read -> check -> lease drop (plan 0025 A4).
    let saved = {
        let guard = crate::storage::WriteGuard::acquire(repo_root)?;
        let records = issues::read_all(repo_root)?;
        let ticket = require(&records, id)?;
        if lease_is_live(ticket, Utc::now())
            && lease_actor(ticket) != Some(actor.as_kind_id().as_str())
            && actor.kind != ActorKind::Human
        {
            return Err(PulseError::kernel(
                "release_not_holder",
                format!(
                    "{id}'s lease is held by {}, not {}",
                    lease_actor(ticket).unwrap_or("?"),
                    actor.as_kind_id()
                ),
                "wait for expiry (`pulse doctor` lists expired leases) or have a human release it",
            ));
        }
        issues::mutate_locked(&guard, repo_root, |records| {
            apply_to_record(records, id, |record| {
                let object = record.as_object_mut().expect("records are always objects");
                object.insert("lease".to_string(), Value::Null);
                if object.get("status").and_then(Value::as_str) == Some("active") {
                    object.insert("status".to_string(), Value::String("ready".to_string()));
                }
                bump(object);
                Ok(())
            })
        })?
    };
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

    fn ready_ticket_with_touches(id: &str, touches: &[&str]) -> Value {
        let mut ticket = ready_ticket(id);
        ticket["touches"] = json!(touches);
        ticket
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
    fn two_tickets_with_disjoint_touches_can_both_be_active() {
        // Decision 0025: the whole point — two workers, one checkout, no
        // shared file.
        let repo = tempfile::tempdir().unwrap();
        issues::mutate(repo.path(), |mut r| {
            r.push(ready_ticket_with_touches("TK-a3f9", &["src/api/**"]));
            r.push(ready_ticket_with_touches("TK-bbbb", &["web/**"]));
            Ok(r)
        })
        .unwrap();
        acquire_lease(
            repo.path(),
            &agent("worker-1"),
            "TK-a3f9",
            "worker",
            "run_1",
            3600,
        )
        .unwrap();
        let second = acquire_lease(
            repo.path(),
            &agent("worker-2"),
            "TK-bbbb",
            "worker",
            "run_2",
            3600,
        )
        .unwrap();
        assert_eq!(second["status"], "active");
    }

    #[test]
    fn claim_is_refused_when_touches_overlap_an_active_ticket() {
        let repo = tempfile::tempdir().unwrap();
        issues::mutate(repo.path(), |mut r| {
            r.push(ready_ticket_with_touches("TK-a3f9", &["src/**"]));
            r.push(ready_ticket_with_touches("TK-bbbb", &["src/api/x.rs"]));
            Ok(r)
        })
        .unwrap();
        acquire_lease(
            repo.path(),
            &agent("worker-1"),
            "TK-a3f9",
            "worker",
            "run_1",
            3600,
        )
        .unwrap();
        let err = acquire_lease(
            repo.path(),
            &agent("worker-2"),
            "TK-bbbb",
            "worker",
            "run_2",
            3600,
        )
        .unwrap_err();
        assert_eq!(err.code(), "claim_files_reserved");
        assert!(err.hint().is_some());
        assert!(err.to_string().contains("TK-a3f9"), "{err}");
        assert!(err.to_string().contains("src/**"), "{err}");
    }

    #[test]
    fn claim_is_refused_when_touches_overlap_a_verifying_ticket() {
        // Files stay held through review (decision 0025): a close compares
        // the handoff fence, so a mid-review edit would stale it forever.
        let repo = tempfile::tempdir().unwrap();
        issues::mutate(repo.path(), |mut r| {
            let mut held = ready_ticket_with_touches("TK-a3f9", &["src/**"]);
            held["status"] = json!("verifying");
            r.push(held);
            r.push(ready_ticket_with_touches("TK-bbbb", &["src/lib.rs"]));
            Ok(r)
        })
        .unwrap();
        let err = acquire_lease(
            repo.path(),
            &agent("worker-2"),
            "TK-bbbb",
            "worker",
            "run_2",
            3600,
        )
        .unwrap_err();
        assert_eq!(err.code(), "claim_files_reserved");
        assert!(err.to_string().contains("in review"), "{err}");
    }

    #[test]
    fn a_ticket_without_touches_is_exclusive() {
        // Decision 0025 keeps today's behavior for data without `touches`:
        // an empty list collides with every held ticket, disjoint or not.
        let repo = tempfile::tempdir().unwrap();
        issues::mutate(repo.path(), |mut r| {
            r.push(ready_ticket("TK-a3f9"));
            r.push(ready_ticket_with_touches("TK-bbbb", &["web/**"]));
            Ok(r)
        })
        .unwrap();
        acquire_lease(
            repo.path(),
            &agent("worker-1"),
            "TK-a3f9",
            "worker",
            "run_1",
            3600,
        )
        .unwrap();
        let err = acquire_lease(
            repo.path(),
            &agent("worker-2"),
            "TK-bbbb",
            "worker",
            "run_2",
            3600,
        )
        .unwrap_err();
        assert_eq!(err.code(), "claim_files_reserved");
    }

    #[test]
    fn an_expired_lease_does_not_hold_its_files() {
        let repo = tempfile::tempdir().unwrap();
        issues::mutate(repo.path(), |mut r| {
            r.push(ready_ticket_with_touches("TK-a3f9", &["src/**"]));
            r.push(ready_ticket_with_touches("TK-bbbb", &["src/**"]));
            Ok(r)
        })
        .unwrap();
        // A zero-second ttl is already expired at the next read.
        acquire_lease(
            repo.path(),
            &agent("worker-1"),
            "TK-a3f9",
            "worker",
            "run_1",
            0,
        )
        .unwrap();
        let second = acquire_lease(
            repo.path(),
            &agent("worker-2"),
            "TK-bbbb",
            "worker",
            "run_2",
            3600,
        )
        .unwrap();
        assert_eq!(second["status"], "active");
    }

    #[test]
    fn claim_actor_busy_when_the_actor_already_holds_another_ticket() {
        // Two parallel workers must not share one actor name: the close gate
        // tells work from review by actor, and interleaved edits would be
        // indistinguishable.
        let repo = tempfile::tempdir().unwrap();
        issues::mutate(repo.path(), |mut r| {
            r.push(ready_ticket_with_touches("TK-a3f9", &["src/api/**"]));
            r.push(ready_ticket_with_touches("TK-bbbb", &["web/**"]));
            Ok(r)
        })
        .unwrap();
        acquire_lease(
            repo.path(),
            &agent("worker-1"),
            "TK-a3f9",
            "worker",
            "run_1",
            3600,
        )
        .unwrap();
        let err = acquire_lease(
            repo.path(),
            &agent("worker-1"),
            "TK-bbbb",
            "worker",
            "run_2",
            3600,
        )
        .unwrap_err();
        assert_eq!(err.code(), "claim_actor_busy");
        assert!(
            err.hint().is_some_and(|h| h.contains("worker-<n>")),
            "{err}"
        );
    }

    #[test]
    fn release_by_a_non_holder_agent_is_refused() {
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
        let err = release(repo.path(), &agent("someone-else"), "TK-a3f9").unwrap_err();
        assert_eq!(err.code(), "release_not_holder");
        assert!(err.hint().is_some());
        // The holder's live lease is untouched.
        let records = issues::read_all(repo.path()).unwrap();
        let ticket = require(&records, "TK-a3f9").unwrap();
        assert_eq!(ticket["lease"]["actor"], "agent:worker");
        assert_eq!(ticket["status"], "active");
    }

    #[test]
    fn a_human_may_release_another_actors_live_lease() {
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
        let human = ActorRef {
            kind: crate::identity::actor::ActorKind::Human,
            id: "quan".to_string(),
        };
        let released = release(repo.path(), &human, "TK-a3f9").unwrap();
        assert_eq!(released["status"], "ready");
        assert!(released["lease"].is_null());
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

    // --- reserve (decision 0025 B4) ---

    fn claimed_ticket_with_touches(id: &str, touches: &[&str], worker: &str) -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        issues::mutate(repo.path(), |mut r| {
            r.push(ready_ticket_with_touches(id, touches));
            Ok(r)
        })
        .unwrap();
        acquire_lease(repo.path(), &agent(worker), id, "worker", "run_1", 3600).unwrap();
        repo
    }

    #[test]
    fn reserve_appends_to_touches_and_dedups() {
        let repo = claimed_ticket_with_touches("TK-a3f9", &["src/api/**"], "worker-1");
        let updated = reserve(
            repo.path(),
            &agent("worker-1"),
            "TK-a3f9",
            &["src/api/extra.rs".to_string(), "src/api/**".to_string()],
        )
        .unwrap();
        // Dedup keeps first-occurrence order: the original glob first, the
        // newly reserved exact path after it.
        assert_eq!(
            updated["touches"],
            json!(["src/api/**", "src/api/extra.rs"])
        );
        let reserved = crate::event::read_events(repo.path())
            .unwrap()
            .into_iter()
            .any(|event| {
                event.event_type == "lease.reserved"
                    && event.payload["paths"] == json!(["src/api/extra.rs", "src/api/**"])
            });
        assert!(reserved, "no lease.reserved event with the reserved paths");
    }

    #[test]
    fn reserve_is_refused_without_the_live_lease() {
        let repo = claimed_ticket_with_touches("TK-a3f9", &["src/api/**"], "worker-1");
        let err = reserve(
            repo.path(),
            &agent("worker-2"),
            "TK-a3f9",
            &["src/api/more.rs".to_string()],
        )
        .unwrap_err();
        assert_eq!(err.code(), "reserve_lease_mismatch");
        assert!(err.hint().is_some_and(|h| h.contains("pulse claim")));
        // A held ticket is also refused: handoff dropped the lease, so
        // nobody may widen the claim anymore (a verifying ticket is under
        // review, not under work).
        issues::mutate(repo.path(), |mut r| {
            if let Some(record) = r.iter_mut().find(|record| record["id"] == "TK-a3f9") {
                record["status"] = json!("verifying");
                record["lease"] = Value::Null;
            }
            Ok(r)
        })
        .unwrap();
        let err = reserve(
            repo.path(),
            &agent("worker-1"),
            "TK-a3f9",
            &["src/api/more.rs".to_string()],
        )
        .unwrap_err();
        assert_eq!(err.code(), "reserve_lease_mismatch");
    }

    #[test]
    fn reserve_is_refused_when_the_path_is_held_by_another_ticket() {
        let repo = claimed_ticket_with_touches("TK-a3f9", &["src/api/**"], "worker-1");
        // A live lease must be live relative to real time, so the expiry is
        // computed, not hardcoded (a past stamp reads as expired, not held).
        let expires = (Utc::now() + Duration::hours(1)).to_rfc3339();
        issues::mutate(repo.path(), |mut r| {
            let mut other = ready_ticket_with_touches("TK-bbbb", &["web/**"]);
            other["status"] = json!("active");
            other["lease"] = json!({
                "role": "worker", "actor": "agent:worker-2",
                "run_id": "run_2", "expires_at": expires,
            });
            r.push(other);
            Ok(r)
        })
        .unwrap();
        let err = reserve(
            repo.path(),
            &agent("worker-1"),
            "TK-a3f9",
            &["web/nav.rs".to_string()],
        )
        .unwrap_err();
        assert_eq!(err.code(), "claim_files_reserved");
        assert!(err.to_string().contains("TK-bbbb"), "{err}");
        assert!(err.hint().is_some_and(|h| h.contains("checkpoint")));
    }

    #[test]
    fn reserve_is_refused_for_a_path_held_by_a_verifying_ticket() {
        // Files stay held through review (decision 0025): widening a claim
        // into them would stale the reviewer's fence.
        let repo = claimed_ticket_with_touches("TK-a3f9", &["web/**"], "worker-1");
        issues::mutate(repo.path(), |mut r| {
            let mut held = ready_ticket_with_touches("TK-bbbb", &["src/**"]);
            held["status"] = json!("verifying");
            held["lease"] = Value::Null;
            r.push(held);
            Ok(r)
        })
        .unwrap();
        let err = reserve(
            repo.path(),
            &agent("worker-1"),
            "TK-a3f9",
            &["src/lib.rs".to_string()],
        )
        .unwrap_err();
        assert_eq!(err.code(), "claim_files_reserved");
        assert!(err.to_string().contains("in review"), "{err}");
    }

    #[test]
    fn reserve_rejects_an_escaping_path() {
        let repo = claimed_ticket_with_touches("TK-a3f9", &["src/api/**"], "worker-1");
        for bad in [
            vec!["../outside.rs".to_string()],
            vec!["/etc/passwd".to_string()],
            vec!["src/api/**".to_string(), "".to_string()],
            vec![],
        ] {
            let err = reserve(repo.path(), &agent("worker-1"), "TK-a3f9", &bad).unwrap_err();
            assert_eq!(err.code(), "reserve_paths_invalid", "paths: {bad:?}");
            assert!(err.hint().is_some());
        }
        // Nothing was appended by any of the refusals.
        let records = issues::read_all(repo.path()).unwrap();
        let ticket = require(&records, "TK-a3f9").unwrap();
        assert_eq!(ticket["touches"], json!(["src/api/**"]));
    }
}
