//! Daemon project identity and lifecycle use cases.
//!
//! This module touches persisted project records and project lifecycle events;
//! it performs only host-local project-root canonicalization and no Core
//! repository semantics or external repository I/O. Project state changes and
//! timeline events remain one durable store transaction. Dependencies are
//! limited to the parent application store/event helper and daemon project,
//! protocol, persistence, and error/value types.

use serde_json::{json, Value};
use std::path::PathBuf;

use super::{append_event, DaemonApplication};
use crate::canonical_json::hash_bytes;
use crate::daemon::project::ProjectRecord;
use crate::daemon::protocol::DaemonResponse;
use crate::daemon::workspace::WorkspaceLifecycle;
use crate::{PulseError, Result};

impl DaemonApplication {
    pub(super) fn project_open(&self, root: &str) -> Result<DaemonResponse> {
        let canonical = PathBuf::from(root)
            .canonicalize()
            .map_err(|error| PulseError::io(root, error))?;
        if !canonical.is_dir() {
            return Err(PulseError::validation(
                "project_root_invalid",
                "project root must be a directory",
            ));
        }
        let root = canonical.to_string_lossy().to_string();
        self.store.with_state(true, |state| {
            if let Some(project) = state
                .projects
                .values()
                .find(|project| project.canonical_root == root)
                .cloned()
            {
                return Ok(DaemonResponse::Project { project });
            }
            let now = chrono::Utc::now().to_rfc3339();
            let project = ProjectRecord {
                schema_version: 1,
                project_id: format!("prj_{}", ulid::Ulid::new()),
                canonical_root: root.clone(),
                repository_id: Some(format!(
                    "repo_{}",
                    hash_bytes(root.as_bytes())
                        .trim_start_matches("sha256:")
                        .chars()
                        .take(24)
                        .collect::<String>()
                )),
                created_at: now.clone(),
                updated_at: now,
                archived_at: None,
            };
            state
                .projects
                .insert(project.project_id.clone(), project.clone());
            append_event(
                state,
                "project.opened",
                Some(&project.project_id),
                None,
                None,
                json!({"root": project.canonical_root}),
            );
            Ok(DaemonResponse::Project { project })
        })
    }

    pub(super) fn project_list(&self, include_archived: bool) -> Result<DaemonResponse> {
        self.store.with_state(false, |state| {
            let projects = state
                .projects
                .values()
                .filter(|item| include_archived || item.archived_at.is_none())
                .cloned()
                .collect();
            Ok(DaemonResponse::Projects { projects })
        })
    }

    pub(super) fn project_archive(&self, project_id: &str) -> Result<DaemonResponse> {
        self.store.with_state(true, |state| {
            if state.workspaces.values().any(|workspace| {
                workspace.project_id == project_id
                    && workspace.lifecycle != WorkspaceLifecycle::Archived
            }) {
                return Err(PulseError::validation(
                    "project_has_open_workspaces",
                    "archive all project workspaces before archiving the project",
                ));
            }
            let now = chrono::Utc::now().to_rfc3339();
            let project =
                state
                    .projects
                    .get_mut(project_id)
                    .ok_or_else(|| PulseError::NotFound {
                        subject: format!("project {project_id}"),
                    })?;
            project.archived_at = Some(now.clone());
            project.updated_at = now;
            let project = project.clone();
            append_event(
                state,
                "project.archived",
                Some(project_id),
                None,
                None,
                Value::Null,
            );
            Ok(DaemonResponse::Project { project })
        })
    }

    pub(super) fn project_record(&self, project_id: &str) -> Result<ProjectRecord> {
        self.store.with_state(false, |state| {
            state
                .projects
                .get(project_id)
                .cloned()
                .ok_or_else(|| PulseError::NotFound {
                    subject: format!("project {project_id}"),
                })
        })
    }
}
