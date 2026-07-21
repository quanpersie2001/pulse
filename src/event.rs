use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::canonical_json::to_canonical_bytes;
use crate::id::new_event_id;
use crate::storage;
use crate::PulseResult;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventEnvelope {
    pub schema_version: u32,
    pub id: String,
    pub event_type: String,
    pub actor: String,
    pub occurred_at: DateTime<Utc>,
    pub subject: String,
    pub payload: Value,
}

impl EventEnvelope {
    pub fn new(
        id: impl Into<String>,
        event_type: impl Into<String>,
        actor: impl Into<String>,
        subject: impl Into<String>,
        payload: Value,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            schema_version: 1,
            id: id.into(),
            event_type: event_type.into(),
            actor: actor.into(),
            occurred_at: now,
            subject: subject.into(),
            payload,
        }
    }
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
    actor: impl Into<String>,
    subject: impl Into<String>,
    payload: Value,
    now: DateTime<Utc>,
) -> PulseResult<PathBuf> {
    let event = EventEnvelope::new(new_event_id(), event_type, actor, subject, payload, now);
    write_event(repo_root, &event)
}
