use std::fs;
use std::path::{Path, PathBuf};

use crate::PulseError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::storage;
use crate::PulseResult;

/// Generate a fresh event identifier (`evt_<ulid>`).
///
/// Event identity generation is owned by the event module. A compatibility
/// re-export remains at `pulse::id::new_event_id` for historical callers.
pub fn new_event_id() -> String {
    format!("evt_{}", ulid::Ulid::new())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EventEnvelope {
    pub schema_version: u32,
    pub id: String,
    pub event_type: String,
    pub occurred_at: DateTime<Utc>,
    pub actor: EventActor,
    pub subject: EventSubject,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation: Option<EventCorrelation>,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EventActor {
    pub kind: EventActorKind,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventActorKind {
    Human,
    Agent,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EventSubject {
    pub kind: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EventCorrelation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lease_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_id: Option<String>,
}

impl EventActor {
    pub fn new(kind: EventActorKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
        }
    }

    pub fn parse(actor: impl AsRef<str>) -> Self {
        let actor = actor.as_ref();
        let (kind, id) = actor
            .split_once(':')
            .map_or(("system", actor), |(kind, id)| (kind, id));
        let kind = match kind {
            "human" => EventActorKind::Human,
            "agent" => EventActorKind::Agent,
            "system" => EventActorKind::System,
            _ => EventActorKind::System,
        };
        Self::new(kind, id)
    }

    pub fn legacy_id(&self) -> String {
        let kind = match self.kind {
            EventActorKind::Human => "human",
            EventActorKind::Agent => "agent",
            EventActorKind::System => "system",
        };
        format!("{kind}:{}", self.id)
    }
}

impl EventSubject {
    pub fn new(kind: impl Into<String>, id: impl Into<String>, revision: Option<u64>) -> Self {
        Self {
            kind: kind.into(),
            id: id.into(),
            revision,
        }
    }

    pub fn from_event(event_type: &str, subject: impl AsRef<str>, payload: &Value) -> Self {
        let id = subject.as_ref().to_string();
        let kind = infer_subject_kind(event_type, &id);
        let revision = infer_subject_revision(event_type, payload);
        Self::new(kind, id, revision)
    }
}

impl EventEnvelope {
    pub fn new(
        id: impl Into<String>,
        event_type: impl Into<String>,
        actor: impl AsRef<str>,
        subject: impl AsRef<str>,
        payload: Value,
        now: DateTime<Utc>,
    ) -> Self {
        let event_type = event_type.into();
        Self::new_typed(
            id,
            event_type.clone(),
            EventActor::parse(actor),
            EventSubject::from_event(&event_type, subject, &payload),
            None,
            payload,
            now,
        )
    }

    pub fn new_typed(
        id: impl Into<String>,
        event_type: impl Into<String>,
        actor: EventActor,
        subject: EventSubject,
        correlation: Option<EventCorrelation>,
        payload: Value,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            schema_version: 1,
            id: id.into(),
            event_type: event_type.into(),
            occurred_at: now,
            actor,
            subject,
            correlation,
            payload,
        }
    }
}

fn infer_subject_kind(event_type: &str, id: &str) -> String {
    if event_type.starts_with("work.") {
        return match id.split_once('-').map(|(prefix, _)| prefix) {
            Some("EP") => "epic",
            Some("ST") => "story",
            Some("TK") => "ticket",
            Some("DEC") => "decision",
            _ => "work",
        }
        .to_string();
    }
    // Plan 0025 E1: `friction.dismissed` sits on a work item (the record the
    // friction note was written against), so the id prefix decides — same
    // mapping as `work.` above.
    if event_type.starts_with("friction.") {
        return match id.split_once('-').map(|(prefix, _)| prefix) {
            Some("EP") => "epic",
            Some("ST") => "story",
            Some("TK") => "ticket",
            Some("DEC") => "decision",
            _ => "work",
        }
        .to_string();
    }
    if event_type.starts_with("docs.") {
        "document".to_string()
    } else if event_type.starts_with("evidence.receipt.") {
        "receipt".to_string()
    } else if event_type.starts_with("evidence.artifact.") {
        "artifact".to_string()
    } else if event_type.starts_with("knowledge.learning.")
        || event_type.starts_with("knowledge.relation.")
    {
        "learning".to_string()
    } else {
        "resource".to_string()
    }
}

fn infer_subject_revision(event_type: &str, payload: &Value) -> Option<u64> {
    if event_type == "work.node.created" {
        return payload.get("node")?.get("revision")?.as_u64();
    }
    if event_type.starts_with("work.") {
        if let Some(revision) = payload.get("new_revision").and_then(Value::as_u64) {
            return Some(revision);
        }
        if event_type == "work.node.transitioned" {
            return payload
                .get("expected_revision")
                .and_then(Value::as_u64)
                .map(|revision| revision + 1);
        }
    }
    payload
        .get("revision_after")
        .or_else(|| payload.get("entry_revision_after"))
        .or_else(|| payload.get("document_revision_after"))
        .and_then(Value::as_u64)
}

/// Directory holding the event log of a repository.
fn events_root(repo_root: &Path) -> PathBuf {
    repo_root.join(".pulse/events")
}

/// Day file covering `when`: `.pulse/events/<YYYY-MM-DD>.jsonl` in UTC.
///
/// Sole owner of the day-file naming rule. Callers that build a transaction
/// intent know the timestamp before they have an envelope and come here; those
/// holding an envelope use [`event_path`]. Both must agree, or a crash-recovery
/// intent looks for the event in a file no writer targets.
pub fn day_file_path(repo_root: &Path, when: DateTime<Utc>) -> PathBuf {
    events_root(repo_root).join(format!("{}.jsonl", when.format("%Y-%m-%d")))
}

/// Day file an event belongs to, keyed on `occurred_at` in UTC so an event
/// never lands in the wrong file (Decision 0011 §2).
pub fn event_path(repo_root: &Path, event: &EventEnvelope) -> PathBuf {
    day_file_path(repo_root, event.occurred_at)
}

/// Append one event to its day file.
///
/// Line order is write order; the ULID `id` remains the cursor. The append is
/// fsynced, so a returned `Ok` means the event survives a crash.
///
/// # Errors
/// Returns an error when the envelope cannot be serialised or the append
/// fails.
pub fn write_event(repo_root: &Path, event: &EventEnvelope) -> PulseResult<PathBuf> {
    let path = event_path(repo_root, event);
    let bytes = crate::canonical_json::to_canonical_line_bytes(event)?;
    storage::append_line_fsync(&path, &bytes)?;
    Ok(path)
}

/// Outcome of reading the event log.
///
/// `torn_tails` names day files whose final line did not parse — the shape a
/// crash mid-append leaves behind (Decision 0011 §5). The events before it are
/// intact and returned; the caller reports the file rather than treating the
/// gap as absence of events.
#[derive(Debug, Clone, Default, Serialize)]
pub struct EventLogRead {
    pub events: Vec<EventEnvelope>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub torn_tails: Vec<String>,
}

/// Read every recorded event, sorted by id (ULID order == chronological
/// order), reporting any torn day-file tail.
///
/// Both layouts are read: `<date>.jsonl` written today, and the legacy
/// `<date>/evt_<ulid>.json` per-event layout a v2 repository could still
/// carry. No writer produces that layout in v3.
///
/// # Errors
/// Returns an error when the event directory cannot be listed.
pub fn read_event_log(repo_root: &Path) -> PulseResult<EventLogRead> {
    let root = events_root(repo_root);
    if !root.exists() {
        return Ok(EventLogRead::default());
    }
    let mut read = EventLogRead::default();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir).map_err(|error| PulseError::io(&dir, error))?;
        for entry in entries {
            let path = entry.map_err(|error| PulseError::io(&dir, error))?.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            match path.extension().and_then(|ext| ext.to_str()) {
                Some("jsonl") => read_day_file(&path, &mut read),
                // Legacy one-file-per-event layout, pre-compaction.
                Some("json") => {
                    if let Ok(bytes) = fs::read(&path) {
                        if let Ok(event) = serde_json::from_slice::<EventEnvelope>(&bytes) {
                            read.events.push(event);
                        }
                    }
                }
                _ => continue,
            }
        }
    }
    read.events.sort_by(|left, right| left.id.cmp(&right.id));
    read.torn_tails.sort();
    Ok(read)
}

/// Parse one `<date>.jsonl` file into `read`.
///
/// An unreadable file is skipped rather than failing the whole read, matching
/// the legacy behaviour. Only the final segment can be torn: every earlier
/// line was followed by a `\n` that the writer fsynced.
fn read_day_file(path: &Path, read: &mut EventLogRead) {
    let Ok(bytes) = fs::read(path) else {
        return;
    };
    if bytes.is_empty() {
        return;
    }
    let terminated = bytes.last() == Some(&b'\n');
    let mut lines: Vec<&[u8]> = bytes.split(|byte| *byte == b'\n').collect();
    if terminated {
        // The split after a trailing separator yields one empty tail.
        lines.pop();
    }
    let last_index = lines.len().saturating_sub(1);
    for (index, line) in lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        match serde_json::from_slice::<EventEnvelope>(line) {
            Ok(event) => read.events.push(event),
            Err(_) => {
                if index == last_index && !terminated {
                    read.torn_tails.push(path.display().to_string());
                }
            }
        }
    }
}

/// Read every recorded event, sorted by id. Torn tails are dropped silently;
/// callers that report them use [`read_event_log`].
///
/// # Errors
/// Returns an error when the event directory cannot be listed.
pub fn read_events(repo_root: &Path) -> PulseResult<Vec<EventEnvelope>> {
    Ok(read_event_log(repo_root)?.events)
}

/// Append an event to the log, stamping it `now`.
///
/// The event id's time part is derived from `now` (not the wall clock), so
/// id order — the log's sort key — always equals `occurred_at` order, even
/// when a caller passes a backdated or synthetic timestamp. Random low bits
/// keep same-millisecond ids unique.
///
/// # Errors
/// Returns an error when the event cannot be written (see [`write_event`]).
pub fn emit_event(
    repo_root: &Path,
    event_type: impl Into<String>,
    actor: impl AsRef<str>,
    subject: impl AsRef<str>,
    payload: Value,
    now: DateTime<Utc>,
) -> PulseResult<PathBuf> {
    let id = format!("evt_{}", ulid::Ulid::from_datetime(now.into()));
    let event = EventEnvelope::new(id, event_type, actor, subject, payload, now);
    write_event(repo_root, &event)
}
