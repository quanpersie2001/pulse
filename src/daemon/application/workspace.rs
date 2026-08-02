//! Host-local workspace lifecycle and concrete Git worktree effects.
//!
//! This module touches persisted workspace records, lifecycle events, and the
//! external Git worktree effect ledger. It commits intent before Git I/O,
//! records Attempting before the operation, and acknowledges the effect only
//! after the result while preserving the existing ledger failpoints and
//! workspace transaction ordering. Dependencies are limited to the parent
//! application store/effect/event helpers and daemon project, persistence,
//! session, workspace, protocol, and error/value types.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{append_event, deterministic_id, effects::external_effect_blocked, DaemonApplication};
use crate::canonical_json::hash_bytes;
use crate::daemon::persistence::{ExternalEffectKind, ExternalEffectState};
use crate::daemon::protocol::DaemonResponse;
use crate::daemon::session::SessionLifecycle;
use crate::daemon::workspace::{IsolationMode, WorkspaceLifecycle, WorkspaceRecord};
use crate::{PulseError, Result};

impl DaemonApplication {
    pub(super) fn workspace_create(
        &self,
        project_id: &str,
        name: &str,
        isolation: IsolationMode,
        requested_base: Option<&str>,
        idempotency_key: &str,
    ) -> Result<DaemonResponse> {
        validate_name(name)?;
        let workspace_id = deterministic_id("wks", idempotency_key);
        if let Some(workspace) = self.store.with_state(false, |state| {
            Ok(state.workspaces.get(&workspace_id).cloned())
        })? {
            return Ok(DaemonResponse::Workspace { workspace });
        }
        let prepared_worktree = if isolation == IsolationMode::Worktree {
            let project = self.project_record(project_id)?;
            let project_root = PathBuf::from(&project.canonical_root);
            let base = match requested_base {
                Some(base) => crate::source::resolve_full_commit(&project_root, base)?,
                None => crate::source::head_commit(&project_root)?,
            };
            let workspace_root = self.store.root().join("workspaces").join(&workspace_id);
            let effect_id = format!("effect-worktree-{workspace_id}");
            let existing = self.record_external_effect(
                &effect_id,
                ExternalEffectKind::WorktreeCreate,
                &workspace_id,
                &hash_bytes(format!("{project_id}:{name}:{base}").as_bytes()),
                format!("root={} base={base}", workspace_root.display()),
                None,
            )?;
            if matches!(
                existing.state,
                ExternalEffectState::Attempting | ExternalEffectState::OutcomeUnknown
            ) {
                return Err(external_effect_blocked(&effect_id));
            }
            if existing.state == ExternalEffectState::DefinitivelyFailed {
                return Err(external_effect_blocked(&effect_id));
            }
            if existing.state == ExternalEffectState::Acknowledged {
                if validate_owned_worktree(&project_root, &workspace_root, &base) {
                    Some((workspace_root.to_string_lossy().to_string(), base))
                } else {
                    let _ = self.update_external_effect(
                        &effect_id,
                        ExternalEffectState::OutcomeUnknown,
                        None,
                        Some(
                            "acknowledged worktree could not be adopted or safely validated"
                                .to_string(),
                        ),
                    );
                    return Err(external_effect_blocked(&effect_id));
                }
            } else {
                self.store.check_failpoint("after_external_effect_intent")?;
                self.update_external_effect(
                    &effect_id,
                    ExternalEffectState::Attempting,
                    None,
                    Some(format!(
                        "dispatching worktree root={}",
                        workspace_root.display()
                    )),
                )?;
                if let Err(error) = create_worktree(&project_root, &workspace_root, &base) {
                    let _ = self.update_external_effect(
                        &effect_id,
                        ExternalEffectState::DefinitivelyFailed,
                        None,
                        Some(error.to_string()),
                    );
                    return Err(error);
                }
                self.update_external_effect(
                    &effect_id,
                    ExternalEffectState::Attempting,
                    Some(workspace_root.to_string_lossy().to_string()),
                    Some(format!(
                        "worktree created root={}",
                        workspace_root.display()
                    )),
                )?;
                self.store
                    .check_failpoint("after_worktree_success_before_ack")?;
                self.update_external_effect(
                    &effect_id,
                    ExternalEffectState::Acknowledged,
                    Some(workspace_root.to_string_lossy().to_string()),
                    Some(format!(
                        "created root={} base={base}",
                        workspace_root.display()
                    )),
                )?;
                Some((workspace_root.to_string_lossy().to_string(), base))
            }
        } else {
            None
        };
        if let Err(error) = self.store.check_failpoint("before_workspace_ledger_commit") {
            if isolation == IsolationMode::Worktree {
                let _ = self.update_external_effect(
                    &format!("effect-worktree-{workspace_id}"),
                    ExternalEffectState::OutcomeUnknown,
                    prepared_worktree.as_ref().map(|(root, _)| root.clone()),
                    Some(error.to_string()),
                );
            }
            return Err(error);
        }
        let result =
            self.store.with_state(true, |state| {
                let project = state.projects.get(project_id).cloned().ok_or_else(|| {
                    PulseError::NotFound {
                        subject: format!("project {project_id}"),
                    }
                })?;
                if project.archived_at.is_some() {
                    return Err(PulseError::validation(
                        "project_archived",
                        "cannot create a workspace in an archived project",
                    ));
                }
                if let Some(workspace) = state.workspaces.get(&workspace_id).cloned() {
                    return Ok(DaemonResponse::Workspace { workspace });
                }
                let (root, managed, base_commit) = match isolation {
                    IsolationMode::Local => (
                        project.canonical_root.clone(),
                        false,
                        requested_base.map(str::to_string),
                    ),
                    IsolationMode::Worktree => {
                        let (root, base) = prepared_worktree
                            .clone()
                            .expect("worktree effect is prepared before persistence");
                        (root, true, Some(base))
                    }
                };
                let now = chrono::Utc::now().to_rfc3339();
                let workspace = WorkspaceRecord {
                    schema_version: 1,
                    workspace_id: workspace_id.clone(),
                    project_id: project_id.to_string(),
                    name: name.to_string(),
                    isolation,
                    root,
                    managed,
                    base_commit,
                    lifecycle: WorkspaceLifecycle::Open,
                    created_at: now.clone(),
                    updated_at: now,
                    archived_at: None,
                };
                state
                    .workspaces
                    .insert(workspace_id.clone(), workspace.clone());
                append_event(
                    state,
                    "workspace.created",
                    Some(project_id),
                    Some(&workspace_id),
                    None,
                    json!({"isolation": isolation}),
                );
                Ok(DaemonResponse::Workspace { workspace })
            });
        if result.is_err() && isolation == IsolationMode::Worktree {
            let _ = self.update_external_effect(
                &format!("effect-worktree-{workspace_id}"),
                ExternalEffectState::OutcomeUnknown,
                prepared_worktree.as_ref().map(|(root, _)| root.clone()),
                Some("workspace ledger commit failed after worktree creation".to_string()),
            );
        }
        result
    }

    pub(super) fn workspace_list(
        &self,
        project_id: Option<&str>,
        include_archived: bool,
    ) -> Result<DaemonResponse> {
        self.store.with_state(false, |state| {
            let workspaces = state
                .workspaces
                .values()
                .filter(|item| project_id.map_or(true, |id| item.project_id == id))
                .filter(|item| include_archived || item.lifecycle != WorkspaceLifecycle::Archived)
                .cloned()
                .collect();
            Ok(DaemonResponse::Workspaces { workspaces })
        })
    }

    pub(super) fn workspace_archive(&self, workspace_id: &str) -> Result<DaemonResponse> {
        self.store.with_state(true, |state| {
            if state.sessions.values().any(|session| {
                session.workspace_id == workspace_id
                    && session.lifecycle != SessionLifecycle::Closed
            }) {
                return Err(PulseError::validation(
                    "workspace_has_live_sessions",
                    "close all live sessions before archiving the workspace",
                ));
            }
            let now = chrono::Utc::now().to_rfc3339();
            let workspace =
                state
                    .workspaces
                    .get_mut(workspace_id)
                    .ok_or_else(|| PulseError::NotFound {
                        subject: format!("workspace {workspace_id}"),
                    })?;
            workspace.lifecycle = WorkspaceLifecycle::Archived;
            workspace.archived_at = Some(now.clone());
            workspace.updated_at = now;
            let project_id = workspace.project_id.clone();
            let workspace = workspace.clone();
            append_event(
                state,
                "workspace.archived",
                Some(&project_id),
                Some(workspace_id),
                None,
                Value::Null,
            );
            Ok(DaemonResponse::Workspace { workspace })
        })
    }

    pub(super) fn workspace_restore(&self, workspace_id: &str) -> Result<DaemonResponse> {
        self.store.with_state(true, |state| {
            let now = chrono::Utc::now().to_rfc3339();
            let workspace =
                state
                    .workspaces
                    .get_mut(workspace_id)
                    .ok_or_else(|| PulseError::NotFound {
                        subject: format!("workspace {workspace_id}"),
                    })?;
            if !Path::new(&workspace.root).is_dir() {
                return Err(PulseError::validation(
                    "workspace_root_missing",
                    "workspace root no longer exists",
                ));
            }
            workspace.lifecycle = WorkspaceLifecycle::Open;
            workspace.archived_at = None;
            workspace.updated_at = now;
            let project_id = workspace.project_id.clone();
            let workspace = workspace.clone();
            append_event(
                state,
                "workspace.restored",
                Some(&project_id),
                Some(workspace_id),
                None,
                Value::Null,
            );
            Ok(DaemonResponse::Workspace { workspace })
        })
    }
}

fn validate_name(name: &str) -> Result<()> {
    if name.trim().is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
        return Err(PulseError::validation(
            "workspace_name_invalid",
            "workspace name must be 1..=128 printable characters",
        ));
    }
    Ok(())
}

fn create_worktree(repo_root: &Path, workspace_root: &Path, base_commit: &str) -> Result<()> {
    if workspace_root.exists() {
        return Err(PulseError::AlreadyExists {
            subject: format!("workspace root {}", workspace_root.display()),
        });
    }
    if let Some(parent) = workspace_root.parent() {
        std::fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["worktree", "add", "--detach"])
        .arg(workspace_root)
        .arg(base_commit)
        .output()
        .map_err(|error| PulseError::io(repo_root, error))?;
    if !output.status.success() {
        return Err(PulseError::validation(
            "workspace_create_failed",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(())
}

fn validate_owned_worktree(repo_root: &Path, workspace_root: &Path, base_commit: &str) -> bool {
    if !workspace_root.is_dir() {
        return false;
    }
    let head = Command::new("git")
        .args(["-C"])
        .arg(workspace_root)
        .args(["rev-parse", "HEAD"])
        .output();
    let root = Command::new("git")
        .args(["-C"])
        .arg(repo_root)
        .args(["worktree", "list", "--porcelain"])
        .output();
    let Ok(head) = head else { return false };
    let Ok(root) = root else { return false };
    head.status.success()
        && root.status.success()
        && String::from_utf8_lossy(&head.stdout).trim() == base_commit
        && String::from_utf8_lossy(&root.stdout).contains(workspace_root.to_string_lossy().as_ref())
}
