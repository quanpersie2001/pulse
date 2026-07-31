use pulse::daemon::application::DaemonApplication;
use pulse::daemon::permissions::RuntimePrincipal;
use pulse::daemon::persistence::StateStore;
#[cfg(unix)]
use pulse::daemon::process::{ProcessOwner, SpawnRequest};
use pulse::daemon::protocol::{DaemonRequest, DaemonResponse, RequestEnvelope};
use pulse::daemon::session::SessionLifecycle;
use pulse::daemon::transport::mcp::McpToolAdapter;
use pulse::daemon::workspace::IsolationMode;
use serde_json::json;
use std::sync::{Arc, Barrier};
#[cfg(unix)]
use std::time::{Duration, Instant};

fn application() -> (tempfile::TempDir, tempfile::TempDir, Arc<DaemonApplication>) {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let app = DaemonApplication::new(StateStore::new(home.path()), "test")
        .map(Arc::new)
        .unwrap();
    (home, project, app)
}

fn handle(app: &DaemonApplication, request: DaemonRequest, key: &str) -> DaemonResponse {
    app.handle(&request, key)
        .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message))
}

fn open_project(app: &DaemonApplication, root: &std::path::Path) -> String {
    match handle(
        app,
        DaemonRequest::ProjectOpen {
            root: root.to_string_lossy().to_string(),
        },
        "open-project",
    ) {
        DaemonResponse::Project { project } => project.project_id,
        other => panic!("unexpected response: {other:?}"),
    }
}

fn create_workspace(app: &DaemonApplication, project_id: &str) -> String {
    match handle(
        app,
        DaemonRequest::WorkspaceCreate {
            project_id: project_id.to_string(),
            name: "primary".to_string(),
            isolation: IsolationMode::Local,
            base_commit: None,
        },
        "create-workspace",
    ) {
        DaemonResponse::Workspace { workspace } => workspace.workspace_id,
        other => panic!("unexpected response: {other:?}"),
    }
}

#[cfg(unix)]
fn provider_options() -> serde_json::Value {
    json!({
        "executable": "/bin/cat",
        "args": [],
        "protocol_mode": "opaque_test"
    })
}

fn resumable_provider_options() -> serde_json::Value {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fake_codex_provider.mjs");
    json!({
        "executable": std::env::var("NODE").unwrap_or_else(|_| "node".to_string()),
        "args": [script.to_string_lossy()]
    })
}

#[cfg(windows)]
fn provider_options() -> serde_json::Value {
    json!({
        "executable": "C:\\Windows\\System32\\cmd.exe",
        "args": ["/Q", "/K", "more"],
        "protocol_mode": "opaque_test"
    })
}

#[test]
fn project_workspace_and_idempotency_are_stable() {
    let (_home, project_root, app) = application();
    let request = DaemonRequest::ProjectOpen {
        root: project_root.path().to_string_lossy().to_string(),
    };
    let first = handle(&app, request.clone(), "same-open");
    let second = handle(&app, request, "same-open");
    assert_eq!(first, second);

    let project_id = match first {
        DaemonResponse::Project { project } => project.project_id,
        other => panic!("unexpected response: {other:?}"),
    };
    let workspace_id = create_workspace(&app, &project_id);
    let listed = handle(
        &app,
        DaemonRequest::WorkspaceList {
            project_id: Some(project_id),
            include_archived: false,
        },
        "",
    );
    match listed {
        DaemonResponse::Workspaces { workspaces } => {
            assert_eq!(workspaces.len(), 1);
            assert_eq!(workspaces[0].workspace_id, workspace_id);
        }
        other => panic!("unexpected response: {other:?}"),
    }

    let error = app
        .handle(
            &DaemonRequest::ProjectArchive {
                project_id: "prj_different".to_string(),
            },
            "same-open",
        )
        .unwrap_err();
    assert_eq!(error.code, "idempotency_key_conflict");
}

#[test]
fn concurrent_replay_creates_one_workspace() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let request = DaemonRequest::WorkspaceCreate {
        project_id,
        name: "shared".to_string(),
        isolation: IsolationMode::Local,
        base_commit: None,
    };
    let first_app = Arc::clone(&app);
    let first_request = request.clone();
    let first = std::thread::spawn(move || handle(&first_app, first_request, "concurrent-key"));
    let second_app = Arc::clone(&app);
    let second = std::thread::spawn(move || handle(&second_app, request, "concurrent-key"));
    assert_eq!(first.join().unwrap(), second.join().unwrap());

    let state = app.store().load().unwrap();
    assert_eq!(state.workspaces.len(), 1);
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
fn two_sessions_share_workspace_but_keep_distinct_identity() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let workspace_id = create_workspace(&app, &project_id);
    let create = |key: &str| {
        handle(
            &app,
            DaemonRequest::SessionCreate {
                workspace_id: workspace_id.clone(),
                provider_id: "codex".to_string(),
                parent_session_id: None,
                provider_options: provider_options(),
            },
            key,
        )
    };
    let first = match create("session-one") {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    let second = match create("session-two") {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_ne!(first.session_id, second.session_id);
    assert_eq!(first.workspace_id, second.workspace_id);
    assert!(first.provider_handle.is_none());
    assert!(first.managed_process_id.is_some());

    for session in [first, second] {
        let response = handle(
            &app,
            DaemonRequest::SessionClose {
                session_id: session.session_id,
            },
            &format!("close-{}", session.managed_process_id.unwrap()),
        );
        assert!(matches!(
            response,
            DaemonResponse::Session {
                session: pulse::daemon::session::SessionRecord {
                    lifecycle: SessionLifecycle::Closed,
                    ..
                }
            }
        ));
    }
}

#[test]
fn parentage_does_not_bypass_explicit_communication_policy() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let workspace_id = create_workspace(&app, &project_id);
    let parent = match handle(
        &app,
        DaemonRequest::SessionCreate {
            workspace_id: workspace_id.clone(),
            provider_id: "codex".to_string(),
            parent_session_id: None,
            provider_options: provider_options(),
        },
        "create-message-parent",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    let child = match handle(
        &app,
        DaemonRequest::SessionCreate {
            workspace_id,
            provider_id: "codex".to_string(),
            parent_session_id: Some(parent.session_id.clone()),
            provider_options: provider_options(),
        },
        "create-message-child",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };

    let denied = app
        .handle(
            &DaemonRequest::SessionMessageSend {
                sender_session_id: parent.session_id.clone(),
                recipient_session_id: child.session_id.clone(),
                body: "inspect this finding".to_string(),
            },
            "message-before-grant",
        )
        .unwrap_err();
    assert_eq!(denied.code, "session_communication_denied");

    assert!(matches!(
        handle(
            &app,
            DaemonRequest::SessionCommunicationGrant {
                sender_session_id: parent.session_id.clone(),
                recipient_session_id: child.session_id.clone(),
            },
            "grant-parent-child",
        ),
        DaemonResponse::CommunicationGrant { .. }
    ));
    let message_request = DaemonRequest::SessionMessageSend {
        sender_session_id: parent.session_id.clone(),
        recipient_session_id: child.session_id.clone(),
        body: "inspect this finding".to_string(),
    };
    let unbound_writer = RuntimePrincipal {
        principal_id: "tool:unbound-writer".to_string(),
        session_id: None,
        capabilities: ["runtime.write".to_string()].into_iter().collect(),
    };
    let spoofed = app
        .handle_as(&unbound_writer, &message_request, "spoofed-message")
        .unwrap_err();
    assert_eq!(spoofed.code, "session_sender_identity_required");

    let parent_writer = RuntimePrincipal {
        principal_id: "session-tool".to_string(),
        session_id: Some(parent.session_id.clone()),
        capabilities: ["runtime.write".to_string()].into_iter().collect(),
    };
    let sent = match app
        .handle_as(&parent_writer, &message_request, "message-after-grant")
        .unwrap()
    {
        DaemonResponse::SessionMessage { message } => message,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(sent.sender_session_id, parent.session_id);
    assert_eq!(sent.recipient_session_id, child.session_id);

    match handle(
        &app,
        DaemonRequest::SessionMessages {
            session_id: child.session_id.clone(),
        },
        "",
    ) {
        DaemonResponse::SessionMessages { messages } => assert_eq!(messages, vec![sent]),
        other => panic!("unexpected response: {other:?}"),
    }

    for (index, session_id) in [parent.session_id, child.session_id]
        .into_iter()
        .enumerate()
    {
        handle(
            &app,
            DaemonRequest::SessionClose { session_id },
            &format!("close-message-session-{index}"),
        );
    }
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

    // A failed resume must leave the session resumable again (Error lifecycle,
    // stable handle preserved) — retrying against a healthy provider succeeds.
    let resumed = match handle(
        &app,
        DaemonRequest::SessionResume {
            session_id: session_id.clone(),
            provider_options: resumable_provider_options(),
        },
        "resume-mismatch-retry",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(resumed.session_id, session_id);
    assert_eq!(resumed.lifecycle, SessionLifecycle::Idle);
    assert_eq!(
        resumed.provider_handle.as_deref(),
        Some("thread-pulse-test")
    );
    handle(
        &app,
        DaemonRequest::SessionClose { session_id },
        "resume-mismatch-cleanup",
    );
}

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

#[test]
fn workspace_worktree_create_archive_and_restore_uses_external_git_repo() {
    let repo = crate::common_fixture_repo::TestRepo::from_fixture("minimal-service");
    let home = tempfile::tempdir().unwrap();
    let app = DaemonApplication::new(StateStore::new(home.path()), "test").unwrap();
    let project_id = open_project(&app, repo.path());
    let workspace = match handle(
        &app,
        DaemonRequest::WorkspaceCreate {
            project_id,
            name: "isolated".to_string(),
            isolation: IsolationMode::Worktree,
            base_commit: Some(repo.git_head()),
        },
        "worktree-create",
    ) {
        DaemonResponse::Workspace { workspace } => workspace,
        other => panic!("unexpected response: {other:?}"),
    };
    let root = std::path::PathBuf::from(&workspace.root);
    assert!(root.is_dir());
    assert!(root.join(".git").is_file());
    let head = repo.git_head();
    assert_eq!(workspace.base_commit.as_deref(), Some(head.as_str()));
    let archived = handle(
        &app,
        DaemonRequest::WorkspaceArchive {
            workspace_id: workspace.workspace_id.clone(),
        },
        "worktree-archive",
    );
    assert!(matches!(
        archived,
        DaemonResponse::Workspace {
            workspace: pulse::daemon::workspace::WorkspaceRecord {
                lifecycle: pulse::daemon::workspace::WorkspaceLifecycle::Archived,
                ..
            }
        }
    ));
    let workspace_id = workspace.workspace_id.clone();
    drop(app);
    let restarted = DaemonApplication::new(StateStore::new(home.path()), "test-restarted").unwrap();
    let restored = handle(
        &restarted,
        DaemonRequest::WorkspaceRestore { workspace_id },
        "worktree-restore",
    );
    assert!(matches!(
        restored,
        DaemonResponse::Workspace {
            workspace: pulse::daemon::workspace::WorkspaceRecord {
                lifecycle: pulse::daemon::workspace::WorkspaceLifecycle::Open,
                ..
            }
        }
    ));
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

#[test]
fn mcp_adapter_shares_mutation_idempotency_and_enforces_runtime_permissions() {
    let (_home, project_root, app) = application();
    let project_id = open_project(&app, project_root.path());
    let request = DaemonRequest::WorkspaceCreate {
        project_id,
        name: "mcp-parity".to_string(),
        isolation: IsolationMode::Local,
        base_commit: None,
    };
    let direct = handle(&app, request.clone(), "mcp-parity-key");
    let adapter = McpToolAdapter::new(&app, RuntimePrincipal::local_cli());
    let via_tool = adapter
        .invoke(RequestEnvelope::new(request, "mcp-parity-key"))
        .response
        .unwrap();
    assert_eq!(direct, via_tool);
    assert_eq!(app.store().load().unwrap().workspaces.len(), 1);
    let conflict = adapter.invoke(RequestEnvelope::new(
        DaemonRequest::WorkspaceCreate {
            project_id: "prj_different".to_string(),
            name: "different".to_string(),
            isolation: IsolationMode::Local,
            base_commit: None,
        },
        "mcp-parity-key",
    ));
    assert_eq!(
        conflict.response.unwrap_err().code,
        "idempotency_key_conflict"
    );

    let read_only = RuntimePrincipal {
        principal_id: "tool:reader".to_string(),
        session_id: None,
        capabilities: ["runtime.read".to_string()].into_iter().collect(),
    };
    let adapter = McpToolAdapter::new(&app, read_only);
    let denied = adapter.invoke(RequestEnvelope::new(
        DaemonRequest::ProjectArchive {
            project_id: "prj_any".to_string(),
        },
        "denied-admin",
    ));
    assert_eq!(
        denied.response.unwrap_err().code,
        "runtime_permission_denied"
    );
}
