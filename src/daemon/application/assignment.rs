//! Assignment provisioning, delivery acknowledgement, and recovery saga mechanics.
//!
//! This module touches the daemon's assignment sagas, reservations, workspaces,
//! sessions, delivery ledger, and timeline. It calls host-local workspace/session
//! operations and Core reservation/handoff/verification APIs while preserving the
//! existing reservation, durable-intent, provider-I/O, acknowledgement, and proof
//! ordering. Failures remain fail-closed: uncertain delivery retains the lease
//! and is never blindly resent. Dependencies are limited to the parent daemon
//! application, its private use-case callees, daemon persistence/value types, and
//! Core APIs; this module owns no second facade, store, or generic saga engine.

use serde_json::{json, Value};
use std::path::Path;

use super::{
    append_event, deterministic_id, effects::external_effect_blocked,
    is_ambiguous_provider_outcome, provider_request_id, replace_provider_request_id,
    session_send_effect_id, DaemonApplication,
};
use crate::canonical_json::{hash_bytes, hash_serializable};
use crate::daemon::assignment::{
    AssignmentSagaRecord, AssignmentSagaState, DeliveryRecord, DeliveryState,
};
use crate::daemon::permissions::RuntimePrincipal;
use crate::daemon::persistence::{ExternalEffectKind, ExternalEffectState};
use crate::daemon::protocol::DaemonResponse;
use crate::daemon::provider::ProviderRegistry;
use crate::daemon::session::SessionLifecycle;
use crate::daemon::workspace::IsolationMode;
use crate::{PulseError, Result};

const DELIVERY_UNCERTAIN_MESSAGE: &str = "bootstrap delivery outcome cannot be proven: the delivery intent was recorded before provider I/O but no delivered acknowledgement was persisted. The assignment was not released and will not be re-sent; resolve the provider session manually and record a typed acknowledgement only with proof.";

impl DaemonApplication {
    pub(super) fn reconcile_assignment_sagas(&self) -> Result<()> {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum Recovery {
            Recoverable,
            UncertainDelivery,
            Activated,
        }
        let snapshot = self.store.load()?;
        let mut reconciled = Vec::new();
        for saga in snapshot.assignment_sagas.values() {
            let recovery = match saga.state {
                AssignmentSagaState::Reserving
                | AssignmentSagaState::Reserved
                | AssignmentSagaState::WorkspaceReady
                | AssignmentSagaState::SessionReady => Some(Recovery::Recoverable),
                AssignmentSagaState::DeliveryPending => {
                    if pending_bootstrap_delivery_is_retryable(&snapshot, saga, &self.providers) {
                        None
                    } else {
                        Some(Recovery::UncertainDelivery)
                    }
                }
                AssignmentSagaState::Acknowledged => {
                    let project = snapshot.projects.get(&saga.project_id);
                    let lease = saga.lease_id.as_deref();
                    match (project, lease) {
                        (Some(project), Some(lease)) => {
                            let reservations = crate::kernel::reservation::list_reservations(
                                Path::new(&project.canonical_root),
                            )?;
                            if reservations.iter().any(|reservation| {
                                reservation.lease_id == lease
                                    && reservation.state
                                        == crate::reservation::ReservationState::Active
                            }) {
                                Some(Recovery::Activated)
                            } else {
                                Some(Recovery::Recoverable)
                            }
                        }
                        _ => Some(Recovery::Recoverable),
                    }
                }
                _ => None,
            };
            if let Some(recovery) = recovery {
                reconciled.push((saga.saga_id.clone(), recovery));
            }
        }
        if reconciled.is_empty() {
            return Ok(());
        }
        self.store.with_state(true, |state| {
            for (saga_id, recovery) in &reconciled {
                let Some(saga) = state.assignment_sagas.get_mut(saga_id) else {
                    continue;
                };
                let now = chrono::Utc::now().to_rfc3339();
                match recovery {
                    Recovery::Recoverable => {
                        saga.state = AssignmentSagaState::Recoverable;
                        saga.last_error = Some(
                            "daemon restart interrupted assignment provisioning; retry with the original idempotency key"
                                .to_string(),
                        );
                    }
                    Recovery::UncertainDelivery => {
                        saga.last_error = Some(DELIVERY_UNCERTAIN_MESSAGE.to_string());
                    }
                    Recovery::Activated => {
                        saga.state = AssignmentSagaState::Activated;
                    }
                }
                saga.updated_at = now;
                // Snapshot the fields the delivery/event bookkeeping needs so
                // the mutable borrow of the saga can end before the rest of
                // `state` is touched.
                let project_id = saga.project_id.clone();
                let workspace_id = saga.workspace_id.clone();
                let session_id = saga.session_id.clone();
                let delivery_id = saga.delivery_id.clone();
                if *recovery == Recovery::UncertainDelivery {
                    // Fail closed: the provider outcome cannot be proven, so
                    // the saga stays in `DeliveryPending`. It is never blindly
                    // re-sent and its lease is never released; only a typed
                    // acknowledgement backed by proof can move it.
                    if let Some(delivery_id) = delivery_id.as_deref() {
                        if let Some(delivery) = state.deliveries.get_mut(delivery_id) {
                            if delivery.state == DeliveryState::IntentRecorded {
                                delivery.state = DeliveryState::Uncertain;
                                delivery.updated_at = chrono::Utc::now().to_rfc3339();
                            }
                        }
                    }
                    append_event(
                        state,
                        "assignment.delivery_uncertain",
                        Some(&project_id),
                        workspace_id.as_deref(),
                        session_id.as_deref(),
                        json!({
                            "saga_id": saga_id,
                            "delivery_id": delivery_id,
                        }),
                    );
                }
            }
            Ok(())
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn assignment_start(
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
        let mut normalized_capabilities = capabilities.to_vec();
        normalized_capabilities.sort();
        normalized_capabilities.dedup();
        let request_fingerprint = hash_serializable(&json!({
            "project_id": project_id,
            "ticket_id": ticket_id,
            "actor": actor,
            "assignee": assignee,
            "capabilities": normalized_capabilities,
            "isolation": isolation,
            "provider_id": provider_id,
            "provider_options": provider_options,
            "ttl_seconds": ttl_seconds,
        }))?;

        // ── Phase 0: ensure a saga record exists ──────────────────────
        let _is_recoverable = if let Some(mut existing) = self.store.with_state(false, |state| {
            Ok(state.assignment_sagas.get(&saga_id).cloned())
        })? {
            if (!existing.request_fingerprint.is_empty()
                && existing.request_fingerprint != request_fingerprint)
                || existing.project_id != project_id
                || existing.ticket_id != ticket_id
                || existing.actor != actor
                || existing.assignee != assignee
            {
                return Err(PulseError::validation(
                    "assignment_idempotency_conflict",
                    "assignment idempotency key is bound to different inputs",
                ));
            }
            if existing.request_fingerprint.is_empty() {
                self.store.with_state(true, |state| {
                    let saga = state.assignment_sagas.get_mut(&saga_id).ok_or_else(|| {
                        PulseError::NotFound {
                            subject: format!("assignment saga {saga_id}"),
                        }
                    })?;
                    saga.request_fingerprint = request_fingerprint.clone();
                    saga.updated_at = chrono::Utc::now().to_rfc3339();
                    Ok(())
                })?;
            }
            let can_resume_pending_delivery = existing.state
                == AssignmentSagaState::DeliveryPending
                && self.store.with_state(false, |state| {
                    Ok(pending_bootstrap_delivery_is_retryable(
                        state,
                        &existing,
                        &self.providers,
                    ))
                })?;
            if matches!(
                existing.state,
                AssignmentSagaState::DeliveryPending
                    | AssignmentSagaState::BootstrapDelivered
                    | AssignmentSagaState::Acknowledged
                    | AssignmentSagaState::Activated
            ) && !can_resume_pending_delivery
            {
                // A `DeliveryPending` saga is the fail-closed bootstrap state:
                // the provider outcome cannot be proven, so it is never re-sent
                // and never released. Surface the operator explanation once it
                // is observed, then replay the pending saga unchanged.
                if existing.state == AssignmentSagaState::DeliveryPending
                    && existing.last_error.is_none()
                {
                    let message = DELIVERY_UNCERTAIN_MESSAGE.to_string();
                    let _ = self.store.with_state(true, |state| {
                        if let Some(saga) = state.assignment_sagas.get_mut(&saga_id) {
                            saga.last_error = Some(message.clone());
                            saga.updated_at = chrono::Utc::now().to_rfc3339();
                        }
                        Ok(())
                    });
                    existing.last_error = Some(message);
                }
                if existing.state == AssignmentSagaState::DeliveryPending {
                    if let Some(session_id) = existing.session_id.as_deref() {
                        let unresolved_effect = self.store.with_state(false, |state| {
                            Ok(state.external_effects.values().find_map(|effect| {
                                (effect.owner_id == session_id
                                    && effect.kind == ExternalEffectKind::SessionSend
                                    && matches!(
                                        effect.state,
                                        ExternalEffectState::Attempting
                                            | ExternalEffectState::OutcomeUnknown
                                    ))
                                .then_some(effect.effect_id.clone())
                            }))
                        })?;
                        if let Some(effect_id) = unresolved_effect {
                            return Err(external_effect_blocked(&effect_id));
                        }
                    }
                }
                // Fast path: already provisioned — idempotent return.
                return Ok(DaemonResponse::Assignment { saga: existing });
            }
            existing.state == AssignmentSagaState::Recoverable
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
                request_fingerprint: request_fingerprint.clone(),
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
            false
        };

        // ── Phase 1: reserve (or reuse existing) Core lease ───────────
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

        // ── Phase 2: provision or recover workspace ───────────────────
        let workspace_request_key = format!("{idempotency_key}:workspace");
        let planned_workspace_id = deterministic_id("wks", &workspace_request_key);
        self.store.with_state(true, |state| {
            let saga = state
                .assignment_sagas
                .get_mut(&saga_id)
                .expect("saga exists");
            saga.workspace_id = Some(planned_workspace_id.clone());
            saga.updated_at = chrono::Utc::now().to_rfc3339();
            Ok(())
        })?;
        let workspace = match self.workspace_create(
            project_id,
            &format!("assignment-{ticket_id}"),
            isolation,
            Some(&reservation.packet.source.commit),
            &workspace_request_key,
        ) {
            Ok(DaemonResponse::Workspace { workspace }) => workspace,
            Ok(_) => unreachable!("workspace_create response kind"),
            Err(error) => {
                self.handle_provisioning_failure(
                    &core,
                    &saga_id,
                    actor,
                    &reservation.reservation.lease_id,
                    Some(&planned_workspace_id),
                    None,
                    "workspace provisioning failed",
                    &error,
                )?;
                return Err(error);
            }
        };
        // If this is a recovery retry, the workspace may be archived.
        let workspace =
            if workspace.lifecycle == crate::daemon::workspace::WorkspaceLifecycle::Archived {
                match self.workspace_restore(&workspace.workspace_id)? {
                    DaemonResponse::Workspace { workspace } => workspace,
                    _ => unreachable!("workspace_restore response kind"),
                }
            } else {
                workspace
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

        // ── Phase 3: provision or recover session ─────────────────────
        // On recovery the session may be in Error/Closed state. A native
        // provider handle is resumed through a fresh transport process while
        // the stable Pulse session_id and provider handle stay unchanged.
        let session_request_key = format!("{idempotency_key}:session");
        let planned_session_id = deterministic_id("ses", &session_request_key);
        self.store.with_state(true, |state| {
            let saga = state
                .assignment_sagas
                .get_mut(&saga_id)
                .expect("saga exists");
            saga.session_id = Some(planned_session_id.clone());
            saga.updated_at = chrono::Utc::now().to_rfc3339();
            Ok(())
        })?;
        let session = match self.session_create(
            &workspace.workspace_id,
            provider_id,
            None,
            provider_options,
            &session_request_key,
        ) {
            Ok(DaemonResponse::Session { session }) => session,
            Ok(_) => unreachable!("session_create response kind"),
            Err(error) => {
                self.handle_provisioning_failure(
                    &core,
                    &saga_id,
                    actor,
                    &reservation.reservation.lease_id,
                    Some(&workspace.workspace_id),
                    Some(&planned_session_id),
                    "session provisioning failed",
                    &error,
                )?;
                return Err(error);
            }
        };
        let session = if session.lifecycle != SessionLifecycle::Idle {
            match self.session_resume(&session.session_id, provider_options) {
                Ok(DaemonResponse::Session { session }) => session,
                Ok(_) => unreachable!("session_resume response kind"),
                Err(error) => {
                    self.handle_provisioning_failure(
                        &core,
                        &saga_id,
                        actor,
                        &reservation.reservation.lease_id,
                        Some(&workspace.workspace_id),
                        Some(&session.session_id),
                        "session resume failed",
                        &error,
                    )?;
                    return Err(error);
                }
            }
        } else {
            session
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

        // ── Phase 4: deliver bootstrap ────────────────────────────────
        let delivery_id = deterministic_id("delivery", idempotency_key);
        let bootstrap = assignment_bootstrap_payload(
            ticket_id,
            &reservation.reservation.lease_id,
            &reservation.reservation.packet_fingerprint,
        );
        let prepared = match self.prepare_session_turn(
            &session.session_id,
            &bootstrap,
            idempotency_key,
            self.store
                .with_state(false, |state| {
                    Ok(state
                        .deliveries
                        .get(&delivery_id)
                        .and_then(|delivery| delivery.correlation_request_id.clone()))
                })?
                .as_deref(),
        ) {
            Ok(prepared) => prepared,
            Err(error)
                if is_ambiguous_provider_outcome(&error)
                    || error.code() == "external_effect_reconciliation_required" =>
            {
                self.persist_bootstrap_uncertainty(
                    &saga_id,
                    &delivery_id,
                    &session.session_id,
                    &bootstrap,
                    &error,
                )?;
                return Err(error);
            }
            Err(error) => {
                self.compensate_failed_bootstrap_delivery(
                    &core,
                    &saga_id,
                    &session.session_id,
                    &workspace.workspace_id,
                    &reservation.reservation.lease_id,
                    actor,
                    &delivery_id,
                    &error,
                )?;
                return Err(error);
            }
        };
        let intent_now = chrono::Utc::now().to_rfc3339();
        self.store.with_state(true, |state| {
            let saga = state
                .assignment_sagas
                .get_mut(&saga_id)
                .expect("saga exists");
            saga.delivery_id = Some(delivery_id.clone());
            saga.state = AssignmentSagaState::DeliveryPending;
            saga.last_error = None;
            saga.updated_at = chrono::Utc::now().to_rfc3339();
            state
                .deliveries
                .entry(delivery_id.clone())
                .or_insert(DeliveryRecord {
                    schema_version: 1,
                    delivery_id: delivery_id.clone(),
                    saga_id: saga_id.clone(),
                    session_id: session.session_id.clone(),
                    payload: bootstrap.clone(),
                    correlation_request_id: prepared.request_id.clone(),
                    correlation_turn_id: None,
                    state: DeliveryState::IntentRecorded,
                    created_at: intent_now.clone(),
                    updated_at: intent_now,
                });
            append_event(
                state,
                "assignment.delivery_intent_recorded",
                Some(project_id),
                Some(&workspace.workspace_id),
                Some(&session.session_id),
                json!({"saga_id": saga_id, "delivery_id": delivery_id}),
            );
            Ok(())
        })?;
        // Crash-consistency boundary: the durable delivery intent now exists
        // BEFORE any provider I/O. From here on, a crash or commit failure must
        // never blindly re-send the bootstrap and must never release the
        // reservation without proof of the provider outcome.
        self.store.check_failpoint("after_delivery_intent")?;
        let committed = match self.execute_session_turn(prepared, &bootstrap) {
            Ok(committed) => committed,
            Err(error) if is_ambiguous_provider_outcome(&error) => {
                self.mark_bootstrap_delivery_uncertain(&saga_id, &delivery_id, &error)?;
                return Err(error);
            }
            Err(error) => {
                self.compensate_failed_bootstrap_delivery(
                    &core,
                    &saga_id,
                    &session.session_id,
                    &workspace.workspace_id,
                    &reservation.reservation.lease_id,
                    actor,
                    &delivery_id,
                    &error,
                )?;
                return Err(error);
            }
        };
        self.store
            .check_failpoint("before_delivery_delivered_commit")?;
        self.store.with_state(true, |state| {
            let saga = state
                .assignment_sagas
                .get_mut(&saga_id)
                .expect("saga exists");
            saga.state = AssignmentSagaState::BootstrapDelivered;
            saga.last_error = None;
            saga.updated_at = chrono::Utc::now().to_rfc3339();
            if let Some(delivery) = state.deliveries.get_mut(&delivery_id) {
                delivery.state = DeliveryState::Delivered;
                delivery.correlation_turn_id = committed.provider_turn_id.clone();
                delivery.updated_at = chrono::Utc::now().to_rfc3339();
            }
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

    pub(super) fn mark_bootstrap_delivery_uncertain(
        &self,
        saga_id: &str,
        delivery_id: &str,
        error: &PulseError,
    ) -> Result<()> {
        self.store.with_state(true, |state| {
            if let Some(saga) = state.assignment_sagas.get_mut(saga_id) {
                saga.state = AssignmentSagaState::DeliveryPending;
                saga.last_error = Some(DELIVERY_UNCERTAIN_MESSAGE.to_string());
                saga.updated_at = chrono::Utc::now().to_rfc3339();
            }
            if let Some(delivery) = state.deliveries.get_mut(delivery_id) {
                delivery.state = DeliveryState::Uncertain;
                delivery.updated_at = chrono::Utc::now().to_rfc3339();
            }
            append_event(
                state,
                "assignment.delivery_uncertain",
                None,
                None,
                None,
                json!({"saga_id": saga_id, "delivery_id": delivery_id, "error": error.to_string()}),
            );
            Ok(())
        })
    }

    fn persist_bootstrap_uncertainty(
        &self,
        saga_id: &str,
        delivery_id: &str,
        session_id: &str,
        payload: &str,
        error: &PulseError,
    ) -> Result<()> {
        self.store.with_state(true, |state| {
            let now = chrono::Utc::now().to_rfc3339();
            if let Some(saga) = state.assignment_sagas.get_mut(saga_id) {
                saga.delivery_id = Some(delivery_id.to_string());
                saga.state = AssignmentSagaState::DeliveryPending;
                saga.last_error = Some(DELIVERY_UNCERTAIN_MESSAGE.to_string());
                saga.updated_at = now.clone();
            }
            state
                .deliveries
                .entry(delivery_id.to_string())
                .or_insert(DeliveryRecord {
                    schema_version: 1,
                    delivery_id: delivery_id.to_string(),
                    saga_id: saga_id.to_string(),
                    session_id: session_id.to_string(),
                    payload: payload.to_string(),
                    correlation_request_id: None,
                    correlation_turn_id: None,
                    state: DeliveryState::Uncertain,
                    created_at: now.clone(),
                    updated_at: now,
                });
            append_event(
                state,
                "assignment.delivery_uncertain",
                None,
                None,
                Some(session_id),
                json!({"saga_id": saga_id, "delivery_id": delivery_id, "error": error.to_string()}),
            );
            Ok(())
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_provisioning_failure(
        &self,
        core: &crate::JsonGraphStore,
        saga_id: &str,
        actor: &str,
        lease_id: &str,
        workspace_id: Option<&str>,
        session_id: Option<&str>,
        operation: &str,
        error: &PulseError,
    ) -> Result<()> {
        if !self.provisioning_effects_are_definitively_safe(workspace_id, session_id)? {
            return self.mark_provisioning_uncertain(saga_id, operation, error);
        }
        if let Some(session_id) = session_id {
            let _ = self.session_close(session_id);
        }
        if let Some(workspace_id) = workspace_id {
            let _ = self.workspace_archive(workspace_id);
        }
        let released = core.release_reservation(lease_id, actor, operation).is_ok();
        self.mark_saga_error(
            saga_id,
            if released {
                AssignmentSagaState::Released
            } else {
                AssignmentSagaState::Recoverable
            },
            error,
        )
    }

    fn provisioning_effects_are_definitively_safe(
        &self,
        workspace_id: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<bool> {
        self.store.with_state(false, |state| {
            Ok(state
                .external_effects
                .values()
                .filter(|effect| {
                    workspace_id.is_some_and(|id| {
                        effect.owner_id == id
                            && matches!(effect.kind, ExternalEffectKind::WorktreeCreate)
                    }) || session_id.is_some_and(|id| {
                        effect.owner_id == id
                            && matches!(
                                effect.kind,
                                ExternalEffectKind::ProviderProcessCreate
                                    | ExternalEffectKind::ProviderSessionCreate
                                    | ExternalEffectKind::ProviderSessionResume
                                    | ExternalEffectKind::SessionSend
                            )
                    })
                })
                .all(|effect| {
                    matches!(
                        effect.state,
                        ExternalEffectState::NotSent | ExternalEffectState::DefinitivelyFailed
                    )
                }))
        })
    }

    fn mark_provisioning_uncertain(
        &self,
        saga_id: &str,
        operation: &str,
        error: &PulseError,
    ) -> Result<()> {
        let detail = format!(
            "{operation} outcome is uncertain; Core lease retained. Reconcile or clean up all external effects before retrying: {error}"
        );
        self.store.with_state(true, |state| {
            let saga =
                state
                    .assignment_sagas
                    .get_mut(saga_id)
                    .ok_or_else(|| PulseError::NotFound {
                        subject: format!("assignment saga {saga_id}"),
                    })?;
            saga.state = AssignmentSagaState::Recoverable;
            saga.last_error = Some(detail.clone());
            saga.updated_at = chrono::Utc::now().to_rfc3339();
            let project_id = saga.project_id.clone();
            let workspace_id = saga.workspace_id.clone();
            let session_id = saga.session_id.clone();
            append_event(
                state,
                "assignment.provisioning_uncertain",
                Some(&project_id),
                workspace_id.as_deref(),
                session_id.as_deref(),
                json!({
                    "saga_id": saga_id,
                    "operation": operation,
                    "error": error.to_string(),
                    "operator_action": "reconcile_or_cleanup_external_effects_before_retry"
                }),
            );
            Ok(())
        })
    }

    /// Records the delivery as failed and applies the existing delivery-failure
    /// compensation: close the session, archive the workspace and release the
    /// Core reservation. This is only safe while the daemon is live and the
    /// failure was observed synchronously; crash recovery never takes this path
    /// and instead fails closed in `DeliveryPending`.
    #[allow(clippy::too_many_arguments)]
    fn compensate_failed_bootstrap_delivery(
        &self,
        core: &crate::JsonGraphStore,
        saga_id: &str,
        session_id: &str,
        workspace_id: &str,
        lease_id: &str,
        actor: &str,
        delivery_id: &str,
        error: &PulseError,
    ) -> Result<()> {
        if !self.provisioning_effects_are_definitively_safe(Some(workspace_id), Some(session_id))? {
            return self.mark_bootstrap_delivery_uncertain(saga_id, delivery_id, error);
        }
        let _ = self.store.with_state(true, |state| {
            if let Some(delivery) = state.deliveries.get_mut(delivery_id) {
                delivery.state = DeliveryState::Failed;
                delivery.updated_at = chrono::Utc::now().to_rfc3339();
            }
            Ok(())
        });
        let _ = self.session_close(session_id);
        let _ = self.workspace_archive(workspace_id);
        let released = core
            .release_reservation(lease_id, actor, "bootstrap delivery failed")
            .is_ok();
        self.mark_saga_error(
            saga_id,
            if released {
                AssignmentSagaState::Released
            } else {
                AssignmentSagaState::Recoverable
            },
            error,
        )
    }

    pub(super) fn assignment_acknowledge_admin(
        &self,
        principal: &RuntimePrincipal,
        saga_id: &str,
        acknowledgement_id: &str,
    ) -> Result<DaemonResponse> {
        principal.require("runtime.admin").map_err(|code| {
            PulseError::validation(
                code,
                "legacy acknowledgement requires explicit admin recovery",
            )
        })?;
        let response = self.assignment_acknowledge(saga_id, acknowledgement_id)?;
        self.store.with_state(true, |state| {
            append_event(
                state,
                "assignment.admin_acknowledged",
                None,
                None,
                None,
                json!({"saga_id": saga_id, "acknowledgement_id": acknowledgement_id}),
            );
            Ok(())
        })?;
        Ok(response)
    }

    pub(super) fn assignment_acknowledge_bound(
        &self,
        saga_id: &str,
        acknowledgement_id: &str,
        lease_id: &str,
        session_id: &str,
        packet_fingerprint: &str,
        delivery_id: &str,
    ) -> Result<DaemonResponse> {
        let saga = self.store.with_state(false, |state| {
            state
                .assignment_sagas
                .get(saga_id)
                .cloned()
                .ok_or_else(|| PulseError::NotFound {
                    subject: format!("assignment saga {saga_id}"),
                })
        })?;
        if saga.lease_id.as_deref() != Some(lease_id)
            || saga.session_id.as_deref() != Some(session_id)
            || saga.packet_fingerprint != packet_fingerprint
            || saga.delivery_id.as_deref() != Some(delivery_id)
        {
            return Err(PulseError::validation(
                "assignment_acknowledgement_mismatch",
                "acknowledgement does not bind the exact saga lease, session, packet and delivery",
            ));
        }
        let delivery = self.store.with_state(false, |state| {
            state.deliveries.get(delivery_id).cloned().ok_or_else(|| {
                PulseError::validation(
                    "assignment_acknowledgement_mismatch",
                    "acknowledgement delivery does not exist",
                )
            })
        })?;
        if delivery.saga_id != saga_id
            || delivery.session_id != session_id
            || delivery.state != DeliveryState::Delivered
        {
            return Err(PulseError::validation(
                "assignment_acknowledgement_mismatch",
                "acknowledgement delivery is not the delivered record for this saga session",
            ));
        }
        self.assignment_acknowledge(saga_id, acknowledgement_id)
    }

    fn assignment_acknowledge(
        &self,
        saga_id: &str,
        acknowledgement_id: &str,
    ) -> Result<DaemonResponse> {
        let _saga_guard = self
            .store
            .acquire_idempotency(&format!("assignment-saga:{saga_id}"))?;
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
            if saga.acknowledgement_id.as_deref() != Some(acknowledgement_id) {
                return Err(PulseError::validation(
                    "assignment_acknowledgement_conflict",
                    "activated assignment is bound to a different acknowledgement",
                ));
            }
            return Ok(DaemonResponse::Assignment { saga });
        }
        if let Some(existing_acknowledgement_id) = saga.acknowledgement_id.as_deref() {
            if existing_acknowledgement_id != acknowledgement_id {
                return Err(PulseError::validation(
                    "assignment_acknowledgement_conflict",
                    "assignment saga is already bound to a different acknowledgement",
                ));
            }
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

    pub(super) fn assignment_inspect(&self, saga_id: &str) -> Result<DaemonResponse> {
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

    pub(super) fn handoff_submit(
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
    pub(super) fn verification_complete(
        &self,
        saga_id: &str,
        actor: &str,
        source_commit: &str,
        disposition: crate::execution::VerificationDisposition,
        summary: &str,
        checks: &[crate::execution::VerificationCheck],
        acceptance_proofs: &[crate::execution::AcceptanceProof],
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
                acceptance_proofs: acceptance_proofs.to_vec(),
                idempotency_key: format!("{idempotency_key}:core-verification"),
            })?;
        self.store.with_state(true, |state| {
            let saga = state
                .assignment_sagas
                .get_mut(saga_id)
                .expect("saga exists");
            saga.state = verification_saga_state(verification.disposition);
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

    pub(super) fn assignment_close(
        &self,
        saga_id: &str,
        actor: &str,
        source_commit: &str,
        summary: &str,
        idempotency_key: &str,
    ) -> Result<DaemonResponse> {
        let saga = self.assignment_saga(saga_id)?;
        if saga.state != AssignmentSagaState::Verifying {
            return Err(PulseError::validation(
                "assignment_not_verifying",
                "proof close requires a passed verification in the nonterminal verifying state",
            ));
        }
        let verification_id = saga.verification_id.clone().ok_or_else(|| {
            PulseError::validation(
                "assignment_verification_missing",
                "proof close requires a persisted verification receipt",
            )
        })?;
        let project = self.project_record(&saga.project_id)?;
        let core = crate::JsonGraphStore::new(&project.canonical_root);
        let close = core.close_execution_ticket(crate::execution::CloseTicketArgs {
            verification_id,
            actor: actor.to_string(),
            source_commit: source_commit.to_string(),
            summary: summary.to_string(),
            idempotency_key: format!("{idempotency_key}:core-close"),
        })?;
        self.store.with_state(true, |state| {
            let saga = state
                .assignment_sagas
                .get_mut(saga_id)
                .expect("saga exists");
            saga.state = AssignmentSagaState::Done;
            saga.updated_at = chrono::Utc::now().to_rfc3339();
            let project_id = saga.project_id.clone();
            let workspace_id = saga.workspace_id.clone();
            let session_id = saga.session_id.clone();
            append_event(
                state,
                "assignment.closed",
                Some(&project_id),
                workspace_id.as_deref(),
                session_id.as_deref(),
                json!({
                    "saga_id": saga_id,
                    "close_id": close.close_id,
                    "verification_id": close.verification_id,
                }),
            );
            Ok(())
        })?;
        Ok(DaemonResponse::Close { close })
    }

    pub(super) fn assignment_saga(&self, saga_id: &str) -> Result<AssignmentSagaRecord> {
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
}

fn assignment_bootstrap_payload(
    ticket_id: &str,
    lease_id: &str,
    packet_fingerprint: &str,
) -> String {
    format!(
        "Pulse assignment {ticket_id}\nlease={lease_id}\npacket_fingerprint={packet_fingerprint}\nload: pulse work packet {ticket_id} --lease {lease_id} --json\nAuthority: implement only the exact lease-bound contract. Submit typed handoff evidence; process exit is not completion."
    )
}

fn pending_bootstrap_delivery_is_retryable(
    state: &crate::daemon::persistence::DaemonState,
    saga: &AssignmentSagaRecord,
    providers: &ProviderRegistry,
) -> bool {
    if !state.assignment_sagas.contains_key(&saga.saga_id) {
        return false;
    }
    let (Some(lease_id), Some(workspace_id), Some(session_id), Some(delivery_id)) = (
        saga.lease_id.as_deref(),
        saga.workspace_id.as_deref(),
        saga.session_id.as_deref(),
        saga.delivery_id.as_deref(),
    ) else {
        return false;
    };
    if !state.projects.contains_key(&saga.project_id) {
        return false;
    }
    let Some(workspace) = state.workspaces.get(workspace_id) else {
        return false;
    };
    if workspace.project_id != saga.project_id {
        return false;
    }
    let Some(delivery) = state.deliveries.get(delivery_id) else {
        return false;
    };
    if delivery.delivery_id != delivery_id
        || delivery.saga_id != saga.saga_id
        || delivery.session_id != session_id
        || delivery.state != DeliveryState::IntentRecorded
        || delivery.correlation_turn_id.is_some()
    {
        return false;
    }
    if delivery.payload
        != assignment_bootstrap_payload(&saga.ticket_id, lease_id, &saga.packet_fingerprint)
    {
        return false;
    }
    let Some(session) = state.sessions.get(session_id) else {
        return false;
    };
    if session.project_id != saga.project_id
        || session.workspace_id != workspace_id
        || session.provider_handle.is_none()
        || session.managed_process_id.is_none()
        || delivery.correlation_request_id.is_none()
    {
        return false;
    }
    let process_id = session.managed_process_id.as_deref().unwrap();
    let Some(process) = state.processes.get(process_id) else {
        return false;
    };
    if process.owner_kind != "session"
        || process.owner_id != session_id
        || process.provider_id != session.provider_id
    {
        return false;
    }
    let effect_id = session_send_effect_id(session_id, &saga.idempotency_key);
    let Some(effect) = state.external_effects.get(&effect_id) else {
        return false;
    };
    let expected_detail = format!(
        "request_id={:?} input_hash={}",
        delivery.correlation_request_id,
        hash_bytes(delivery.payload.as_bytes())
    );
    let Some(request_message) = effect.request_message.as_deref() else {
        return false;
    };
    let Ok(request_message_id) = provider_request_id(request_message) else {
        return false;
    };
    let Ok(provider) = providers.get(&session.provider_id) else {
        return false;
    };
    let Some(provider_handle) = session.provider_handle.as_deref() else {
        return false;
    };
    let Ok(expected_request) = provider.encode_send(provider_handle, &delivery.payload) else {
        return false;
    };
    let Ok(expected_message) = replace_provider_request_id(
        &expected_request.message,
        delivery.correlation_request_id.as_deref().unwrap(),
    ) else {
        return false;
    };
    let Ok(stored_json) = serde_json::from_str::<Value>(request_message) else {
        return false;
    };
    let Ok(expected_json) = serde_json::from_str::<Value>(&expected_message) else {
        return false;
    };
    effect.kind == ExternalEffectKind::SessionSend
        && effect.owner_id == session_id
        && effect.state == ExternalEffectState::NotSent
        && effect.resource_id.is_none()
        && effect.request_fingerprint == hash_bytes(saga.idempotency_key.as_bytes())
        && effect.detail == expected_detail
        && Some(request_message_id) == delivery.correlation_request_id
        && stored_json == expected_json
}

pub(super) fn verification_saga_state(
    disposition: crate::execution::VerificationDisposition,
) -> AssignmentSagaState {
    match disposition {
        // Core intentionally leaves the ticket in `verifying` until its close
        // gate proves the final transition. The daemon saga mirrors that
        // nonterminal authority boundary.
        crate::execution::VerificationDisposition::Passed => AssignmentSagaState::Verifying,
        crate::execution::VerificationDisposition::Rework => AssignmentSagaState::Rework,
        crate::execution::VerificationDisposition::Blocked => AssignmentSagaState::Blocked,
    }
}
