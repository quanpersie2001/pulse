use super::*;

#[cfg(unix)]
#[test]
fn daemon_restart_with_live_process_fails_closed_and_terminates_matching_identity() {
    let (home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let workspace_id = create_workspace(&app, &project_id);
    let session = match handle(
        &app,
        DaemonRequest::SessionCreate {
            workspace_id,
            provider_id: "codex".to_string(),
            parent_session_id: None,
            provider_options: provider_options(),
        },
        "restart-live-process",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    let process_id = session.managed_process_id.clone().unwrap();
    assert!(app.managed_process_is_alive(&process_id).unwrap());
    drop(app);

    let restarted = DaemonApplication::new(StateStore::new(home.path()), "test-restarted").unwrap();
    let state = restarted.store().load().unwrap();
    assert_eq!(
        state.processes[&process_id].state,
        pulse::daemon::process::ManagedProcessState::Exited
    );
    let recovered_session = &state.sessions[&session.session_id];
    assert_eq!(recovered_session.lifecycle, SessionLifecycle::Error);
    assert!(recovered_session
        .last_error
        .as_deref()
        .is_some_and(|error| error.contains("daemon restarted")));
}

#[cfg(unix)]
#[test]
fn pid_reuse_identity_mismatch_refuses_cancellation() {
    let home = tempfile::tempdir().unwrap();
    let owner = ProcessOwner::default();
    let args = vec!["30".to_string()];
    let record = owner
        .spawn(SpawnRequest {
            owner_kind: "test",
            owner_id: "pid-reuse",
            provider_id: "fixture",
            executable: std::path::Path::new("/bin/sleep"),
            args: &args,
            cwd: home.path(),
            log_root: home.path(),
            max_log_bytes: 1024,
        })
        .unwrap();
    let mut mismatched = record.clone();
    mismatched.platform_start_marker = "sha256:reused-pid-marker".to_string();
    let error = owner.terminate_record(&mismatched).unwrap_err();
    assert_eq!(error.code(), "managed_process_identity_mismatch");
    assert_eq!(
        owner.classify_recovery(&record).unwrap(),
        pulse::daemon::process::ManagedProcessState::StaleNeedsOperator
    );
    owner.terminate(&record.process_id).unwrap();
}

#[cfg(unix)]
#[test]
fn assignment_after_delivery_intent_restarts_and_resumes_not_sent_bootstrap() {
    let (_repo, store, home, app, project_id, ticket_id) = assignment_application();
    let provider_script = tempfile::NamedTempFile::new().unwrap();
    let provider_log = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        provider_script.path(),
        r#"import fs from "node:fs";
import readline from "node:readline";
const log = process.argv[2];
const lines = readline.createInterface({ input: process.stdin });
for await (const raw of lines) {
  const request = JSON.parse(raw);
  if (request.method === "initialize") {
    process.stdout.write(JSON.stringify({ id: request.id, result: {} }) + "\n");
  } else if (request.method === "thread/start" || request.method === "thread/resume") {
    process.stdout.write(JSON.stringify({ id: request.id, result: { thread: { id: "restart-thread" } } }) + "\n");
  } else if (request.method === "turn/start") {
    fs.appendFileSync(log, "turn/start\n");
    process.stdout.write(JSON.stringify({ id: request.id, result: { turn: { id: "restart-turn" } } }) + "\n");
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
            "args": [provider_script.path(), provider_log.path()]
        }),
    );
    app.store()
        .arm_failpoint("after_delivery_intent", FailpointMode::Panic)
        .unwrap();
    let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        app.handle(&request, "assignment-restart-not-sent")
    }));
    assert!(
        crashed.is_err(),
        "delivery intent failpoint must crash the request"
    );
    app.store()
        .disarm_failpoint("after_delivery_intent", FailpointMode::Panic)
        .unwrap();
    let before = app.store().load().unwrap();
    let saga_before = before
        .assignment_sagas
        .values()
        .find(|saga| saga.idempotency_key == "assignment-restart-not-sent")
        .unwrap();
    let lease_before = saga_before.lease_id.clone().unwrap();
    let effect_id = format!(
        "effect-send-{}-{}",
        saga_before.session_id.as_deref().unwrap(),
        pulse::canonical_json::hash_bytes(saga_before.idempotency_key.as_bytes())
            .trim_start_matches("sha256:")
            .chars()
            .take(20)
            .collect::<String>()
    );
    assert_eq!(
        before.external_effects[&effect_id].state,
        pulse::daemon::persistence::ExternalEffectState::NotSent
    );
    let delivery_id_before = saga_before.delivery_id.clone().unwrap();
    let request_id_before = before.deliveries[&delivery_id_before]
        .correlation_request_id
        .clone()
        .unwrap();
    let request_message_before = before.external_effects[&effect_id]
        .request_message
        .clone()
        .expect("bootstrap request body must be durable before provider I/O");
    drop(app);

    let restarted = DaemonApplication::new(StateStore::new(home.path()), "test-restarted").unwrap();
    let delivered = match restarted
        .handle(&request, "assignment-restart-not-sent")
        .unwrap()
    {
        DaemonResponse::Assignment { saga } => saga,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(
        delivered.state,
        pulse::daemon::assignment::AssignmentSagaState::BootstrapDelivered
    );
    assert_eq!(delivered.lease_id.as_deref(), Some(lease_before.as_str()));
    let after = restarted.store().load().unwrap();
    assert_eq!(
        after.external_effects[&effect_id].state,
        pulse::daemon::persistence::ExternalEffectState::Acknowledged
    );
    assert_eq!(
        after.deliveries[&delivery_id_before]
            .correlation_request_id
            .as_deref(),
        Some(request_id_before.as_str())
    );
    assert_eq!(
        after.external_effects[&effect_id]
            .request_message
            .as_deref(),
        Some(request_message_before.as_str())
    );
    assert_eq!(
        after.deliveries[delivered.delivery_id.as_ref().unwrap()].state,
        pulse::daemon::assignment::DeliveryState::Delivered
    );
    assert_eq!(
        std::fs::read_to_string(provider_log.path())
            .unwrap()
            .lines()
            .filter(|line| *line == "turn/start")
            .count(),
        1
    );
    let reservation = pulse::kernel::reservation::list_reservations(store.repo_root())
        .unwrap()
        .into_iter()
        .find(|reservation| reservation.lease_id == lease_before)
        .unwrap();
    assert_eq!(
        reservation.state,
        pulse::reservation::ReservationState::Reserved
    );
}

#[cfg(unix)]
#[test]
fn not_sent_bootstrap_recovery_rejects_each_broken_identity_link_without_resend() {
    for mismatch in [
        "saga",
        "delivery_session",
        "process_provider",
        "payload",
        "correlation",
        "effect_fingerprint",
        "effect_detail",
        "method",
        "provider_thread",
        "input",
    ] {
        let (_repo, _store, home, app, project_id, ticket_id) = assignment_application();
        let provider_script = tempfile::NamedTempFile::new().unwrap();
        let provider_log = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            provider_script.path(),
            r#"import fs from "node:fs";
import readline from "node:readline";
const log = process.argv[2];
const lines = readline.createInterface({ input: process.stdin });
for await (const raw of lines) {
  const request = JSON.parse(raw);
  fs.appendFileSync(log, request.method + "\n");
  if (request.method === "initialize") {
    process.stdout.write(JSON.stringify({ id: request.id, result: {} }) + "\n");
  } else if (request.method === "thread/start" || request.method === "thread/resume") {
    process.stdout.write(JSON.stringify({ id: request.id, result: { thread: { id: "link-thread" } } }) + "\n");
  } else if (request.method === "turn/start") {
    fs.appendFileSync(log, "turn/start\n");
    process.stdout.write(JSON.stringify({ id: request.id, result: { turn: { id: "link-turn" } } }) + "\n");
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
                "args": [provider_script.path(), provider_log.path()]
            }),
        );
        app.store()
            .arm_failpoint("after_delivery_intent", FailpointMode::Panic)
            .unwrap();
        let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            app.handle(&request, &format!("identity-link-{mismatch}"))
        }));
        assert!(crashed.is_err());
        app.store()
            .disarm_failpoint("after_delivery_intent", FailpointMode::Panic)
            .unwrap();
        let before = app.store().load().unwrap();
        let provider_writes_before = std::fs::read_to_string(provider_log.path()).unwrap();
        let saga = before
            .assignment_sagas
            .values()
            .find(|saga| saga.idempotency_key == format!("identity-link-{mismatch}"))
            .unwrap();
        let saga_id = saga.saga_id.clone();
        let session_id = saga.session_id.clone().unwrap();
        let delivery_id = saga.delivery_id.clone().unwrap();
        let effect_id = format!(
            "effect-send-{}-{}",
            session_id,
            pulse::canonical_json::hash_bytes(saga.idempotency_key.as_bytes())
                .trim_start_matches("sha256:")
                .chars()
                .take(20)
                .collect::<String>()
        );
        app.store()
            .with_state(true, |state| {
                match mismatch {
                    "saga" => {
                        state.deliveries.get_mut(&delivery_id).unwrap().saga_id =
                            "wrong-saga".to_string()
                    }
                    "delivery_session" => {
                        state.deliveries.get_mut(&delivery_id).unwrap().session_id =
                            "wrong-session".to_string()
                    }
                    "process_provider" => {
                        let process_id = state.sessions[&session_id]
                            .managed_process_id
                            .clone()
                            .unwrap();
                        state.processes.get_mut(&process_id).unwrap().provider_id =
                            "wrong-provider".to_string();
                    }
                    "payload" => {
                        state.deliveries.get_mut(&delivery_id).unwrap().payload =
                            "wrong-payload".to_string();
                    }
                    "correlation" => {
                        state
                            .deliveries
                            .get_mut(&delivery_id)
                            .unwrap()
                            .correlation_request_id = Some("wrong-request-id".to_string());
                    }
                    "effect_fingerprint" => {
                        state
                            .external_effects
                            .get_mut(&effect_id)
                            .unwrap()
                            .request_fingerprint = "wrong-fingerprint".to_string();
                    }
                    "effect_detail" => {
                        state.external_effects.get_mut(&effect_id).unwrap().detail =
                            "wrong-detail".to_string();
                    }
                    "method" => {
                        let effect = state.external_effects.get_mut(&effect_id).unwrap();
                        let mut message: serde_json::Value =
                            serde_json::from_str(effect.request_message.as_deref().unwrap())
                                .unwrap();
                        message["method"] = serde_json::Value::String("thread/start".to_string());
                        effect.request_message = Some(serde_json::to_string(&message).unwrap());
                    }
                    "provider_thread" => {
                        let effect = state.external_effects.get_mut(&effect_id).unwrap();
                        let mut message: serde_json::Value =
                            serde_json::from_str(effect.request_message.as_deref().unwrap())
                                .unwrap();
                        message["params"]["threadId"] =
                            serde_json::Value::String("wrong-thread".to_string());
                        effect.request_message = Some(serde_json::to_string(&message).unwrap());
                    }
                    "input" => {
                        let effect = state.external_effects.get_mut(&effect_id).unwrap();
                        let mut message: serde_json::Value =
                            serde_json::from_str(effect.request_message.as_deref().unwrap())
                                .unwrap();
                        message["params"]["input"][0]["text"] =
                            serde_json::Value::String("wrong-payload".to_string());
                        effect.request_message = Some(serde_json::to_string(&message).unwrap());
                    }
                    _ => unreachable!(),
                }
                Ok(())
            })
            .unwrap();
        drop(app);

        let restarted =
            DaemonApplication::new(StateStore::new(home.path()), "test-restarted").unwrap();
        let response = restarted
            .handle(&request, &format!("identity-link-{mismatch}"))
            .unwrap();
        let returned_saga = match response {
            DaemonResponse::Assignment { saga } => saga,
            other => panic!("unexpected response: {other:?}"),
        };
        assert_ne!(
            returned_saga.state,
            pulse::daemon::assignment::AssignmentSagaState::BootstrapDelivered,
            "mismatch {mismatch} must not replay bootstrap"
        );
        assert_eq!(
            std::fs::read_to_string(provider_log.path()).unwrap(),
            provider_writes_before,
            "mismatch {mismatch} must perform no provider writes"
        );
        assert_eq!(
            restarted.store().load().unwrap().external_effects[&effect_id].state,
            pulse::daemon::persistence::ExternalEffectState::NotSent,
            "mismatch {mismatch} must not advance the effect"
        );
        assert_eq!(saga_id, returned_saga.saga_id);
    }
}
