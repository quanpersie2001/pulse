use super::*;

#[cfg(unix)]
#[test]
fn session_attach_reuses_live_process_and_rejects_conflicting_sender() {
    let (_home, project_root, app) = application();
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
        "attach-create",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    let process_id = session.managed_process_id.clone().unwrap();
    let before = app.store().load().unwrap();
    let attached = handle(
        &app,
        DaemonRequest::SessionAttach {
            session_id: session.session_id.clone(),
        },
        "attach-live",
    );
    let attached = match attached {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(attached.session_id, session.session_id);
    assert_eq!(attached.provider_handle, session.provider_handle);
    assert_eq!(
        attached.managed_process_id.as_deref(),
        Some(process_id.as_str())
    );
    assert_eq!(
        app.store().load().unwrap().timeline.len(),
        before.timeline.len()
    );

    let conflicting = RuntimePrincipal {
        principal_id: "worker:other".to_string(),
        session_id: Some("ses_other".to_string()),
        capabilities: ["runtime.write".to_string()].into_iter().collect(),
    };
    let error = app
        .handle_as(
            &conflicting,
            &DaemonRequest::SessionAttach {
                session_id: session.session_id,
            },
            "attach-conflict",
        )
        .unwrap_err();
    assert_eq!(error.code, "session_access_denied");
    assert!(app.managed_process_is_alive(&process_id).unwrap());
}

#[cfg(unix)]
#[test]
fn session_inspect_and_logs_read_only_daemon_owned_bounded_capture() {
    let (_home, project_root, app) = application();
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
        "inspect-create",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    let process_id = session.managed_process_id.clone().unwrap();
    let process = app.store().load().unwrap().processes[&process_id].clone();
    std::fs::write(&process.stdout_prefix_path, b"stdout-prefix").unwrap();
    std::fs::write(&process.stdout_tail_path, b"stdout-tail").unwrap();
    std::fs::write(&process.stderr_prefix_path, b"stderr-prefix").unwrap();
    std::fs::write(&process.stderr_tail_path, b"stderr-tail").unwrap();

    let inspected = handle(
        &app,
        DaemonRequest::SessionInspect {
            session_id: session.session_id.clone(),
        },
        "",
    );
    match inspected {
        DaemonResponse::SessionInspection {
            session: inspected_session,
            process: inspected_process,
        } => {
            assert_eq!(inspected_session.session_id, session.session_id);
            assert_eq!(
                inspected_process.as_ref().as_ref().unwrap().process_id,
                process_id
            );
        }
        other => panic!("unexpected response: {other:?}"),
    }

    let logs = handle(
        &app,
        DaemonRequest::SessionLogs {
            session_id: session.session_id.clone(),
        },
        "",
    );
    match logs {
        DaemonResponse::SessionLogs {
            session_id,
            process_id: logged_process_id,
            logs,
        } => {
            assert_eq!(session_id, session.session_id);
            assert_eq!(logged_process_id, process_id);
            assert_eq!(logs.stdout_prefix, "stdout-prefix");
            assert_eq!(logs.stdout_tail, "stdout-tail");
            assert_eq!(logs.stderr_prefix, "stderr-prefix");
            assert_eq!(logs.stderr_tail, "stderr-tail");
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn provider_request_wait_does_not_block_another_managed_process() {
    let log_root = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let marker = cwd.path().join("slow-request-started");
    let slow_args = vec![
        "-c".to_string(),
        "read line; touch \"$1\"; sleep 1; echo \"$line\"; cat".to_string(),
        "pulse-test".to_string(),
        marker.to_string_lossy().to_string(),
    ];
    let owner = Arc::new(ProcessOwner::default());
    let slow = owner
        .spawn(SpawnRequest {
            owner_kind: "test",
            owner_id: "slow",
            provider_id: "test",
            executable: std::path::Path::new("/bin/sh"),
            args: &slow_args,
            cwd: cwd.path(),
            log_root: log_root.path(),
            max_log_bytes: 1024,
        })
        .unwrap();
    let fast_args = Vec::new();
    let fast = owner
        .spawn(SpawnRequest {
            owner_kind: "test",
            owner_id: "fast",
            provider_id: "test",
            executable: std::path::Path::new("/bin/cat"),
            args: &fast_args,
            cwd: cwd.path(),
            log_root: log_root.path(),
            max_log_bytes: 1024,
        })
        .unwrap();

    let slow_owner = Arc::clone(&owner);
    let slow_process_id = slow.process_id.clone();
    let slow_request = std::thread::spawn(move || {
        slow_owner.request_json(
            &slow_process_id,
            "slow",
            r#"{"id":"slow"}"#,
            Duration::from_secs(3),
        )
    });
    let marker_deadline = Instant::now() + Duration::from_secs(2);
    while !marker.exists() && Instant::now() < marker_deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(marker.exists(), "slow provider never received its request");

    let started = Instant::now();
    let (response, _) = owner
        .request_json(
            &fast.process_id,
            "fast",
            r#"{"id":"fast"}"#,
            Duration::from_secs(2),
        )
        .unwrap();
    assert_eq!(response["id"], "fast");
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "waiting on one provider serialized an unrelated provider"
    );

    slow_request.join().unwrap().unwrap();
    owner.terminate(&slow.process_id).unwrap();
    owner.terminate(&fast.process_id).unwrap();
}

#[test]
fn failed_interrupt_never_reports_false_idle() {
    let (_home, project_root, app) = application();
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
        "create-interrupt-session",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    let session = match handle(
        &app,
        DaemonRequest::SessionSend {
            session_id: session.session_id,
            input: "test input".to_string(),
        },
        "send-turn",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(session.lifecycle, SessionLifecycle::Running);
    let interrupted = match handle(
        &app,
        DaemonRequest::SessionInterrupt {
            session_id: session.session_id.clone(),
        },
        "interrupt-turn",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(interrupted.lifecycle, SessionLifecycle::Running);
    assert!(interrupted.last_error.is_some());

    handle(
        &app,
        DaemonRequest::SessionClose {
            session_id: interrupted.session_id,
        },
        "close-interrupt-session",
    );
}

#[cfg(unix)]
#[test]
fn session_resume_replaces_transport_while_preserving_session_and_provider_handle() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let workspace_id = create_workspace(&app, &project_id);

    // Create a session
    let session = match handle(
        &app,
        DaemonRequest::SessionCreate {
            workspace_id: workspace_id.clone(),
            provider_id: "codex".to_string(),
            parent_session_id: None,
            provider_options: resumable_provider_options(),
        },
        "resume-session-create",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(session.lifecycle, SessionLifecycle::Idle);
    assert_eq!(
        session.provider_handle.as_deref(),
        Some("thread-pulse-test")
    );
    let original_process_id = session.managed_process_id.clone().unwrap();
    let original_session_id = session.session_id.clone();
    let original_provider_handle = session.provider_handle.clone();

    // Close the session to put it in a state that allows resume.
    handle(
        &app,
        DaemonRequest::SessionClose {
            session_id: original_session_id.clone(),
        },
        "close-for-reattach",
    );

    let resumed = match handle(
        &app,
        DaemonRequest::SessionResume {
            session_id: original_session_id.clone(),
            provider_options: resumable_provider_options(),
        },
        "resume-session",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };

    assert_eq!(resumed.session_id, original_session_id);
    assert_eq!(resumed.provider_handle, original_provider_handle);
    assert_ne!(
        resumed.managed_process_id.as_deref(),
        Some(original_process_id.as_str())
    );
    assert_eq!(resumed.lifecycle, SessionLifecycle::Idle);
    assert!(resumed.last_error.is_none());
    let state = app.store().load().unwrap();
    let resume_effect = state
        .external_effects
        .values()
        .find(|effect| {
            effect.kind == pulse::daemon::persistence::ExternalEffectKind::ProviderSessionResume
        })
        .expect("session resume effect");
    assert_eq!(
        resume_effect.state,
        pulse::daemon::persistence::ExternalEffectState::Acknowledged
    );
    assert_eq!(
        resume_effect.resource_id.as_deref(),
        resumed.managed_process_id.as_deref()
    );

    // Timeline shows the resume event.
    let timeline = handle(
        &app,
        DaemonRequest::TimelineList {
            cursor: None,
            limit: 100,
            session_id: Some(original_session_id.clone()),
        },
        "",
    );
    match timeline {
        DaemonResponse::Timeline { page } => {
            assert!(
                page.events
                    .iter()
                    .any(|event| event.event_type == "session.resumed"),
                "timeline should contain session.resumed event"
            );
        }
        other => panic!("unexpected response: {other:?}"),
    }

    // Cleanup.
    handle(
        &app,
        DaemonRequest::SessionClose {
            session_id: original_session_id,
        },
        "close-after-reattach",
    );
}

#[cfg(unix)]
#[test]
fn concurrent_session_resume_creates_one_replacement_process() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let workspace_id = create_workspace(&app, &project_id);
    let session = match handle(
        &app,
        DaemonRequest::SessionCreate {
            workspace_id,
            provider_id: "codex".to_string(),
            parent_session_id: None,
            provider_options: resumable_provider_options(),
        },
        "concurrent-resume-session",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    handle(
        &app,
        DaemonRequest::SessionClose {
            session_id: session.session_id.clone(),
        },
        "concurrent-resume-close",
    );

    let barrier = Arc::new(Barrier::new(3));
    let spawn_resume = |key: &'static str| {
        let app = Arc::clone(&app);
        let barrier = Arc::clone(&barrier);
        let session_id = session.session_id.clone();
        std::thread::spawn(move || {
            barrier.wait();
            app.handle(
                &DaemonRequest::SessionResume {
                    session_id,
                    provider_options: resumable_provider_options(),
                },
                key,
            )
        })
    };
    let first = spawn_resume("concurrent-resume-one");
    let second = spawn_resume("concurrent-resume-two");
    barrier.wait();
    let outcomes = [first.join().unwrap(), second.join().unwrap()];
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert!(outcomes.iter().any(|outcome| {
        outcome
            .as_ref()
            .is_err_and(|error| error.code == "session_resume_not_required")
    }));
    let state = app.store().load().unwrap();
    let current = state.sessions.get(&session.session_id).unwrap();
    let current_process_id = current.managed_process_id.as_deref().unwrap();
    assert_eq!(
        state
            .processes
            .values()
            .filter(|process| {
                process.owner_id == session.session_id
                    && process.state == pulse::daemon::process::ManagedProcessState::Running
            })
            .count(),
        1
    );
    assert!(state.processes.contains_key(current_process_id));
    handle(
        &app,
        DaemonRequest::SessionClose {
            session_id: session.session_id,
        },
        "concurrent-resume-cleanup",
    );
}

#[cfg(unix)]
#[test]
fn session_resume_recovers_after_error_lifecycle() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let workspace_id = create_workspace(&app, &project_id);

    let session = match handle(
        &app,
        DaemonRequest::SessionCreate {
            workspace_id: workspace_id.clone(),
            provider_id: "codex".to_string(),
            parent_session_id: None,
            provider_options: resumable_provider_options(),
        },
        "resume-error-session",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    let session_id = session.session_id.clone();

    // Close the session to produce a non-idle lifecycle.
    handle(
        &app,
        DaemonRequest::SessionClose {
            session_id: session_id.clone(),
        },
        "close-for-resume",
    );

    let resumed = match handle(
        &app,
        DaemonRequest::SessionResume {
            session_id: session_id.clone(),
            provider_options: resumable_provider_options(),
        },
        "resume-error-session-retry",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(resumed.session_id, session_id);
    assert_eq!(resumed.lifecycle, SessionLifecycle::Idle);

    handle(
        &app,
        DaemonRequest::SessionClose {
            session_id: session_id.clone(),
        },
        "close-after-resume",
    );
}

#[cfg(unix)]
fn resume_fail_provider_options(mode: &str) -> serde_json::Value {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fake_codex_resume_fail.mjs");
    json!({
        "executable": std::env::var("NODE").unwrap_or_else(|_| "node".to_string()),
        "args": [script.to_string_lossy(), mode],
    })
}

/// Assert the persisted failure state for a session resume that could not
/// complete: explicit `Error` lifecycle with an actionable `last_error`, the
/// old and candidate processes both recorded as `Exited`, no `Running` process
/// owned by the session, neither process owned by the `ProcessOwner`, and
/// exactly one `session.resume_failed` timeline event correlated to the
/// old/candidate ids and the failure code. Returns the candidate process id.
#[cfg(unix)]
fn assert_resume_failure_persisted(
    app: &DaemonApplication,
    session_id: &str,
    old_process_id: &str,
    expected_code: &str,
) -> String {
    use pulse::daemon::process::ManagedProcessState;

    let state = app.store().load().unwrap();
    let session = state.sessions.get(session_id).unwrap();
    assert_eq!(
        session.lifecycle,
        SessionLifecycle::Error,
        "session must be an explicit Error after resume failure"
    );
    let last_error = session
        .last_error
        .as_deref()
        .expect("session must carry an actionable last_error after resume failure");
    assert!(
        last_error.contains(expected_code),
        "last_error {last_error:?} should name the failure code {expected_code:?}"
    );

    // The old process was terminated at the start of resume and must be Exited.
    assert_eq!(
        state.processes[old_process_id].state,
        ManagedProcessState::Exited
    );

    // Exactly one session.resume_failed event, correlated to old + candidate.
    let timeline = match handle(
        app,
        DaemonRequest::TimelineList {
            cursor: None,
            limit: 1000,
            session_id: Some(session_id.to_string()),
        },
        "",
    ) {
        DaemonResponse::Timeline { page } => page,
        other => panic!("unexpected response: {other:?}"),
    };
    let failures: Vec<_> = timeline
        .events
        .iter()
        .filter(|event| event.event_type == "session.resume_failed")
        .collect();
    assert_eq!(
        failures.len(),
        1,
        "exactly one session.resume_failed event must be emitted, found {}",
        failures.len()
    );
    let payload = &failures[0].payload;
    assert_eq!(
        payload["failure_code"].as_str(),
        Some(expected_code),
        "session.resume_failed failure_code mismatch: {:?}",
        payload
    );
    assert_eq!(
        payload["old_process_id"].as_str(),
        Some(old_process_id),
        "session.resume_failed old_process_id mismatch: {:?}",
        payload
    );
    let candidate_process_id = payload["candidate_process_id"]
        .as_str()
        .expect("a spawned candidate must be correlated in session.resume_failed")
        .to_string();

    // The candidate must be recorded as Exited, never Running.
    let candidate = state
        .processes
        .get(&candidate_process_id)
        .unwrap_or_else(|| panic!("candidate process {candidate_process_id} must be persisted"));
    assert_eq!(candidate.state, ManagedProcessState::Exited);
    assert_ne!(candidate_process_id, old_process_id);

    // No process owned by this session may still appear Running.
    let running = state
        .processes
        .values()
        .filter(|process| {
            process.owner_id == session_id && process.state == ManagedProcessState::Running
        })
        .count();
    assert_eq!(running, 0, "no process for the session may remain Running");

    // ProcessOwner liveness: neither the old nor the candidate process is owned
    // (both were terminated and reaped), so they cannot be mistaken for live.
    assert!(
        app.managed_process_is_alive(old_process_id).is_err(),
        "old process must no longer be owned by ProcessOwner"
    );
    assert!(
        app.managed_process_is_alive(&candidate_process_id).is_err(),
        "candidate process must no longer be owned by ProcessOwner"
    );

    candidate_process_id
}

#[cfg(unix)]
#[test]
fn session_resume_provider_failure_persists_error_and_terminates_candidate() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let workspace_id = create_workspace(&app, &project_id);

    // Create a healthy session backed by the good fixture.
    let session = match handle(
        &app,
        DaemonRequest::SessionCreate {
            workspace_id: workspace_id.clone(),
            provider_id: "codex".to_string(),
            parent_session_id: None,
            provider_options: resumable_provider_options(),
        },
        "resume-fail-create",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    let session_id = session.session_id.clone();
    let old_process_id = session.managed_process_id.clone().unwrap();

    // Close it so resume is permitted, then resume against a provider that
    // answers thread/resume with a JSON-RPC error.
    handle(
        &app,
        DaemonRequest::SessionClose {
            session_id: session_id.clone(),
        },
        "resume-fail-close",
    );
    let error = app
        .handle(
            &DaemonRequest::SessionResume {
                session_id: session_id.clone(),
                provider_options: resume_fail_provider_options("resume_error"),
            },
            "resume-fail-provider-error",
        )
        .unwrap_err();
    assert_eq!(error.code, "provider_request_failed");

    let candidate_process_id = assert_resume_failure_persisted(
        &app,
        &session_id,
        &old_process_id,
        "provider_request_failed",
    );
    assert_ne!(candidate_process_id, old_process_id);
}

#[cfg(unix)]
#[test]
fn session_resume_handle_mismatch_persists_error_and_terminates_candidate() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let workspace_id = create_workspace(&app, &project_id);

    let session = match handle(
        &app,
        DaemonRequest::SessionCreate {
            workspace_id: workspace_id.clone(),
            provider_id: "codex".to_string(),
            parent_session_id: None,
            provider_options: resumable_provider_options(),
        },
        "resume-mismatch-create",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(
        session.provider_handle.as_deref(),
        Some("thread-pulse-test")
    );
    let session_id = session.session_id.clone();
    let old_process_id = session.managed_process_id.clone().unwrap();

    handle(
        &app,
        DaemonRequest::SessionClose {
            session_id: session_id.clone(),
        },
        "resume-mismatch-close",
    );

    // The provider resumes a *different* native thread id, so the daemon must
    // reject the handle and record the failure.
    let error = app
        .handle(
            &DaemonRequest::SessionResume {
                session_id: session_id.clone(),
                provider_options: resume_fail_provider_options("handle_mismatch"),
            },
            "resume-fail-handle-mismatch",
        )
        .unwrap_err();
    assert_eq!(error.code, "provider_resume_identity_mismatch");

    let candidate_process_id = assert_resume_failure_persisted(
        &app,
        &session_id,
        &old_process_id,
        "provider_resume_identity_mismatch",
    );
    assert_ne!(candidate_process_id, old_process_id);

    // The provider answered, but with a different native resource identity.
    // That is not safe to classify as a retryable rejection: automatic retry
    // is blocked until the candidate/session relationship is reconciled.
    let retry_error = app
        .handle(
            &DaemonRequest::SessionResume {
                session_id,
                provider_options: resumable_provider_options(),
            },
            "resume-mismatch-retry",
        )
        .unwrap_err();
    assert_eq!(retry_error.code, "external_effect_reconciliation_required");
}

#[test]
fn shutdown_cleanup_marks_processes_and_sessions_as_exited() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let workspace_id = create_workspace(&app, &project_id);

    // Create a session to get a managed process.
    let response = handle(
        &app,
        DaemonRequest::SessionCreate {
            workspace_id: workspace_id.clone(),
            provider_id: "codex".to_string(),
            parent_session_id: None,
            provider_options: provider_options(),
        },
        "shutdown-session",
    );
    let session_id = match &response {
        DaemonResponse::Session { session } => session.session_id.clone(),
        other => panic!("unexpected response: {other:?}"),
    };

    // Call shutdown cleanup directly (not via Shutdown which also disconnects).
    app.shutdown_cleanup().unwrap();

    // Check the daemon state:
    let state = app.store().load().unwrap();
    // Processes are Exited.
    for process in state.processes.values() {
        assert!(
            matches!(
                process.state,
                pulse::daemon::process::ManagedProcessState::Exited
            ),
            "process {} should be Exited after shutdown_cleanup",
            process.process_id
        );
    }
    // Sessions are Error (since they weren't gracefully closed).
    let session = state
        .sessions
        .get(&session_id)
        .expect("session should exist");
    assert!(
        matches!(session.lifecycle, SessionLifecycle::Error),
        "session should be Error after shutdown_cleanup, got {:?}",
        session.lifecycle
    );
    // Timeline contains shutdown event.
    assert!(
        state
            .timeline
            .iter()
            .any(|event| event.event_type == "daemon.shutdown"),
        "timeline should contain daemon.shutdown event"
    );
}

#[cfg(unix)]
#[test]
fn provider_process_intent_failpoint_leaves_not_sent_recoverable_record() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let workspace_id = create_workspace(&app, &project_id);
    app.store()
        .arm_failpoint("after_provider_process_intent", FailpointMode::Error)
        .unwrap();
    let error = app
        .handle(
            &DaemonRequest::SessionCreate {
                workspace_id,
                provider_id: "codex".to_string(),
                parent_session_id: None,
                provider_options: provider_options(),
            },
            "effect-intent-failpoint",
        )
        .unwrap_err();
    assert_eq!(error.code, "injected_failpoint");
    let state = app.store().load().unwrap();
    assert!(state.external_effects.values().any(|effect| {
        effect.kind == pulse::daemon::persistence::ExternalEffectKind::ProviderProcessCreate
            && effect.state == pulse::daemon::persistence::ExternalEffectState::NotSent
    }));
}

#[cfg(unix)]
#[test]
fn provider_process_success_before_ack_is_attempting_and_blocks_retry() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let workspace_id = create_workspace(&app, &project_id);
    let retry_workspace_id = workspace_id.clone();
    app.store()
        .arm_failpoint(
            "after_provider_process_success_before_ack",
            FailpointMode::Error,
        )
        .unwrap();
    let error = app
        .handle(
            &DaemonRequest::SessionCreate {
                workspace_id,
                provider_id: "codex".to_string(),
                parent_session_id: None,
                provider_options: json!({
                    "executable": "/bin/sleep",
                    "args": ["5"],
                    "protocol_mode": "opaque_test"
                }),
            },
            "effect-process-success-failpoint",
        )
        .unwrap_err();
    assert_eq!(error.code, "injected_failpoint");
    let state = app.store().load().unwrap();
    let effect = state
        .external_effects
        .values()
        .find(|effect| {
            effect.kind == pulse::daemon::persistence::ExternalEffectKind::ProviderProcessCreate
        })
        .expect("provider process effect");
    assert_eq!(
        effect.state,
        pulse::daemon::persistence::ExternalEffectState::Attempting
    );
    assert!(effect.resource_id.is_some());
    app.store()
        .disarm_failpoint(
            "after_provider_process_success_before_ack",
            FailpointMode::Error,
        )
        .unwrap();
    let retry = app
        .handle(
            &DaemonRequest::SessionCreate {
                workspace_id: retry_workspace_id,
                provider_id: "codex".to_string(),
                parent_session_id: None,
                provider_options: json!({
                    "executable": "/bin/sleep",
                    "args": ["5"],
                    "protocol_mode": "opaque_test"
                }),
            },
            "effect-process-success-failpoint",
        )
        .unwrap_err();
    assert_eq!(
        retry.code, "external_effect_reconciliation_required",
        "{}",
        retry.message
    );
}

#[cfg(unix)]
#[test]
fn provider_session_transport_ambiguity_remains_operator_actionable() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let workspace_id = create_workspace(&app, &project_id);
    let error = app
        .handle(
            &DaemonRequest::SessionCreate {
                workspace_id,
                provider_id: "codex".to_string(),
                parent_session_id: None,
                provider_options: json!({
                    "executable": "/bin/sh",
                    "args": ["-c", "exec 1>&-; sleep 5"]
                }),
            },
            "provider-session-ambiguity",
        )
        .unwrap_err();
    assert!(matches!(
        error.code.as_str(),
        "provider_transport_closed" | "io_error" | "provider_response_timeout"
    ));
    let state = app.store().load().unwrap();
    let effect = state
        .external_effects
        .values()
        .find(|effect| {
            effect.kind == pulse::daemon::persistence::ExternalEffectKind::ProviderSessionCreate
        })
        .expect("provider session effect");
    assert_eq!(
        effect.state,
        pulse::daemon::persistence::ExternalEffectState::OutcomeUnknown
    );
    assert!(effect.detail.contains("provider") || effect.detail.contains("cleanup"));
}

#[cfg(unix)]
#[test]
fn concurrent_close_requeues_provider_event_before_handle_release() {
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
    process.stdout.write(JSON.stringify({ id: request.id, result: { thread: { id: "fenced-thread" } } }) + "\n");
  } else if (request.method === "turn/start") {
    process.stdout.write(JSON.stringify({ id: request.id, result: { turn: { id: "fenced-turn" } } }) + "\n");
    setTimeout(() => process.stdout.write(JSON.stringify({ method: "fenced/event", params: { once: true } }) + "\n"), 60);
  } else if (request.method === "turn/interrupt") {
    process.stdout.write(JSON.stringify({ id: request.id, result: {} }) + "\n");
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
        "fenced-event-session",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    handle(
        &app,
        DaemonRequest::SessionSend {
            session_id: session.session_id.clone(),
            input: "fenced".to_string(),
        },
        "fenced-event-send",
    );
    app.store()
        .arm_failpoint("before_provider_event_commit", FailpointMode::Error)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut failpoint_observed = false;
    while Instant::now() < deadline {
        match app.handle(
            &DaemonRequest::TimelineList {
                cursor: None,
                limit: 100,
                session_id: Some(session.session_id.clone()),
            },
            "",
        ) {
            Err(error) if error.code == "injected_failpoint" => {
                failpoint_observed = true;
                break;
            }
            Ok(_) => std::thread::sleep(Duration::from_millis(10)),
            Err(error) => panic!("unexpected timeline refresh error: {error:?}"),
        }
    }
    assert!(
        failpoint_observed,
        "provider event did not reach before_provider_event_commit before close deadline"
    );
    let close_app = Arc::clone(&app);
    let close_session_id = session.session_id.clone();
    let close = std::thread::spawn(move || {
        close_app.handle(
            &DaemonRequest::SessionClose {
                session_id: close_session_id,
            },
            "fenced-event-close",
        )
    })
    .join()
    .unwrap();
    assert_eq!(close.unwrap_err().code, "injected_failpoint");
    app.store()
        .disarm_failpoint("before_provider_event_commit", FailpointMode::Error)
        .unwrap();
    let closed = handle(
        &app,
        DaemonRequest::SessionClose {
            session_id: session.session_id.clone(),
        },
        "fenced-event-close-retry",
    );
    assert!(matches!(
        closed,
        DaemonResponse::Session {
            session: pulse::daemon::session::SessionRecord {
                lifecycle: SessionLifecycle::Closed,
                ..
            }
        }
    ));
    let state = app.store().load().unwrap();
    assert_eq!(
        state
            .timeline
            .iter()
            .filter(|event| {
                event.event_type == "provider.notification"
                    && event.payload.get("method").and_then(|value| value.as_str())
                        == Some("fenced/event")
            })
            .count(),
        1
    );
}

#[cfg(unix)]
#[test]
fn close_retry_skips_interrupt_for_dead_child_with_shutdown_notification() {
    let (_home, project_root, app) = application();
    let provider_script = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        provider_script.path(),
        r#"import readline from "node:readline";
process.on("SIGTERM", () => {
  process.stdout.write(JSON.stringify({ method: "shutdown/notification", params: { once: true } }) + "\n", () => process.exit(0));
});
const lines = readline.createInterface({ input: process.stdin });
for await (const raw of lines) {
  const request = JSON.parse(raw);
  if (request.method === "initialize") {
    process.stdout.write(JSON.stringify({ id: request.id, result: {} }) + "\n");
  } else if (request.method === "thread/start") {
    process.stdout.write(JSON.stringify({ id: request.id, result: { thread: { id: "shutdown-thread" } } }) + "\n");
  } else if (request.method === "turn/start") {
    process.stdout.write(JSON.stringify({ id: request.id, result: { turn: { id: "shutdown-turn" } } }) + "\n");
  } else if (request.method === "turn/interrupt") {
    process.stdout.write(JSON.stringify({ id: request.id, result: {} }) + "\n");
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
        "shutdown-retry-session",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    handle(
        &app,
        DaemonRequest::SessionSend {
            session_id: session.session_id.clone(),
            input: "shutdown retry turn".to_string(),
        },
        "shutdown-retry-send",
    );
    app.store()
        .arm_failpoint("before_provider_event_commit", FailpointMode::Error)
        .unwrap();
    let first_close = app.handle(
        &DaemonRequest::SessionClose {
            session_id: session.session_id.clone(),
        },
        "shutdown-retry-first-close",
    );
    assert_eq!(first_close.unwrap_err().code, "injected_failpoint");
    app.store()
        .disarm_failpoint("before_provider_event_commit", FailpointMode::Error)
        .unwrap();
    let closed = handle(
        &app,
        DaemonRequest::SessionClose {
            session_id: session.session_id.clone(),
        },
        "shutdown-retry-second-close",
    );
    assert!(matches!(
        closed,
        DaemonResponse::Session {
            session: pulse::daemon::session::SessionRecord {
                lifecycle: SessionLifecycle::Closed,
                ..
            }
        }
    ));
    let state = app.store().load().unwrap();
    assert_eq!(
        state
            .timeline
            .iter()
            .filter(|event| {
                event.event_type == "provider.notification"
                    && event.payload.get("method").and_then(|value| value.as_str())
                        == Some("shutdown/notification")
            })
            .count(),
        1
    );
}

#[cfg(unix)]
#[test]
fn close_completes_with_unmatched_response_pressure_and_preserves_notifications() {
    let (_home, project_root, app) = application();
    let provider_script = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        provider_script.path(),
        r#"import readline from "node:readline";
const unmatched = (prefix) => {
  for (let index = 0; index < 96; index += 1) {
    process.stdout.write(JSON.stringify({ id: `${prefix}-${index}`, result: {} }) + "\n");
  }
};
process.on("SIGTERM", () => {
  process.stdout.write(JSON.stringify({ method: "pressure/notification", params: { durable: true } }) + "\n", () => process.exit(0));
});
const lines = readline.createInterface({ input: process.stdin });
for await (const raw of lines) {
  const request = JSON.parse(raw);
  if (request.method === "initialize") {
    process.stdout.write(JSON.stringify({ id: request.id, result: {} }) + "\n");
  } else if (request.method === "thread/start") {
    process.stdout.write(JSON.stringify({ id: request.id, result: { thread: { id: "pressure-thread" } } }) + "\n");
  } else if (request.method === "turn/start") {
    unmatched("turn-unmatched");
    process.stdout.write(JSON.stringify({ id: request.id, result: { turn: { id: "pressure-turn" } } }) + "\n");
  } else if (request.method === "turn/interrupt") {
    unmatched("interrupt-unmatched");
    process.stdout.write(JSON.stringify({ id: request.id, result: {} }) + "\n");
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
        "pressure-session",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    handle(
        &app,
        DaemonRequest::SessionSend {
            session_id: session.session_id.clone(),
            input: "response pressure".to_string(),
        },
        "pressure-send",
    );
    let (sender, receiver) = std::sync::mpsc::channel();
    let close_app = Arc::clone(&app);
    let close_session_id = session.session_id.clone();
    std::thread::spawn(move || {
        sender
            .send(close_app.handle(
                &DaemonRequest::SessionClose {
                    session_id: close_session_id,
                },
                "pressure-close",
            ))
            .unwrap();
    });
    let closed = receiver
        .recv_timeout(Duration::from_secs(3))
        .expect("session close must not deadlock on unmatched responses")
        .unwrap();
    assert!(matches!(
        closed,
        DaemonResponse::Session {
            session: pulse::daemon::session::SessionRecord {
                lifecycle: SessionLifecycle::Closed,
                ..
            }
        }
    ));
    let state = app.store().load().unwrap();
    assert_eq!(
        state
            .timeline
            .iter()
            .filter(|event| {
                event.event_type == "provider.notification"
                    && event.payload.get("method").and_then(|value| value.as_str())
                        == Some("pressure/notification")
            })
            .count(),
        1
    );
}

#[cfg(unix)]
#[test]
fn close_persists_interrupt_and_stdout_close_notifications_once() {
    let (_home, project_root, app) = application();
    let provider_script = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        provider_script.path(),
        r#"import readline from "node:readline";
let interrupts = 0;
process.on("SIGTERM", () => {
  process.stdout.write(JSON.stringify({ method: "close/notification", params: { phase: "stdout-close" } }) + "\n", () => process.exit(0));
});
const lines = readline.createInterface({ input: process.stdin });
for await (const raw of lines) {
  const request = JSON.parse(raw);
  if (request.method === "initialize") {
    process.stdout.write(JSON.stringify({ id: request.id, result: {} }) + "\n");
  } else if (request.method === "thread/start") {
    process.stdout.write(JSON.stringify({ id: request.id, result: { thread: { id: "close-thread" } } }) + "\n");
  } else if (request.method === "turn/start") {
    process.stdout.write(JSON.stringify({ id: request.id, result: { turn: { id: "close-turn" } } }) + "\n");
  } else if (request.method === "turn/interrupt") {
    interrupts += 1;
    if (interrupts === 1) {
      process.stdout.write(JSON.stringify({ method: "close/notification", params: { phase: "interrupt-response" } }) + "\n");
    }
    process.stdout.write(JSON.stringify({ id: request.id, result: {} }) + "\n");
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
        "close-notification-session",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    handle(
        &app,
        DaemonRequest::SessionSend {
            session_id: session.session_id.clone(),
            input: "close notification turn".to_string(),
        },
        "close-notification-send",
    );
    app.store()
        .arm_failpoint("before_provider_event_commit", FailpointMode::Error)
        .unwrap();
    let first_close = app.handle(
        &DaemonRequest::SessionClose {
            session_id: session.session_id.clone(),
        },
        "close-notification-first",
    );
    assert_eq!(first_close.unwrap_err().code, "injected_failpoint");
    app.store()
        .disarm_failpoint("before_provider_event_commit", FailpointMode::Error)
        .unwrap();
    let closed = handle(
        &app,
        DaemonRequest::SessionClose {
            session_id: session.session_id.clone(),
        },
        "close-notification-retry",
    );
    assert!(matches!(
        closed,
        DaemonResponse::Session {
            session: pulse::daemon::session::SessionRecord {
                lifecycle: SessionLifecycle::Closed,
                ..
            }
        }
    ));
    let state = app.store().load().unwrap();
    for phase in ["interrupt-response", "stdout-close"] {
        assert_eq!(
            state
                .timeline
                .iter()
                .filter(|event| {
                    event.event_type == "provider.notification"
                        && event.payload.get("method").and_then(|value| value.as_str())
                            == Some("close/notification")
                        && event
                            .payload
                            .pointer("/params/phase")
                            .and_then(|value| value.as_str())
                            == Some(phase)
                })
                .count(),
            1,
            "notification phase {phase} should be durable exactly once"
        );
    }
}
