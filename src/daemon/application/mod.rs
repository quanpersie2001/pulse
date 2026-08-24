//! Transport-neutral daemon application services behind one concrete facade.
//!
//! [`DaemonApplication`] owns composition over one [`StateStore`], the daemon
//! [`ProcessOwner`], provider registries, startup recovery, and shutdown. Its
//! private child modules own cohesive use cases; Core semantics remain behind
//! typed public reservation/proof gates. The important invariant is one runtime
//! lifecycle writer: recovery completes before provider serving begins, and no
//! second facade or store is introduced.

mod assignment;
mod communication;
mod dispatch;
mod effects;
mod project;
mod qa;
mod recovery;
mod session;
mod timeline;
mod turn;
mod workspace;

use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::canonical_json::hash_bytes;
use crate::daemon::permissions::RuntimePrincipal;
use crate::daemon::persistence::StateStore;
use crate::daemon::process::{ManagedProcessState, ProcessOwner};
use crate::daemon::protocol::{DaemonRequest, DaemonResponse, ProtocolError};
use crate::daemon::provider::ProviderRegistry;
use crate::daemon::session::SessionLifecycle;
use crate::{PulseError, Result};
use dispatch::protocol_error_from_pulse;
use effects::{effect_has_committed_owner, external_effect_blocked};
use timeline::{append_event, ingest_provider_events, persist_provider_event_batch};
use turn::{provider_request_id, replace_provider_request_id, session_send_effect_id};

/// Transport-neutral daemon facade for host-local lifecycle operations.
///
/// The facade composes the single runtime store, process owner, and provider
/// registry while preserving the stable daemon application paths.
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
    /// Creates the daemon application, performs startup recovery, and starts
    /// provider-event ingestion.
    ///
    /// # Errors
    /// Returns an error if startup recovery cannot establish the durable
    /// runtime state needed before serving requests.
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
        application.start_provider_ingestion();
        Ok(application)
    }

    fn start_provider_ingestion(&self) {
        let store = self.store.clone();
        let owner = self.process_owner.clone();
        let shutdown = Arc::clone(&self.shutdown);
        thread::spawn(move || {
            while !shutdown.load(Ordering::SeqCst) {
                if let Err(error) = ingest_provider_events(&store, &owner) {
                    eprintln!("daemon provider event ingestion failed: {error}");
                }
                thread::sleep(Duration::from_millis(25));
            }
        });
    }

    /// Returns the shared shutdown flag used by daemon background work.
    pub fn shutdown_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown)
    }

    /// Returns the single daemon state store.
    pub fn store(&self) -> &StateStore {
        &self.store
    }

    /// Reports whether a daemon-owned process is currently alive.
    ///
    /// # Errors
    /// Returns an error if the process owner cannot determine liveness.
    pub fn managed_process_is_alive(&self, process_id: &str) -> Result<bool> {
        self.process_owner.is_alive(process_id)
    }

    /// Handles a request using the local CLI runtime principal.
    ///
    /// # Errors
    /// Returns a protocol error when authorization, request policy, routing,
    /// or the selected use case rejects the request.
    pub fn handle(
        &self,
        request: &DaemonRequest,
        idempotency_key: &str,
    ) -> std::result::Result<DaemonResponse, ProtocolError> {
        self.handle_as(&RuntimePrincipal::local_cli(), request, idempotency_key)
    }

    /// Handles a request for an explicitly authenticated runtime principal.
    ///
    /// # Errors
    /// Returns a protocol error when the principal lacks the coarse capability
    /// or request policy, routing, or the selected use case rejects the request.
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

    /// Request transport shutdown. Final process cleanup is owned by the serve
    /// loop after all accepted request workers have been joined.
    pub fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    /// Gracefully tear down managed processes and persist the observed result.
    /// This is called exactly once by the transport owner.
    ///
    /// # Errors
    /// Returns an error when shutdown state or managed-process termination
    /// cannot be durably recorded.
    pub fn shutdown_cleanup(&self) -> Result<()> {
        self.request_shutdown();
        let processes = self.store.with_state(false, |state| {
            Ok(state
                .processes
                .values()
                .filter(|process| process.state != ManagedProcessState::Exited)
                .cloned()
                .collect::<Vec<_>>())
        })?;
        let mut outcomes = std::collections::BTreeMap::new();
        for process in &processes {
            let outcome = match self.process_owner.terminate(&process.process_id) {
                Ok(()) => Ok(()),
                Err(error) if error.code() == "managed_process_not_owned" => {
                    self.process_owner.terminate_record(process)
                }
                Err(error) => Err(error),
            };
            outcomes.insert(
                process.process_id.clone(),
                outcome
                    .err()
                    .map(|error| format!("{}: {error}", error.code())),
            );
        }
        self.store.with_state(true, |state| {
            let now = chrono::Utc::now().to_rfc3339();
            for process in state
                .processes
                .values_mut()
                .filter(|process| outcomes.contains_key(&process.process_id))
            {
                let failure = outcomes.get(&process.process_id).cloned().flatten();
                process.state = if failure.is_none() {
                    ManagedProcessState::Exited
                } else {
                    ManagedProcessState::StaleNeedsOperator
                };
                process.updated_at = now.clone();
                if let Some(session) = state.sessions.get_mut(&process.owner_id) {
                    if session.lifecycle != SessionLifecycle::Closed {
                        session.lifecycle = SessionLifecycle::Error;
                    }
                    session.last_error = Some(match failure {
                        Some(failure) => format!(
                            "daemon shutdown could not prove managed process termination: {failure}"
                        ),
                        None => "daemon shut down; managed process was terminated".to_string(),
                    });
                    session.updated_at = now.clone();
                }
            }
            append_event(
                state,
                "daemon.shutdown",
                None,
                None,
                None,
                json!({
                    "pid": self.pid,
                    "terminated": outcomes.values().filter(|failure| failure.is_none()).count(),
                    "stale_needs_operator": outcomes.values().filter(|failure| failure.is_some()).count(),
                }),
            );
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

fn is_ambiguous_provider_outcome(error: &PulseError) -> bool {
    matches!(
        error.code(),
        "provider_response_timeout"
            | "provider_transport_closed"
            | "provider_transport_write_failed"
            | "provider_protocol_invalid_after_transport"
            | "io_error"
    )
}

fn provider_protocol_after_transport(error: PulseError) -> PulseError {
    if error.code() == "provider_protocol_invalid" {
        PulseError::validation(
            "provider_protocol_invalid_after_transport",
            error.to_string(),
        )
    } else {
        error
    }
}

impl Drop for DaemonApplication {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::assignment::{
        AssignmentSagaRecord, AssignmentSagaState, DeliveryRecord, DeliveryState,
    };
    use crate::daemon::persistence::StateStore;

    #[test]
    fn passed_verification_waits_for_core_close_gate() {
        assert_eq!(
            assignment::verification_saga_state(crate::execution::VerificationDisposition::Passed),
            AssignmentSagaState::Verifying
        );
        assert_ne!(
            assignment::verification_saga_state(crate::execution::VerificationDisposition::Passed),
            AssignmentSagaState::Done
        );
    }

    #[test]
    fn accepted_bootstrap_timeout_keeps_lease_and_delivery_pending() {
        let home = tempfile::tempdir().expect("daemon home");
        let app = DaemonApplication::new(StateStore::new(home.path()), "test").unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        app.store
            .with_state(true, |state| {
                state.assignment_sagas.insert(
                    "saga-timeout".to_string(),
                    AssignmentSagaRecord {
                        schema_version: 1,
                        saga_id: "saga-timeout".to_string(),
                        idempotency_key: "timeout".to_string(),
                        request_fingerprint: "fingerprint".to_string(),
                        project_id: "project".to_string(),
                        ticket_id: "ticket".to_string(),
                        actor: "actor".to_string(),
                        assignee: "assignee".to_string(),
                        ticket_revision: 1,
                        packet_fingerprint: "packet".to_string(),
                        lease_id: Some("lease-keep".to_string()),
                        workspace_id: Some("workspace".to_string()),
                        session_id: Some("session".to_string()),
                        delivery_id: Some("delivery-timeout".to_string()),
                        acknowledgement_id: None,
                        handoff_id: None,
                        verification_id: None,
                        qa_checkpoint_receipt_ids: Vec::new(),
                        state: AssignmentSagaState::DeliveryPending,
                        last_error: None,
                        created_at: now.clone(),
                        updated_at: now.clone(),
                    },
                );
                state.deliveries.insert(
                    "delivery-timeout".to_string(),
                    DeliveryRecord {
                        schema_version: 1,
                        delivery_id: "delivery-timeout".to_string(),
                        saga_id: "saga-timeout".to_string(),
                        session_id: "session".to_string(),
                        payload: "bootstrap".to_string(),
                        correlation_request_id: Some("request".to_string()),
                        correlation_turn_id: None,
                        state: DeliveryState::IntentRecorded,
                        created_at: now.clone(),
                        updated_at: now,
                    },
                );
                Ok(())
            })
            .unwrap();
        app.mark_bootstrap_delivery_uncertain(
            "saga-timeout",
            "delivery-timeout",
            &PulseError::validation("provider_response_timeout", "accepted then timed out"),
        )
        .unwrap();
        let state = app.store.load().unwrap();
        assert_eq!(
            state.assignment_sagas["saga-timeout"].lease_id.as_deref(),
            Some("lease-keep")
        );
        assert_eq!(
            state.deliveries["delivery-timeout"].state,
            DeliveryState::Uncertain
        );
        assert_eq!(
            state.assignment_sagas["saga-timeout"].state,
            AssignmentSagaState::DeliveryPending
        );
    }
}
