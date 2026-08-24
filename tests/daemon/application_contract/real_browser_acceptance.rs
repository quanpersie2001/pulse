use super::*;

use pulse::evidence::model::{ReceiptPayload, ReceiptResult};
use pulse::execution::CloseStoryArgs;
use pulse::graph::edge::EdgeType;
use pulse::graph::node::{Node, NodeStatus};
use std::process::Command;

#[test]
#[ignore = "explicit acceptance: installs pinned Playwright and a Chromium binary"]
fn real_browser_story_qualification_binds_deployment_trace_and_close_replay() {
    let mismatch = prepared_playwright_target();
    std::fs::create_dir_all(mismatch.repo.path().join(".pulse/runtime")).unwrap();
    std::fs::write(
        mismatch
            .repo
            .path()
            .join(".pulse/runtime/force-build-mismatch"),
        b"mismatch\n",
    )
    .unwrap();
    let mismatch_receipt = run_qualification(&mismatch, "real-browser-mismatch");
    assert_eq!(mismatch_receipt.result, ReceiptResult::Inconclusive);
    let ReceiptPayload::QaCheckpoint(mismatch_payload) = &mismatch_receipt.payload else {
        panic!("expected QA qualification payload");
    };
    assert!(mismatch_payload.browser.is_none());
    assert!(mismatch_payload
        .observations
        .iter()
        .any(|observation| observation.contains("source/build/deployment mismatch")));
    assert!(mismatch
        .repo
        .path()
        .join(".pulse/runtime/real-browser/cleanup.log")
        .is_file());

    let target = prepared_playwright_target();
    install_playwright(&target.repo);
    let browsers_path = target.repo.path().join(".pulse/runtime/ms-playwright");
    let previous_browsers_path = std::env::var_os("PLAYWRIGHT_BROWSERS_PATH");
    std::env::set_var("PLAYWRIGHT_BROWSERS_PATH", &browsers_path);

    let receipt = run_qualification(&target, "real-browser-happy");
    assert_eq!(receipt.result, ReceiptResult::Passed);
    let ReceiptPayload::QaCheckpoint(payload) = &receipt.payload else {
        panic!("expected QA qualification payload");
    };
    assert_eq!(payload.payload_version, 4);
    assert_eq!(payload.qa_scope, pulse::qa::QaExecutionScope::StoryClose);
    assert_eq!(
        payload.qualification.as_ref().unwrap().matrix_entry_id,
        "default"
    );
    let lifecycle = payload.environment.lifecycle.as_ref().unwrap();
    assert!(lifecycle.start_passed);
    assert!(lifecycle.healthcheck_passed);
    assert!(lifecycle.reset_passed);
    assert!(lifecycle.cleanup_passed);
    assert_eq!(lifecycle.identity.source_commit, target.source_commit);
    let deployment = lifecycle.identity.deployment.as_ref().unwrap();
    assert!(deployment.build_id.starts_with("sha256:"));
    assert!(deployment.deployment_id.starts_with("local-"));
    assert_eq!(deployment.base_url, "http://127.0.0.1:4173");
    let browser = payload.browser.as_ref().unwrap();
    assert_eq!(browser.deployment.as_ref(), Some(deployment));
    assert!(browser.assertions.iter().all(|assertion| assertion.passed));
    assert!(browser.console_errors.is_empty());
    assert!(browser.network_errors.is_empty());

    let trace = receipt
        .bindings
        .artifacts
        .iter()
        .find(|artifact| artifact.role == "trace")
        .expect("trace artifact binding");
    let trace_bytes = std::fs::read(
        target
            .repo
            .path()
            .join(".pulse/runtime/real-browser/trace.zip"),
    )
    .unwrap();
    assert!(trace_bytes.starts_with(b"PK"));
    assert_eq!(
        pulse::canonical_json::hash_bytes(&trace_bytes),
        trace.sha256
    );
    pulse::evidence::verify_artifact(target.repo.path(), &trace.sha256).unwrap();
    pulse::evidence::verify_receipt(target.repo.path(), &receipt.id, true, None).unwrap();

    let args = CloseStoryArgs {
        story_id: target.story_id.clone(),
        qualification_receipt_ids: vec![receipt.id.clone()],
        actor: "human:tester".to_string(),
        source_commit: target.source_commit.clone(),
        summary: "Real browser qualified the frozen candidate deployment.".to_string(),
        idempotency_key: "real-browser-story-close".to_string(),
    };
    let close = target.graph.close_story(args.clone()).unwrap();
    assert_eq!(close.qualification_receipt_ids, vec![receipt.id]);
    assert_eq!(target.graph.close_story(args).unwrap(), close);
    assert_eq!(
        target.graph.show_node(&target.story_id).unwrap().status,
        NodeStatus::Done
    );
    assert!(target
        .repo
        .path()
        .join(".pulse/runtime/real-browser/cleanup.log")
        .is_file());

    match previous_browsers_path {
        Some(value) => std::env::set_var("PLAYWRIGHT_BROWSERS_PATH", value),
        None => std::env::remove_var("PLAYWRIGHT_BROWSERS_PATH"),
    }
}

struct PlaywrightTarget {
    repo: TestRepo,
    graph: pulse::JsonGraphStore,
    story_id: String,
    ticket_id: String,
    source_commit: String,
}

fn prepared_playwright_target() -> PlaywrightTarget {
    let repo = TestRepo::from_fixture("playwright-service");
    let graph = pulse::JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &graph);
    write_policy(
        repo.path(),
        &[
            "qa.defer_to_story_close",
            "work.assignment.release",
            "work.story.close",
        ],
    );
    let ticket_id = setup_ready_ticket_with_story_qa(repo.path(), &graph);
    let ticket = graph.show_node(&ticket_id).unwrap();
    let story_id = ticket
        .qa
        .as_ref()
        .and_then(|qa| qa.impact.behavioral_owner.as_ref())
        .unwrap()
        .clone();
    graph
        .add_edge(
            EdgeType::Parent,
            ticket_id.clone(),
            story_id.clone(),
            "human:tester".to_string(),
        )
        .unwrap();
    write_browser_baseline(repo.path(), &story_id);
    write_executor_manifest(repo.path());
    set_status(repo.path(), &ticket_id, NodeStatus::Done);
    set_status(repo.path(), &story_id, NodeStatus::Ready);
    let source_commit = common_git::commit_all(repo.path());
    PlaywrightTarget {
        repo,
        graph,
        story_id,
        ticket_id,
        source_commit,
    }
}

fn write_browser_baseline(repo: &std::path::Path, story_id: &str) {
    let path = repo.join(format!("works/{story_id}/qa.md"));
    let baseline = std::fs::read_to_string(&path)
        .unwrap()
        .replace(
            "\"environment_profile\": \"fixture\"",
            "\"environment_profile\": \"local-web\"",
        )
        .replace("\"surface\": \"api\"", "\"surface\": \"web\"")
        .replace(
            "\"required_capabilities\": [\"api\"]",
            "\"required_capabilities\": [\"browser\", \"deterministic-assertion\", \"playwright\"]",
        )
        .replace(
            "\"required_evidence\": []",
            "\"required_evidence\": [\"trace\"]",
        );
    std::fs::write(path, baseline).unwrap();
}

fn write_executor_manifest(repo: &std::path::Path) {
    let path = repo.join(".pulse/qa/executors/browser.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let lifecycle = |phase: &str| {
        serde_json::json!({
            "executable": "scripts/qa-environment.mjs",
            "args": [phase],
            "timeout_seconds": 30,
            "max_output_bytes": 16384
        })
    };
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": 1,
            "id": "browser",
            "version": "1.0.0",
            "kind": "playwright",
            "executable": "scripts/qa-playwright.mjs",
            "args": [],
            "timeout_seconds": 60,
            "max_output_bytes": 65536,
            "capabilities": ["browser", "deterministic-assertion", "playwright"],
            "environment_profile": "local-web",
            "fixture_revision": "playwright-service-1",
            "environment": {
                "start": lifecycle("start"),
                "healthcheck": lifecycle("healthcheck"),
                "reset": lifecycle("reset"),
                "cleanup": lifecycle("cleanup")
            },
            "browser": {
                "engine": "chromium",
                "base_url": "http://127.0.0.1:4173",
                "trace_role": "trace"
            }
        }))
        .unwrap(),
    )
    .unwrap();
}

fn set_status(repo: &std::path::Path, id: &str, status: NodeStatus) {
    let path = repo
        .join(".pulse/workgraph/nodes")
        .join(format!("{id}.json"));
    let mut node: Node = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    node.status = status;
    node.status_reason = None;
    node.revision += 1;
    node.updated_at = chrono::Utc::now();
    std::fs::write(
        path,
        pulse::canonical_json::to_canonical_bytes(&node).unwrap(),
    )
    .unwrap();
}

fn run_qualification(
    target: &PlaywrightTarget,
    request_id: &str,
) -> pulse::evidence::model::ReceiptEnvelope {
    let home = tempfile::tempdir().unwrap();
    let app = DaemonApplication::new(StateStore::new(home.path()), "test").unwrap();
    let project_id = open_project(&app, target.repo.path());
    let workspace_id = create_workspace(&app, &project_id);
    let saga_id = format!("saga-{request_id}");
    insert_verifying_assignment(
        &app,
        &saga_id,
        &project_id,
        &workspace_id,
        &target.ticket_id,
        target.graph.show_node(&target.ticket_id).unwrap().revision,
    );
    match handle(
        &app,
        DaemonRequest::QaStoryQualificationRun {
            saga_id,
            story_id: target.story_id.clone(),
            actor: "human:qa-reviewer".to_string(),
            source_commit: target.source_commit.clone(),
            executor_id: "browser".to_string(),
            matrix_entry_id: "default".to_string(),
            retry_of: None,
            waiver_reason: None,
        },
        request_id,
    ) {
        DaemonResponse::QaCheckpoint { receipt } => *receipt,
        other => panic!("unexpected response: {other:?}"),
    }
}

fn insert_verifying_assignment(
    app: &DaemonApplication,
    saga_id: &str,
    project_id: &str,
    workspace_id: &str,
    ticket_id: &str,
    ticket_revision: u64,
) {
    let now = chrono::Utc::now().to_rfc3339();
    app.store()
        .with_state(true, |state| {
            state.assignment_sagas.insert(
                saga_id.to_string(),
                pulse::daemon::assignment::AssignmentSagaRecord {
                    schema_version: 1,
                    saga_id: saga_id.to_string(),
                    idempotency_key: format!("{saga_id}-assignment"),
                    request_fingerprint: format!("{saga_id}-assignment"),
                    project_id: project_id.to_string(),
                    ticket_id: ticket_id.to_string(),
                    actor: "agent:worker".to_string(),
                    assignee: "agent:worker".to_string(),
                    ticket_revision,
                    packet_fingerprint: "packet".to_string(),
                    lease_id: Some("lease".to_string()),
                    workspace_id: Some(workspace_id.to_string()),
                    session_id: Some("session".to_string()),
                    delivery_id: None,
                    acknowledgement_id: None,
                    handoff_id: Some("handoff".to_string()),
                    verification_id: None,
                    qa_checkpoint_receipt_ids: Vec::new(),
                    state: pulse::daemon::assignment::AssignmentSagaState::Verifying,
                    last_error: None,
                    created_at: now.clone(),
                    updated_at: now,
                },
            );
            Ok(())
        })
        .unwrap();
}

fn install_playwright(repo: &TestRepo) {
    let npm_cache = repo.path().join(".pulse/runtime/npm-cache");
    let npm = Command::new("npm")
        .current_dir(repo.path())
        .args(["ci", "--ignore-scripts", "--cache"])
        .arg(&npm_cache)
        .output()
        .expect("run npm ci in external target copy");
    assert_command_success("npm ci", npm);

    let browsers_path = repo.path().join(".pulse/runtime/ms-playwright");
    let install = Command::new(repo.path().join("node_modules/.bin/playwright"))
        .current_dir(repo.path())
        .env("PLAYWRIGHT_BROWSERS_PATH", &browsers_path)
        .args(["install", "chromium"])
        .output()
        .expect("install Playwright Chromium in external target copy");
    assert_command_success("playwright install chromium", install);
}

fn assert_command_success(label: &str, output: std::process::Output) {
    assert!(
        output.status.success(),
        "{label} failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
