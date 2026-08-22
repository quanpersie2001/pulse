use super::*;

#[test]
fn handoff_and_verification_bind_to_session_and_reviewer_principal() {
    let (_home, _project_root, app) = application();
    let now = chrono::Utc::now().to_rfc3339();
    app.store()
        .with_state(true, |state| {
            state.assignment_sagas.insert(
                "saga-review-auth".to_string(),
                pulse::daemon::assignment::AssignmentSagaRecord {
                    schema_version: 1,
                    saga_id: "saga-review-auth".to_string(),
                    idempotency_key: "review-auth".to_string(),
                    request_fingerprint: String::new(),
                    project_id: "missing-project".to_string(),
                    ticket_id: "ticket".to_string(),
                    actor: "worker".to_string(),
                    assignee: "worker".to_string(),
                    ticket_revision: 1,
                    packet_fingerprint: "packet".to_string(),
                    lease_id: Some("lease".to_string()),
                    workspace_id: Some("workspace".to_string()),
                    session_id: Some("ses-worker".to_string()),
                    delivery_id: None,
                    acknowledgement_id: None,
                    handoff_id: Some("handoff".to_string()),
                    verification_id: None,
                    state: pulse::daemon::assignment::AssignmentSagaState::Verifying,
                    last_error: None,
                    created_at: now.clone(),
                    updated_at: now,
                },
            );
            Ok(())
        })
        .unwrap();

    let handoff = DaemonRequest::HandoffSubmit {
        saga_id: "saga-review-auth".to_string(),
        source_commit: "commit".to_string(),
        summary: "summary".to_string(),
        changed_paths: Vec::new(),
        evidence_receipt_ids: Vec::new(),
    };
    let other_session = RuntimePrincipal {
        principal_id: "other".to_string(),
        session_id: Some("ses-other".to_string()),
        capabilities: ["runtime.write".to_string()].into_iter().collect(),
    };
    assert_eq!(
        app.handle_as(&other_session, &handoff, "handoff-other")
            .unwrap_err()
            .code,
        "saga_session_identity_required"
    );

    let verification = DaemonRequest::VerificationComplete {
        saga_id: "saga-review-auth".to_string(),
        actor: "spoofed".to_string(),
        source_commit: "commit".to_string(),
        disposition: pulse::execution::VerificationDisposition::Passed,
        summary: "verified".to_string(),
        checks: Vec::new(),
        acceptance_proofs: Vec::new(),
    };
    let worker = RuntimePrincipal {
        principal_id: "worker".to_string(),
        session_id: Some("ses-worker".to_string()),
        capabilities: ["runtime.write".to_string()].into_iter().collect(),
    };
    assert_eq!(
        app.handle_as(&worker, &verification, "verify-spoof")
            .unwrap_err()
            .code,
        "verification_actor_mismatch"
    );
    let self_review = DaemonRequest::VerificationComplete {
        saga_id: "saga-review-auth".to_string(),
        actor: "worker".to_string(),
        source_commit: "commit".to_string(),
        disposition: pulse::execution::VerificationDisposition::Passed,
        summary: "verified".to_string(),
        checks: Vec::new(),
        acceptance_proofs: Vec::new(),
    };
    assert_eq!(
        app.handle_as(&worker, &self_review, "verify-self")
            .unwrap_err()
            .code,
        "verification_self_review_denied"
    );
    let reviewer = RuntimePrincipal {
        principal_id: "reviewer".to_string(),
        session_id: Some("ses-reviewer".to_string()),
        capabilities: ["runtime.write".to_string()].into_iter().collect(),
    };
    let valid_review = DaemonRequest::VerificationComplete {
        saga_id: "saga-review-auth".to_string(),
        actor: "reviewer".to_string(),
        source_commit: "commit".to_string(),
        disposition: pulse::execution::VerificationDisposition::Passed,
        summary: "verified".to_string(),
        checks: Vec::new(),
        acceptance_proofs: Vec::new(),
    };
    let error = app
        .handle_as(&reviewer, &valid_review, "verify-valid")
        .unwrap_err();
    assert_ne!(error.code, "verification_actor_mismatch");
    assert_ne!(error.code, "verification_self_review_denied");
}

#[test]
fn bound_assignment_ack_rejects_wrong_sender_and_exact_binding_mismatch_without_mutation() {
    let (_home, _project_root, app) = application();
    let now = chrono::Utc::now().to_rfc3339();
    app.store()
        .with_state(true, |state| {
            state.assignment_sagas.insert(
                "saga_bound".to_string(),
                pulse::daemon::assignment::AssignmentSagaRecord {
                    schema_version: 1,
                    saga_id: "saga_bound".to_string(),
                    idempotency_key: "bound-key".to_string(),
                    request_fingerprint: String::new(),
                    project_id: "project".to_string(),
                    ticket_id: "ticket".to_string(),
                    actor: "worker".to_string(),
                    assignee: "worker".to_string(),
                    ticket_revision: 1,
                    packet_fingerprint: "packet-good".to_string(),
                    lease_id: Some("lease-good".to_string()),
                    workspace_id: Some("workspace".to_string()),
                    session_id: Some("ses-worker".to_string()),
                    delivery_id: Some("delivery-good".to_string()),
                    acknowledgement_id: None,
                    handoff_id: None,
                    verification_id: None,
                    state: pulse::daemon::assignment::AssignmentSagaState::BootstrapDelivered,
                    last_error: None,
                    created_at: now.clone(),
                    updated_at: now.clone(),
                },
            );
            state.deliveries.insert(
                "delivery-good".to_string(),
                pulse::daemon::assignment::DeliveryRecord {
                    schema_version: 1,
                    delivery_id: "delivery-good".to_string(),
                    saga_id: "saga_bound".to_string(),
                    session_id: "ses-worker".to_string(),
                    payload: "packet".to_string(),
                    correlation_request_id: None,
                    correlation_turn_id: Some("turn".to_string()),
                    state: pulse::daemon::assignment::DeliveryState::Delivered,
                    created_at: now.clone(),
                    updated_at: now,
                },
            );
            Ok(())
        })
        .unwrap();
    let before = app.store().load().unwrap();
    let wrong_sender = RuntimePrincipal {
        principal_id: "worker:other".to_string(),
        session_id: Some("ses-other".to_string()),
        capabilities: ["runtime.write".to_string()].into_iter().collect(),
    };
    let request = DaemonRequest::AssignmentAcknowledgeBound {
        saga_id: "saga_bound".to_string(),
        acknowledgement_id: "ack".to_string(),
        lease_id: "lease-good".to_string(),
        session_id: "ses-worker".to_string(),
        packet_fingerprint: "packet-good".to_string(),
        delivery_id: "delivery-good".to_string(),
    };
    let error = app
        .handle_as(&wrong_sender, &request, "bound-wrong-sender")
        .unwrap_err();
    assert_eq!(error.code, "session_sender_identity_required");
    let worker = RuntimePrincipal {
        principal_id: "worker:session".to_string(),
        session_id: Some("ses-worker".to_string()),
        capabilities: ["runtime.write".to_string()].into_iter().collect(),
    };
    let mut mismatched = request;
    if let DaemonRequest::AssignmentAcknowledgeBound { delivery_id, .. } = &mut mismatched {
        *delivery_id = "delivery-wrong".to_string();
    }
    let error = app
        .handle_as(&worker, &mismatched, "bound-wrong-binding")
        .unwrap_err();
    assert_eq!(error.code, "assignment_acknowledgement_mismatch");
    let after = app.store().load().unwrap();
    assert_eq!(before.assignment_sagas, after.assignment_sagas);
    assert_eq!(before.deliveries, after.deliveries);
}

#[test]
fn acknowledgement_saga_serialization_preserves_identical_replay_and_rejects_conflict() {
    let (_home, _project_root, app) = application();
    let now = chrono::Utc::now().to_rfc3339();
    app.store()
        .with_state(true, |state| {
            state.assignment_sagas.insert(
                "saga_ack_lock".to_string(),
                pulse::daemon::assignment::AssignmentSagaRecord {
                    schema_version: 1,
                    saga_id: "saga_ack_lock".to_string(),
                    idempotency_key: "ack-lock-key".to_string(),
                    request_fingerprint: String::new(),
                    project_id: "project".to_string(),
                    ticket_id: "ticket".to_string(),
                    actor: "worker".to_string(),
                    assignee: "worker".to_string(),
                    ticket_revision: 1,
                    packet_fingerprint: "packet".to_string(),
                    lease_id: Some("lease".to_string()),
                    workspace_id: Some("workspace".to_string()),
                    session_id: Some("session".to_string()),
                    delivery_id: Some("delivery".to_string()),
                    acknowledgement_id: Some("ack-same".to_string()),
                    handoff_id: None,
                    verification_id: None,
                    state: pulse::daemon::assignment::AssignmentSagaState::Activated,
                    last_error: None,
                    created_at: now.clone(),
                    updated_at: now,
                },
            );
            Ok(())
        })
        .unwrap();
    let first_app = Arc::clone(&app);
    let first = std::thread::spawn(move || {
        first_app.handle(
            &DaemonRequest::AssignmentAcknowledge {
                saga_id: "saga_ack_lock".to_string(),
                acknowledgement_id: "ack-same".to_string(),
            },
            "ack-replay-one",
        )
    });
    let second_app = Arc::clone(&app);
    let second = std::thread::spawn(move || {
        second_app.handle(
            &DaemonRequest::AssignmentAcknowledge {
                saga_id: "saga_ack_lock".to_string(),
                acknowledgement_id: "ack-same".to_string(),
            },
            "ack-replay-two",
        )
    });
    assert!(first.join().unwrap().is_ok());
    assert!(second.join().unwrap().is_ok());
    let conflict = app
        .handle(
            &DaemonRequest::AssignmentAcknowledge {
                saga_id: "saga_ack_lock".to_string(),
                acknowledgement_id: "ack-other".to_string(),
            },
            "ack-conflict",
        )
        .unwrap_err();
    assert_eq!(conflict.code, "assignment_acknowledgement_conflict");
}

#[test]
fn assignment_retry_with_changed_inputs_is_rejected_before_core_mutation() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let key = "recoverable-input-conflict";
    let saga_id = format!(
        "saga_{}",
        pulse::canonical_json::hash_bytes(key.as_bytes())
            .trim_start_matches("sha256:")
            .chars()
            .take(26)
            .collect::<String>()
    );
    let now = chrono::Utc::now().to_rfc3339();
    app.store()
        .with_state(true, |state| {
            state.assignment_sagas.insert(
                saga_id.clone(),
                pulse::daemon::assignment::AssignmentSagaRecord {
                    schema_version: 1,
                    saga_id,
                    idempotency_key: key.to_string(),
                    request_fingerprint: String::new(),
                    project_id: project_id.clone(),
                    ticket_id: "ticket-original".to_string(),
                    actor: "agent:tester".to_string(),
                    assignee: "agent:codex-local".to_string(),
                    ticket_revision: 0,
                    packet_fingerprint: String::new(),
                    lease_id: None,
                    workspace_id: None,
                    session_id: None,
                    delivery_id: None,
                    acknowledgement_id: None,
                    handoff_id: None,
                    verification_id: None,
                    state: pulse::daemon::assignment::AssignmentSagaState::Recoverable,
                    last_error: None,
                    created_at: now.clone(),
                    updated_at: now.clone(),
                },
            );
            Ok(())
        })
        .unwrap();
    let error = app
        .handle(
            &DaemonRequest::AssignmentStart {
                project_id,
                ticket_id: "ticket-changed".to_string(),
                actor: "agent:tester".to_string(),
                assignee: "agent:codex-local".to_string(),
                capabilities: vec!["source.read".to_string()],
                isolation: IsolationMode::Local,
                provider_id: "codex".to_string(),
                provider_options: provider_options(),
                ttl_seconds: 1800,
            },
            key,
        )
        .unwrap_err();
    assert_eq!(error.code, "assignment_idempotency_conflict");
}

#[test]
fn legacy_recoverable_saga_pins_full_request_fingerprint_on_first_retry() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let key = "legacy-fingerprint-pin";
    let saga_id = format!(
        "saga_{}",
        pulse::canonical_json::hash_bytes(key.as_bytes())
            .trim_start_matches("sha256:")
            .chars()
            .take(26)
            .collect::<String>()
    );
    let now = chrono::Utc::now().to_rfc3339();
    app.store()
        .with_state(true, |state| {
            state.assignment_sagas.insert(
                saga_id.clone(),
                pulse::daemon::assignment::AssignmentSagaRecord {
                    schema_version: 1,
                    saga_id: saga_id.clone(),
                    idempotency_key: key.to_string(),
                    request_fingerprint: String::new(),
                    project_id: project_id.clone(),
                    ticket_id: "ticket-legacy".to_string(),
                    actor: "agent:tester".to_string(),
                    assignee: "agent:codex-local".to_string(),
                    ticket_revision: 0,
                    packet_fingerprint: String::new(),
                    lease_id: None,
                    workspace_id: None,
                    session_id: None,
                    delivery_id: None,
                    acknowledgement_id: None,
                    handoff_id: None,
                    verification_id: None,
                    state: pulse::daemon::assignment::AssignmentSagaState::Recoverable,
                    last_error: None,
                    created_at: now.clone(),
                    updated_at: now.clone(),
                },
            );
            Ok(())
        })
        .unwrap();
    let request = DaemonRequest::AssignmentStart {
        project_id,
        ticket_id: "ticket-legacy".to_string(),
        actor: "agent:tester".to_string(),
        assignee: "agent:codex-local".to_string(),
        capabilities: vec!["source.read".to_string()],
        isolation: IsolationMode::Local,
        provider_id: "codex".to_string(),
        provider_options: provider_options(),
        ttl_seconds: 1800,
    };
    assert!(app.handle(&request, key).is_err());
    let pinned = app.store().load().unwrap().assignment_sagas[&saga_id]
        .request_fingerprint
        .clone();
    assert!(!pinned.is_empty());

    let mut changed = request;
    if let DaemonRequest::AssignmentStart { capabilities, .. } = &mut changed {
        capabilities.push("test.run".to_string());
    }
    let error = app.handle(&changed, key).unwrap_err();
    assert_eq!(error.code, "assignment_idempotency_conflict");
}

#[test]
fn assign_idempotency_key_contract_allows_retry_after_recoverable_failure() {
    // Verify that calling assignment_start with the same idempotency key
    // after a Recoverable error properly re-provisions.
    //
    // This test exercises the retry path without a full daemon restart:
    // we pre-create a saga with Recoverable state, then call
    // assignment_start with the original key to prove the retry flow.
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let original_key = "recoverable-retry-contract";
    let saga_id = format!(
        "saga_{}",
        pulse::canonical_json::hash_bytes(original_key.as_bytes())
            .trim_start_matches("sha256:")
            .chars()
            .take(26)
            .collect::<String>()
    );

    // Manually insert a Recoverable saga (simulating a crashed daemon).
    let now = chrono::Utc::now().to_rfc3339();
    let saga = pulse::daemon::assignment::AssignmentSagaRecord {
        schema_version: 1,
        saga_id: saga_id.clone(),
        idempotency_key: original_key.to_string(),
        request_fingerprint: String::new(),
        project_id: project_id.clone(),
        ticket_id: "ticket_fake".to_string(),
        actor: "agent:tester".to_string(),
        assignee: "agent:codex-local".to_string(),
        ticket_revision: 0,
        packet_fingerprint: String::new(),
        lease_id: None,
        workspace_id: None,
        session_id: None,
        delivery_id: None,
        acknowledgement_id: None,
        handoff_id: None,
        verification_id: None,
        state: pulse::daemon::assignment::AssignmentSagaState::Recoverable,
        last_error: Some("simulated daemon crash".to_string()),
        created_at: now.clone(),
        updated_at: now,
    };
    app.store()
        .with_state(true, |state| {
            state.assignment_sagas.insert(saga_id.clone(), saga);
            Ok(())
        })
        .unwrap();

    // Now retry assignment_start — it should fail on actual core reservation
    // since the ticket doesn't exist in a real enrolled repo, but the important
    // contract below is that it does *not* return a duplicate-active-ownership
    // error or a stale state — it falls through to real provisioning.
    let result = app.handle(
        &DaemonRequest::AssignmentStart {
            project_id: project_id.clone(),
            ticket_id: "ticket_fake".to_string(),
            actor: "agent:tester".to_string(),
            assignee: "agent:codex-local".to_string(),
            capabilities: vec!["source.read".to_string()],
            isolation: IsolationMode::Local,
            provider_id: "codex".to_string(),
            provider_options: provider_options(),
            ttl_seconds: 1800,
        },
        original_key,
    );
    // Expected: either a provisioning error (no enrolled repo) or a graceful
    // hand-off. The key contract: no "assignment_live_lease_exists" or
    // "reservation_idempotency_conflict".
    if let Err(error) = &result {
        assert_ne!(
            error.code, "assignment_live_lease_exists",
            "retry should not reject with live-lease conflict"
        );
        assert_ne!(
            error.code, "reservation_idempotency_conflict",
            "retry should not reject with idempotency conflict"
        );
        assert_ne!(
            error.code, "reservation_not_activatable",
            "retry should not reject with stale reservation"
        );
    }
}

#[cfg(unix)]
#[test]
fn assignment_malformed_thread_start_retains_release_authorized_lease() {
    let (_repo, store, _home, app, project_id, ticket_id) = assignment_application();
    let script = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        script.path(),
        r#"import readline from "node:readline";
const lines = readline.createInterface({ input: process.stdin });
for await (const raw of lines) {
  const request = JSON.parse(raw);
  if (request.method === "initialize") {
    process.stdout.write(JSON.stringify({ id: request.id, result: {} }) + "\n");
  } else if (request.method === "thread/start") {
    process.stdout.write(JSON.stringify({ id: request.id, result: { thread: {} } }) + "\n");
  }
}
"#,
    )
    .unwrap();
    let request = assignment_start_request(
        &project_id,
        &ticket_id,
        json!({
            "executable": std::env::var("NODE").unwrap_or_else(|_| "node".to_string()),
            "args": [script.path()]
        }),
    );
    let error = app
        .handle(&request, "assignment-malformed-thread")
        .unwrap_err();
    assert_eq!(error.code, "provider_protocol_invalid_after_transport");
    let state = app.store().load().unwrap();
    let saga = state
        .assignment_sagas
        .values()
        .find(|saga| saga.idempotency_key == "assignment-malformed-thread")
        .unwrap();
    assert_eq!(
        saga.state,
        pulse::daemon::assignment::AssignmentSagaState::Recoverable
    );
    assert!(saga
        .last_error
        .as_deref()
        .unwrap()
        .contains("lease retained"));
    let lease_id = saga.lease_id.as_deref().unwrap();
    let reservation = pulse::kernel::reservation::list_reservations(store.repo_root())
        .unwrap()
        .into_iter()
        .find(|reservation| reservation.lease_id == lease_id)
        .unwrap();
    assert_eq!(
        reservation.state,
        pulse::reservation::ReservationState::Reserved
    );
    let retry = app
        .handle(&request, "assignment-malformed-thread")
        .unwrap_err();
    assert_eq!(retry.code, "external_effect_reconciliation_required");
}

#[cfg(unix)]
#[test]
fn assignment_provisioning_attempting_retains_release_authorized_lease() {
    let (_repo, store, _home, app, project_id, ticket_id) = assignment_application();
    app.store()
        .arm_failpoint(
            "after_provider_process_success_before_ack",
            FailpointMode::Error,
        )
        .unwrap();
    let request = assignment_start_request(
        &project_id,
        &ticket_id,
        json!({
            "executable": "/bin/sleep",
            "args": ["1"],
            "protocol_mode": "opaque_test"
        }),
    );
    let error = app
        .handle(&request, "assignment-process-attempting")
        .unwrap_err();
    assert_eq!(error.code, "injected_failpoint");
    let state = app.store().load().unwrap();
    let saga = state
        .assignment_sagas
        .values()
        .find(|saga| saga.idempotency_key == "assignment-process-attempting")
        .unwrap();
    assert_eq!(
        saga.state,
        pulse::daemon::assignment::AssignmentSagaState::Recoverable
    );
    let lease_id = saga.lease_id.as_deref().unwrap();
    let reservation = pulse::kernel::reservation::list_reservations(store.repo_root())
        .unwrap()
        .into_iter()
        .find(|reservation| reservation.lease_id == lease_id)
        .unwrap();
    assert_eq!(
        reservation.state,
        pulse::reservation::ReservationState::Reserved
    );
    assert!(state.external_effects.values().any(|effect| {
        effect.kind == pulse::daemon::persistence::ExternalEffectKind::ProviderProcessCreate
            && effect.state == pulse::daemon::persistence::ExternalEffectState::Attempting
    }));
    app.store()
        .disarm_failpoint(
            "after_provider_process_success_before_ack",
            FailpointMode::Error,
        )
        .unwrap();
    let retry = app
        .handle(&request, "assignment-process-attempting")
        .unwrap_err();
    assert_eq!(retry.code, "external_effect_reconciliation_required");
}

#[cfg(unix)]
#[test]
fn assignment_bootstrap_send_attempting_retains_lease_and_blocks_retry() {
    let (_repo, store, _home, app, project_id, ticket_id) = assignment_application();
    app.store()
        .arm_failpoint(
            "after_session_send_success_before_ack",
            FailpointMode::Error,
        )
        .unwrap();
    let request = assignment_start_request(&project_id, &ticket_id, high_volume_provider_options());
    let error = app
        .handle(&request, "assignment-bootstrap-send-attempting")
        .unwrap_err();
    assert_eq!(error.code, "injected_failpoint");
    let state = app.store().load().unwrap();
    let saga = state
        .assignment_sagas
        .values()
        .find(|saga| saga.idempotency_key == "assignment-bootstrap-send-attempting")
        .unwrap();
    assert_ne!(
        saga.state,
        pulse::daemon::assignment::AssignmentSagaState::Released
    );
    assert!(saga
        .last_error
        .as_deref()
        .is_some_and(|error| error.contains("not released")));
    let lease_id = saga.lease_id.as_deref().unwrap();
    let reservation = pulse::kernel::reservation::list_reservations(store.repo_root())
        .unwrap()
        .into_iter()
        .find(|reservation| reservation.lease_id == lease_id)
        .unwrap();
    assert_eq!(
        reservation.state,
        pulse::reservation::ReservationState::Reserved
    );
    assert!(state.external_effects.values().any(|effect| {
        effect.kind == pulse::daemon::persistence::ExternalEffectKind::SessionSend
            && effect.state == pulse::daemon::persistence::ExternalEffectState::Attempting
    }));
    app.store()
        .disarm_failpoint(
            "after_session_send_success_before_ack",
            FailpointMode::Error,
        )
        .unwrap();
    let retry = app
        .handle(&request, "assignment-bootstrap-send-attempting")
        .unwrap_err();
    assert_eq!(retry.code, "external_effect_reconciliation_required");
}
