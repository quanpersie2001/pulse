use super::*;

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
