use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::canonical_json::to_canonical_bytes;
use crate::storage;
use crate::PulseResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub schema_version: u32,
    pub id: String,
    pub event_type: String,
    pub actor: String,
    pub occurred_at: DateTime<Utc>,
    pub subject: String,
    pub payload: Value,
}

pub fn emit_event(
    repo_root: &Path,
    event_type: impl Into<String>,
    actor: impl Into<String>,
    subject: impl Into<String>,
    payload: Value,
    now: DateTime<Utc>,
) -> PulseResult<PathBuf> {
    let event = EventEnvelope {
        schema_version: 1,
        id: format!("evt_{}", Uuid::new_v4().simple()),
        event_type: event_type.into(),
        actor: actor.into(),
        occurred_at: now,
        subject: subject.into(),
        payload,
    };
    let date = now.format("%Y-%m-%d").to_string();
    let path = repo_root
        .join(".pulse/events")
        .join(date)
        .join(format!("{}.json", event.id));
    let bytes = to_canonical_bytes(&event)?;
    storage::create_new(&path, &bytes)?;
    Ok(path)
}
