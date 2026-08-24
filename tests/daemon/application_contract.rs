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
use assignment_fixture::{
    bootstrap_repo, setup_ready_ticket, setup_ready_ticket_with_required_qa, write_policy,
};
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
#[cfg(windows)]
fn provider_options() -> serde_json::Value {
    json!({
        "executable": "C:\\Windows\\System32\\cmd.exe",
        "args": ["/Q", "/K", "more"],
        "protocol_mode": "opaque_test"
    })
}

#[path = "application_contract/assignment_contract.rs"]
mod assignment_contract;
#[path = "application_contract/dispatch_contract.rs"]
mod dispatch_contract;
#[path = "application_contract/project_workspace_contract.rs"]
mod project_workspace_contract;
#[path = "application_contract/recovery_contract.rs"]
mod recovery_contract;
#[path = "application_contract/session_lifecycle_contract.rs"]
mod session_lifecycle_contract;
#[path = "application_contract/turn_timeline_contract.rs"]
mod turn_timeline_contract;
