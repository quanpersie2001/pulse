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
use std::sync::Arc;
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
        "executable": "/bin/sh",
        "args": ["-c", "cat"],
        "protocol_mode": "opaque_test"
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

#[test]
fn mcp_adapter_shares_semantics_and_enforces_runtime_permissions() {
    let (_home, project_root, app) = application();
    open_project(&app, project_root.path());
    let request = DaemonRequest::ProjectList {
        include_archived: false,
    };
    let direct = handle(&app, request.clone(), "");
    let adapter = McpToolAdapter::new(&app, RuntimePrincipal::local_cli());
    let via_tool = adapter
        .invoke(RequestEnvelope::new(request, ""))
        .response
        .unwrap();
    assert_eq!(direct, via_tool);

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
