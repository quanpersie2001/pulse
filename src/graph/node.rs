use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::WorkKind;
use crate::{PulseError, PulseResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub schema_version: u32,
    pub id: String,
    pub kind: WorkKind,
    pub revision: u64,
    pub title: String,
    pub status: NodeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<StatusReason>,
    pub content_dir: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StatusReason {
    pub code: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

impl StatusReason {
    pub fn new(
        code: impl Into<String>,
        summary: impl Into<String>,
        reference: Option<String>,
    ) -> PulseResult<Self> {
        let code = code.into();
        let summary = summary.into();
        if code.trim().is_empty() {
            return Err(PulseError::validation(
                "invalid_status_reason",
                "status reason code must not be empty",
            ));
        }
        if summary.trim().is_empty() {
            return Err(PulseError::validation(
                "reason_required",
                "status reason summary must not be empty",
            ));
        }
        Ok(Self {
            code,
            summary,
            reference,
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Draft,
    Shaped,
    Ready,
    Active,
    Verifying,
    Done,
    Rework,
    Blocked,
    Cancelled,
    Superseded,
}

impl Node {
    pub fn new(id: String, kind: WorkKind, title: String, now: DateTime<Utc>) -> PulseResult<Self> {
        if title.trim().is_empty() {
            return Err(PulseError::validation(
                "invalid_title",
                "title must not be empty",
            ));
        }
        Ok(Self {
            schema_version: 1,
            content_dir: format!("works/{id}"),
            id,
            kind,
            revision: 1,
            title,
            status: NodeStatus::Draft,
            status_reason: None,
            created_at: now,
            updated_at: now,
        })
    }
}
