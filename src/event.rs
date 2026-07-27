use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::canonical_json::to_canonical_bytes;
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

pub fn event_path(repo_root: &Path, event: &EventEnvelope) -> PathBuf {
    repo_root
        .join(".pulse/events")
        .join(event.occurred_at.format("%Y-%m-%d").to_string())
        .join(format!("{}.json", event.id))
}

pub fn write_event(repo_root: &Path, event: &EventEnvelope) -> PulseResult<PathBuf> {
    let path = event_path(repo_root, event);
    let bytes = to_canonical_bytes(event)?;
    storage::create_new(&path, &bytes)?;
    Ok(path)
}

pub fn emit_event(
    repo_root: &Path,
    event_type: impl Into<String>,
    actor: impl AsRef<str>,
    subject: impl AsRef<str>,
    payload: Value,
    now: DateTime<Utc>,
) -> PulseResult<PathBuf> {
    let event = EventEnvelope::new(new_event_id(), event_type, actor, subject, payload, now);
    write_event(repo_root, &event)
}
