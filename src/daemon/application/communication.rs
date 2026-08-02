//! Explicit daemon communication grants and mailbox message use cases.
//!
//! This module touches persisted communication grants, session messages, and
//! their timeline events. It never infers authority from runtime parentage;
//! grant checks, message identity/direction, and authorization remain explicit.
//! State mutation and its timeline event stay in the same durable `with_state`
//! transaction. Dependencies are limited to the parent application store,
//! deterministic-id and event helpers, daemon session/protocol value types,
//! runtime principals, and shared error/result types.

use serde_json::json;

use super::{append_event, deterministic_id, DaemonApplication};
use crate::daemon::permissions::RuntimePrincipal;
use crate::daemon::protocol::DaemonResponse;
use crate::daemon::session::{CommunicationGrantRecord, SessionLifecycle, SessionMessageRecord};
use crate::{PulseError, Result};

impl DaemonApplication {
    pub(super) fn session_communication_grant(
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

    pub(super) fn session_message_send(
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

    pub(super) fn session_messages(&self, session_id: &str) -> Result<DaemonResponse> {
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
}
