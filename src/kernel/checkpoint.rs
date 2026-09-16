//! `pulse checkpoint <id> --from cp.json` (plan 0022 §10.3).
//!
//! Appends to `checkpoints[]` (kept to the most recent 10; older ones move
//! to `.pulse/evidence/<id>/checkpoint-<n>.json`), seals a `checkpoint`
//! receipt, and does not change status. The actor must hold the Ticket's
//! lease — the same rule handoff uses, since both are worker-only actions
//! (plan §6.2).

use std::fs;
use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{PulseError, Result};
use crate::event::emit_event;
use crate::evidence::receipt::{record_receipt, NewReceipt, ReceiptSource, ReceiptSubject};
use crate::identity::actor::ActorRef;
use crate::kernel::issues::{apply_to_record, bump, now_rfc3339, require};
use crate::kernel::roles::{authorize, Action};
use crate::source;
use crate::store::issues;

const MAX_CHECKPOINTS_ON_RECORD: usize = 10;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointCommand {
    pub argv: Vec<String>,
    pub exit: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointInput {
    pub run_id: String,
    #[serde(default)]
    pub done_ac: Vec<String>,
    #[serde(default)]
    pub in_progress: String,
    #[serde(default)]
    pub next: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub gotchas: Vec<String>,
    #[serde(default)]
    pub commands_run: Vec<CheckpointCommand>,
}

fn checkpoint_record(input: &CheckpointInput, at: &str) -> Value {
    json!({
        "at": at,
        "run_id": input.run_id,
        "done_ac": input.done_ac,
        "in_progress": input.in_progress,
        "next": input.next,
        "files": input.files,
        "decisions": input.decisions,
        "gotchas": input.gotchas,
        "commands_run": input.commands_run,
    })
}

/// # Errors
/// `role_forbidden` if `actor` may not checkpoint; `checkpoint_lease_mismatch`
/// if `actor` does not hold the Ticket's lease.
pub fn checkpoint(
    repo_root: &Path,
    actor: &ActorRef,
    id: &str,
    input: CheckpointInput,
) -> Result<Value> {
    authorize(actor, Action::CheckpointOrHandoff)?;

    let records = issues::read_all(repo_root)?;
    let ticket = require(&records, id)?;
    let lease_actor = ticket.pointer("/lease/actor").and_then(Value::as_str);
    if lease_actor != Some(actor.as_kind_id().as_str()) {
        return Err(PulseError::kernel(
            "checkpoint_lease_mismatch",
            format!(
                "lease is held by {}, not the calling actor {}",
                lease_actor.unwrap_or("<none>"),
                actor.as_kind_id()
            ),
            "only the actor holding the Ticket's lease may checkpoint it",
        ));
    }
    let revision = ticket.get("revision").and_then(Value::as_u64);

    let at = now_rfc3339();
    let entry = checkpoint_record(&input, &at);

    record_receipt(
        repo_root,
        None,
        NewReceipt {
            kind: "checkpoint".to_string(),
            subject: ReceiptSubject {
                id: id.to_string(),
                revision,
            },
            actor: actor.as_kind_id(),
            source: {
                let snapshot = source::snapshot(repo_root, &[])?;
                ReceiptSource {
                    commit: snapshot.commit,
                    dirty_hash: snapshot.dirty_hash,
                }
            },
            run_id: Some(input.run_id.clone()),
            payload: entry.clone(),
            artifact_paths: Vec::new(),
        },
    )?;

    let mut archived = Vec::new();
    let saved = issues::mutate(repo_root, |records| {
        apply_to_record(records, id, |record| {
            let object = record.as_object_mut().expect("records are always objects");
            let checkpoints = object
                .entry("checkpoints")
                .or_insert_with(|| Value::Array(Vec::new()));
            let array = checkpoints
                .as_array_mut()
                .expect("checkpoints is always an array");
            array.push(entry.clone());
            let overflow = array.len().saturating_sub(MAX_CHECKPOINTS_ON_RECORD);
            if overflow > 0 {
                archived = array.drain(0..overflow).collect();
            }
            bump(object);
            Ok(())
        })
    })?;

    if !archived.is_empty() {
        archive_overflow_checkpoints(repo_root, id, &archived)?;
    }

    emit_event(
        repo_root,
        // Its own event type — emitting `run.completed` here made an operator
        // watching `events tail` misread a mid-run checkpoint as the end of
        // the run (dogfood ST-1, F7).
        "checkpoint.recorded",
        actor.as_kind_id(),
        id,
        json!({"checkpoint": true, "run_id": input.run_id}),
        Utc::now(),
    )?;

    Ok(require(&saved, id)?.clone())
}

fn archive_overflow_checkpoints(repo_root: &Path, id: &str, overflow: &[Value]) -> Result<()> {
    let dir = repo_root.join(".pulse/evidence").join(id);
    fs::create_dir_all(&dir).map_err(|error| PulseError::io(&dir, error))?;
    let existing = fs::read_dir(&dir)
        .map_err(|error| PulseError::io(&dir, error))?
        .filter_map(std::result::Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("checkpoint-") && name.ends_with(".json"))
        })
        .count();
    for (offset, checkpoint) in overflow.iter().enumerate() {
        let n = existing + offset + 1;
        let path = dir.join(format!("checkpoint-{n}.json"));
        let bytes = crate::canonical_json::to_canonical_bytes(checkpoint)?;
        crate::storage::atomic_write(&path, &bytes)?;
    }
    Ok(())
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

    fn git_repo_with_leased_ticket() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(std::process::Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .status()
                .unwrap()
                .success());
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "test"]);
        std::fs::write(dir.path().join("README.md"), "x\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "init"]);
        crate::store::issues::mutate(dir.path(), |mut records| {
            records.push(json!({
                "schema": 3, "id": "TK-a3f9", "kind": "ticket", "title": "t",
                "status": "active", "revision": 1,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "role": "implementation",
                "lease": {"role": "worker", "actor": "agent:worker", "run_id": "run_1", "expires_at": "2026-09-17T00:00:00Z"},
            }));
            Ok(records)
        })
        .unwrap();
        dir
    }

    fn input(run_id: &str) -> CheckpointInput {
        CheckpointInput {
            run_id: run_id.to_string(),
            done_ac: vec!["AC-1".to_string()],
            in_progress: "AC-2".to_string(),
            next: vec![],
            files: vec!["src/lib.rs".to_string()],
            decisions: vec![],
            gotchas: vec![],
            commands_run: vec![CheckpointCommand {
                argv: vec!["cargo".to_string(), "test".to_string()],
                exit: 0,
            }],
        }
    }

    #[test]
    fn checkpoint_lease_mismatch_is_rejected() {
        let repo = git_repo_with_leased_ticket();
        let err = checkpoint(
            repo.path(),
            &agent("someone-else"),
            "TK-a3f9",
            input("run_1"),
        )
        .unwrap_err();
        assert_eq!(err.code(), "checkpoint_lease_mismatch");
    }

    #[test]
    fn checkpoint_appends_and_does_not_change_status() {
        let repo = git_repo_with_leased_ticket();
        let updated = checkpoint(repo.path(), &agent("worker"), "TK-a3f9", input("run_1")).unwrap();
        assert_eq!(updated["status"], "active");
        assert_eq!(updated["checkpoints"].as_array().unwrap().len(), 1);
        assert_eq!(updated["checkpoints"][0]["in_progress"], "AC-2");
    }

    #[test]
    fn checkpoint_emits_its_own_event_type() {
        // Dogfood ST-1 F7: checkpointing used to emit `run.completed`, which
        // an operator watching `events tail` cannot tell apart from the
        // runner's own end-of-run event.
        let repo = git_repo_with_leased_ticket();
        checkpoint(repo.path(), &agent("worker"), "TK-a3f9", input("run_1")).unwrap();
        let log = crate::event::read_event_log(repo.path()).unwrap();
        let kinds: Vec<&str> = log
            .events
            .iter()
            .map(|event| event.event_type.as_str())
            .collect();
        assert!(kinds.contains(&"checkpoint.recorded"), "kinds: {kinds:?}");
        assert!(
            !kinds.contains(&"run.completed"),
            "checkpointing must not look like the run finished: {kinds:?}"
        );
    }

    #[test]
    fn checkpoint_seals_a_receipt() {
        let repo = git_repo_with_leased_ticket();
        checkpoint(repo.path(), &agent("worker"), "TK-a3f9", input("run_1")).unwrap();
        let receipts = crate::evidence::receipt::list_receipts(repo.path())
            .unwrap()
            .receipts;
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].kind, "checkpoint");
    }

    #[test]
    fn the_eleventh_checkpoint_archives_the_oldest_to_the_evidence_dir() {
        let repo = git_repo_with_leased_ticket();
        for i in 0..11 {
            checkpoint(
                repo.path(),
                &agent("worker"),
                "TK-a3f9",
                input(&format!("run_{i}")),
            )
            .unwrap();
        }
        let records = crate::store::issues::read_all(repo.path()).unwrap();
        let ticket = records.iter().find(|r| r["id"] == "TK-a3f9").unwrap();
        assert_eq!(ticket["checkpoints"].as_array().unwrap().len(), 10);
        assert_eq!(
            ticket["checkpoints"][0]["run_id"], "run_1",
            "the oldest (run_0) was archived"
        );
        let archived = repo
            .path()
            .join(".pulse/evidence/TK-a3f9/checkpoint-1.json");
        assert!(archived.exists());
        let archived: Value = serde_json::from_slice(&std::fs::read(&archived).unwrap()).unwrap();
        assert_eq!(archived["run_id"], "run_0");
    }
}
