use super::*;

#[cfg(unix)]
#[test]
fn high_volume_turn_preserves_completion_and_returns_idle_with_loss_marker() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let workspace_id = create_workspace(&app, &project_id);
    let session = match handle(
        &app,
        DaemonRequest::SessionCreate {
            workspace_id,
            provider_id: "codex".to_string(),
            parent_session_id: None,
            provider_options: high_volume_provider_options(),
        },
        "high-volume-create",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    let sent = match handle(
        &app,
        DaemonRequest::SessionSend {
            session_id: session.session_id.clone(),
            input: "high volume".to_string(),
        },
        "high-volume-send",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(sent.lifecycle, SessionLifecycle::Idle);
    assert!(sent.active_turn_id.is_none());
    std::thread::sleep(Duration::from_millis(100));
    let timeline = app.store().load().unwrap().timeline;
    assert!(timeline.iter().any(|event| {
        event.event_type == "provider.notification"
            && event.payload.get("method").and_then(|value| value.as_str())
                == Some("turn/completed")
    }));
    assert!(timeline.iter().any(|event| {
        event.event_type == "provider.notification"
            && event.payload.get("method").and_then(|value| value.as_str())
                == Some("pulse/notification_loss")
    }));
}

#[test]
fn timeline_cursor_pages_without_duplicates() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    create_workspace(&app, &project_id);
    let first = match handle(
        &app,
        DaemonRequest::TimelineList {
            cursor: None,
            limit: 2,
            session_id: None,
        },
        "",
    ) {
        DaemonResponse::Timeline { page } => page,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(first.events.len(), 2);
    assert!(first.has_newer);
    let second = match handle(
        &app,
        DaemonRequest::TimelineList {
            cursor: Some(first.next_cursor),
            limit: 100,
            session_id: None,
        },
        "",
    ) {
        DaemonResponse::Timeline { page } => page,
        other => panic!("unexpected response: {other:?}"),
    };
    let first_ids = first
        .events
        .iter()
        .map(|event| &event.event_id)
        .collect::<std::collections::BTreeSet<_>>();
    assert!(second
        .events
        .iter()
        .all(|event| !first_ids.contains(&event.event_id)));
    assert!(!second.has_newer);
}

#[test]
fn timeline_subscription_catches_up_after_a_live_event() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let snapshot = match handle(
        &app,
        DaemonRequest::TimelineList {
            cursor: None,
            limit: 100,
            session_id: None,
        },
        "",
    ) {
        DaemonResponse::Timeline { page } => page,
        other => panic!("unexpected response: {other:?}"),
    };
    let subscriber = Arc::clone(&app);
    let cursor = snapshot.next_cursor;
    let waiting = std::thread::spawn(move || {
        handle(
            &subscriber,
            DaemonRequest::TimelineSubscribe {
                cursor,
                limit: 100,
                session_id: None,
                wait_ms: 2_000,
            },
            "",
        )
    });
    std::thread::sleep(std::time::Duration::from_millis(50));
    create_workspace(&app, &project_id);
    let page = match waiting.join().unwrap() {
        DaemonResponse::Timeline { page } => page,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].event_type, "workspace.created");
}

#[cfg(unix)]
#[test]
fn identical_messages_with_distinct_request_ids_create_distinct_effects() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let workspace_id = create_workspace(&app, &project_id);
    let session = match handle(
        &app,
        DaemonRequest::SessionCreate {
            workspace_id,
            provider_id: "codex".to_string(),
            parent_session_id: None,
            provider_options: high_volume_provider_options(),
        },
        "identical-message-session",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    handle(
        &app,
        DaemonRequest::SessionSend {
            session_id: session.session_id.clone(),
            input: "same input".to_string(),
        },
        "identical-message-one",
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    handle(
        &app,
        DaemonRequest::SessionSend {
            session_id: session.session_id,
            input: "same input".to_string(),
        },
        "identical-message-two",
    );
    let effects = app.store().load().unwrap();
    assert_eq!(
        effects
            .external_effects
            .values()
            .filter(|effect| {
                effect.kind == pulse::daemon::persistence::ExternalEffectKind::SessionSend
            })
            .count(),
        2
    );
}

#[cfg(unix)]
#[test]
fn acknowledged_turn_commit_failure_is_unknown_and_retry_is_blocked() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let workspace_id = create_workspace(&app, &project_id);
    let session = match handle(
        &app,
        DaemonRequest::SessionCreate {
            workspace_id,
            provider_id: "codex".to_string(),
            parent_session_id: None,
            provider_options: high_volume_provider_options(),
        },
        "turn-commit-failure-session",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    app.store()
        .arm_failpoint("before_session_turn_commit", FailpointMode::Error)
        .unwrap();
    let request = DaemonRequest::SessionSend {
        session_id: session.session_id.clone(),
        input: "commit failure".to_string(),
    };
    let first = app.handle(&request, "turn-commit-failure").unwrap_err();
    assert_eq!(first.code, "injected_failpoint");
    let state = app.store().load().unwrap();
    let effect = state
        .external_effects
        .values()
        .find(|effect| effect.kind == pulse::daemon::persistence::ExternalEffectKind::SessionSend)
        .expect("session send effect");
    assert_eq!(
        effect.state,
        pulse::daemon::persistence::ExternalEffectState::OutcomeUnknown
    );
    app.store()
        .disarm_failpoint("before_session_turn_commit", FailpointMode::Error)
        .unwrap();
    let retry = app.handle(&request, "turn-commit-failure").unwrap_err();
    assert_eq!(retry.code, "external_effect_reconciliation_required");
}

#[cfg(unix)]
#[test]
fn provider_send_success_before_ack_is_attempting_and_blocks_retry() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let workspace_id = create_workspace(&app, &project_id);
    let session = match handle(
        &app,
        DaemonRequest::SessionCreate {
            workspace_id,
            provider_id: "codex".to_string(),
            parent_session_id: None,
            provider_options: high_volume_provider_options(),
        },
        "send-attempting-session",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    app.store()
        .arm_failpoint(
            "after_session_send_success_before_ack",
            FailpointMode::Error,
        )
        .unwrap();
    let request = DaemonRequest::SessionSend {
        session_id: session.session_id.clone(),
        input: "attempting send".to_string(),
    };
    let error = app.handle(&request, "send-attempting").unwrap_err();
    assert_eq!(error.code, "injected_failpoint");
    let state = app.store().load().unwrap();
    let effect = state
        .external_effects
        .values()
        .find(|effect| effect.kind == pulse::daemon::persistence::ExternalEffectKind::SessionSend)
        .expect("session send effect");
    assert_eq!(
        effect.state,
        pulse::daemon::persistence::ExternalEffectState::Attempting
    );
    assert!(effect.resource_id.is_some());
    app.store()
        .disarm_failpoint(
            "after_session_send_success_before_ack",
            FailpointMode::Error,
        )
        .unwrap();
    let retry = app.handle(&request, "send-attempting").unwrap_err();
    assert_eq!(retry.code, "external_effect_reconciliation_required");
    let _ = app.handle(
        &DaemonRequest::SessionClose {
            session_id: session.session_id,
        },
        "send-attempting-close",
    );
}

#[cfg(unix)]
#[test]
fn malformed_accepted_turn_response_is_unknown_and_not_resendable() {
    let (_home, project_root, app) = application();
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
    process.stdout.write(JSON.stringify({ id: request.id, result: { thread: { id: "malformed-thread" } } }) + "\n");
  } else if (request.method === "turn/start") {
    process.stdout.write(JSON.stringify({ id: request.id, result: { turn: {} } }) + "\n");
  }
}
"#,
    )
    .unwrap();
    let project_id = open_project(&app, project_root.path());
    let workspace_id = create_workspace(&app, &project_id);
    let session = match handle(
        &app,
        DaemonRequest::SessionCreate {
            workspace_id,
            provider_id: "codex".to_string(),
            parent_session_id: None,
            provider_options: json!({
                "executable": std::env::var("NODE").unwrap_or_else(|_| "node".to_string()),
                "args": [script.path()]
            }),
        },
        "malformed-turn-session",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    let request = DaemonRequest::SessionSend {
        session_id: session.session_id,
        input: "malformed turn".to_string(),
    };
    let error = app.handle(&request, "malformed-turn-send").unwrap_err();
    assert_eq!(error.code, "provider_protocol_invalid_after_transport");
    let state = app.store().load().unwrap();
    let effect = state
        .external_effects
        .values()
        .find(|effect| effect.kind == pulse::daemon::persistence::ExternalEffectKind::SessionSend)
        .expect("session send effect");
    assert_eq!(
        effect.state,
        pulse::daemon::persistence::ExternalEffectState::OutcomeUnknown
    );
    let retry = app.handle(&request, "malformed-turn-send").unwrap_err();
    assert_eq!(retry.code, "external_effect_reconciliation_required");
}

#[cfg(unix)]
#[test]
fn delayed_provider_event_is_durable_without_session_or_timeline_read() {
    let (_home, project_root, app) = application();
    let provider_script = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        provider_script.path(),
        r#"import readline from "node:readline";
const lines = readline.createInterface({ input: process.stdin });
for await (const raw of lines) {
  const request = JSON.parse(raw);
  if (request.method === "initialize") {
    process.stdout.write(JSON.stringify({ id: request.id, result: {} }) + "\n");
  } else if (request.method === "thread/start") {
    process.stdout.write(JSON.stringify({ id: request.id, result: { thread: { id: "delayed-thread" } } }) + "\n");
  } else if (request.method === "turn/start") {
    process.stdout.write(JSON.stringify({ id: request.id, result: { turn: { id: "delayed-turn" } } }) + "\n");
    setTimeout(() => process.stdout.write(JSON.stringify({ method: "delayed/event", params: { durable: true } }) + "\n"), 80);
  }
}
"#,
    )
    .unwrap();
    let project_id = open_project(&app, project_root.path());
    let workspace_id = create_workspace(&app, &project_id);
    let session = match handle(
        &app,
        DaemonRequest::SessionCreate {
            workspace_id,
            provider_id: "codex".to_string(),
            parent_session_id: None,
            provider_options: json!({
                "executable": std::env::var("NODE").unwrap_or_else(|_| "node".to_string()),
                "args": [provider_script.path()]
            }),
        },
        "delayed-event-session",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    handle(
        &app,
        DaemonRequest::SessionSend {
            session_id: session.session_id,
            input: "delayed".to_string(),
        },
        "delayed-event-send",
    );
    app.store()
        .arm_failpoint("before_provider_event_commit", FailpointMode::Error)
        .unwrap();
    std::thread::sleep(Duration::from_millis(120));
    app.store()
        .disarm_failpoint("before_provider_event_commit", FailpointMode::Error)
        .unwrap();
    std::thread::sleep(Duration::from_millis(180));
    let state = app.store().load().unwrap();
    assert_eq!(
        state
            .timeline
            .iter()
            .filter(|event| {
                event.event_type == "provider.notification"
                    && event.payload.get("method").and_then(|value| value.as_str())
                        == Some("delayed/event")
            })
            .count(),
        1
    );
}
