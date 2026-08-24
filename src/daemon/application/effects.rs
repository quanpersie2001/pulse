//! Durable external-effect ledger mechanics for the daemon application.
//!
//! This module touches the single [`DaemonApplication`] state store's external
//! effect records and related recovery-owned state. Its invariant is that
//! journal mechanics never own caller I/O ordering or compensation. It may
//! depend only on daemon persistence/value types and the application store;
//! provider, process, timeline, and Core I/O ordering remain with callers.

use serde_json::Value;

use super::DaemonApplication;
use crate::daemon::assignment::DeliveryState;
use crate::daemon::persistence::{
    DaemonState, ExternalEffectKind, ExternalEffectRecord, ExternalEffectState,
};
use crate::daemon::process::ManagedProcessState;
use crate::{PulseError, Result};

impl DaemonApplication {
    pub(super) fn record_external_effect(
        &self,
        effect_id: &str,
        kind: ExternalEffectKind,
        owner_id: &str,
        request_fingerprint: &str,
        detail: String,
        request_message: Option<String>,
    ) -> Result<ExternalEffectRecord> {
        let now = chrono::Utc::now().to_rfc3339();
        self.store.with_state(true, |state| {
            if let Some(existing) = state.external_effects.get(effect_id).cloned() {
                if !existing.request_fingerprint.is_empty()
                    && existing.request_fingerprint != request_fingerprint
                {
                    return Err(PulseError::validation(
                        "external_effect_fingerprint_conflict",
                        format!("external effect {effect_id} was requested with different inputs"),
                    ));
                }
                if existing.kind != kind || existing.owner_id != owner_id {
                    return Err(PulseError::validation(
                        "external_effect_identity_conflict",
                        format!("external effect {effect_id} belongs to a different resource"),
                    ));
                }
                if let (Some(existing_message), Some(request_message)) =
                    (existing.request_message.as_deref(), request_message.as_deref())
                {
                    if existing_message != request_message {
                        return Err(PulseError::validation(
                            "external_effect_request_conflict",
                            format!("external effect {effect_id} was requested with different provider payloads"),
                        ));
                    }
                }
                if existing.state == ExternalEffectState::NotSent
                    && (existing.request_fingerprint.is_empty()
                        || (existing.request_message.is_none() && request_message.is_some()))
                {
                    let effect = state
                        .external_effects
                        .get_mut(effect_id)
                        .expect("effect exists");
                    if effect.request_fingerprint.is_empty() {
                        effect.request_fingerprint = request_fingerprint.to_string();
                    }
                    if effect.request_message.is_none() {
                        effect.request_message = request_message.clone();
                    }
                    effect.updated_at = now;
                    return Ok(effect.clone());
                }
                return Ok(existing);
            }
            let record = ExternalEffectRecord {
                schema_version: 1,
                effect_id: effect_id.to_string(),
                kind,
                state: ExternalEffectState::NotSent,
                owner_id: owner_id.to_string(),
                request_fingerprint: request_fingerprint.to_string(),
                resource_id: None,
                request_message,
                attempt_process: None,
                detail,
                created_at: now.clone(),
                updated_at: now,
            };
            state
                .external_effects
                .insert(effect_id.to_string(), record.clone());
            Ok(record)
        })
    }

    pub(super) fn update_external_effect(
        &self,
        effect_id: &str,
        state_value: ExternalEffectState,
        resource_id: Option<String>,
        detail: Option<String>,
    ) -> Result<()> {
        self.store.with_state(true, |state| {
            let effect =
                state
                    .external_effects
                    .get_mut(effect_id)
                    .ok_or_else(|| PulseError::NotFound {
                        subject: format!("external effect {effect_id}"),
                    })?;
            effect.state = state_value;
            if resource_id.is_some() {
                effect.resource_id = resource_id;
            }
            if let Some(detail) = detail {
                effect.detail = detail;
            }
            effect.updated_at = chrono::Utc::now().to_rfc3339();
            Ok(())
        })
    }
}

pub(super) fn external_effect_blocked(effect_id: &str) -> PulseError {
    PulseError::validation(
        "external_effect_reconciliation_required",
        format!(
            "external effect {effect_id} has an unresolved resource; reconcile, adopt, or clean it up before retrying"
        ),
    )
}

pub(super) fn effect_has_committed_owner(
    state: &DaemonState,
    effect: &ExternalEffectRecord,
) -> bool {
    match &effect.kind {
        ExternalEffectKind::WorktreeCreate => state.workspaces.contains_key(&effect.owner_id),
        ExternalEffectKind::ProviderProcessCreate => effect
            .resource_id
            .as_ref()
            .is_some_and(|id| state.processes.contains_key(id)),
        ExternalEffectKind::ProviderSessionCreate => state
            .sessions
            .get(&effect.owner_id)
            .is_some_and(|session| session.provider_handle.is_some()),
        ExternalEffectKind::ProviderSessionResume => {
            effect.resource_id.as_ref().is_some_and(|process_id| {
                state
                    .sessions
                    .get(&effect.owner_id)
                    .and_then(|session| session.managed_process_id.as_ref())
                    == Some(process_id)
                    && state
                        .processes
                        .get(process_id)
                        .is_some_and(|process| process.state == ManagedProcessState::Running)
            })
        }
        ExternalEffectKind::SessionSend => effect.resource_id.as_ref().is_some_and(|turn_id| {
            state.timeline.iter().any(|event| {
                event.event_type == "session.turn_started"
                    && event.session_id.as_deref() == Some(effect.owner_id.as_str())
                    && event.payload.get("turn_id").and_then(Value::as_str) == Some(turn_id)
            })
        }),
        ExternalEffectKind::BootstrapDelivery => state
            .deliveries
            .get(&effect.owner_id)
            .is_some_and(|delivery| delivery.state == DeliveryState::Delivered),
        ExternalEffectKind::QaCheckpointRun => {
            effect.resource_id.as_ref().is_some_and(|receipt_id| {
                state.timeline.iter().any(|event| {
                    event.event_type == "assignment.qa_checkpoint_completed"
                        && event.payload.get("saga_id").and_then(Value::as_str)
                            == Some(effect.owner_id.as_str())
                        && event.payload.get("receipt_id").and_then(Value::as_str)
                            == Some(receipt_id.as_str())
                })
            })
        }
    }
}
