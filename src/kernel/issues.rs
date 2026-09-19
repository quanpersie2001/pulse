//! Cross-domain composition for the basic `issues.jsonl` mutations: `new`,
//! `update`, `dep add|rm`, `transition`, `ready`, `note` (plan 0022 §6, §7.1).
//!
//! Not named in the plan's file-fate table, which allocates these commands
//! to `cli/work.rs` without naming a backing kernel module. Keeping the
//! composition here instead follows the repo's standing layering rule
//! (`cli/` is a thin transport/renderer, `kernel/` owns cross-domain
//! composition) rather than growing `cli/work.rs` past a renderer.
//!
//! Every function here: authorizes the actor (`kernel::roles`), mutates
//! `store::issues` under its lock, and emits exactly one correlated event —
//! plan §4.1's "một mutation = một event".

use chrono::Utc;
use serde_json::{Map, Value};
use std::path::Path;

use crate::error::{PulseError, Result};
use crate::event::emit_event;
use crate::identity::actor::ActorRef;
use crate::kernel::ready::evaluate as evaluate_ready;
use crate::kernel::roles::{authorize, Action};
use crate::store::issues;

const RUNTIME_OWNED_FIELDS: [&str; 3] = ["lease", "verdicts", "checkpoints"];

pub(crate) fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

pub(crate) fn find<'a>(records: &'a [Value], id: &str) -> Option<&'a Value> {
    records
        .iter()
        .find(|record| record.get("id").and_then(Value::as_str) == Some(id))
}

pub(crate) fn require<'a>(records: &'a [Value], id: &str) -> Result<&'a Value> {
    find(records, id).ok_or_else(|| {
        PulseError::kernel(
            "issue_not_found",
            format!("no record with id {id}"),
            "check the id with `pulse list`; ids are hash-based and never recycled",
        )
    })
}

fn initial_status(kind: &str) -> &'static str {
    // Plan §4.7: every kind starts `draft` except `decision`, which starts
    // `proposed` (its own two-state acceptance lifecycle).
    if kind == "decision" {
        "proposed"
    } else {
        "draft"
    }
}

/// `pulse new <kind> <title> [--story] [--epic] [--risk] [--surface]
/// [--from file.json]`. `extra` carries any additional fields the CLI parsed
/// (from flags or `--from`); common fields below always win over `extra` so
/// a hostile or stale `--from` payload cannot forge identity or status.
///
/// # Errors
/// `role_forbidden` if `actor` may not mutate the graph, or
/// `issues_record_invalid` if the assembled record fails the embedded
/// schema (for example: a `ticket` created without `--role`).
pub fn create(
    repo_root: &Path,
    actor: &ActorRef,
    kind: &str,
    title: &str,
    extra: Map<String, Value>,
) -> Result<Value> {
    authorize(actor, Action::MutateGraph)?;
    let work_kind = kind_for(kind)?;
    let now = now_rfc3339();
    let id = crate::id::generate_hash_id(work_kind, title, &now);

    let mut record = extra;
    if kind == "ticket" && !record.contains_key("role") {
        // The CLI has no `--role` flag (plan §6's `new` table row doesn't
        // list one); most created tickets are implementation work, and a
        // decision_work ticket is created with `--from file.json` naming
        // its role explicitly.
        record.insert(
            "role".to_string(),
            Value::String("implementation".to_string()),
        );
    }
    record.insert("schema".to_string(), Value::from(3));
    record.insert("id".to_string(), Value::String(id.to_string()));
    record.insert("kind".to_string(), Value::String(kind.to_string()));
    record.insert("title".to_string(), Value::String(title.to_string()));
    record.insert(
        "status".to_string(),
        Value::String(initial_status(kind).to_string()),
    );
    record.insert("revision".to_string(), Value::from(1_u64));
    record.insert("created_at".to_string(), Value::String(now.clone()));
    record.insert("updated_at".to_string(), Value::String(now));
    let record = Value::Object(record);

    let saved = issues::mutate(repo_root, |mut records| {
        check_parents(&records, &record)?;
        records.push(record);
        Ok(records)
    })?;
    let created = require(&saved, id.as_str())?.clone();
    emit_event(
        repo_root,
        "issue.created",
        actor.as_kind_id(),
        id.as_str(),
        serde_json::json!({"kind": kind, "revision_after": 1}),
        Utc::now(),
    )?;
    Ok(created)
}

/// A record's declared parent must exist and be the right kind (decision
/// 0028). Without this a Story could name an Epic that was never created —
/// the ready gate checks that every `context.anchors` path exists on disk,
/// so a dangling *parent* slipping through was the larger hole: `pulse work
/// tree` silently drops the Story, and the Epic's `out_of_scope` never
/// reaches the planner that needed it.
fn check_parents(records: &[Value], record: &Value) -> Result<()> {
    for (field, parent_kind) in [("epic", "epic"), ("story", "story")] {
        let Some(parent_id) = record.get(field).and_then(Value::as_str) else {
            continue;
        };
        if parent_id.trim().is_empty() {
            continue;
        }
        let found = records
            .iter()
            .find(|candidate| candidate.get("id").and_then(Value::as_str) == Some(parent_id));
        match found {
            Some(parent) if parent.get("kind").and_then(Value::as_str) == Some(parent_kind) => {}
            Some(parent) => {
                let actual = parent.get("kind").and_then(Value::as_str).unwrap_or("?");
                return Err(PulseError::kernel(
                    "issue_parent_unknown",
                    format!("{field} {parent_id} is a {actual}, not a {parent_kind}"),
                    "point the field at a record of the right kind",
                ));
            }
            None => {
                return Err(PulseError::kernel(
                    "issue_parent_unknown",
                    format!("{field} {parent_id} does not exist"),
                    "create the parent first, or check the id with `pulse work list`",
                ));
            }
        }
    }
    Ok(())
}

fn kind_for(kind: &str) -> Result<crate::id::WorkKind> {
    match kind {
        "epic" => Ok(crate::id::WorkKind::Epic),
        "story" => Ok(crate::id::WorkKind::Story),
        "ticket" => Ok(crate::id::WorkKind::Ticket),
        "decision" => Ok(crate::id::WorkKind::Decision),
        other => Err(PulseError::kernel(
            "issue_kind_invalid",
            format!("unknown kind {other}"),
            "kind must be one of epic, story, ticket, decision",
        )),
    }
}

/// `pulse update <id> --set k=v ... | --from file.json | --stdin`. Merges
/// `updates` into the record (shallow: a key in `updates` replaces the
/// existing value entirely, matching a flat `--set` model).
///
/// # Errors
/// `field_owned_by_runtime` if `updates` touches `lease`, `verdicts` or
/// `checkpoints` — those are runtime-written only.
pub fn update(
    repo_root: &Path,
    actor: &ActorRef,
    id: &str,
    updates: Map<String, Value>,
) -> Result<Value> {
    authorize(actor, Action::MutateGraph)?;
    for field in RUNTIME_OWNED_FIELDS {
        if updates.contains_key(field) {
            return Err(PulseError::kernel(
                "field_owned_by_runtime",
                format!("{field} is written by the runtime, not `pulse update`"),
                "checkpoint/handoff/run write this field; update cannot touch it",
            ));
        }
    }
    let saved = issues::mutate(repo_root, |records| {
        let records = apply_to_record(records, id, |record| {
            let object = record.as_object_mut().expect("records are always objects");
            for (key, value) in &updates {
                object.insert(key.clone(), value.clone());
            }
            bump(object);
            Ok(())
        })?;
        // After the merge, not before: `--set epic=EP-…` is exactly the
        // path that would otherwise introduce a dangling parent.
        check_parents(&records, require(&records, id)?)?;
        Ok(records)
    })?;
    let updated = require(&saved, id)?.clone();
    let revision = updated.get("revision").and_then(Value::as_u64);
    emit_event(
        repo_root,
        "issue.updated",
        actor.as_kind_id(),
        id,
        serde_json::json!({"revision_after": revision}),
        Utc::now(),
    )?;
    Ok(updated)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepType {
    BlockedBy,
    Supersedes,
}

impl DepType {
    fn as_str(self) -> &'static str {
        match self {
            Self::BlockedBy => "blocked_by",
            Self::Supersedes => "supersedes",
        }
    }
}

/// `pulse dep add <id> blocked_by|supersedes <other>`.
///
/// # Errors
/// `dep_cycle` if adding a `blocked_by` edge would create a cycle through
/// existing `blocked_by` edges.
/// `pulse dep rm <id> blocked_by|supersedes <other>` was removed at P3.2:
/// undoing a dep edge is a hand edit of `.pulse/issues.jsonl`, the single
/// human-editable store.
pub fn dep_add(
    repo_root: &Path,
    actor: &ActorRef,
    id: &str,
    dep_type: DepType,
    other_id: &str,
) -> Result<Value> {
    authorize(actor, Action::MutateGraph)?;
    let saved = issues::mutate(repo_root, |records| {
        if dep_type == DepType::BlockedBy && creates_blocked_by_cycle(&records, id, other_id) {
            return Err(PulseError::kernel(
                "dep_cycle",
                format!("blocked_by {other_id} would create a cycle back to {id}"),
                "break the cycle by removing one of the conflicting blocked_by edges first",
            ));
        }
        apply_to_record(records, id, |record| {
            let object = record.as_object_mut().expect("records are always objects");
            let deps = object
                .entry("deps")
                .or_insert_with(|| Value::Array(Vec::new()));
            let array = deps.as_array_mut().expect("deps is always an array");
            let already_present = array.iter().any(|dep| {
                dep.get("type").and_then(Value::as_str) == Some(dep_type.as_str())
                    && dep.get("id").and_then(Value::as_str) == Some(other_id)
            });
            if !already_present {
                array.push(serde_json::json!({"type": dep_type.as_str(), "id": other_id}));
            }
            bump(object);
            Ok(())
        })
    })?;
    let updated = require(&saved, id)?.clone();
    emit_event(
        repo_root,
        "issue.updated",
        actor.as_kind_id(),
        id,
        serde_json::json!({"dep_added": {"type": dep_type.as_str(), "id": other_id}}),
        Utc::now(),
    )?;
    Ok(updated)
}

fn creates_blocked_by_cycle(records: &[Value], from_id: &str, new_blocker_id: &str) -> bool {
    // Adding `from_id blocked_by new_blocker_id` cycles iff `new_blocker_id`
    // (transitively, following its own blocked_by edges) is already blocked
    // by `from_id`.
    let mut stack = vec![new_blocker_id.to_string()];
    let mut seen = std::collections::HashSet::new();
    while let Some(current) = stack.pop() {
        if current == from_id {
            return true;
        }
        if !seen.insert(current.clone()) {
            continue;
        }
        if let Some(record) = find(records, &current) {
            if let Some(deps) = record.get("deps").and_then(Value::as_array) {
                for dep in deps {
                    if dep.get("type").and_then(Value::as_str) == Some("blocked_by") {
                        if let Some(next) = dep.get("id").and_then(Value::as_str) {
                            stack.push(next.to_string());
                        }
                    }
                }
            }
        }
    }
    false
}

/// `pulse transition <id> --to blocked|cancelled --reason "..."`: the manual
/// transitions plan §4.7 leaves outside the gated ones (`ready`, `run`,
/// `handoff`, `close`).
///
/// # Errors
/// `transition_not_allowed` if `to` is not reachable from the record's
/// current status by a manual transition.
pub fn transition(
    repo_root: &Path,
    actor: &ActorRef,
    id: &str,
    to: &str,
    reason: &str,
) -> Result<Value> {
    authorize(actor, Action::MutateGraph)?;
    let saved = issues::mutate(repo_root, |records| {
        let current_status = require(&records, id)?
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if !manual_transition_allowed(&current_status, to) {
            return Err(PulseError::kernel(
                "transition_not_allowed",
                format!("cannot manually transition {id} from {current_status} to {to}"),
                "manual transitions are draft|ready|blocked -> cancelled, and ready|active -> blocked; \
                 draft->ready goes through `pulse work ready`, active transitions through `pulse claim`/`handoff`/`close`",
            ));
        }
        apply_to_record(records, id, |record| {
            let object = record.as_object_mut().expect("records are always objects");
            object.insert("status".to_string(), Value::String(to.to_string()));
            bump(object);
            Ok(())
        })
    })?;
    let updated = require(&saved, id)?.clone();
    emit_event(
        repo_root,
        "issue.transitioned",
        actor.as_kind_id(),
        id,
        serde_json::json!({"to": to, "reason": reason}),
        Utc::now(),
    )?;
    Ok(updated)
}

fn manual_transition_allowed(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("draft", "cancelled")
            | ("ready", "cancelled")
            | ("blocked", "cancelled")
            | ("ready", "blocked")
            | ("active", "blocked")
    )
}

/// `pulse ready <id>`: run the ready gate (`kernel::ready`) and, on a clean
/// report, transition `draft -> ready`.
///
/// # Errors
/// `gate_failed` carrying every violation when the gate does not pass;
/// nothing is written in that case.
pub fn ready(repo_root: &Path, actor: &ActorRef, id: &str) -> Result<Value> {
    authorize(actor, Action::MutateGraph)?;
    let saved = issues::mutate(repo_root, |records| {
        let record = require(&records, id)?.clone();
        let status = record.get("status").and_then(Value::as_str).unwrap_or("");
        if status != "draft" {
            return Err(PulseError::kernel(
                "transition_not_allowed",
                format!("{id} is {status}, not draft; only draft -> ready runs the ready gate"),
                "only a draft record can become ready",
            ));
        }
        let report = evaluate_ready(repo_root, &record, &records);
        if !report.is_ready() {
            let messages: Vec<String> = report
                .violations
                .iter()
                .map(|violation| format!("{}: {}", violation.code, violation.message))
                .collect();
            return Err(PulseError::kernel(
                "gate_failed",
                format!("{id} is not ready: {}", messages.join("; ")),
                "fix every violation listed; the ready gate reports all of them at once",
            ));
        }
        apply_to_record(records, id, |record| {
            let object = record.as_object_mut().expect("records are always objects");
            object.insert("status".to_string(), Value::String("ready".to_string()));
            bump(object);
            Ok(())
        })
    })?;
    let updated = require(&saved, id)?.clone();
    emit_event(
        repo_root,
        "issue.transitioned",
        actor.as_kind_id(),
        id,
        serde_json::json!({"to": "ready"}),
        Utc::now(),
    )?;
    Ok(updated)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    Note,
    Friction,
}

impl NoteKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Friction => "friction",
        }
    }
}

/// `pulse note <id> <text> [--friction] [--from actor]`. Every actor kind
/// may add a note (plan §6.2); this only ever appends, never gates.
///
/// # Errors
/// `issue_not_found` if `id` does not exist.
pub fn append_note(
    repo_root: &Path,
    actor: &ActorRef,
    id: &str,
    text: &str,
    kind: NoteKind,
) -> Result<Value> {
    authorize(actor, Action::NoteOrLearnAdd)?;
    let now = now_rfc3339();
    let saved = issues::mutate(repo_root, |records| {
        apply_to_record(records, id, |record| {
            let object = record.as_object_mut().expect("records are always objects");
            let notes = object
                .entry("notes")
                .or_insert_with(|| Value::Array(Vec::new()));
            let array = notes.as_array_mut().expect("notes is always an array");
            array.push(serde_json::json!({
                "at": now,
                "from": actor.as_kind_id(),
                "kind": kind.as_str(),
                "text": text,
            }));
            // Plan §4.3: at most 50 notes stay on the record; older ones are
            // cut (the event log remains the source of truth for all of them).
            let overflow = array.len().saturating_sub(50);
            if overflow > 0 {
                array.drain(0..overflow);
            }
            bump(object);
            Ok(())
        })
    })?;
    let updated = require(&saved, id)?.clone();
    emit_event(
        repo_root,
        "note.recorded",
        actor.as_kind_id(),
        id,
        serde_json::json!({"kind": kind.as_str(), "text": text}),
        Utc::now(),
    )?;
    Ok(updated)
}

/// Set `id`'s status without going through [`transition`]'s human-only
/// authorization: the lifecycle moves a gate makes on the graph's behalf
/// (a lane's `fail` verdict reworking a Ticket) are system-level, not a
/// human decision to re-authorize.
pub(crate) fn set_status(repo_root: &Path, id: &str, status: &str) -> Result<Value> {
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

pub(crate) fn apply_to_record(
    mut records: Vec<Value>,
    id: &str,
    edit: impl FnOnce(&mut Value) -> Result<()>,
) -> Result<Vec<Value>> {
    let index = records
        .iter()
        .position(|record| record.get("id").and_then(Value::as_str) == Some(id))
        .ok_or_else(|| {
            PulseError::kernel(
                "issue_not_found",
                format!("no record with id {id}"),
                "check the id with `pulse list`; ids are hash-based and never recycled",
            )
        })?;
    edit(&mut records[index])?;
    Ok(records)
}

pub(crate) fn bump(object: &mut Map<String, Value>) {
    let revision = object.get("revision").and_then(Value::as_u64).unwrap_or(0);
    object.insert("revision".to_string(), Value::from(revision + 1));
    object.insert("updated_at".to_string(), Value::String(now_rfc3339()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn epic() -> Value {
        json!({"id": "EP-1111", "kind": "epic", "title": "e"})
    }

    #[test]
    fn a_parent_that_exists_and_has_the_right_kind_passes() {
        let story = json!({"id": "ST-2222", "kind": "story", "epic": "EP-1111"});
        check_parents(&[epic()], &story).unwrap();
    }

    #[test]
    fn a_parent_that_does_not_exist_is_refused() {
        let story = json!({"id": "ST-2222", "kind": "story", "epic": "EP-ffff"});
        let error = check_parents(&[epic()], &story).unwrap_err();
        assert_eq!(error.code(), "issue_parent_unknown");
        assert!(format!("{error}").contains("EP-ffff"));
    }

    #[test]
    fn a_parent_of_the_wrong_kind_is_refused() {
        // Pointing `epic` at a Story id is the typo this catches: both are
        // four hex characters, so nothing else would notice.
        let other = json!({"id": "ST-9999", "kind": "story"});
        let story = json!({"id": "ST-2222", "kind": "story", "epic": "ST-9999"});
        let error = check_parents(&[other], &story).unwrap_err();
        assert_eq!(error.code(), "issue_parent_unknown");
        assert!(format!("{error}").contains("not a epic"), "{error}");
    }

    #[test]
    fn a_record_with_no_parent_field_passes() {
        check_parents(&[], &epic()).unwrap();
    }

    #[test]
    fn an_empty_parent_string_is_treated_as_absent() {
        let story = json!({"id": "ST-2222", "kind": "story", "epic": ""});
        check_parents(&[], &story).unwrap();
    }
}
