//! `pulse packet <id>` (plan 0022 §9): the one bounded JSON a worker reads
//! before doing anything.
//!
//! `docs.applicable` and `learnings` are empty stubs here — `docs::{
//! applicable,check}` and `learn::*` are P2.1's job, after Phase 1 finishes
//! rebuilding the source tree. `last_verdicts` passes through
//! `ticket.verdicts` as recorded (lane/verdict/commit); resolving each
//! verdict's findings would mean reading the referenced receipt, which has
//! no caller producing `verdict: fail` receipts yet (`kernel::lane` is
//! P1.9). Packet staleness has one fence: `source` (plan §9 — "Không
//! fingerprint từng input; fence duy nhất là `source`").

use std::path::Path;

use serde_json::{json, Value};

use crate::error::Result;
use crate::kernel::issues::{find, require};
use crate::source;
use crate::store::issues::read_all;

fn strip_runtime_fields(record: &Value) -> Value {
    let mut record = record.clone();
    if let Some(object) = record.as_object_mut() {
        object.remove("lease");
        object.remove("verdicts");
    }
    record
}

fn story_view(story: &Value) -> Value {
    json!({
        "id": story.get("id"),
        "outcome": story.get("outcome"),
        "rules": story.get("rules").cloned().unwrap_or(json!([])),
        "exceptions": story.get("exceptions").cloned().unwrap_or(json!([])),
        "approach": story.get("approach"),
    })
}

fn epic_view(epic: &Value) -> Value {
    json!({
        "id": epic.get("id"),
        "outcome": epic.get("outcome"),
        "out_of_scope": epic.get("out_of_scope").cloned().unwrap_or(json!([])),
    })
}

fn decision_view(decision: &Value) -> Value {
    json!({
        "id": decision.get("id"),
        "title": decision.get("title"),
        "decision": decision.get("decision"),
        "consequences": decision.get("consequences"),
    })
}

fn blockers(records: &[Value], ticket: &Value) -> Vec<Value> {
    ticket
        .get("deps")
        .and_then(Value::as_array)
        .map(|deps| {
            deps.iter()
                .filter(|dep| dep.get("type").and_then(Value::as_str) == Some("blocked_by"))
                .filter_map(|dep| dep.get("id").and_then(Value::as_str))
                .map(|id| {
                    let status = find(records, id)
                        .and_then(|record| record.get("status"))
                        .cloned()
                        .unwrap_or(Value::Null);
                    json!({"id": id, "status": status})
                })
                .collect()
        })
        .unwrap_or_default()
}

fn last_verdicts(ticket: &Value) -> Vec<Value> {
    ticket
        .get("verdicts")
        .and_then(Value::as_object)
        .map(|verdicts| {
            verdicts
                .iter()
                .map(|(lane, v)| {
                    json!({
                        "lane": lane,
                        "verdict": v.get("verdict"),
                        "findings": [],
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn recent_notes(ticket: &Value, limit: usize) -> Vec<Value> {
    ticket
        .get("notes")
        .and_then(Value::as_array)
        .map(|notes| {
            let start = notes.len().saturating_sub(limit);
            notes[start..].to_vec()
        })
        .unwrap_or_default()
}

/// # Errors
/// `issue_not_found` if `id` does not exist; propagates a git error from
/// [`source::snapshot`].
pub fn build_packet(repo_root: &Path, id: &str) -> Result<Value> {
    let records = read_all(repo_root)?;
    let ticket = require(&records, id)?;

    let story = ticket
        .get("story")
        .and_then(Value::as_str)
        .and_then(|story_id| find(&records, story_id));
    let epic = story
        .and_then(|story| story.get("epic").and_then(Value::as_str))
        .and_then(|epic_id| find(&records, epic_id));
    let decisions: Vec<Value> = ticket
        .pointer("/context/decisions")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .filter_map(|decision_id| find(&records, decision_id))
                .map(decision_view)
                .collect()
        })
        .unwrap_or_default();

    let snapshot = source::snapshot(repo_root, &[])?;

    Ok(json!({
        "issue": strip_runtime_fields(ticket),
        "story": story.map(story_view),
        "epic": epic.map(epic_view),
        "decisions": decisions,
        "blockers": blockers(&records, ticket),
        "docs": {"applicable": [], "map": "docs/README.md"},
        "learnings": [],
        "checkpoint": ticket.get("checkpoints").and_then(Value::as_array).and_then(|cps| cps.last()).cloned(),
        "last_verdicts": last_verdicts(ticket),
        "notes": recent_notes(ticket, 8),
        "source": {"commit": snapshot.commit, "dirty": !snapshot.dirty_paths.is_empty()},
        "protocol": {
            "checkpoint": format!("pulse checkpoint {id} --from <path>"),
            "handoff": format!("pulse handoff {id} --from <path>"),
            "continue_exit": "{\"status\":\"continue\"}",
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;

    fn git_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(StdCommand::new("git")
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
        dir
    }

    #[test]
    fn builds_a_packet_with_resolved_story_epic_and_blockers() {
        let repo = git_repo();
        crate::store::issues::mutate(repo.path(), |mut records| {
            records.push(json!({
                "schema": 3, "id": "EP-1111", "kind": "epic", "title": "e",
                "status": "active", "revision": 1,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "outcome": "epic outcome",
            }));
            records.push(json!({
                "schema": 3, "id": "ST-1111", "kind": "story", "title": "s",
                "status": "ready", "revision": 1,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "epic": "EP-1111", "outcome": "story outcome",
                "rules": [{"id": "BR-1", "text": "x"}],
            }));
            records.push(json!({
                "schema": 3, "id": "TK-9999", "kind": "ticket", "title": "blocker",
                "status": "done", "revision": 1,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "role": "implementation",
            }));
            records.push(json!({
                "schema": 3, "id": "TK-a3f9", "kind": "ticket", "title": "t",
                "status": "active", "revision": 1,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "role": "implementation", "story": "ST-1111",
                "deps": [{"type": "blocked_by", "id": "TK-9999"}],
                "notes": [{"at": "t", "from": "human:x", "kind": "note", "text": "n1"}],
            }));
            Ok(records)
        })
        .unwrap();

        let packet = build_packet(repo.path(), "TK-a3f9").unwrap();
        assert_eq!(packet["issue"]["id"], "TK-a3f9");
        assert_eq!(packet["story"]["outcome"], "story outcome");
        assert_eq!(packet["epic"]["outcome"], "epic outcome");
        assert_eq!(packet["blockers"][0]["id"], "TK-9999");
        assert_eq!(packet["blockers"][0]["status"], "done");
        assert_eq!(packet["notes"][0]["text"], "n1");
        assert_eq!(packet["source"]["dirty"], false);
        assert!(packet["issue"].get("lease").is_none());
    }

    #[test]
    fn notes_are_bounded_to_the_last_eight() {
        let repo = git_repo();
        let notes: Vec<Value> = (0..10)
            .map(|i| json!({"at": "t", "from": "human:x", "kind": "note", "text": format!("n{i}")}))
            .collect();
        crate::store::issues::mutate(repo.path(), |mut records| {
            records.push(json!({
                "schema": 3, "id": "TK-a3f9", "kind": "ticket", "title": "t",
                "status": "draft", "revision": 1,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "role": "implementation", "notes": notes,
            }));
            Ok(records)
        })
        .unwrap();
        let packet = build_packet(repo.path(), "TK-a3f9").unwrap();
        let packet_notes = packet["notes"].as_array().unwrap();
        assert_eq!(packet_notes.len(), 8);
        assert_eq!(packet_notes[0]["text"], "n2");
        assert_eq!(packet_notes[7]["text"], "n9");
    }

    #[test]
    fn missing_ticket_is_reported() {
        let repo = git_repo();
        let err = build_packet(repo.path(), "TK-ffff").unwrap_err();
        assert_eq!(err.code(), "issue_not_found");
    }
}
