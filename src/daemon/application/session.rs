//! Owns daemon host-local session and provider-process lifecycle state.
//!
//! This module touches session/process records and external-effect state, performs provider and process I/O, and preserves no-false-idle plus handle-release/requeue ordering. It may depend on the application facade, StateStore, ProcessOwner, provider registry, and timeline/effect helpers; it does not own Core or repository semantics.

use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

use crate::canonical_json::hash_bytes;
use crate::daemon::persistence::{ExternalEffectKind, ExternalEffectState};
use crate::daemon::process::{
    read_captured_logs, ManagedProcessRecord, ManagedProcessState, SpawnRequest,
};
use crate::daemon::protocol::DaemonResponse;
use crate::daemon::session::{SessionLifecycle, SessionRecord};
use crate::daemon::workspace::WorkspaceLifecycle;
use crate::{PulseError, Result};

use super::{
    append_event, deterministic_id, effect_has_committed_owner, external_effect_blocked,
    is_ambiguous_provider_outcome, persist_provider_event_batch, provider_protocol_after_transport,
    DaemonApplication,
};

impl DaemonApplication {
    pub(super) fn session_create(
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
        let process_effect_id = format!("effect-process-{session_id}");
        let process_effect = self.record_external_effect(
            &process_effect_id,
            ExternalEffectKind::ProviderProcessCreate,
            &session_id,
            &hash_bytes(
                format!(
                    "{provider_id}:{}:{:?}",
                    launch.executable.display(),
                    launch.args
                )
                .as_bytes(),
            ),
            format!("planned executable={}", launch.executable.display()),
            None,
        )?;
        if matches!(
            process_effect.state,
            ExternalEffectState::Attempting
                | ExternalEffectState::Acknowledged
                | ExternalEffectState::OutcomeUnknown
        ) {
            return Err(external_effect_blocked(&process_effect_id));
        }
        self.store
            .check_failpoint("after_provider_process_intent")?;
        self.update_external_effect(
            &process_effect_id,
            ExternalEffectState::Attempting,
            None,
            Some("dispatching provider process".to_string()),
        )?;
        let process = match self.process_owner.spawn(SpawnRequest {
            owner_kind: "session",
            owner_id: &session_id,
            provider_id,
            executable: &launch.executable,
            args: &launch.args,
            cwd: Path::new(&workspace.root),
            log_root: &self.store.root().join("logs"),
            max_log_bytes: 4 * 1024 * 1024,
        }) {
            Ok(process) => process,
            Err(error) => {
                let _ = self.update_external_effect(
                    &process_effect_id,
                    ExternalEffectState::DefinitivelyFailed,
                    None,
                    Some(error.to_string()),
                );
                return Err(error);
            }
        };
        let process_id = process.process_id.clone();
        self.update_external_effect(
            &process_effect_id,
            ExternalEffectState::Attempting,
            Some(process_id.clone()),
            Some(format!(
                "spawned process_id={process_id}; awaiting acknowledgement"
            )),
        )?;
        self.store
            .check_failpoint("after_provider_process_success_before_ack")?;
        self.update_external_effect(
            &process_effect_id,
            ExternalEffectState::Acknowledged,
            Some(process_id.clone()),
            Some(format!("spawned process_id={process_id}")),
        )?;
        let session_effect_id = format!("effect-provider-session-{session_id}");
        if launch.native_protocol {
            if let Err(error) = self.record_external_effect(
                &session_effect_id,
                ExternalEffectKind::ProviderSessionCreate,
                &session_id,
                &hash_bytes(format!("{provider_id}:{session_id}").as_bytes()),
                format!("provider={provider_id}"),
                None,
            ) {
                let _ = self.process_owner.terminate(&process_id);
                let _ = self.update_external_effect(
                    &process_effect_id,
                    ExternalEffectState::OutcomeUnknown,
                    Some(process_id.clone()),
                    Some("provider-session intent could not be persisted".to_string()),
                );
                return Err(error);
            }
            let session_effect = self.store.with_state(false, |state| {
                Ok(state.external_effects.get(&session_effect_id).cloned())
            })?;
            if session_effect
                .as_ref()
                .is_some_and(|effect| effect.state == ExternalEffectState::Attempting)
            {
                return Err(external_effect_blocked(&session_effect_id));
            }
            if let Err(error) = self.store.check_failpoint("after_provider_session_intent") {
                let terminated = self.process_owner.terminate(&process_id).is_ok();
                let _ = self.update_external_effect(
                    &process_effect_id,
                    if terminated {
                        ExternalEffectState::DefinitivelyFailed
                    } else {
                        ExternalEffectState::OutcomeUnknown
                    },
                    Some(process_id.clone()),
                    Some(error.to_string()),
                );
                let _ = self.update_external_effect(
                    &session_effect_id,
                    ExternalEffectState::OutcomeUnknown,
                    None,
                    Some(error.to_string()),
                );
                return Err(error);
            }
        }
        if launch.native_protocol {
            self.update_external_effect(
                &session_effect_id,
                ExternalEffectState::Attempting,
                None,
                Some("dispatching provider session creation".to_string()),
            )?;
        }
        let mut provider_acknowledged = false;
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
            provider_acknowledged = true;
            self.process_owner
                .send_line(&process_id, &provider.initialized_notification()?)?;
            let create = provider.create_session_request(&workspace.root, provider_options)?;
            let (response, create_notifications) = self.process_owner.request_json(
                &process_id,
                &create.request_id,
                &create.message,
                Duration::from_secs(30),
            )?;
            provider_acknowledged = true;
            notifications.extend(create_notifications);
            Ok((
                Some(
                    provider
                        .parse_session_handle(&response)
                        .map_err(provider_protocol_after_transport)?,
                ),
                notifications,
            ))
        })();
        let (provider_handle, provider_notifications) = match provider_session {
            Ok(provider_session) => provider_session,
            Err(error) => {
                let termination = self.process_owner.terminate(&process_id);
                let cleanup_detail = termination
                    .as_ref()
                    .err()
                    .map(|cleanup| format!("provider process cleanup failed: {cleanup}"));
                let process_state = if termination.is_ok()
                    && !provider_acknowledged
                    && !is_ambiguous_provider_outcome(&error)
                {
                    ExternalEffectState::DefinitivelyFailed
                } else {
                    ExternalEffectState::OutcomeUnknown
                };
                let session_state = if provider_acknowledged
                    || is_ambiguous_provider_outcome(&error)
                    || termination.is_err()
                {
                    ExternalEffectState::OutcomeUnknown
                } else {
                    ExternalEffectState::DefinitivelyFailed
                };
                let _ = self.update_external_effect(
                    &process_effect_id,
                    process_state,
                    Some(process_id.clone()),
                    Some(format_failure_detail(&error, cleanup_detail.as_deref())),
                );
                if launch.native_protocol {
                    let _ = self.update_external_effect(
                        &session_effect_id,
                        session_state,
                        None,
                        Some(format_failure_detail(&error, cleanup_detail.as_deref())),
                    );
                }
                return Err(error);
            }
        };
        if launch.native_protocol {
            self.update_external_effect(
                &session_effect_id,
                ExternalEffectState::Attempting,
                provider_handle.clone(),
                Some("provider session accepted; awaiting acknowledgement".to_string()),
            )?;
            self.store
                .check_failpoint("after_provider_session_success_before_ack")?;
            self.update_external_effect(
                &session_effect_id,
                ExternalEffectState::Acknowledged,
                None,
                Some("provider session acknowledged".to_string()),
            )?;
        }
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
            let _ = self.update_external_effect(
                &process_effect_id,
                ExternalEffectState::OutcomeUnknown,
                Some(process_id.clone()),
                Some(
                    "session commit failed after process creation; reconcile process ledger"
                        .to_string(),
                ),
            );
            if launch.native_protocol {
                let _ = self.update_external_effect(
                    &session_effect_id,
                    ExternalEffectState::OutcomeUnknown,
                    None,
                    Some("session commit failed after provider acknowledgement".to_string()),
                );
            }
        }
        result
    }

    /// Attach to an already-managed live session. This is deliberately not a
    /// resume: it never launches a process or asks the provider to recreate a
    /// native thread, and therefore preserves both Pulse and provider identity.
    pub(super) fn session_attach(&self, session_id: &str) -> Result<DaemonResponse> {
        let session = self.store.with_state(false, |state| {
            state
                .sessions
                .get(session_id)
                .cloned()
                .ok_or_else(|| PulseError::NotFound {
                    subject: format!("session {session_id}"),
                })
        })?;
        if !matches!(
            session.lifecycle,
            SessionLifecycle::Idle | SessionLifecycle::Running
        ) {
            return Err(PulseError::validation(
                "session_attach_conflict",
                "only an idle or running live session can be attached",
            ));
        }
        let process_id = session.managed_process_id.as_deref().ok_or_else(|| {
            PulseError::validation("session_attach_conflict", "session has no managed process")
        })?;
        if !self.process_owner.is_alive(process_id)? {
            return Err(PulseError::validation(
                "session_attach_conflict",
                "session process is no longer live",
            ));
        }
        Ok(DaemonResponse::Session { session })
    }

    pub(super) fn session_resume(
        &self,
        session_id: &str,
        provider_options: &Value,
    ) -> Result<DaemonResponse> {
        let _session_guard = self
            .store
            .acquire_idempotency(&format!("session-operation:{session_id}"))?;
        let session_snapshot = self.store.with_state(false, |state| {
            state
                .sessions
                .get(session_id)
                .cloned()
                .ok_or_else(|| PulseError::NotFound {
                    subject: format!("session {session_id}"),
                })
        })?;
        if !matches!(
            session_snapshot.lifecycle,
            SessionLifecycle::Error | SessionLifecycle::Closed | SessionLifecycle::Initializing
        ) {
            return Err(PulseError::validation(
                "session_resume_not_required",
                "session is already usable and does not need resumption",
            ));
        }
        if session_snapshot.archived_at.is_some() {
            return Err(PulseError::validation(
                "session_archived",
                "archived sessions cannot be resumed",
            ));
        }
        let provider_handle = session_snapshot.provider_handle.clone().ok_or_else(|| {
            PulseError::validation(
                "provider_resume_handle_missing",
                "session has no persisted provider handle to resume",
            )
        })?;
        let provider = self.providers.get(&session_snapshot.provider_id)?;
        if !provider.capabilities().resume {
            return Err(PulseError::validation(
                "provider_resume_unsupported",
                "provider does not support resuming a persisted session",
            ));
        }
        provider.availability()?;
        let launch = provider.launch(provider_options)?;
        if !launch.native_protocol {
            return Err(PulseError::validation(
                "provider_resume_unsupported",
                "opaque provider sessions cannot resume a persisted provider handle",
            ));
        }
        let workspace = self.store.with_state(false, |state| {
            state
                .workspaces
                .get(&session_snapshot.workspace_id)
                .cloned()
                .ok_or_else(|| PulseError::NotFound {
                    subject: format!("workspace {}", session_snapshot.workspace_id),
                })
        })?;
        if workspace.lifecycle != WorkspaceLifecycle::Open {
            return Err(PulseError::validation(
                "workspace_not_open",
                "session resumption requires an open workspace",
            ));
        }
        let resume_effect_id = format!(
            "effect-provider-resume-{session_id}-{}",
            hash_bytes(format!("{provider_handle}:{}", session_snapshot.updated_at).as_bytes())
                .trim_start_matches("sha256:")
                .chars()
                .take(20)
                .collect::<String>()
        );
        let unresolved_resume = self.store.with_state(false, |state| {
            Ok(state
                .external_effects
                .values()
                .find(|effect| {
                    effect.kind == ExternalEffectKind::ProviderSessionResume
                        && effect.owner_id == session_id
                        && matches!(
                            effect.state,
                            ExternalEffectState::Attempting | ExternalEffectState::OutcomeUnknown
                        )
                })
                .cloned())
        })?;
        if let Some(effect) = unresolved_resume {
            return Err(external_effect_blocked(&effect.effect_id));
        }
        let resume_effect = self.record_external_effect(
            &resume_effect_id,
            ExternalEffectKind::ProviderSessionResume,
            session_id,
            &hash_bytes(format!("{}:{provider_handle}", session_snapshot.provider_id).as_bytes()),
            format!(
                "provider={} handle={provider_handle}",
                session_snapshot.provider_id
            ),
            None,
        )?;
        match resume_effect.state {
            ExternalEffectState::NotSent => {}
            ExternalEffectState::Acknowledged => {
                let committed = self.store.with_state(false, |state| {
                    Ok(effect_has_committed_owner(state, &resume_effect))
                })?;
                if committed {
                    return self.store.with_state(false, |state| {
                        let session = state.sessions.get(session_id).cloned().ok_or_else(|| {
                            PulseError::NotFound {
                                subject: format!("session {session_id}"),
                            }
                        })?;
                        Ok(DaemonResponse::Session { session })
                    });
                }
                return Err(external_effect_blocked(&resume_effect_id));
            }
            ExternalEffectState::Attempting
            | ExternalEffectState::OutcomeUnknown
            | ExternalEffectState::DefinitivelyFailed => {
                return Err(external_effect_blocked(&resume_effect_id));
            }
        }
        self.store.check_failpoint("after_session_resume_intent")?;
        self.update_external_effect(
            &resume_effect_id,
            ExternalEffectState::Attempting,
            session_snapshot.managed_process_id.clone(),
            Some("dispatching session resume".to_string()),
        )?;
        if let Some(old_process_id) = session_snapshot.managed_process_id.as_deref() {
            let old_record = self.store.with_state(false, |state| {
                Ok(state.processes.get(old_process_id).cloned())
            })?;
            if let Some(old_record) = old_record {
                match self.process_owner.terminate(old_process_id) {
                    Ok(()) => {}
                    Err(error) if error.code() == "managed_process_not_owned" => {
                        self.process_owner.terminate_record(&old_record)?;
                    }
                    Err(error) => {
                        let _ = self.update_external_effect(
                            &resume_effect_id,
                            ExternalEffectState::OutcomeUnknown,
                            Some(old_process_id.to_string()),
                            Some(format!("old process termination failed: {error}")),
                        );
                        return Err(error);
                    }
                }
            }
        }
        let process = match self.process_owner.spawn(SpawnRequest {
            owner_kind: "session",
            owner_id: session_id,
            provider_id: &session_snapshot.provider_id,
            executable: &launch.executable,
            args: &launch.args,
            cwd: Path::new(&workspace.root),
            log_root: &self.store.root().join("logs"),
            max_log_bytes: 4 * 1024 * 1024,
        }) {
            Ok(process) => process,
            Err(error) => {
                let _ = self.update_external_effect(
                    &resume_effect_id,
                    ExternalEffectState::OutcomeUnknown,
                    None,
                    Some(format!(
                        "replacement process spawn outcome unknown: {error}"
                    )),
                );
                // The old process was already terminated above; record the
                // accurate ledger/session state before surfacing the spawn error.
                return self.fail_session_resume(session_id, &session_snapshot, None, error);
            }
        };
        let new_process_id = process.process_id.clone();
        self.update_external_effect(
            &resume_effect_id,
            ExternalEffectState::Attempting,
            Some(new_process_id.clone()),
            Some("replacement process spawned; awaiting provider resume".to_string()),
        )?;
        self.store
            .check_failpoint("after_session_resume_spawn_before_ack")?;
        let mut provider_acknowledged = false;
        let provider_session_result = (|| -> Result<Vec<Value>> {
            let initialize = provider.initialize_request()?;
            let (_, mut notifications) = self.process_owner.request_json(
                &new_process_id,
                &initialize.request_id,
                &initialize.message,
                Duration::from_secs(10),
            )?;
            provider_acknowledged = true;
            self.process_owner
                .send_line(&new_process_id, &provider.initialized_notification()?)?;
            let resume = provider.resume_session_request(
                &provider_handle,
                &workspace.root,
                provider_options,
            )?;
            let (response, resume_notifications) = self.process_owner.request_json(
                &new_process_id,
                &resume.request_id,
                &resume.message,
                Duration::from_secs(30),
            )?;
            provider_acknowledged = true;
            let resumed_handle = provider
                .parse_session_handle(&response)
                .map_err(provider_protocol_after_transport)?;
            if resumed_handle != provider_handle {
                return Err(PulseError::validation(
                    "provider_resume_identity_mismatch",
                    "provider resumed a different native session handle",
                ));
            }
            notifications.extend(resume_notifications);
            Ok(notifications)
        })();
        let provider_notifications = match provider_session_result {
            Ok(result) => result,
            Err(error) => {
                let _ = self.process_owner.terminate(&new_process_id);
                let _ = self.update_external_effect(
                    &resume_effect_id,
                    if provider_acknowledged || is_ambiguous_provider_outcome(&error) {
                        ExternalEffectState::OutcomeUnknown
                    } else {
                        ExternalEffectState::DefinitivelyFailed
                    },
                    Some(new_process_id.clone()),
                    Some(error.to_string()),
                );
                return self.fail_session_resume(
                    session_id,
                    &session_snapshot,
                    Some(&process),
                    error,
                );
            }
        };
        self.update_external_effect(
            &resume_effect_id,
            ExternalEffectState::Attempting,
            Some(new_process_id.clone()),
            Some("provider resume accepted; awaiting acknowledgement".to_string()),
        )?;
        self.store
            .check_failpoint("after_session_resume_success_before_ack")?;
        self.update_external_effect(
            &resume_effect_id,
            ExternalEffectState::Acknowledged,
            Some(new_process_id.clone()),
            Some("provider session resume acknowledged".to_string()),
        )?;
        // `process` is moved into the commit closure below; keep an independent
        // copy so the rare final-commit failure can still record the candidate.
        let candidate_record = process.clone();
        let result = self.store.with_state(true, |state| {
            if let Some(old_process_id) = session_snapshot.managed_process_id.as_deref() {
                if let Some(old_process) = state.processes.get_mut(old_process_id) {
                    old_process.state = ManagedProcessState::Exited;
                    old_process.updated_at = chrono::Utc::now().to_rfc3339();
                }
            }
            state.processes.insert(new_process_id.clone(), process);
            let session =
                state
                    .sessions
                    .get_mut(session_id)
                    .ok_or_else(|| PulseError::NotFound {
                        subject: format!("session {session_id}"),
                    })?;
            if session.lifecycle != session_snapshot.lifecycle
                || session.managed_process_id != session_snapshot.managed_process_id
            {
                return Err(PulseError::validation(
                    "session_resume_conflict",
                    "session changed while provider resumption was in progress",
                ));
            }
            session.managed_process_id = Some(new_process_id.clone());
            session.lifecycle = SessionLifecycle::Idle;
            session.active_turn_id = None;
            session.last_error = None;
            session.provider_detail = launch.provider_detail.clone();
            session.updated_at = chrono::Utc::now().to_rfc3339();
            let session = session.clone();
            for notification in provider_notifications {
                append_event(
                    state,
                    "provider.notification",
                    Some(&session.project_id),
                    Some(&session.workspace_id),
                    Some(session_id),
                    notification,
                );
            }
            append_event(
                state,
                "session.resumed",
                Some(&session.project_id),
                Some(&session.workspace_id),
                Some(session_id),
                json!({
                    "provider_id": session.provider_id,
                    "provider_handle": provider_handle,
                    "new_process_id": new_process_id,
                    "old_process_id": session_snapshot.managed_process_id,
                }),
            );
            Ok(DaemonResponse::Session { session })
        });
        match result {
            Ok(response) => Ok(response),
            Err(error) => {
                let _ = self.process_owner.terminate(&new_process_id);
                let _ = self.update_external_effect(
                    &resume_effect_id,
                    ExternalEffectState::OutcomeUnknown,
                    Some(new_process_id.clone()),
                    Some(format!(
                        "session resume commit failed after provider acknowledgement: {error}"
                    )),
                );
                self.fail_session_resume(
                    session_id,
                    &session_snapshot,
                    Some(&candidate_record),
                    error,
                )
            }
        }
    }

    /// Persist a precise failure outcome for a session resume that could not
    /// complete, then return the original error.
    ///
    /// By the time a resume fails the previously-managed ("old") process has
    /// already been terminated, and the candidate process — when one was
    /// spawned — has been terminated too. This records both terminal states in
    /// the process ledger, transitions the session to an explicit [`Error`]
    /// lifecycle with an actionable `last_error`, and emits a single
    /// `session.resume_failed` timeline event correlated to the old/candidate
    /// process ids and the failure code. The candidate never remains alive or
    /// appears `Running`.
    ///
    /// For a `session_resume_conflict` (a concurrent replacement won the session
    /// while this resume was in flight) the session lifecycle is left untouched
    /// so the winning state is not clobbered; the dead processes are still
    /// recorded. This is the smallest safe transition — no state-machine
    /// redesign — and it preserves the strict no-false-idle invariant.
    ///
    /// [`Error`]: SessionLifecycle::Error
    fn fail_session_resume(
        &self,
        session_id: &str,
        session_snapshot: &SessionRecord,
        candidate: Option<&ManagedProcessRecord>,
        error: PulseError,
    ) -> Result<DaemonResponse> {
        let failure_code = error.code();
        let detail = error.to_string();
        let preserve_session = failure_code == "session_resume_conflict";
        // Best-effort persistence: the original resume error is the primary
        // signal and is always propagated regardless of whether this write lands.
        let _ = self.store.with_state(true, |state| {
            let now = chrono::Utc::now().to_rfc3339();
            if let Some(old_process_id) = session_snapshot.managed_process_id.as_deref() {
                if let Some(old_process) = state.processes.get_mut(old_process_id) {
                    old_process.state = ManagedProcessState::Exited;
                    old_process.updated_at = now.clone();
                }
            }
            if let Some(candidate) = candidate {
                let mut record = candidate.clone();
                record.state = ManagedProcessState::Exited;
                record.updated_at = now.clone();
                state.processes.insert(record.process_id.clone(), record);
            }
            if !preserve_session {
                if let Some(session) = state.sessions.get_mut(session_id) {
                    session.lifecycle = SessionLifecycle::Error;
                    session.last_error = Some(format!(
                        "session resume failed ({failure_code}): {detail}; the provider process was terminated and the session is not idle"
                    ));
                    session.updated_at = now.clone();
                }
            }
            append_event(
                state,
                "session.resume_failed",
                Some(&session_snapshot.project_id),
                Some(&session_snapshot.workspace_id),
                Some(session_id),
                json!({
                    "failure_code": failure_code,
                    "old_process_id": session_snapshot.managed_process_id,
                    "candidate_process_id": candidate.map(|record| record.process_id.clone()),
                }),
            );
            Ok(())
        });
        Err(error)
    }

    pub(super) fn session_list(
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

    pub(super) fn session_show(&self, session_id: &str) -> Result<DaemonResponse> {
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

    pub(super) fn session_inspect(&self, session_id: &str) -> Result<DaemonResponse> {
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
            let process = session
                .managed_process_id
                .as_ref()
                .and_then(|process_id| state.processes.get(process_id).cloned());
            Ok(DaemonResponse::SessionInspection {
                session,
                process: Box::new(process),
            })
        })
    }

    pub(super) fn session_logs(&self, session_id: &str) -> Result<DaemonResponse> {
        let process = self.store.with_state(false, |state| {
            let session = state
                .sessions
                .get(session_id)
                .ok_or_else(|| PulseError::NotFound {
                    subject: format!("session {session_id}"),
                })?;
            let process_id = session.managed_process_id.as_ref().ok_or_else(|| {
                PulseError::validation(
                    "session_logs_unavailable",
                    "session has no daemon-managed process logs",
                )
            })?;
            state
                .processes
                .get(process_id)
                .cloned()
                .ok_or_else(|| PulseError::NotFound {
                    subject: format!("managed process {process_id}"),
                })
        })?;
        let logs = read_captured_logs(&process)?;
        Ok(DaemonResponse::SessionLogs {
            session_id: session_id.to_string(),
            process_id: process.process_id,
            logs,
        })
    }

    pub(super) fn session_interrupt(&self, session_id: &str) -> Result<DaemonResponse> {
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
            self.process_owner
                .request_json(
                    process_id,
                    &request.request_id,
                    &request.message,
                    Duration::from_secs(10),
                )
                .is_ok()
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

    pub(super) fn session_close(&self, session_id: &str) -> Result<DaemonResponse> {
        let _session_guard = self
            .store
            .acquire_idempotency(&format!("session-operation:{session_id}"))?;
        let _initial_snapshot = self.store.with_state(false, |state| {
            let session = state
                .sessions
                .get(session_id)
                .ok_or_else(|| PulseError::NotFound {
                    subject: format!("session {session_id}"),
                })?;
            Ok(session.clone())
        })?;
        self.refresh_session_provider_events_locked(session_id)?;
        let snapshot = self.store.with_state(false, |state| {
            state
                .sessions
                .get(session_id)
                .cloned()
                .ok_or_else(|| PulseError::NotFound {
                    subject: format!("session {session_id}"),
                })
        })?;
        if snapshot.lifecycle == SessionLifecycle::Running {
            if let (Some(provider_handle), Some(turn_handle), Some(process_id)) = (
                snapshot.provider_handle.as_deref(),
                snapshot.active_turn_id.as_deref(),
                snapshot.managed_process_id.as_deref(),
            ) {
                let process_exited = self
                    .process_owner
                    .is_alive(process_id)
                    .map(|alive| !alive)
                    .unwrap_or(false);
                if !process_exited {
                    let provider = self.providers.get(&snapshot.provider_id)?;
                    let request = provider.encode_interrupt(provider_handle, turn_handle)?;
                    let (_, interrupt_notifications) = self.process_owner.request_json(
                        process_id,
                        &request.request_id,
                        &request.message,
                        Duration::from_secs(10),
                    )?;
                    if !interrupt_notifications.is_empty() {
                        persist_provider_event_batch(
                            &self.store,
                            &self.process_owner,
                            process_id,
                            &snapshot,
                            session_id,
                            interrupt_notifications,
                        )?;
                    }
                }
            } else if snapshot.provider_handle.is_some() {
                return Err(PulseError::validation(
                    "provider_interrupt_unacknowledged",
                    "running session has no complete provider interrupt identity",
                ));
            }
        }
        let Some(process_id) = snapshot.managed_process_id.as_deref() else {
            return Err(PulseError::validation(
                "managed_process_missing",
                "session has no daemon-managed process to terminate",
            ));
        };
        let drained_after_termination = match self.process_owner.terminate_and_drain(process_id) {
            Ok(events) => events,
            Err(error) if error.code() == "managed_process_not_owned" => {
                let process =
                    self.store.with_state(false, |state| {
                        state.processes.get(process_id).cloned().ok_or_else(|| {
                            PulseError::NotFound {
                                subject: format!("managed process {process_id}"),
                            }
                        })
                    })?;
                self.process_owner.terminate_record(&process)?;
                Vec::new()
            }
            Err(error) => return Err(error),
        };
        if !drained_after_termination.is_empty() {
            persist_provider_event_batch(
                &self.store,
                &self.process_owner,
                process_id,
                &snapshot,
                session_id,
                drained_after_termination,
            )?;
        }
        // A successful persistence is the acknowledgement that permits the
        // dead child handle to be removed. On persistence failure the helper
        // requeues while the handle remains available for background retry.
        if self.process_owner.release_handle(process_id).is_err() {
            // The fallback `terminate_record` path has no local handle to
            // release; the durable process ledger remains the authority.
        }
        self.store.with_state(true, |state| {
            clear_interrupted_session_send_effect(
                state,
                session_id,
                snapshot.active_turn_id.as_deref(),
            );
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
            if let Some(process_id) = snapshot.managed_process_id.as_deref() {
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

    pub(super) fn session_force_close(&self, session_id: &str) -> Result<DaemonResponse> {
        let _session_guard = self
            .store
            .acquire_idempotency(&format!("session-operation:{session_id}"))?;
        let process_id = self.store.with_state(false, |state| {
            state
                .sessions
                .get(session_id)
                .ok_or_else(|| PulseError::NotFound {
                    subject: format!("session {session_id}"),
                })
                .and_then(|session| {
                    session.managed_process_id.clone().ok_or_else(|| {
                        PulseError::validation(
                            "provider_force_close_invalid",
                            "session has no managed process",
                        )
                    })
                })
        })?;
        self.process_owner.terminate(&process_id)?;
        self.store.with_state(true, |state| {
            let active_turn_id = state
                .sessions
                .get(session_id)
                .and_then(|session| session.active_turn_id.clone());
            clear_interrupted_session_send_effect(state, session_id, active_turn_id.as_deref());
            let session = state.sessions.get_mut(session_id).expect("session exists");
            session.lifecycle = SessionLifecycle::Closed;
            session.active_turn_id = None;
            session.last_error = Some("session force-closed by an administrator".to_string());
            session.updated_at = chrono::Utc::now().to_rfc3339();
            if let Some(process) = state.processes.get_mut(&process_id) {
                process.state = ManagedProcessState::Exited;
                process.updated_at = chrono::Utc::now().to_rfc3339();
            }
            let session = session.clone();
            append_event(
                state,
                "session.force_closed",
                Some(&session.project_id),
                Some(&session.workspace_id),
                Some(session_id),
                json!({"process_id": process_id}),
            );
            Ok(DaemonResponse::Session { session })
        })
    }

    pub(super) fn session_archive(&self, session_id: &str) -> Result<DaemonResponse> {
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
}

fn format_failure_detail(error: &PulseError, cleanup: Option<&str>) -> String {
    match cleanup {
        Some(cleanup) => format!("{error}; {cleanup}"),
        None => error.to_string(),
    }
}

/// An explicitly closed session cannot resume an in-flight turn through the
/// old provider transport. Retire only that acknowledged, interrupted intent
/// so a later lifecycle retry can create a fresh intent. OutcomeUnknown is
/// deliberately never removed: it remains the operator-reconciliation fence.
fn clear_interrupted_session_send_effect(
    state: &mut crate::daemon::persistence::DaemonState,
    session_id: &str,
    active_turn_id: Option<&str>,
) {
    let Some(active_turn_id) = active_turn_id else {
        return;
    };
    state.external_effects.retain(|_, effect| {
        !(effect.kind == ExternalEffectKind::SessionSend
            && effect.owner_id == session_id
            && effect.state == ExternalEffectState::Acknowledged
            && effect.resource_id.as_deref() == Some(active_turn_id))
    });
}
