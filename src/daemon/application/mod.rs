//! Transport-neutral daemon application services.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::canonical_json::{hash_bytes, hash_serializable};
use crate::daemon::assignment::{AssignmentSagaRecord, AssignmentSagaState};
use crate::daemon::permissions::RuntimePrincipal;
use crate::daemon::persistence::{IdempotencyRecord, StateStore};
use crate::daemon::process::{ManagedProcessState, ProcessOwner, SpawnRequest};
use crate::daemon::project::ProjectRecord;
use crate::daemon::protocol::{DaemonRequest, DaemonResponse, ProtocolError, DAEMON_CAPABILITIES};
use crate::daemon::provider::ProviderRegistry;
use crate::daemon::session::{
    CommunicationGrantRecord, SessionLifecycle, SessionMessageRecord, SessionRecord,
};
use crate::daemon::timeline::{TimelineCursor, TimelineEvent, TimelinePage};
use crate::daemon::workspace::{IsolationMode, WorkspaceLifecycle, WorkspaceRecord};
use crate::{PulseError, Result};

pub struct DaemonApplication {
    store: StateStore,
    providers: ProviderRegistry,
    process_owner: ProcessOwner,
    shutdown: Arc<AtomicBool>,
    pid: u32,
    started_at: String,
    endpoint: String,
}

impl DaemonApplication {
    pub fn new(store: StateStore, endpoint: impl Into<String>) -> Result<Self> {
        let application = Self {
            store,
            providers: ProviderRegistry::built_in(),
            process_owner: ProcessOwner::default(),
            shutdown: Arc::new(AtomicBool::new(false)),
            pid: std::process::id(),
            started_at: chrono::Utc::now().to_rfc3339(),
            endpoint: endpoint.into(),
        };
        application.begin_epoch_and_recover()?;
        Ok(application)
    }

    pub fn shutdown_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown)
    }

    pub fn store(&self) -> &StateStore {
        &self.store
    }

    pub fn handle(
        &self,
        request: &DaemonRequest,
        idempotency_key: &str,
    ) -> std::result::Result<DaemonResponse, ProtocolError> {
        self.handle_as(&RuntimePrincipal::local_cli(), request, idempotency_key)
    }

    pub fn handle_as(
        &self,
        principal: &RuntimePrincipal,
        request: &DaemonRequest,
        idempotency_key: &str,
    ) -> std::result::Result<DaemonResponse, ProtocolError> {
        principal
            .require(request.runtime_capability())
            .map_err(|code| ProtocolError::new(code, "runtime capability is required", false))?;
        self.handle_inner(principal, request, idempotency_key)
            .map_err(protocol_error_from_pulse)
    }

    fn handle_inner(
        &self,
        principal: &RuntimePrincipal,
        request: &DaemonRequest,
        idempotency_key: &str,
    ) -> Result<DaemonResponse> {
        if request.is_mutating() && idempotency_key.trim().is_empty() {
            return Err(PulseError::validation(
                "idempotency_key_required",
                "mutating daemon requests require a non-empty idempotency key",
            ));
        }
        let _idempotency_guard = if idempotency_key.is_empty() {
            None
        } else {
            Some(self.store.acquire_idempotency(idempotency_key)?)
        };
        if !idempotency_key.is_empty() {
            let fingerprint = hash_serializable(request)?;
            if let Some(cached) = self.store.with_state(false, |state| {
                Ok(state.idempotency_results.get(idempotency_key).cloned())
            })? {
                if cached.request_fingerprint != fingerprint {
                    return Err(PulseError::validation(
                        "idempotency_key_conflict",
                        "idempotency key was already used for a different request",
                    ));
                }
                return serde_json::from_value(cached.response).map_err(PulseError::from);
            }
        }

        let response = match request {
            DaemonRequest::Handshake { .. } => DaemonResponse::Handshake {
                daemon_version: env!("CARGO_PKG_VERSION").to_string(),
                capabilities: DAEMON_CAPABILITIES
                    .iter()
                    .map(|item| item.to_string())
                    .collect(),
            },
            DaemonRequest::Status => self.status()?,
            DaemonRequest::Shutdown => {
                self.shutdown.store(true, Ordering::SeqCst);
                DaemonResponse::ShuttingDown
            }
            DaemonRequest::ProjectOpen { root } => self.project_open(root)?,
            DaemonRequest::ProjectList { include_archived } => {
                self.project_list(*include_archived)?
            }
            DaemonRequest::ProjectArchive { project_id } => self.project_archive(project_id)?,
            DaemonRequest::WorkspaceCreate {
                project_id,
                name,
                isolation,
                base_commit,
            } => self.workspace_create(
                project_id,
                name,
                *isolation,
                base_commit.as_deref(),
                idempotency_key,
            )?,
            DaemonRequest::WorkspaceList {
                project_id,
                include_archived,
            } => self.workspace_list(project_id.as_deref(), *include_archived)?,
            DaemonRequest::WorkspaceArchive { workspace_id } => {
                self.workspace_archive(workspace_id)?
            }
            DaemonRequest::WorkspaceRestore { workspace_id } => {
                self.workspace_restore(workspace_id)?
            }
            DaemonRequest::SessionCreate {
                workspace_id,
                provider_id,
                parent_session_id,
                provider_options,
            } => self.session_create(
                workspace_id,
                provider_id,
                parent_session_id.as_deref(),
                provider_options,
                idempotency_key,
            )?,
            DaemonRequest::SessionList {
                workspace_id,
                include_archived,
            } => self.session_list(workspace_id.as_deref(), *include_archived)?,
            DaemonRequest::SessionShow { session_id } => self.session_show(session_id)?,
            DaemonRequest::SessionSend { session_id, input } => {
                self.session_send(session_id, input)?
            }
            DaemonRequest::SessionInterrupt { session_id } => self.session_interrupt(session_id)?,
            DaemonRequest::SessionClose { session_id } => self.session_close(session_id)?,
            DaemonRequest::SessionArchive { session_id } => self.session_archive(session_id)?,
            DaemonRequest::SessionCommunicationGrant {
                sender_session_id,
                recipient_session_id,
            } => self.session_communication_grant(
                principal,
                sender_session_id,
                recipient_session_id,
                idempotency_key,
            )?,
            DaemonRequest::SessionMessageSend {
                sender_session_id,
                recipient_session_id,
                body,
            } => self.session_message_send(
                principal,
                sender_session_id,
                recipient_session_id,
                body,
                idempotency_key,
            )?,
            DaemonRequest::SessionMessages { session_id } => self.session_messages(session_id)?,
            DaemonRequest::AssignmentStart {
                project_id,
                ticket_id,
                actor,
                assignee,
                capabilities,
                isolation,
                provider_id,
                provider_options,
                ttl_seconds,
            } => self.assignment_start(
                project_id,
                ticket_id,
                actor,
                assignee,
                capabilities,
                *isolation,
                provider_id,
                provider_options,
                *ttl_seconds,
                idempotency_key,
            )?,
            DaemonRequest::AssignmentAcknowledge {
                saga_id,
                acknowledgement_id,
            } => self.assignment_acknowledge(saga_id, acknowledgement_id)?,
            DaemonRequest::AssignmentInspect { saga_id } => self.assignment_inspect(saga_id)?,
            DaemonRequest::HandoffSubmit {
                saga_id,
                source_commit,
                summary,
                changed_paths,
                evidence_receipt_ids,
            } => self.handoff_submit(
                saga_id,
                source_commit,
                summary,
                changed_paths,
                evidence_receipt_ids,
                idempotency_key,
            )?,
            DaemonRequest::VerificationComplete {
                saga_id,
                actor,
                source_commit,
                disposition,
                summary,
                checks,
            } => self.verification_complete(
                saga_id,
                actor,
                source_commit,
                *disposition,
                summary,
                checks,
                idempotency_key,
            )?,
            DaemonRequest::TimelineList {
                cursor,
                limit,
                session_id,
            } => self.timeline_list(cursor.as_ref(), *limit, session_id.as_deref())?,
            DaemonRequest::TimelineSubscribe {
                cursor,
                limit,
                session_id,
                wait_ms,
            } => self.timeline_subscribe(cursor, *limit, session_id.as_deref(), *wait_ms)?,
        };

        if !idempotency_key.is_empty() {
            let fingerprint = hash_serializable(request)?;
            let value = serde_json::to_value(&response).map_err(PulseError::from)?;
            self.store.with_state(true, |state| {
                state.idempotency_results.insert(
                    idempotency_key.to_string(),
                    IdempotencyRecord {
                        request_fingerprint: fingerprint,
                        response: value,
                        recorded_at: chrono::Utc::now().to_rfc3339(),
                    },
                );
                Ok(())
            })?;
        }
        Ok(response)
    }

    fn begin_epoch_and_recover(&self) -> Result<()> {
        self.store.with_state(true, |state| {
            state.epoch = format!("epoch_{}", ulid::Ulid::new());
            state.next_sequence = 1;
            let process_ids = state.processes.keys().cloned().collect::<Vec<_>>();
            for process_id in process_ids {
                let Some(record) = state.processes.get_mut(&process_id) else {
                    continue;
                };
                if matches!(
                    record.state,
                    ManagedProcessState::Exited | ManagedProcessState::StaleNeedsOperator
                ) {
                    continue;
                }
                record.state = match self.process_owner.classify_recovery(record)? {
                    ManagedProcessState::StaleNeedsOperator => {
                        match self.process_owner.terminate_record(record) {
                            Ok(()) => ManagedProcessState::Exited,
                            Err(_) => ManagedProcessState::StaleNeedsOperator,
                        }
                    }
                    status => status,
                };
                record.updated_at = chrono::Utc::now().to_rfc3339();
                if let Some(session) = state.sessions.get_mut(&record.owner_id) {
                    session.lifecycle = SessionLifecycle::Error;
                    session.last_error = Some(
                        "daemon restarted without an adoptable provider transport; process was not assumed idle"
                            .to_string(),
                    );
                    session.updated_at = chrono::Utc::now().to_rfc3339();
                }
            }
            append_event(
                state,
                "daemon.epoch_started",
                None,
                None,
                None,
                json!({"pid": self.pid}),
            );
            Ok(())
        })?;
        self.reconcile_assignment_sagas()
    }

    fn reconcile_assignment_sagas(&self) -> Result<()> {
        let snapshot = self.store.load()?;
        let mut reconciled = Vec::new();
        for saga in snapshot.assignment_sagas.values() {
            let next = match saga.state {
                AssignmentSagaState::Reserving
                | AssignmentSagaState::Reserved
                | AssignmentSagaState::WorkspaceReady
                | AssignmentSagaState::SessionReady => Some(AssignmentSagaState::Recoverable),
                AssignmentSagaState::Acknowledged => {
                    let project = snapshot.projects.get(&saga.project_id);
                    let lease = saga.lease_id.as_deref();
                    match (project, lease) {
                        (Some(project), Some(lease)) => {
                            let reservations = crate::kernel::reservation::list_reservations(
                                Path::new(&project.canonical_root),
                            )
                            .unwrap_or_default();
                            if reservations.iter().any(|reservation| {
                                reservation.lease_id == lease
                                    && reservation.state
                                        == crate::reservation::ReservationState::Active
                            }) {
                                Some(AssignmentSagaState::Activated)
                            } else {
                                Some(AssignmentSagaState::Recoverable)
                            }
                        }
                        _ => Some(AssignmentSagaState::Recoverable),
                    }
                }
                _ => None,
            };
            if let Some(next) = next {
                reconciled.push((saga.saga_id.clone(), next));
            }
        }
        if reconciled.is_empty() {
            return Ok(());
        }
        self.store.with_state(true, |state| {
            for (saga_id, next) in &reconciled {
                if let Some(saga) = state.assignment_sagas.get_mut(saga_id) {
                    saga.state = *next;
                    saga.last_error = (next == &AssignmentSagaState::Recoverable).then(|| {
                        "daemon restart interrupted assignment provisioning; retry with the original idempotency key"
                            .to_string()
                    });
                    saga.updated_at = chrono::Utc::now().to_rfc3339();
                }
            }
            Ok(())
        })
    }

    fn status(&self) -> Result<DaemonResponse> {
        let epoch = self
            .store
            .with_state(false, |state| Ok(state.epoch.clone()))?;
        Ok(DaemonResponse::Status {
            pid: self.pid,
            epoch,
            started_at: self.started_at.clone(),
            endpoint: self.endpoint.clone(),
        })
    }

    fn project_open(&self, root: &str) -> Result<DaemonResponse> {
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

    fn project_list(&self, include_archived: bool) -> Result<DaemonResponse> {
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

    fn project_archive(&self, project_id: &str) -> Result<DaemonResponse> {
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

    fn workspace_create(
        &self,
        project_id: &str,
        name: &str,
        isolation: IsolationMode,
        requested_base: Option<&str>,
        idempotency_key: &str,
    ) -> Result<DaemonResponse> {
        validate_name(name)?;
        self.store.with_state(true, |state| {
            let project =
                state
                    .projects
                    .get(project_id)
                    .cloned()
                    .ok_or_else(|| PulseError::NotFound {
                        subject: format!("project {project_id}"),
                    })?;
            if project.archived_at.is_some() {
                return Err(PulseError::validation(
                    "project_archived",
                    "cannot create a workspace in an archived project",
                ));
            }
            let workspace_id = deterministic_id("wks", idempotency_key);
            if let Some(workspace) = state.workspaces.get(&workspace_id).cloned() {
                return Ok(DaemonResponse::Workspace { workspace });
            }
            let project_root = PathBuf::from(&project.canonical_root);
            let (root, managed, base_commit) = match isolation {
                IsolationMode::Local => (
                    project.canonical_root.clone(),
                    false,
                    requested_base.map(str::to_string),
                ),
                IsolationMode::Worktree => {
                    let base = match requested_base {
                        Some(base) => crate::source::resolve_full_commit(&project_root, base)?,
                        None => crate::source::head_commit(&project_root)?,
                    };
                    let workspace_root = self.store.root().join("workspaces").join(&workspace_id);
                    create_worktree(&project_root, &workspace_root, &base)?;
                    (
                        workspace_root.to_string_lossy().to_string(),
                        true,
                        Some(base),
                    )
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
        })
    }

    fn workspace_list(
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

    fn workspace_archive(&self, workspace_id: &str) -> Result<DaemonResponse> {
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

    fn workspace_restore(&self, workspace_id: &str) -> Result<DaemonResponse> {
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

    fn session_create(
        &self,
        workspace_id: &str,
        provider_id: &str,
        parent_session_id: Option<&str>,
        provider_options: &Value,
        idempotency_key: &str,
    ) -> Result<DaemonResponse> {
        let provider = self.providers.get(provider_id)?;
        provider.availability()?;
        let launch = provider.launch(provider_options)?;
        let session_id = deterministic_id("ses", idempotency_key);
        let workspace = self.store.with_state(false, |state| {
            if let Some(session) = state.sessions.get(&session_id).cloned() {
                return Ok(Err(DaemonResponse::Session { session }));
            }
            let workspace = state.workspaces.get(workspace_id).cloned().ok_or_else(|| {
                PulseError::NotFound {
                    subject: format!("workspace {workspace_id}"),
                }
            })?;
            if workspace.lifecycle != WorkspaceLifecycle::Open {
                return Err(PulseError::validation(
                    "workspace_not_open",
                    "session creation requires an open workspace",
                ));
            }
            if let Some(parent) = parent_session_id {
                let parent = state
                    .sessions
                    .get(parent)
                    .ok_or_else(|| PulseError::NotFound {
                        subject: format!("parent session {parent}"),
                    })?;
                if parent.project_id != workspace.project_id {
                    return Err(PulseError::validation(
                        "session_parent_project_mismatch",
                        "parent and child sessions must belong to the same project",
                    ));
                }
            }
            Ok(Ok(workspace))
        })?;
        let workspace = match workspace {
            Ok(workspace) => workspace,
            Err(existing) => return Ok(existing),
        };
        let process = self.process_owner.spawn(SpawnRequest {
            owner_kind: "session",
            owner_id: &session_id,
            provider_id,
            executable: &launch.executable,
            args: &launch.args,
            cwd: Path::new(&workspace.root),
            log_root: &self.store.root().join("logs"),
            max_log_bytes: 4 * 1024 * 1024,
        })?;
        let process_id = process.process_id.clone();
        let provider_session = (|| -> Result<(Option<String>, Vec<Value>)> {
            if !launch.native_protocol {
                return Ok((None, Vec::new()));
            }
            let initialize = provider.initialize_request()?;
            let (_, mut notifications) = self.process_owner.request_json(
                &process_id,
                &initialize.request_id,
                &initialize.message,
                Duration::from_secs(10),
            )?;
            self.process_owner
                .send_line(&process_id, &provider.initialized_notification()?)?;
            let create = provider.create_session_request(&workspace.root, provider_options)?;
            let (response, create_notifications) = self.process_owner.request_json(
                &process_id,
                &create.request_id,
                &create.message,
                Duration::from_secs(30),
            )?;
            notifications.extend(create_notifications);
            Ok((
                Some(provider.parse_session_handle(&response)?),
                notifications,
            ))
        })();
        let (provider_handle, provider_notifications) = match provider_session {
            Ok(provider_session) => provider_session,
            Err(error) => {
                let _ = self.process_owner.terminate(&process_id);
                return Err(error);
            }
        };
        let result = self.store.with_state(true, |state| {
            let now = chrono::Utc::now().to_rfc3339();
            let session = SessionRecord {
                schema_version: 1,
                session_id: session_id.clone(),
                project_id: workspace.project_id.clone(),
                workspace_id: workspace_id.to_string(),
                provider_id: provider_id.to_string(),
                provider_handle: provider_handle.clone(),
                managed_process_id: Some(process_id.clone()),
                parent_session_id: parent_session_id.map(str::to_string),
                lifecycle: SessionLifecycle::Idle,
                archived_at: None,
                active_turn_id: None,
                last_error: None,
                provider_detail: launch.provider_detail.clone(),
                created_at: now.clone(),
                updated_at: now,
            };
            state.processes.insert(process_id.clone(), process);
            state.sessions.insert(session_id.clone(), session.clone());
            for notification in provider_notifications {
                append_event(
                    state,
                    "provider.notification",
                    Some(&workspace.project_id),
                    Some(workspace_id),
                    Some(&session_id),
                    notification,
                );
            }
            append_event(
                state,
                "session.created",
                Some(&workspace.project_id),
                Some(workspace_id),
                Some(&session_id),
                json!({"provider_id": provider_id}),
            );
            Ok(DaemonResponse::Session { session })
        });
        if result.is_err() {
            let _ = self.process_owner.terminate(&process_id);
        }
        result
    }

    fn session_list(
        &self,
        workspace_id: Option<&str>,
        include_archived: bool,
    ) -> Result<DaemonResponse> {
        self.refresh_all_provider_events()?;
        self.store.with_state(false, |state| {
            let sessions = state
                .sessions
                .values()
                .filter(|item| workspace_id.map_or(true, |id| item.workspace_id == id))
                .filter(|item| include_archived || item.archived_at.is_none())
                .cloned()
                .collect();
            Ok(DaemonResponse::Sessions { sessions })
        })
    }

    fn session_show(&self, session_id: &str) -> Result<DaemonResponse> {
        self.refresh_session_provider_events(session_id)?;
        self.store.with_state(false, |state| {
            let session =
                state
                    .sessions
                    .get(session_id)
                    .cloned()
                    .ok_or_else(|| PulseError::NotFound {
                        subject: format!("session {session_id}"),
                    })?;
            Ok(DaemonResponse::Session { session })
        })
    }

    fn session_send(&self, session_id: &str, input: &str) -> Result<DaemonResponse> {
        if input.trim().is_empty() {
            return Err(PulseError::validation(
                "session_input_empty",
                "session input must not be empty",
            ));
        }
        let _session_guard = self
            .store
            .acquire_idempotency(&format!("session-operation:{session_id}"))?;
        let snapshot = self.store.with_state(false, |state| {
            state
                .sessions
                .get(session_id)
                .cloned()
                .ok_or_else(|| PulseError::NotFound {
                    subject: format!("session {session_id}"),
                })
        })?;
        if snapshot.lifecycle != SessionLifecycle::Idle {
            return Err(PulseError::validation(
                "session_not_idle",
                "a new turn requires an idle session",
            ));
        }
        let process_id = snapshot.managed_process_id.clone().ok_or_else(|| {
            PulseError::validation(
                "provider_handle_missing",
                "session has no provider process handle",
            )
        })?;
        let provider = self.providers.get(&snapshot.provider_id)?;
        let (turn_id, notifications) =
            if let Some(provider_handle) = snapshot.provider_handle.as_deref() {
                let request = provider.encode_send(provider_handle, input)?;
                let (response, notifications) = self.process_owner.request_json(
                    &process_id,
                    &request.request_id,
                    &request.message,
                    Duration::from_secs(30),
                )?;
                (provider.parse_turn_handle(&response)?, notifications)
            } else {
                self.process_owner.send_line(&process_id, input)?;
                (format!("turn_{}", ulid::Ulid::new()), Vec::new())
            };
        self.store.with_state(true, |state| {
            for notification in notifications {
                append_event(
                    state,
                    "provider.notification",
                    Some(&snapshot.project_id),
                    Some(&snapshot.workspace_id),
                    Some(session_id),
                    notification,
                );
            }
            let session = state
                .sessions
                .get_mut(session_id)
                .expect("snapshot existed");
            session.lifecycle = SessionLifecycle::Running;
            session.active_turn_id = Some(turn_id.clone());
            session.updated_at = chrono::Utc::now().to_rfc3339();
            let session = session.clone();
            append_event(
                state,
                "session.turn_started",
                Some(&session.project_id),
                Some(&session.workspace_id),
                Some(session_id),
                json!({"turn_id": turn_id}),
            );
            Ok(DaemonResponse::Session { session })
        })
    }

    fn session_interrupt(&self, session_id: &str) -> Result<DaemonResponse> {
        let _session_guard = self
            .store
            .acquire_idempotency(&format!("session-operation:{session_id}"))?;
        let snapshot = self.store.with_state(false, |state| {
            state
                .sessions
                .get(session_id)
                .cloned()
                .ok_or_else(|| PulseError::NotFound {
                    subject: format!("session {session_id}"),
                })
        })?;
        if snapshot.lifecycle != SessionLifecycle::Running {
            return Err(PulseError::validation(
                "session_not_running",
                "interrupt requires a running session",
            ));
        }
        let acknowledged = if let (Some(provider_handle), Some(turn_handle), Some(process_id)) = (
            snapshot.provider_handle.as_deref(),
            snapshot.active_turn_id.as_deref(),
            snapshot.managed_process_id.as_deref(),
        ) {
            let provider = self.providers.get(&snapshot.provider_id)?;
            let request = provider.encode_interrupt(provider_handle, turn_handle)?;
            self.process_owner.request_json(
                process_id,
                &request.request_id,
                &request.message,
                Duration::from_secs(10),
            )?;
            true
        } else {
            false
        };
        self.store.with_state(true, |state| {
            if acknowledged {
                let session = state
                    .sessions
                    .get_mut(session_id)
                    .expect("snapshot existed");
                session.lifecycle = SessionLifecycle::Idle;
                session.active_turn_id = None;
                session.last_error = None;
                session.updated_at = chrono::Utc::now().to_rfc3339();
                let session = session.clone();
                append_event(
                    state,
                    "session.interrupted",
                    Some(&session.project_id),
                    Some(&session.workspace_id),
                    Some(session_id),
                    Value::Null,
                );
                return Ok(DaemonResponse::Session { session });
            }
            let session = state
                .sessions
                .get_mut(session_id)
                .expect("snapshot existed");
            session.last_error = Some(
                "provider interrupt acknowledgement is unavailable; session remains running"
                    .to_string(),
            );
            session.updated_at = chrono::Utc::now().to_rfc3339();
            let project_id = session.project_id.clone();
            let workspace_id = session.workspace_id.clone();
            let session = session.clone();
            append_event(
                state,
                "session.interrupt_unacknowledged",
                Some(&project_id),
                Some(&workspace_id),
                Some(session_id),
                Value::Null,
            );
            Ok(DaemonResponse::Session { session })
        })
    }

    fn session_close(&self, session_id: &str) -> Result<DaemonResponse> {
        let _session_guard = self
            .store
            .acquire_idempotency(&format!("session-operation:{session_id}"))?;
        let process_id = self.store.with_state(false, |state| {
            let session = state
                .sessions
                .get(session_id)
                .ok_or_else(|| PulseError::NotFound {
                    subject: format!("session {session_id}"),
                })?;
            Ok(session.managed_process_id.clone())
        })?;
        if let Some(process_id) = process_id.as_deref() {
            self.process_owner.terminate(process_id)?;
        }
        self.store.with_state(true, |state| {
            let session =
                state
                    .sessions
                    .get_mut(session_id)
                    .ok_or_else(|| PulseError::NotFound {
                        subject: format!("session {session_id}"),
                    })?;
            session.lifecycle = SessionLifecycle::Closed;
            session.active_turn_id = None;
            session.updated_at = chrono::Utc::now().to_rfc3339();
            let project_id = session.project_id.clone();
            let workspace_id = session.workspace_id.clone();
            let session = session.clone();
            if let Some(process_id) = process_id.as_deref() {
                if let Some(process) = state.processes.get_mut(process_id) {
                    process.state = ManagedProcessState::Exited;
                    process.updated_at = chrono::Utc::now().to_rfc3339();
                }
            }
            append_event(
                state,
                "session.closed",
                Some(&project_id),
                Some(&workspace_id),
                Some(session_id),
                Value::Null,
            );
            Ok(DaemonResponse::Session { session })
        })
    }

    fn session_archive(&self, session_id: &str) -> Result<DaemonResponse> {
        self.store.with_state(true, |state| {
            let session =
                state
                    .sessions
                    .get_mut(session_id)
                    .ok_or_else(|| PulseError::NotFound {
                        subject: format!("session {session_id}"),
                    })?;
            if session.lifecycle != SessionLifecycle::Closed {
                return Err(PulseError::validation(
                    "session_not_closed",
                    "close a session before archiving it",
                ));
            }
            session.archived_at = Some(chrono::Utc::now().to_rfc3339());
            session.updated_at = chrono::Utc::now().to_rfc3339();
            let project_id = session.project_id.clone();
            let workspace_id = session.workspace_id.clone();
            let session = session.clone();
            append_event(
                state,
                "session.archived",
                Some(&project_id),
                Some(&workspace_id),
                Some(session_id),
                Value::Null,
            );
            Ok(DaemonResponse::Session { session })
        })
    }

    fn session_communication_grant(
        &self,
        principal: &RuntimePrincipal,
        sender_session_id: &str,
        recipient_session_id: &str,
        idempotency_key: &str,
    ) -> Result<DaemonResponse> {
        if sender_session_id == recipient_session_id {
            return Err(PulseError::validation(
                "session_communication_self_grant",
                "session communication grants require distinct sessions",
            ));
        }
        self.store.with_state(true, |state| {
            let sender =
                state
                    .sessions
                    .get(sender_session_id)
                    .ok_or_else(|| PulseError::NotFound {
                        subject: format!("sender session {sender_session_id}"),
                    })?;
            let recipient =
                state
                    .sessions
                    .get(recipient_session_id)
                    .ok_or_else(|| PulseError::NotFound {
                        subject: format!("recipient session {recipient_session_id}"),
                    })?;
            if sender.project_id != recipient.project_id {
                return Err(PulseError::validation(
                    "session_communication_project_mismatch",
                    "communication grants require sessions in the same project",
                ));
            }
            let project_id = sender.project_id.clone();
            let workspace_id = sender.workspace_id.clone();
            let now = chrono::Utc::now().to_rfc3339();
            let grant = CommunicationGrantRecord {
                schema_version: 1,
                grant_id: deterministic_id("grant", idempotency_key),
                sender_session_id: sender_session_id.to_string(),
                recipient_session_id: recipient_session_id.to_string(),
                granted_by: principal.principal_id.clone(),
                created_at: now,
            };
            state
                .communication_grants
                .insert(grant.grant_id.clone(), grant.clone());
            append_event(
                state,
                "session.communication_granted",
                Some(&project_id),
                Some(&workspace_id),
                Some(sender_session_id),
                json!({
                    "grant_id": grant.grant_id,
                    "recipient_session_id": recipient_session_id
                }),
            );
            Ok(DaemonResponse::CommunicationGrant { grant })
        })
    }

    fn session_message_send(
        &self,
        principal: &RuntimePrincipal,
        sender_session_id: &str,
        recipient_session_id: &str,
        body: &str,
        idempotency_key: &str,
    ) -> Result<DaemonResponse> {
        principal
            .require_session_sender(sender_session_id)
            .map_err(|code| {
                PulseError::validation(
                    code,
                    "message sender must match the authenticated session or an administrator",
                )
            })?;
        if body.trim().is_empty() || body.len() > 64 * 1024 {
            return Err(PulseError::validation(
                "session_message_invalid",
                "session message must contain between 1 and 65536 bytes",
            ));
        }
        self.store.with_state(true, |state| {
            let sender =
                state
                    .sessions
                    .get(sender_session_id)
                    .ok_or_else(|| PulseError::NotFound {
                        subject: format!("sender session {sender_session_id}"),
                    })?;
            let recipient =
                state
                    .sessions
                    .get(recipient_session_id)
                    .ok_or_else(|| PulseError::NotFound {
                        subject: format!("recipient session {recipient_session_id}"),
                    })?;
            if sender.lifecycle == SessionLifecycle::Closed
                || recipient.lifecycle == SessionLifecycle::Closed
                || sender.archived_at.is_some()
                || recipient.archived_at.is_some()
            {
                return Err(PulseError::validation(
                    "session_communication_inactive",
                    "messages require active, unarchived sender and recipient sessions",
                ));
            }
            let allowed = state.communication_grants.values().any(|grant| {
                grant.sender_session_id == sender_session_id
                    && grant.recipient_session_id == recipient_session_id
            });
            if !allowed {
                return Err(PulseError::validation(
                    "session_communication_denied",
                    "an explicit communication grant is required; parentage is not authority",
                ));
            }
            let project_id = sender.project_id.clone();
            let workspace_id = sender.workspace_id.clone();
            let message = SessionMessageRecord {
                schema_version: 1,
                message_id: deterministic_id("msg", idempotency_key),
                sender_session_id: sender_session_id.to_string(),
                recipient_session_id: recipient_session_id.to_string(),
                body: body.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            state
                .session_messages
                .insert(message.message_id.clone(), message.clone());
            append_event(
                state,
                "session.message_sent",
                Some(&project_id),
                Some(&workspace_id),
                Some(sender_session_id),
                json!({
                    "message_id": message.message_id,
                    "recipient_session_id": recipient_session_id
                }),
            );
            Ok(DaemonResponse::SessionMessage { message })
        })
    }

    fn session_messages(&self, session_id: &str) -> Result<DaemonResponse> {
        self.store.with_state(false, |state| {
            if !state.sessions.contains_key(session_id) {
                return Err(PulseError::NotFound {
                    subject: format!("session {session_id}"),
                });
            }
            let mut messages = state
                .session_messages
                .values()
                .filter(|message| message.recipient_session_id == session_id)
                .cloned()
                .collect::<Vec<_>>();
            messages.sort_by(|left, right| {
                left.created_at
                    .cmp(&right.created_at)
                    .then_with(|| left.message_id.cmp(&right.message_id))
            });
            Ok(DaemonResponse::SessionMessages { messages })
        })
    }

    fn timeline_list(
        &self,
        cursor: Option<&TimelineCursor>,
        limit: usize,
        session_id: Option<&str>,
    ) -> Result<DaemonResponse> {
        self.refresh_all_provider_events()?;
        if !(1..=1000).contains(&limit) {
            return Err(PulseError::validation(
                "timeline_limit_invalid",
                "timeline limit must be between 1 and 1000",
            ));
        }
        self.store.with_state(false, |state| {
            let start = match cursor {
                None => 0,
                Some(cursor) => state
                    .timeline
                    .iter()
                    .position(|event| {
                        event.epoch == cursor.epoch && event.sequence == cursor.sequence
                    })
                    .map(|index| index + 1)
                    .ok_or_else(|| {
                        PulseError::validation(
                            "timeline_cursor_unknown",
                            "timeline cursor is not present in the authoritative log",
                        )
                    })?,
            };
            let filtered = state.timeline[start..]
                .iter()
                .filter(|event| {
                    session_id.map_or(true, |id| event.session_id.as_deref() == Some(id))
                })
                .cloned()
                .collect::<Vec<_>>();
            let events = filtered.iter().take(limit).cloned().collect::<Vec<_>>();
            let has_newer = filtered.len() > events.len();
            let next_cursor = events
                .last()
                .map(|event| TimelineCursor {
                    epoch: event.epoch.clone(),
                    sequence: event.sequence,
                })
                .or_else(|| cursor.cloned())
                .unwrap_or_else(|| TimelineCursor {
                    epoch: state.epoch.clone(),
                    sequence: 0,
                });
            Ok(DaemonResponse::Timeline {
                page: TimelinePage {
                    events,
                    next_cursor,
                    has_newer,
                },
            })
        })
    }

    fn refresh_all_provider_events(&self) -> Result<()> {
        let session_ids = self.store.with_state(false, |state| {
            Ok(state.sessions.keys().cloned().collect::<Vec<_>>())
        })?;
        for session_id in session_ids {
            self.refresh_session_provider_events(&session_id)?;
        }
        Ok(())
    }

    fn refresh_session_provider_events(&self, session_id: &str) -> Result<()> {
        let snapshot = self
            .store
            .with_state(false, |state| Ok(state.sessions.get(session_id).cloned()))?;
        let Some(snapshot) = snapshot else {
            return Ok(());
        };
        let Some(process_id) = snapshot.managed_process_id.as_deref() else {
            return Ok(());
        };
        let events = self.process_owner.drain_json(process_id)?;
        if events.is_empty() {
            return Ok(());
        }
        self.store.with_state(true, |state| {
            for event in events {
                let method = event.get("method").and_then(Value::as_str);
                if method == Some("turn/completed") {
                    let completed_turn = event.pointer("/params/turn/id").and_then(Value::as_str);
                    if let Some(session) = state.sessions.get_mut(session_id) {
                        if completed_turn.is_none()
                            || completed_turn == session.active_turn_id.as_deref()
                        {
                            session.lifecycle = SessionLifecycle::Idle;
                            session.active_turn_id = None;
                            session.updated_at = chrono::Utc::now().to_rfc3339();
                        }
                    }
                } else if method == Some("thread/started") {
                    let provider_handle =
                        event.pointer("/params/thread/id").and_then(Value::as_str);
                    if let (Some(session), Some(provider_handle)) =
                        (state.sessions.get_mut(session_id), provider_handle)
                    {
                        session.provider_handle = Some(provider_handle.to_string());
                        session.updated_at = chrono::Utc::now().to_rfc3339();
                    }
                }
                append_event(
                    state,
                    "provider.notification",
                    Some(&snapshot.project_id),
                    Some(&snapshot.workspace_id),
                    Some(session_id),
                    event,
                );
            }
            Ok(())
        })
    }

    fn timeline_subscribe(
        &self,
        cursor: &TimelineCursor,
        limit: usize,
        session_id: Option<&str>,
        wait_ms: u64,
    ) -> Result<DaemonResponse> {
        if !(1..=30_000).contains(&wait_ms) {
            return Err(PulseError::validation(
                "timeline_wait_invalid",
                "timeline subscription wait must be between 1 and 30000 milliseconds",
            ));
        }
        let deadline = Instant::now() + Duration::from_millis(wait_ms);
        loop {
            let response = self.timeline_list(Some(cursor), limit, session_id)?;
            if matches!(
                &response,
                DaemonResponse::Timeline { page } if !page.events.is_empty()
            ) || Instant::now() >= deadline
            {
                return Ok(response);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn assignment_start(
        &self,
        project_id: &str,
        ticket_id: &str,
        actor: &str,
        assignee: &str,
        capabilities: &[String],
        isolation: IsolationMode,
        provider_id: &str,
        provider_options: &Value,
        ttl_seconds: u64,
        idempotency_key: &str,
    ) -> Result<DaemonResponse> {
        let saga_id = deterministic_id("saga", idempotency_key);
        if let Some(existing) = self.store.with_state(false, |state| {
            Ok(state.assignment_sagas.get(&saga_id).cloned())
        })? {
            if matches!(
                existing.state,
                AssignmentSagaState::BootstrapDelivered
                    | AssignmentSagaState::Acknowledged
                    | AssignmentSagaState::Activated
            ) {
                return Ok(DaemonResponse::Assignment { saga: existing });
            }
        } else {
            let project = self.store.with_state(false, |state| {
                state
                    .projects
                    .get(project_id)
                    .cloned()
                    .ok_or_else(|| PulseError::NotFound {
                        subject: format!("project {project_id}"),
                    })
            })?;
            let now = chrono::Utc::now().to_rfc3339();
            let saga = AssignmentSagaRecord {
                schema_version: 1,
                saga_id: saga_id.clone(),
                idempotency_key: idempotency_key.to_string(),
                project_id: project.project_id,
                ticket_id: ticket_id.to_string(),
                actor: actor.to_string(),
                assignee: assignee.to_string(),
                ticket_revision: 0,
                packet_fingerprint: String::new(),
                lease_id: None,
                workspace_id: None,
                session_id: None,
                delivery_id: None,
                acknowledgement_id: None,
                handoff_id: None,
                verification_id: None,
                state: AssignmentSagaState::Reserving,
                last_error: None,
                created_at: now.clone(),
                updated_at: now,
            };
            self.store.with_state(true, |state| {
                state.assignment_sagas.insert(saga_id.clone(), saga);
                Ok(())
            })?;
        }

        let project = self.store.with_state(false, |state| {
            state
                .projects
                .get(project_id)
                .cloned()
                .ok_or_else(|| PulseError::NotFound {
                    subject: format!("project {project_id}"),
                })
        })?;
        let core = crate::JsonGraphStore::new(&project.canonical_root);
        let inventory = serde_json::to_vec(&json!({
            "schema_version": 1,
            "principal": assignee,
            "inventory_id": format!("daemon:{}", saga_id),
            "capabilities": capabilities,
        }))?;
        let reservation = match core.reserve_work(crate::reservation::ReserveWorkArgs {
            ticket_id: ticket_id.to_string(),
            actor: actor.to_string(),
            assignee: assignee.to_string(),
            capability_inventory_bytes: inventory,
            ttl_seconds,
            idempotency_key: format!("{idempotency_key}:core-reserve"),
        }) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.mark_saga_error(&saga_id, AssignmentSagaState::Recoverable, &error)?;
                return Err(error);
            }
        };
        self.store.with_state(true, |state| {
            let saga = state
                .assignment_sagas
                .get_mut(&saga_id)
                .expect("saga exists");
            saga.ticket_revision = reservation.reservation.subject.ticket_revision;
            saga.packet_fingerprint = reservation.reservation.packet_fingerprint.clone();
            saga.lease_id = Some(reservation.reservation.lease_id.clone());
            saga.state = AssignmentSagaState::Reserved;
            saga.updated_at = chrono::Utc::now().to_rfc3339();
            Ok(())
        })?;

        let workspace = match self.workspace_create(
            project_id,
            &format!("assignment-{ticket_id}"),
            isolation,
            Some(&reservation.packet.source.commit),
            &format!("{idempotency_key}:workspace"),
        ) {
            Ok(DaemonResponse::Workspace { workspace }) => workspace,
            Ok(_) => unreachable!("workspace_create response kind"),
            Err(error) => {
                let _ = core.release_reservation(
                    &reservation.reservation.lease_id,
                    actor,
                    "workspace provisioning failed",
                );
                self.mark_saga_error(&saga_id, AssignmentSagaState::Recoverable, &error)?;
                return Err(error);
            }
        };
        self.store.with_state(true, |state| {
            let saga = state
                .assignment_sagas
                .get_mut(&saga_id)
                .expect("saga exists");
            saga.workspace_id = Some(workspace.workspace_id.clone());
            saga.state = AssignmentSagaState::WorkspaceReady;
            saga.updated_at = chrono::Utc::now().to_rfc3339();
            Ok(())
        })?;

        let session = match self.session_create(
            &workspace.workspace_id,
            provider_id,
            None,
            provider_options,
            &format!("{idempotency_key}:session"),
        ) {
            Ok(DaemonResponse::Session { session }) => session,
            Ok(_) => unreachable!("session_create response kind"),
            Err(error) => {
                let _ = self.workspace_archive(&workspace.workspace_id);
                let _ = core.release_reservation(
                    &reservation.reservation.lease_id,
                    actor,
                    "session provisioning failed",
                );
                self.mark_saga_error(&saga_id, AssignmentSagaState::Recoverable, &error)?;
                return Err(error);
            }
        };
        self.store.with_state(true, |state| {
            let saga = state
                .assignment_sagas
                .get_mut(&saga_id)
                .expect("saga exists");
            saga.session_id = Some(session.session_id.clone());
            saga.state = AssignmentSagaState::SessionReady;
            saga.updated_at = chrono::Utc::now().to_rfc3339();
            Ok(())
        })?;

        let delivery_id = deterministic_id("delivery", idempotency_key);
        let bootstrap = format!(
            "Pulse assignment {ticket_id}\nlease={}\npacket_fingerprint={}\nload: pulse work packet {ticket_id} --lease {} --json\nAuthority: implement only the exact lease-bound contract. Submit typed handoff evidence; process exit is not completion.",
            reservation.reservation.lease_id,
            reservation.reservation.packet_fingerprint,
            reservation.reservation.lease_id,
        );
        if let Err(error) = self.session_send(&session.session_id, &bootstrap) {
            let _ = self.session_close(&session.session_id);
            let _ = self.workspace_archive(&workspace.workspace_id);
            let released = core
                .release_reservation(
                    &reservation.reservation.lease_id,
                    actor,
                    "bootstrap delivery failed",
                )
                .is_ok();
            self.mark_saga_error(
                &saga_id,
                if released {
                    AssignmentSagaState::Released
                } else {
                    AssignmentSagaState::Recoverable
                },
                &error,
            )?;
            return Err(error);
        }
        self.store.with_state(true, |state| {
            let saga = state
                .assignment_sagas
                .get_mut(&saga_id)
                .expect("saga exists");
            saga.delivery_id = Some(delivery_id.clone());
            saga.state = AssignmentSagaState::BootstrapDelivered;
            saga.updated_at = chrono::Utc::now().to_rfc3339();
            let saga = saga.clone();
            append_event(
                state,
                "assignment.bootstrap_delivered",
                Some(project_id),
                Some(&workspace.workspace_id),
                Some(&session.session_id),
                json!({"saga_id": saga_id, "delivery_id": delivery_id}),
            );
            Ok(DaemonResponse::Assignment { saga })
        })
    }

    fn assignment_acknowledge(
        &self,
        saga_id: &str,
        acknowledgement_id: &str,
    ) -> Result<DaemonResponse> {
        if acknowledgement_id.trim().is_empty() {
            return Err(PulseError::validation(
                "assignment_acknowledgement_invalid",
                "acknowledgement ID must not be empty",
            ));
        }
        let saga = self.store.with_state(false, |state| {
            state
                .assignment_sagas
                .get(saga_id)
                .cloned()
                .ok_or_else(|| PulseError::NotFound {
                    subject: format!("assignment saga {saga_id}"),
                })
        })?;
        if saga.state == AssignmentSagaState::Activated {
            return Ok(DaemonResponse::Assignment { saga });
        }
        if saga.state != AssignmentSagaState::BootstrapDelivered
            && saga.state != AssignmentSagaState::Acknowledged
        {
            return Err(PulseError::validation(
                "assignment_not_acknowledgeable",
                "assignment bootstrap has not been delivered",
            ));
        }
        let lease_id = saga.lease_id.clone().ok_or_else(|| {
            PulseError::validation("assignment_saga_invalid", "saga has no lease")
        })?;
        let workspace_id = saga.workspace_id.clone().ok_or_else(|| {
            PulseError::validation("assignment_saga_invalid", "saga has no workspace")
        })?;
        let session_id = saga.session_id.clone().ok_or_else(|| {
            PulseError::validation("assignment_saga_invalid", "saga has no session")
        })?;
        let delivery_id = saga.delivery_id.clone().ok_or_else(|| {
            PulseError::validation("assignment_saga_invalid", "saga has no delivery")
        })?;
        let project = self.store.with_state(false, |state| {
            state
                .projects
                .get(&saga.project_id)
                .cloned()
                .ok_or_else(|| PulseError::NotFound {
                    subject: format!("project {}", saga.project_id),
                })
        })?;
        self.store.with_state(true, |state| {
            let saga = state
                .assignment_sagas
                .get_mut(saga_id)
                .expect("saga exists");
            saga.acknowledgement_id = Some(acknowledgement_id.to_string());
            saga.state = AssignmentSagaState::Acknowledged;
            saga.updated_at = chrono::Utc::now().to_rfc3339();
            Ok(())
        })?;
        let session = self.store.with_state(false, |state| {
            state
                .sessions
                .get(&session_id)
                .cloned()
                .ok_or_else(|| PulseError::NotFound {
                    subject: format!("session {session_id}"),
                })
        })?;
        let core = crate::JsonGraphStore::new(&project.canonical_root);
        let activation = core.activate_reservation(crate::reservation::ActivateReservationArgs {
            lease_id,
            actor: saga.actor.clone(),
            runtime_binding: crate::reservation::RuntimeBinding {
                project_id: saga.project_id.clone(),
                workspace_id: workspace_id.clone(),
                session_id: session_id.clone(),
                provider_id: session.provider_id,
            },
            acknowledgement: crate::reservation::AssignmentAcknowledgement {
                acknowledgement_id: acknowledgement_id.to_string(),
                delivery_id,
                session_id: session_id.clone(),
                packet_fingerprint: saga.packet_fingerprint.clone(),
                acknowledged_at: chrono::Utc::now().to_rfc3339(),
            },
        });
        match activation {
            Ok(_) => self.store.with_state(true, |state| {
                let saga = state
                    .assignment_sagas
                    .get_mut(saga_id)
                    .expect("saga exists");
                saga.state = AssignmentSagaState::Activated;
                saga.updated_at = chrono::Utc::now().to_rfc3339();
                let saga = saga.clone();
                append_event(
                    state,
                    "assignment.activated",
                    Some(&saga.project_id),
                    saga.workspace_id.as_deref(),
                    saga.session_id.as_deref(),
                    json!({"saga_id": saga_id, "acknowledgement_id": acknowledgement_id}),
                );
                Ok(DaemonResponse::Assignment { saga })
            }),
            Err(error) => {
                let _ = self.session_close(&session_id);
                let _ = self.workspace_archive(&workspace_id);
                let released = core
                    .release_reservation(
                        &saga
                            .lease_id
                            .clone()
                            .expect("validated assignment saga lease"),
                        &saga.actor,
                        "Core activation rejected",
                    )
                    .is_ok();
                self.mark_saga_error(
                    saga_id,
                    if released {
                        AssignmentSagaState::Released
                    } else {
                        AssignmentSagaState::Recoverable
                    },
                    &error,
                )?;
                Err(error)
            }
        }
    }

    fn assignment_inspect(&self, saga_id: &str) -> Result<DaemonResponse> {
        self.store.with_state(false, |state| {
            let saga = state
                .assignment_sagas
                .get(saga_id)
                .cloned()
                .ok_or_else(|| PulseError::NotFound {
                    subject: format!("assignment saga {saga_id}"),
                })?;
            Ok(DaemonResponse::Assignment { saga })
        })
    }

    fn mark_saga_error(
        &self,
        saga_id: &str,
        state_value: AssignmentSagaState,
        error: &PulseError,
    ) -> Result<()> {
        self.store.with_state(true, |state| {
            let saga =
                state
                    .assignment_sagas
                    .get_mut(saga_id)
                    .ok_or_else(|| PulseError::NotFound {
                        subject: format!("assignment saga {saga_id}"),
                    })?;
            saga.state = state_value;
            saga.last_error = Some(format!("{}: {}", error.code(), error));
            saga.updated_at = chrono::Utc::now().to_rfc3339();
            Ok(())
        })
    }

    fn handoff_submit(
        &self,
        saga_id: &str,
        source_commit: &str,
        summary: &str,
        changed_paths: &[String],
        evidence_receipt_ids: &[String],
        idempotency_key: &str,
    ) -> Result<DaemonResponse> {
        let saga = self.assignment_saga(saga_id)?;
        if saga.state != AssignmentSagaState::Activated
            && saga.state != AssignmentSagaState::Verifying
        {
            return Err(PulseError::validation(
                "assignment_not_active",
                "handoff requires an activated assignment",
            ));
        }
        let project = self.project_record(&saga.project_id)?;
        let lease_id = saga.lease_id.clone().ok_or_else(|| {
            PulseError::validation("assignment_saga_invalid", "saga has no lease")
        })?;
        let session_id = saga.session_id.clone().ok_or_else(|| {
            PulseError::validation("assignment_saga_invalid", "saga has no session")
        })?;
        let core = crate::JsonGraphStore::new(&project.canonical_root);
        let handoff = core.submit_execution_handoff(crate::execution::SubmitHandoffArgs {
            lease_id,
            actor: saga.actor,
            session_id,
            source_commit: source_commit.to_string(),
            summary: summary.to_string(),
            changed_paths: changed_paths.to_vec(),
            evidence_receipt_ids: evidence_receipt_ids.to_vec(),
            idempotency_key: format!("{idempotency_key}:core-handoff"),
        })?;
        self.store.with_state(true, |state| {
            let saga = state
                .assignment_sagas
                .get_mut(saga_id)
                .expect("saga exists");
            saga.state = AssignmentSagaState::Verifying;
            saga.handoff_id = Some(handoff.handoff_id.clone());
            saga.updated_at = chrono::Utc::now().to_rfc3339();
            append_event(
                state,
                "assignment.handoff_submitted",
                Some(&handoff.project_id),
                Some(&handoff.workspace_id),
                Some(&handoff.session_id),
                json!({"saga_id": saga_id, "handoff_id": handoff.handoff_id}),
            );
            Ok(())
        })?;
        Ok(DaemonResponse::Handoff { handoff })
    }

    #[allow(clippy::too_many_arguments)]
    fn verification_complete(
        &self,
        saga_id: &str,
        actor: &str,
        source_commit: &str,
        disposition: crate::execution::VerificationDisposition,
        summary: &str,
        checks: &[crate::execution::VerificationCheck],
        idempotency_key: &str,
    ) -> Result<DaemonResponse> {
        let saga = self.assignment_saga(saga_id)?;
        if saga.state != AssignmentSagaState::Verifying {
            return Err(PulseError::validation(
                "assignment_not_verifying",
                "verification completion requires a submitted handoff",
            ));
        }
        let project = self.project_record(&saga.project_id)?;
        let core = crate::JsonGraphStore::new(&project.canonical_root);
        let handoff_id = saga.handoff_id.clone().ok_or_else(|| {
            PulseError::validation("assignment_saga_invalid", "saga has no handoff")
        })?;
        let verification =
            core.complete_execution_verification(crate::execution::CompleteVerificationArgs {
                handoff_id,
                actor: actor.to_string(),
                source_commit: source_commit.to_string(),
                disposition,
                summary: summary.to_string(),
                checks: checks.to_vec(),
                idempotency_key: format!("{idempotency_key}:core-verification"),
            })?;
        self.store.with_state(true, |state| {
            let saga = state
                .assignment_sagas
                .get_mut(saga_id)
                .expect("saga exists");
            saga.state = match verification.disposition {
                crate::execution::VerificationDisposition::Passed => AssignmentSagaState::Done,
                crate::execution::VerificationDisposition::Rework => AssignmentSagaState::Rework,
                crate::execution::VerificationDisposition::Blocked => AssignmentSagaState::Blocked,
            };
            saga.verification_id = Some(verification.verification_id.clone());
            saga.updated_at = chrono::Utc::now().to_rfc3339();
            let project_id = saga.project_id.clone();
            let workspace_id = saga.workspace_id.clone();
            let session_id = saga.session_id.clone();
            append_event(
                state,
                "assignment.verification_completed",
                Some(&project_id),
                workspace_id.as_deref(),
                session_id.as_deref(),
                json!({
                    "saga_id": saga_id,
                    "verification_id": verification.verification_id,
                    "disposition": verification.disposition,
                }),
            );
            Ok(())
        })?;
        Ok(DaemonResponse::Verification { verification })
    }

    fn assignment_saga(&self, saga_id: &str) -> Result<AssignmentSagaRecord> {
        self.store.with_state(false, |state| {
            state
                .assignment_sagas
                .get(saga_id)
                .cloned()
                .ok_or_else(|| PulseError::NotFound {
                    subject: format!("assignment saga {saga_id}"),
                })
        })
    }

    fn project_record(&self, project_id: &str) -> Result<ProjectRecord> {
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

fn append_event(
    state: &mut crate::daemon::persistence::DaemonState,
    event_type: &str,
    project_id: Option<&str>,
    workspace_id: Option<&str>,
    session_id: Option<&str>,
    payload: Value,
) {
    let event = TimelineEvent {
        schema_version: 1,
        event_id: format!("rtevt_{}", ulid::Ulid::new()),
        epoch: state.epoch.clone(),
        sequence: state.next_sequence,
        occurred_at: chrono::Utc::now().to_rfc3339(),
        event_type: event_type.to_string(),
        project_id: project_id.map(str::to_string),
        workspace_id: workspace_id.map(str::to_string),
        session_id: session_id.map(str::to_string),
        payload,
    };
    state.next_sequence += 1;
    state.timeline.push(event);
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

fn deterministic_id(prefix: &str, idempotency_key: &str) -> String {
    let digest = hash_bytes(idempotency_key.as_bytes());
    let suffix = digest
        .trim_start_matches("sha256:")
        .chars()
        .take(26)
        .collect::<String>();
    format!("{prefix}_{suffix}")
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

fn protocol_error_from_pulse(error: PulseError) -> ProtocolError {
    let retryable = matches!(
        error.code(),
        "lock_timeout" | "io_error" | "provider_transport_closed"
    );
    ProtocolError::new(error.code(), error.to_string(), retryable)
}
