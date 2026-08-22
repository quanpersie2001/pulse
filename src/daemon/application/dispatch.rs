//! Owns the transport-neutral daemon request policy and routing spine.
//!
//! This module touches only the idempotency cache while enforcing authorization, mutating-key validation, locking, replay fingerprint/principal checks, shutdown gating, routing, failpoint, and response-cache ordering. It depends on the parent facade and private use cases plus protocol, principal, persistence, and canonical-fingerprint types; it contains no transport, service, command-bus, or repository abstraction.

use serde_json::Value;
use std::sync::atomic::Ordering;

use crate::canonical_json::hash_serializable;
use crate::daemon::permissions::RuntimePrincipal;
use crate::daemon::persistence::IdempotencyRecord;
use crate::daemon::protocol::{DaemonRequest, DaemonResponse, ProtocolError, DAEMON_CAPABILITIES};
use crate::{PulseError, Result};

use super::DaemonApplication;

fn has_explicit_provider_launch(options: &Value) -> bool {
    options.get("executable").is_some() || options.get("args").is_some()
}

impl DaemonApplication {
    pub(super) fn handle_inner(
        &self,
        principal: &RuntimePrincipal,
        request: &DaemonRequest,
        idempotency_key: &str,
    ) -> Result<DaemonResponse> {
        self.authorize_request(principal, request)?;
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
        let cacheable = !matches!(request, DaemonRequest::AssignmentStart { .. });
        if cacheable && !idempotency_key.is_empty() {
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
                if cached.principal_id != principal.principal_id {
                    return Err(PulseError::validation(
                        "idempotency_principal_conflict",
                        "idempotency key was already used by a different runtime principal",
                    ));
                }
                return serde_json::from_value(cached.response).map_err(PulseError::from);
            }
        }

        if self.shutdown.load(Ordering::SeqCst) && !matches!(request, DaemonRequest::Shutdown) {
            return Err(PulseError::validation(
                "daemon_shutting_down",
                "daemon is shutting down and no longer accepts requests",
            ));
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
                self.request_shutdown();
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
            DaemonRequest::SessionInspect { session_id } => self.session_inspect(session_id)?,
            DaemonRequest::SessionLogs { session_id } => self.session_logs(session_id)?,
            DaemonRequest::SessionSend { session_id, input } => {
                self.session_send(session_id, input, idempotency_key)?
            }
            DaemonRequest::SessionResume {
                session_id,
                provider_options,
            } => self.session_resume(session_id, provider_options)?,
            DaemonRequest::SessionAttach { session_id } => {
                principal
                    .require_session_access(session_id)
                    .map_err(|code| {
                        PulseError::validation(code, "session attach is not authorized")
                    })?;
                self.session_attach(session_id)?
            }
            DaemonRequest::SessionForceClose { session_id } => {
                self.session_force_close(session_id)?
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
            } => self.assignment_acknowledge_admin(principal, saga_id, acknowledgement_id)?,
            DaemonRequest::AssignmentAcknowledgeBound {
                saga_id,
                acknowledgement_id,
                lease_id,
                session_id,
                packet_fingerprint,
                delivery_id,
            } => {
                principal
                    .require_session_sender(session_id)
                    .map_err(|code| {
                        PulseError::validation(code, "acknowledgement sender is not authorized")
                    })?;
                self.assignment_acknowledge_bound(
                    saga_id,
                    acknowledgement_id,
                    lease_id,
                    session_id,
                    packet_fingerprint,
                    delivery_id,
                )?
            }
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
                acceptance_proofs,
            } => self.verification_complete(
                saga_id,
                actor,
                source_commit,
                *disposition,
                summary,
                checks,
                acceptance_proofs,
                idempotency_key,
            )?,
            DaemonRequest::AssignmentClose {
                saga_id,
                actor,
                source_commit,
                summary,
            } => self.assignment_close(saga_id, actor, source_commit, summary, idempotency_key)?,
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

        // Crash-consistency boundary for client-facing idempotency: the
        // mutation itself is already durably committed at this point, so a
        // crash here only loses the client response and the cached replay
        // record — never the underlying effect.
        self.store
            .check_failpoint("before_idempotency_result_commit")?;
        if cacheable && !idempotency_key.is_empty() {
            let fingerprint = hash_serializable(request)?;
            let value = serde_json::to_value(&response).map_err(PulseError::from)?;
            self.store.with_state(true, |state| {
                state.idempotency_results.insert(
                    idempotency_key.to_string(),
                    IdempotencyRecord {
                        request_fingerprint: fingerprint,
                        response: value,
                        recorded_at: chrono::Utc::now().to_rfc3339(),
                        principal_id: principal.principal_id.clone(),
                    },
                );
                Ok(())
            })?;
        }
        Ok(response)
    }

    fn authorize_request(
        &self,
        principal: &RuntimePrincipal,
        request: &DaemonRequest,
    ) -> Result<()> {
        match request {
            DaemonRequest::SessionCreate {
                provider_options, ..
            }
            | DaemonRequest::SessionResume {
                provider_options, ..
            }
            | DaemonRequest::AssignmentStart {
                provider_options, ..
            } if has_explicit_provider_launch(provider_options)
                && !principal.capabilities.contains("runtime.admin") =>
            {
                Err(PulseError::validation(
                    "runtime_provider_launch_admin_required",
                    "executable and argument selection requires the runtime.admin capability",
                ))
            }
            DaemonRequest::SessionAttach { session_id }
            | DaemonRequest::SessionShow { session_id }
            | DaemonRequest::SessionInspect { session_id }
            | DaemonRequest::SessionLogs { session_id }
            | DaemonRequest::SessionSend { session_id, .. }
            | DaemonRequest::SessionInterrupt { session_id }
            | DaemonRequest::SessionClose { session_id }
            | DaemonRequest::SessionResume { session_id, .. }
            | DaemonRequest::SessionArchive { session_id }
            | DaemonRequest::SessionMessages { session_id } => principal
                .require_session_access(session_id)
                .map_err(|code| PulseError::validation(code, "session access is not authorized")),
            DaemonRequest::SessionCommunicationGrant {
                sender_session_id, ..
            }
            | DaemonRequest::SessionMessageSend {
                sender_session_id, ..
            } => principal
                .require_session_sender(sender_session_id)
                .map_err(|code| PulseError::validation(code, "session sender is not authorized")),
            DaemonRequest::AssignmentAcknowledge { .. }
            | DaemonRequest::SessionForceClose { .. } => {
                principal.require("runtime.admin").map_err(|code| {
                    PulseError::validation(code, "runtime administrator access is required")
                })
            }
            DaemonRequest::AssignmentAcknowledgeBound { session_id, .. } => principal
                .require_session_sender(session_id)
                .map_err(|code| {
                    PulseError::validation(code, "acknowledgement sender is not authorized")
                }),
            DaemonRequest::HandoffSubmit { saga_id, .. } => {
                self.authorize_saga_session(principal, saga_id, "handoff")
            }
            DaemonRequest::VerificationComplete { saga_id, actor, .. } => {
                self.authorize_verification(principal, saga_id, actor)
            }
            DaemonRequest::AssignmentClose { saga_id, actor, .. } => {
                self.authorize_verification(principal, saga_id, actor)
            }
            _ => Ok(()),
        }
    }

    fn authorize_saga_session(
        &self,
        principal: &RuntimePrincipal,
        saga_id: &str,
        operation: &str,
    ) -> Result<()> {
        if principal.capabilities.contains("runtime.admin") {
            return Ok(());
        }
        let session_id = self.assignment_saga(saga_id)?.session_id.ok_or_else(|| {
            PulseError::validation("assignment_saga_invalid", "saga has no session")
        })?;
        if principal.session_id.as_deref() == Some(session_id.as_str()) {
            Ok(())
        } else {
            Err(PulseError::validation(
                "saga_session_identity_required",
                format!("{operation} must come from the saga-bound session"),
            ))
        }
    }

    fn authorize_verification(
        &self,
        principal: &RuntimePrincipal,
        saga_id: &str,
        actor: &str,
    ) -> Result<()> {
        let saga_session = self.assignment_saga(saga_id)?.session_id.ok_or_else(|| {
            PulseError::validation("assignment_saga_invalid", "saga has no session")
        })?;
        if principal.capabilities.contains("runtime.admin") {
            if principal.principal_id == "local_cli" || principal.principal_id == actor {
                return Ok(());
            }
            return Err(PulseError::validation(
                "verification_actor_mismatch",
                "trusted administrators must identify the verification actor",
            ));
        }
        if principal.principal_id != actor {
            return Err(PulseError::validation(
                "verification_actor_mismatch",
                "verification actor must match the authenticated principal",
            ));
        }
        match principal.session_id.as_deref() {
            None => Err(PulseError::validation(
                "verification_reviewer_identity_required",
                "verification requires an authenticated reviewer session",
            )),
            Some(session_id) if session_id == saga_session => Err(PulseError::validation(
                "verification_self_review_denied",
                "the worker session cannot verify its own handoff",
            )),
            Some(_) => Ok(()),
        }
    }
}

pub(super) fn protocol_error_from_pulse(error: PulseError) -> ProtocolError {
    let retryable = matches!(
        error.code(),
        "lock_timeout" | "io_error" | "provider_transport_closed"
    );
    ProtocolError::new(error.code(), error.to_string(), retryable)
}
