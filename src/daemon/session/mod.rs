//! Stable provider session identity and lifecycle.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionLifecycle {
    Initializing,
    Idle,
    Running,
    Error,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SessionRecord {
    pub schema_version: u32,
    pub session_id: String,
    pub project_id: String,
    pub workspace_id: String,
    pub provider_id: String,
    pub provider_handle: Option<String>,
    pub managed_process_id: Option<String>,
    pub parent_session_id: Option<String>,
    pub lifecycle: SessionLifecycle,
    pub archived_at: Option<String>,
    pub active_turn_id: Option<String>,
    pub last_error: Option<String>,
    pub provider_detail: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CommunicationGrantRecord {
    pub schema_version: u32,
    pub grant_id: String,
    pub sender_session_id: String,
    pub recipient_session_id: String,
    pub granted_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SessionMessageRecord {
    pub schema_version: u32,
    pub message_id: String,
    pub sender_session_id: String,
    pub recipient_session_id: String,
    pub body: String,
    pub created_at: String,
}
