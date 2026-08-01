use pulse::daemon::application::DaemonApplication;
use pulse::daemon::permissions::RuntimePrincipal;
use pulse::daemon::persistence::{FailpointMode, StateStore};
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

#[path = "../graph/assignment_fixture.rs"]
#[allow(dead_code)]
mod assignment_fixture;
#[path = "../common/git.rs"]
mod common_git;
use super::common_fixture_repo;
use assignment_fixture::{bootstrap_repo, setup_ready_ticket, write_policy};
use common_fixture_repo::TestRepo;

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

fn assignment_application() -> (
    TestRepo,
    pulse::JsonGraphStore,
    tempfile::TempDir,
    DaemonApplication,
    String,
    String,
) {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = pulse::JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let home = tempfile::tempdir().unwrap();
    let app = DaemonApplication::new(StateStore::new(home.path()), "test").unwrap();
    let project_id = open_project(&app, repo.path());
    (repo, store, home, app, project_id, ticket_id)
}

fn assignment_start_request(
    project_id: &str,
    ticket_id: &str,
    provider_options: serde_json::Value,
) -> DaemonRequest {
    DaemonRequest::AssignmentStart {
        project_id: project_id.to_string(),
        ticket_id: ticket_id.to_string(),
        actor: "agent:tester".to_string(),
        assignee: "agent:codex-local".to_string(),
        capabilities: vec![
            "repository.inspect".to_string(),
            "source.read".to_string(),
            "source.write".to_string(),
            "test.run".to_string(),
            "workspace.worktree".to_string(),
        ],
        isolation: IsolationMode::Local,
        provider_id: "codex".to_string(),
        provider_options,
        ttl_seconds: 1800,
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

fn high_volume_provider_options() -> serde_json::Value {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fake_codex_high_volume_provider.mjs");
    json!({
        "executable": std::env::var("NODE").unwrap_or_else(|_| "node".to_string()),
        "args": [script.to_string_lossy()]
    })
}

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

#[test]
fn cached_session_replay_is_authorized_before_response_lookup() {
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
        "replay-session-create",
    ) {
        DaemonResponse::Session { session } => session,
        other => panic!("unexpected response: {other:?}"),
    };
    let request = DaemonRequest::SessionShow {
        session_id: session.session_id.clone(),
    };
    app.handle_as(
        &RuntimePrincipal::local_cli(),
        &request,
        "cross-principal-replay",
    )
    .unwrap();
    let other_principal = RuntimePrincipal {
        principal_id: "worker:other".to_string(),
        session_id: Some("ses_other".to_string()),
        capabilities: ["runtime.read".to_string()].into_iter().collect(),
    };
    let error = app
        .handle_as(&other_principal, &request, "cross-principal-replay")
        .unwrap_err();
    assert_eq!(error.code, "session_access_denied");
}

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
    };
    let error = app
        .handle_as(&reviewer, &valid_review, "verify-valid")
        .unwrap_err();
    assert_ne!(error.code, "verification_actor_mismatch");
    assert_ne!(error.code, "verification_self_review_denied");
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

    let mut unsupported = RequestEnvelope::new(
        DaemonRequest::ProjectOpen {
            root: project_root.path().to_string_lossy().to_string(),
        },
        "mcp-unknown-capability",
    );
    unsupported.required_capabilities = vec!["not_a_daemon_capability".to_string()];
    let response = McpToolAdapter::new(&app, RuntimePrincipal::local_cli()).invoke(unsupported);
    assert_eq!(
        response.response.unwrap_err().code,
        "daemon_capability_missing"
    );
    assert_eq!(app.store().load().unwrap().projects.len(), 1);
}

#[test]
fn action_scoped_runtime_roles_cannot_register_or_create_runtime_resources() {
    let (_home, project_root, app) = application();
    let writer = RuntimePrincipal {
        principal_id: "worker:write-only".to_string(),
        session_id: None,
        capabilities: ["runtime.write".to_string()].into_iter().collect(),
    };
    let project_open = app.handle_as(
        &writer,
        &DaemonRequest::ProjectOpen {
            root: project_root.path().to_string_lossy().to_string(),
        },
        "role-project-open",
    );
    assert_eq!(project_open.unwrap_err().code, "runtime_permission_denied");
    let workspace_create = app.handle_as(
        &writer,
        &DaemonRequest::WorkspaceCreate {
            project_id: "prj_missing".to_string(),
            name: "worker-workspace".to_string(),
            isolation: IsolationMode::Local,
            base_commit: None,
        },
        "role-workspace-create",
    );
    assert_eq!(
        workspace_create.unwrap_err().code,
        "runtime_permission_denied"
    );
    let session_create = app.handle_as(
        &writer,
        &DaemonRequest::SessionCreate {
            workspace_id: "wks_missing".to_string(),
            provider_id: "codex".to_string(),
            parent_session_id: None,
            provider_options: json!({}),
        },
        "role-session-create",
    );
    assert_eq!(
        session_create.unwrap_err().code,
        "runtime_permission_denied"
    );
    assert!(app.store().load().unwrap().projects.is_empty());
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
    std::thread::sleep(Duration::from_millis(100));
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

#[cfg(unix)]
#[test]
fn acknowledged_worktree_ledger_failure_blocks_retry_with_owned_path_visible() {
    let home = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(["-C", project_root.path().to_str().unwrap()])
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git command failed: {args:?}");
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "pulse@test.invalid"]);
    run(&["config", "user.name", "Pulse Test"]);
    std::fs::write(project_root.path().join("README"), b"fixture").unwrap();
    run(&["add", "README"]);
    run(&["commit", "-qm", "initial"]);
    let app = DaemonApplication::new(StateStore::new(home.path()), "test").unwrap();
    let project_id = open_project(&app, project_root.path());
    app.store()
        .arm_failpoint("before_workspace_ledger_commit", FailpointMode::Error)
        .unwrap();
    let request = DaemonRequest::WorkspaceCreate {
        project_id,
        name: "owned-worktree".to_string(),
        isolation: IsolationMode::Worktree,
        base_commit: None,
    };
    let error = app.handle(&request, "worktree-ledger-failure").unwrap_err();
    assert_eq!(error.code, "injected_failpoint");
    let state = app.store().load().unwrap();
    let effect = state
        .external_effects
        .values()
        .find(|effect| {
            effect.kind == pulse::daemon::persistence::ExternalEffectKind::WorktreeCreate
        })
        .expect("worktree effect");
    assert_eq!(
        effect.state,
        pulse::daemon::persistence::ExternalEffectState::OutcomeUnknown
    );
    let root = effect
        .resource_id
        .as_ref()
        .expect("worktree resource identity");
    assert!(std::path::Path::new(root).is_dir());
    app.store()
        .disarm_failpoint("before_workspace_ledger_commit", FailpointMode::Error)
        .unwrap();
    let retry = app.handle(&request, "worktree-ledger-failure").unwrap_err();
    assert_eq!(retry.code, "external_effect_reconciliation_required");
}

#[cfg(unix)]
#[test]
fn worktree_success_before_ack_is_attempting_and_blocks_retry() {
    let home = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(["-C", project_root.path().to_str().unwrap()])
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git command failed: {args:?}");
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "pulse@test.invalid"]);
    run(&["config", "user.name", "Pulse Test"]);
    std::fs::write(project_root.path().join("README"), b"fixture").unwrap();
    run(&["add", "README"]);
    run(&["commit", "-qm", "initial"]);
    let app = DaemonApplication::new(StateStore::new(home.path()), "test").unwrap();
    let project_id = open_project(&app, project_root.path());
    app.store()
        .arm_failpoint("after_worktree_success_before_ack", FailpointMode::Error)
        .unwrap();
    let request = DaemonRequest::WorkspaceCreate {
        project_id,
        name: "attempting-worktree".to_string(),
        isolation: IsolationMode::Worktree,
        base_commit: None,
    };
    let error = app.handle(&request, "worktree-attempting").unwrap_err();
    assert_eq!(error.code, "injected_failpoint");
    let state = app.store().load().unwrap();
    let effect = state
        .external_effects
        .values()
        .find(|effect| {
            effect.kind == pulse::daemon::persistence::ExternalEffectKind::WorktreeCreate
        })
        .expect("worktree effect");
    assert_eq!(
        effect.state,
        pulse::daemon::persistence::ExternalEffectState::Attempting
    );
    assert!(effect
        .resource_id
        .as_ref()
        .is_some_and(|root| { std::path::Path::new(root).is_dir() }));
    app.store()
        .disarm_failpoint("after_worktree_success_before_ack", FailpointMode::Error)
        .unwrap();
    let retry = app.handle(&request, "worktree-attempting").unwrap_err();
    assert_eq!(retry.code, "external_effect_reconciliation_required");
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
