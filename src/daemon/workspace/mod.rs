//! Stable daemon-owned workspaces and isolation adapters.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IsolationMode {
    Local,
    Worktree,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceLifecycle {
    Open,
    Archived,
    StaleNeedsOperator,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceRecord {
    pub schema_version: u32,
    pub workspace_id: String,
    pub project_id: String,
    pub name: String,
    pub isolation: IsolationMode,
    pub root: String,
    pub managed: bool,
    pub base_commit: Option<String>,
    pub lifecycle: WorkspaceLifecycle,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}
