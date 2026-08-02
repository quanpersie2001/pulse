//! Owns daemon startup recovery ordering over the single application store.
//!
//! This module touches process, session, external-effect, epoch, and timeline state and depends on the daemon ProcessOwner plus parent effect/event helpers. It fails closed, completing durable uncertainty and epoch records before invoking assignment reconciliation; it performs no Core repository semantics beyond that assignment-owned call.

use serde_json::json;

use crate::daemon::persistence::ExternalEffectState;
use crate::daemon::process::ManagedProcessState;
use crate::daemon::session::SessionLifecycle;
use crate::Result;

use super::{append_event, effect_has_committed_owner, DaemonApplication};

impl DaemonApplication {
    pub(super) fn begin_epoch_and_recover(&self) -> Result<()> {
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
            let effect_ids = state.external_effects.keys().cloned().collect::<Vec<_>>();
            for effect_id in effect_ids {
                let uncertain = {
                    let acknowledged_without_owner = state
                        .external_effects
                        .get(&effect_id)
                        .is_some_and(|effect| {
                            effect.state == ExternalEffectState::Acknowledged
                                && !effect_has_committed_owner(state, effect)
                        });
                    let Some(effect) = state.external_effects.get_mut(&effect_id) else {
                        continue;
                    };
                    if effect.state == ExternalEffectState::NotSent
                        || effect.state == ExternalEffectState::DefinitivelyFailed
                        || (effect.state == ExternalEffectState::Acknowledged
                            && !acknowledged_without_owner)
                    {
                        None
                    } else {
                        effect.state = ExternalEffectState::OutcomeUnknown;
                        effect.updated_at = chrono::Utc::now().to_rfc3339();
                        Some((
                            effect.effect_id.clone(),
                            effect.kind.clone(),
                            effect.owner_id.clone(),
                            effect.detail.clone(),
                        ))
                    }
                };
                let Some((effect_id_value, effect_kind, owner_id, detail)) = uncertain else {
                    continue;
                };
                append_event(
                    state,
                    "daemon.external_effect_uncertain",
                    None,
                    None,
                    None,
                    json!({
                        "effect_id": effect_id_value,
                        "kind": effect_kind,
                        "owner_id": owner_id,
                        "detail": detail,
                        "operator_action": "reconcile_or_cleanup_before_retry"
                    }),
                );
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
}
