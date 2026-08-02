use super::*;

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
