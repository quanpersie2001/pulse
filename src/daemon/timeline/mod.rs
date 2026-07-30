//! Authoritative daemon runtime timeline.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TimelineCursor {
    pub epoch: String,
    pub sequence: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TimelineEvent {
    pub schema_version: u32,
    pub event_id: String,
    pub epoch: String,
    pub sequence: u64,
    pub occurred_at: String,
    pub event_type: String,
    pub project_id: Option<String>,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TimelinePage {
    pub events: Vec<TimelineEvent>,
    pub next_cursor: TimelineCursor,
    pub has_newer: bool,
}
