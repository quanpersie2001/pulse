//! Reusable durable provider-turn protocol for daemon application use cases.
//!
//! This module touches session lifecycle state, external-effect records, and
//! provider I/O while preserving the prepared turn's session-operation guard
//! from validation through durable intent, I/O, acknowledgement, and final
//! session/timeline commit. Effects and event commits retain their existing
//! ordering and failure classification. Dependencies are limited to the parent
//! application, its effect/event helpers, daemon provider/process/session and
//! persistence types, and shared error/value types; no repository semantics or
//! new service abstraction is introduced.

use serde_json::{json, Value};

use super::{
    append_event, effects::effect_has_committed_owner, effects::external_effect_blocked,
    is_ambiguous_provider_outcome, provider_protocol_after_transport, DaemonApplication,
};
use crate::canonical_json::hash_bytes;
use crate::daemon::persistence::{ExternalEffectKind, ExternalEffectState, IdempotencyGuard};
use crate::daemon::protocol::DaemonResponse;
use crate::daemon::session::{SessionLifecycle, SessionRecord};
use crate::{PulseError, Result};
use std::time::Duration;

impl DaemonApplication {
    pub(super) fn session_send(
        &self,
        session_id: &str,
        input: &str,
        idempotency_key: &str,
    ) -> Result<DaemonResponse> {
        let prepared = self.prepare_session_turn(session_id, input, idempotency_key, None)?;
        let committed = self.execute_session_turn(prepared, input)?;
        Ok(DaemonResponse::Session {
            session: committed.session,
        })
    }

    /// Loads the session snapshot, validates that a new turn is allowed and
    /// encodes the provider request. The provider request identifier (when the
    /// protocol exposes one) is available here, BEFORE any provider I/O, so a
    /// delivery intent can persist it as correlation before sending.
    pub(super) fn prepare_session_turn(
        &self,
        session_id: &str,
        input: &str,
        request_identity: &str,
        expected_request_id: Option<&str>,
    ) -> Result<PreparedSessionTurn> {
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
        let effect_id = session_send_effect_id(session_id, request_identity);
        let existing_message = self.store.with_state(false, |state| {
            Ok(state
                .external_effects
                .get(&effect_id)
                .and_then(|effect| effect.request_message.clone()))
        })?;
        let provider = self.providers.get(&snapshot.provider_id)?;
        let (request_id, message) = match (snapshot.provider_handle.as_deref(), existing_message) {
            (Some(_), Some(message)) => {
                let request_id = provider_request_id(&message)?;
                if expected_request_id.is_some_and(|expected| expected != request_id) {
                    return Err(PulseError::validation(
                        "external_effect_request_conflict",
                        "durable provider request identity does not match delivery correlation",
                    ));
                }
                (Some(request_id), Some(message))
            }
            (Some(provider_handle), None) => {
                let request = provider.encode_send(provider_handle, input)?;
                let message = if let Some(expected) = expected_request_id {
                    replace_provider_request_id(&request.message, expected)?
                } else {
                    request.message
                };
                let request_id = expected_request_id
                    .map(str::to_string)
                    .unwrap_or(request.request_id);
                (Some(request_id), Some(message))
            }
            (None, None) => (None, None),
            (None, Some(_)) => {
                return Err(PulseError::validation(
                    "provider_handle_missing",
                    "durable provider request requires a provider handle",
                ));
            }
        };
        let effect = self.record_external_effect(
            &effect_id,
            ExternalEffectKind::SessionSend,
            session_id,
            &hash_bytes(request_identity.as_bytes()),
            format!(
                "request_id={:?} input_hash={}",
                request_id,
                hash_bytes(input.as_bytes())
            ),
            message.clone(),
        )?;
        let committed_turn_id = match effect.state {
            ExternalEffectState::NotSent => None,
            ExternalEffectState::Acknowledged => {
                let committed = self.store.with_state(false, |state| {
                    Ok(effect_has_committed_owner(state, &effect))
                })?;
                if !committed {
                    return Err(external_effect_blocked(&effect_id));
                }
                effect.resource_id.clone()
            }
            ExternalEffectState::Attempting
            | ExternalEffectState::OutcomeUnknown
            | ExternalEffectState::DefinitivelyFailed => {
                return Err(external_effect_blocked(&effect_id));
            }
        };
        self.store.check_failpoint("after_session_send_intent")?;
        Ok(PreparedSessionTurn {
            _session_guard,
            snapshot,
            process_id,
            request_id,
            message,
            effect_id,
            committed_turn_id,
        })
    }

    /// Performs the provider I/O of a prepared turn and commits the session to
    /// `Running` with the transport-acknowledged turn identifier.
    pub(super) fn execute_session_turn(
        &self,
        prepared: PreparedSessionTurn,
        input: &str,
    ) -> Result<CommittedSessionTurn> {
        if let Some(turn_id) = prepared.committed_turn_id {
            let session = self.store.with_state(false, |state| {
                state
                    .sessions
                    .get(&prepared.snapshot.session_id)
                    .cloned()
                    .ok_or_else(|| PulseError::NotFound {
                        subject: format!("session {}", prepared.snapshot.session_id),
                    })
            })?;
            return Ok(CommittedSessionTurn {
                provider_turn_id: Some(turn_id),
                session,
            });
        }
        let provider = self.providers.get(&prepared.snapshot.provider_id)?;
        self.update_external_effect(
            &prepared.effect_id,
            ExternalEffectState::Attempting,
            None,
            Some("dispatching session turn".to_string()),
        )?;
        let mut transport_acknowledged = false;
        let provider_io = match prepared.message.as_deref() {
            Some(message) => (|| -> Result<(String, Option<String>, Vec<Value>)> {
                let request_id = prepared
                    .request_id
                    .as_deref()
                    .expect("native provider request has an identifier");
                let (response, notifications) = self.process_owner.request_json(
                    &prepared.process_id,
                    request_id,
                    message,
                    Duration::from_secs(30),
                )?;
                transport_acknowledged = true;
                let turn = provider
                    .parse_turn_handle(&response)
                    .map_err(provider_protocol_after_transport)?;
                Ok((turn.clone(), Some(turn), notifications))
            })(),
            None => self
                .process_owner
                .send_line(&prepared.process_id, input)
                .map(|()| {
                    transport_acknowledged = true;
                    (format!("turn_{}", ulid::Ulid::new()), None, Vec::new())
                }),
        };
        let (turn_id, provider_turn_id, notifications) = match provider_io {
            Ok(value) => {
                let turn_resource_id = value.0.clone();
                self.update_external_effect(
                    &prepared.effect_id,
                    ExternalEffectState::Attempting,
                    Some(turn_resource_id),
                    Some("provider transport accepted turn; awaiting acknowledgement".to_string()),
                )?;
                value
            }
            Err(error) => {
                let _ = self.update_external_effect(
                    &prepared.effect_id,
                    if transport_acknowledged || is_ambiguous_provider_outcome(&error) {
                        ExternalEffectState::OutcomeUnknown
                    } else {
                        ExternalEffectState::DefinitivelyFailed
                    },
                    None,
                    Some(error.to_string()),
                );
                return Err(error);
            }
        };
        self.store
            .check_failpoint("after_session_send_success_before_ack")?;
        self.update_external_effect(
            &prepared.effect_id,
            ExternalEffectState::Acknowledged,
            Some(turn_id.clone()),
            Some("provider transport acknowledged turn".to_string()),
        )?;
        if let Err(error) = self.store.check_failpoint("before_session_turn_commit") {
            let _ = self.update_external_effect(
                &prepared.effect_id,
                ExternalEffectState::OutcomeUnknown,
                Some(turn_id.clone()),
                Some(error.to_string()),
            );
            return Err(error);
        }
        let turn_completed = notifications
            .iter()
            .any(|event| event.get("method").and_then(Value::as_str) == Some("turn/completed"));
        let session_result = self.store.with_state(true, |state| {
            for notification in notifications {
                append_event(
                    state,
                    "provider.notification",
                    Some(&prepared.snapshot.project_id),
                    Some(&prepared.snapshot.workspace_id),
                    Some(&prepared.snapshot.session_id),
                    notification,
                );
            }
            let session = state
                .sessions
                .get_mut(&prepared.snapshot.session_id)
                .expect("snapshot existed");
            session.lifecycle = if turn_completed {
                SessionLifecycle::Idle
            } else {
                SessionLifecycle::Running
            };
            session.active_turn_id = (!turn_completed).then_some(turn_id.clone());
            session.updated_at = chrono::Utc::now().to_rfc3339();
            let session = session.clone();
            append_event(
                state,
                "session.turn_started",
                Some(&session.project_id),
                Some(&session.workspace_id),
                Some(&session.session_id),
                json!({"turn_id": turn_id}),
            );
            Ok(session)
        });
        let session = match session_result {
            Ok(session) => session,
            Err(error) => {
                let _ = self.update_external_effect(
                    &prepared.effect_id,
                    ExternalEffectState::OutcomeUnknown,
                    Some(turn_id.clone()),
                    Some(format!(
                        "session/timeline commit failed after provider acknowledgement: {error}"
                    )),
                );
                return Err(error);
            }
        };
        Ok(CommittedSessionTurn {
            provider_turn_id,
            session,
        })
    }
}

/// A turn prepared for provider I/O: the session lock is held, the session
/// snapshot is fixed, and the provider request (when the protocol has one) is
/// already encoded so its request identifier can be persisted as delivery
/// correlation BEFORE any bytes reach the provider.
pub(super) struct PreparedSessionTurn {
    _session_guard: IdempotencyGuard,
    snapshot: SessionRecord,
    process_id: String,
    pub(super) request_id: Option<String>,
    message: Option<String>,
    effect_id: String,
    committed_turn_id: Option<String>,
}

pub(super) struct CommittedSessionTurn {
    /// Provider-native turn identifier from the transport acknowledgement;
    /// `None` when the provider protocol exposes none (opaque transports).
    pub(super) provider_turn_id: Option<String>,
    session: SessionRecord,
}

pub(super) fn session_send_effect_id(session_id: &str, request_identity: &str) -> String {
    let suffix = hash_bytes(request_identity.as_bytes())
        .trim_start_matches("sha256:")
        .chars()
        .take(20)
        .collect::<String>();
    format!("effect-send-{session_id}-{suffix}")
}

pub(super) fn provider_request_id(message: &str) -> Result<String> {
    serde_json::from_str::<Value>(message)?
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            PulseError::validation(
                "provider_protocol_invalid",
                "durable provider request is missing its correlation id",
            )
        })
}

pub(super) fn replace_provider_request_id(message: &str, request_id: &str) -> Result<String> {
    let mut value = serde_json::from_str::<Value>(message)?;
    let object = value.as_object_mut().ok_or_else(|| {
        PulseError::validation(
            "provider_protocol_invalid",
            "provider request must be a JSON object",
        )
    })?;
    object.insert("id".to_string(), Value::String(request_id.to_string()));
    serde_json::to_string(&value).map_err(Into::into)
}
