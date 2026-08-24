use pulse::evidence::model::{
    ActorKind, ActorRef, ContentBinding, ReceiptBindings, ReceiptEnvelope, ReceiptKind,
    ReceiptPayload, ReceiptResult, SourceBinding, SubjectRef,
};
use pulse::execution::CloseStoryArgs;
use pulse::graph::edge::EdgeType;
use pulse::graph::node::{Node, NodeStatus};
use pulse::qa::{
    QaBrowserAssertion, QaBrowserEngine, QaBrowserReport, QaCaseObservation, QaCaseOutcome,
    QaCheckpointPayload, QaDeploymentIdentity, QaEnvironmentIdentity, QaEnvironmentLifecycle,
    QaExecutionScope, QaExecutor, QaFlakyWaiver, QaQualificationContext, QaRuntimeEnvironment,
};
use pulse::storage::transaction::TransactionFailpoint;
use pulse::JsonGraphStore;

use super::assignment_fixture::{bootstrap_repo, setup_ready_ticket_with_story_qa, write_policy};
use super::common_fixture_repo::TestRepo;
use super::common_git::commit_all;

#[test]
fn ready_story_closes_with_terminal_children_and_full_current_qualification() {
    let (repo, store, story_id, ticket_id, source_commit) = ready_story_fixture(true);
    let qualification_receipt_id =
        record_story_qualification(repo.path(), &story_id, &ticket_id, &source_commit);
    let args = CloseStoryArgs {
        story_id: story_id.clone(),
        qualification_receipt_ids: vec![qualification_receipt_id.clone()],
        actor: "human:tester".to_string(),
        source_commit,
        summary: "Integrated Story outcome is fully qualified.".to_string(),
        idempotency_key: "close-story-happy".to_string(),
    };

    let crashing =
        JsonGraphStore::with_failpoint(repo.path(), TransactionFailpoint::AfterMultiTargetAll);
    assert!(crashing.close_story(args.clone()).is_err());
    let close = store.close_story(args.clone()).unwrap();
    assert_eq!(close.story_id, story_id);
    assert_eq!(
        close.qualification_receipt_ids,
        vec![qualification_receipt_id]
    );
    assert_eq!(close.done_ticket_ids, vec![ticket_id]);
    assert!(close.superseded_ticket_ids.is_empty());
    assert_eq!(store.show_node(&story_id).unwrap().status, NodeStatus::Done);
    assert_eq!(store.close_story(args).unwrap(), close);
    assert_eq!(
        pulse::kernel::story_completion::load_story_close(repo.path(), &close.close_id).unwrap(),
        close
    );
}

#[test]
fn story_close_rejects_nonterminal_child_before_mutation() {
    let (repo, store, story_id, ticket_id, source_commit) = ready_story_fixture(false);
    let qualification_receipt_id =
        record_story_qualification(repo.path(), &story_id, &ticket_id, &source_commit);
    let error = store
        .close_story(CloseStoryArgs {
            story_id: story_id.clone(),
            qualification_receipt_ids: vec![qualification_receipt_id],
            actor: "human:tester".to_string(),
            source_commit,
            summary: "Attempt close before child completion.".to_string(),
            idempotency_key: "close-story-open-child".to_string(),
        })
        .unwrap_err();

    assert_eq!(error.code(), "story_close_children_incomplete");
    assert_eq!(
        store.show_node(&story_id).unwrap().status,
        NodeStatus::Ready
    );
}

#[test]
fn story_close_rejects_tampered_qualification_before_mutation() {
    let (repo, store, story_id, ticket_id, source_commit) = ready_story_fixture(true);
    let qualification_receipt_id =
        record_story_qualification(repo.path(), &story_id, &ticket_id, &source_commit);
    let receipt_path = repo
        .path()
        .join(".pulse/evidence/receipts")
        .join(format!("{qualification_receipt_id}.json"));
    let mut bytes = std::fs::read(&receipt_path).unwrap();
    bytes.extend_from_slice(b" \n");
    std::fs::write(receipt_path, bytes).unwrap();

    let error = store
        .close_story(CloseStoryArgs {
            story_id: story_id.clone(),
            qualification_receipt_ids: vec![qualification_receipt_id],
            actor: "human:tester".to_string(),
            source_commit,
            summary: "Attempt close with tampered proof.".to_string(),
            idempotency_key: "close-story-tampered-proof".to_string(),
        })
        .unwrap_err();

    assert_eq!(error.code(), "story_close_qualification_stale");
    assert_eq!(
        store.show_node(&story_id).unwrap().status,
        NodeStatus::Ready
    );
}

#[test]
fn story_close_rejects_browser_receipt_outside_current_deployment_contract() {
    let (repo, store, story_id, ticket_id, _) = ready_story_fixture(true);
    let baseline_path = repo.path().join(format!("works/{story_id}/qa.md"));
    let baseline = std::fs::read_to_string(&baseline_path)
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
    std::fs::write(&baseline_path, baseline).unwrap();
    let executor_path = repo.path().join(".pulse/qa/executors/browser.json");
    std::fs::create_dir_all(executor_path.parent().unwrap()).unwrap();
    std::fs::write(
        &executor_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": 1,
            "id": "browser",
            "version": "1.0.0",
            "kind": "playwright",
            "executable": "scripts/qa-playwright.mjs",
            "args": [],
            "timeout_seconds": 30,
            "max_output_bytes": 16384,
            "capabilities": ["browser", "deterministic-assertion", "playwright"],
            "environment_profile": "local-web",
            "fixture_revision": "minimal-service-1",
            "environment": {
                "start": environment_command("start"),
                "healthcheck": environment_command("healthcheck"),
                "reset": environment_command("reset"),
                "cleanup": environment_command("cleanup")
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
    let source_commit = commit_all(repo.path());
    let receipt_id = record_deployment_mismatched_browser_qualification(
        repo.path(),
        &story_id,
        &ticket_id,
        &source_commit,
    );
    let report = pulse::evidence::verify_receipt(repo.path(), &receipt_id, true, None).unwrap();
    assert_eq!(report.integrity.status, "valid");
    assert_eq!(report.bindings.status, "current");

    let error = store
        .close_story(CloseStoryArgs {
            story_id: story_id.clone(),
            qualification_receipt_ids: vec![receipt_id],
            actor: "human:tester".to_string(),
            source_commit,
            summary: "Reject a deployment outside the current browser contract.".to_string(),
            idempotency_key: "close-story-deployment-stale".to_string(),
        })
        .unwrap_err();

    assert_eq!(error.code(), "qa_deployment_binding_stale");
    assert_eq!(
        store.show_node(&story_id).unwrap().status,
        NodeStatus::Ready
    );
}

#[test]
fn story_close_requires_every_declared_matrix_entry() {
    let (repo, store, story_id, ticket_id, _) = ready_story_fixture(true);
    let baseline_path = repo.path().join(format!("works/{story_id}/qa.md"));
    let baseline = std::fs::read_to_string(&baseline_path).unwrap().replace(
        "\"matrix\": [{",
        "\"matrix\": [{\"id\":\"secondary\",\"environment_profile\":\"secondary\",\"platform\":\"any\",\"case_ids\":[\"QA-001\"]},{",
    );
    std::fs::write(&baseline_path, baseline).unwrap();
    let source_commit = commit_all(repo.path());
    let qualification_receipt_id =
        record_story_qualification(repo.path(), &story_id, &ticket_id, &source_commit);

    let error = store
        .close_story(CloseStoryArgs {
            story_id: story_id.clone(),
            qualification_receipt_ids: vec![qualification_receipt_id],
            actor: "human:tester".to_string(),
            source_commit,
            summary: "Missing secondary platform proof.".to_string(),
            idempotency_key: "close-story-matrix-incomplete".to_string(),
        })
        .unwrap_err();

    assert_eq!(error.code(), "story_close_matrix_incomplete");
    assert_eq!(
        store.show_node(&story_id).unwrap().status,
        NodeStatus::Ready
    );
}

#[test]
fn retry_pass_remains_flaky_without_an_authorized_waiver() {
    let (repo, store, story_id, ticket_id, source_commit) = ready_story_fixture(true);
    let failed = record_story_qualification_attempt(
        repo.path(),
        &story_id,
        &ticket_id,
        &source_commit,
        "rcpt_01J00000000000000000000021",
        ReceiptResult::Failed,
        QaCaseOutcome::ProductFailure,
        1,
        None,
        None,
    );
    let passed = record_story_qualification_attempt(
        repo.path(),
        &story_id,
        &ticket_id,
        &source_commit,
        "rcpt_01J00000000000000000000022",
        ReceiptResult::Passed,
        QaCaseOutcome::Passed,
        2,
        Some(&failed),
        None,
    );

    let error = store
        .close_story(CloseStoryArgs {
            story_id: story_id.clone(),
            qualification_receipt_ids: vec![passed],
            actor: "human:tester".to_string(),
            source_commit,
            summary: "Retry passed without flaky disposition.".to_string(),
            idempotency_key: "close-story-flaky-unwaived".to_string(),
        })
        .unwrap_err();

    assert_eq!(error.code(), "story_close_qualification_flaky");
    assert_eq!(
        store.show_node(&story_id).unwrap().status,
        NodeStatus::Ready
    );
}

#[test]
fn authorized_policy_bound_flaky_waiver_allows_retry_chain() {
    let (repo, store, story_id, ticket_id, _) = ready_story_fixture(true);
    write_policy(
        repo.path(),
        &[
            "qa.defer_to_story_close",
            "qa.flaky.waive",
            "work.story.close",
        ],
    );
    let source_commit = commit_all(repo.path());
    let report = pulse::policy::load_authority_policy(repo.path()).unwrap();
    let waiver = QaFlakyWaiver {
        rationale: "Known deterministic harness race is accepted for this candidate.".to_string(),
        approved_by: pulse::policy::parse_actor("human:tester"),
        policy_revision: report.policy_revision.unwrap(),
        policy_fingerprint: report.fingerprint.unwrap(),
    };
    let failed = record_story_qualification_attempt(
        repo.path(),
        &story_id,
        &ticket_id,
        &source_commit,
        "rcpt_01J00000000000000000000023",
        ReceiptResult::Inconclusive,
        QaCaseOutcome::InfrastructureFailure,
        1,
        None,
        None,
    );
    let passed = record_story_qualification_attempt(
        repo.path(),
        &story_id,
        &ticket_id,
        &source_commit,
        "rcpt_01J00000000000000000000024",
        ReceiptResult::Passed,
        QaCaseOutcome::Passed,
        2,
        Some(&failed),
        Some(waiver),
    );

    let close = store
        .close_story(CloseStoryArgs {
            story_id: story_id.clone(),
            qualification_receipt_ids: vec![passed],
            actor: "human:tester".to_string(),
            source_commit,
            summary: "Retry chain reviewed and explicitly waived.".to_string(),
            idempotency_key: "close-story-flaky-waived".to_string(),
        })
        .unwrap();

    assert_eq!(close.story_id, story_id);
    assert_eq!(store.show_node(&story_id).unwrap().status, NodeStatus::Done);
}

#[test]
fn non_applicable_case_requires_an_authorized_policy_bound_approval() {
    let (repo, _, story_id, _, _) = ready_story_fixture(true);
    let report = pulse::policy::load_authority_policy(repo.path()).unwrap();
    let policy_revision = report.policy_revision.unwrap();
    let policy_fingerprint = report.fingerprint.unwrap();
    let baseline_path = repo.path().join(format!("works/{story_id}/qa.md"));
    let matrix = concat!(
        "  \"matrix\": [{\n",
        "    \"id\": \"default\",\n",
        "    \"environment_profile\": \"fixture\",\n",
        "    \"platform\": \"any\",\n",
        "    \"case_ids\": [\"QA-001\"]\n",
        "  }],\n"
    );
    let approval = format!(
        concat!(
            "\"applicability\": \"not_applicable\",\n",
            "    \"non_applicable_reason\": \"Unsupported deployment profile.\",\n",
            "    \"non_applicable_approval\": {{",
            "\"actor\":{{\"kind\":\"human\",\"id\":\"tester\"}},",
            "\"rationale\":\"Unsupported deployment profile.\",",
            "\"policy_revision\":{},",
            "\"policy_fingerprint\":\"{}\"}}"
        ),
        policy_revision, policy_fingerprint
    );
    let baseline = std::fs::read_to_string(&baseline_path)
        .unwrap()
        .replace(matrix, "")
        .replace("\"applicability\": \"required\"", &approval);
    std::fs::write(&baseline_path, baseline).unwrap();

    let error = pulse::qa::load_story_baseline(repo.path(), &story_id).unwrap_err();
    assert_eq!(error.code(), "readiness_authority_denied");

    write_policy(
        repo.path(),
        &[
            "qa.defer_to_story_close",
            "qa.non_applicable.approve",
            "work.story.close",
        ],
    );
    let authorized = pulse::policy::load_authority_policy(repo.path()).unwrap();
    let baseline = std::fs::read_to_string(&baseline_path).unwrap().replace(
        &policy_fingerprint,
        authorized.fingerprint.as_deref().unwrap(),
    );
    std::fs::write(&baseline_path, baseline).unwrap();
    pulse::qa::load_story_baseline(repo.path(), &story_id).unwrap();
}

#[test]
fn stale_flaky_waiver_keeps_historical_receipt_integrity() {
    let (repo, _, story_id, ticket_id, _) = ready_story_fixture(true);
    write_policy(
        repo.path(),
        &[
            "qa.defer_to_story_close",
            "qa.flaky.waive",
            "work.story.close",
        ],
    );
    let source_commit = commit_all(repo.path());
    let report = pulse::policy::load_authority_policy(repo.path()).unwrap();
    let waiver = QaFlakyWaiver {
        rationale: "Time-bounded harness instability acceptance.".to_string(),
        approved_by: pulse::policy::parse_actor("human:tester"),
        policy_revision: report.policy_revision.unwrap(),
        policy_fingerprint: report.fingerprint.unwrap(),
    };
    let failed = record_story_qualification_attempt(
        repo.path(),
        &story_id,
        &ticket_id,
        &source_commit,
        "rcpt_01J00000000000000000000025",
        ReceiptResult::Failed,
        QaCaseOutcome::ProductFailure,
        1,
        None,
        None,
    );
    let passed = record_story_qualification_attempt(
        repo.path(),
        &story_id,
        &ticket_id,
        &source_commit,
        "rcpt_01J00000000000000000000026",
        ReceiptResult::Passed,
        QaCaseOutcome::Passed,
        2,
        Some(&failed),
        Some(waiver),
    );
    write_policy(
        repo.path(),
        &["qa.defer_to_story_close", "work.story.close"],
    );

    let verification = pulse::evidence::verify_receipt(repo.path(), &passed, true, None).unwrap();
    assert_eq!(verification.integrity.status, "valid");
}

fn ready_story_fixture(child_done: bool) -> (TestRepo, JsonGraphStore, String, String, String) {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(
        repo.path(),
        &["qa.defer_to_story_close", "work.story.close"],
    );
    let ticket_id = setup_ready_ticket_with_story_qa(repo.path(), &store);
    let ticket = store.show_node(&ticket_id).unwrap();
    let story_id = ticket
        .qa
        .as_ref()
        .and_then(|qa| qa.impact.behavioral_owner.as_ref())
        .unwrap()
        .clone();
    store
        .add_edge(
            EdgeType::Parent,
            ticket_id.clone(),
            story_id.clone(),
            "human:tester".to_string(),
        )
        .unwrap();
    set_status(repo.path(), &story_id, NodeStatus::Ready);
    if child_done {
        set_status(repo.path(), &ticket_id, NodeStatus::Done);
    }
    let source_commit = commit_all(repo.path());
    (repo, store, story_id, ticket_id, source_commit)
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

fn record_story_qualification(
    repo: &std::path::Path,
    story_id: &str,
    ticket_id: &str,
    source_commit: &str,
) -> String {
    record_story_qualification_attempt(
        repo,
        story_id,
        ticket_id,
        source_commit,
        "rcpt_01J00000000000000000000020",
        ReceiptResult::Passed,
        QaCaseOutcome::Passed,
        1,
        None,
        None,
    )
}

fn record_deployment_mismatched_browser_qualification(
    repo: &std::path::Path,
    story_id: &str,
    ticket_id: &str,
    source_commit: &str,
) -> String {
    let resolution = pulse::qa::resolve_story_cases(repo, story_id).unwrap();
    let evidence = pulse::evidence::manifest::load(repo).unwrap();
    let trace_path = repo.join(".pulse/runtime/deployment-mismatch-trace.zip");
    std::fs::create_dir_all(trace_path.parent().unwrap()).unwrap();
    std::fs::write(&trace_path, b"PK\x03\x04contract").unwrap();
    let artifact = pulse::evidence::put_artifact(
        repo,
        None,
        &trace_path,
        "playwright_trace".to_string(),
        Some("application/zip".to_string()),
        None,
        evidence.max_artifact_bytes,
    )
    .unwrap();
    let deployment = QaDeploymentIdentity {
        build_id: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_string(),
        deployment_id: "deployment-outside-contract".to_string(),
        base_url: "http://127.0.0.1:9999".to_string(),
    };
    let manifest_path = ".pulse/qa/executors/browser.json";
    let receipt_id = "rcpt_01J00000000000000000000027".to_string();
    let receipt = ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 2,
        id: receipt_id.clone(),
        kind: ReceiptKind::QaCheckpoint,
        result: ReceiptResult::Passed,
        actor: ActorRef {
            kind: ActorKind::Human,
            id: "qa-reviewer".to_string(),
        },
        recorded_at: chrono::Utc::now(),
        subject: SubjectRef {
            kind: "work".to_string(),
            id: story_id.to_string(),
        },
        bindings: ReceiptBindings {
            work: vec![],
            source: Some(SourceBinding {
                kind: "git_commit".to_string(),
                commit: source_commit.to_string(),
                repository_id: evidence.repository_id,
            }),
            content: vec![
                ContentBinding {
                    path: resolution.path.clone(),
                    sha256: resolution.content_hash.clone(),
                },
                ContentBinding {
                    path: manifest_path.to_string(),
                    sha256: pulse::canonical_json::hash_bytes(
                        &std::fs::read(repo.join(manifest_path)).unwrap(),
                    ),
                },
            ],
            artifacts: vec![pulse::evidence::model::ArtifactBinding {
                sha256: artifact.artifact.digest,
                role: "trace".to_string(),
            }],
            graph_fingerprint_observed: None,
        },
        payload: ReceiptPayload::QaCheckpoint(QaCheckpointPayload {
            payload_version: 4,
            qa_scope: QaExecutionScope::StoryClose,
            story_id: story_id.to_string(),
            ticket_id: ticket_id.to_string(),
            baseline_revision: resolution.revision,
            baseline_content_hash: resolution.content_hash,
            cases: resolution
                .cases
                .into_iter()
                .map(|case| QaCaseObservation {
                    case_id: case.id,
                    case_revision: case.revision,
                    outcome: QaCaseOutcome::Passed,
                })
                .collect(),
            executor: QaExecutor {
                name: "browser".to_string(),
                version: "1.0.0".to_string(),
                capabilities: vec![
                    "browser".to_string(),
                    "deterministic-assertion".to_string(),
                    "playwright".to_string(),
                ],
            },
            environment: QaRuntimeEnvironment {
                profile: "local-web".to_string(),
                platform: std::env::consts::OS.to_string(),
                fixture_revision: "minimal-service-1".to_string(),
                lifecycle: Some(QaEnvironmentLifecycle {
                    identity: QaEnvironmentIdentity {
                        environment_instance_id: "environment-outside-contract".to_string(),
                        source_commit: source_commit.to_string(),
                        fixture_revision: "minimal-service-1".to_string(),
                        deployment: Some(deployment.clone()),
                    },
                    start_passed: true,
                    healthcheck_passed: true,
                    reset_passed: true,
                    cleanup_passed: true,
                }),
            },
            browser: Some(QaBrowserReport {
                engine: QaBrowserEngine::Chromium,
                base_url: deployment.base_url.clone(),
                trace_role: "trace".to_string(),
                deployment: Some(deployment),
                assertions: vec![QaBrowserAssertion {
                    case_id: "QA-001".to_string(),
                    kind: "visible_state".to_string(),
                    expected: "one stable reservation".to_string(),
                    actual: "one stable reservation".to_string(),
                    passed: true,
                }],
                console_errors: vec![],
                network_errors: vec![],
            }),
            qualification: Some(QaQualificationContext {
                matrix_entry_id: "default".to_string(),
                attempt: 1,
                previous_attempt_receipt_id: None,
                flaky_waiver: None,
            }),
            observations: vec!["Historical browser observation is internally valid.".to_string()],
            cleanup_passed: true,
        }),
    };
    pulse::evidence::record_receipt_envelope(repo, None, receipt).unwrap();
    receipt_id
}

fn environment_command(phase: &str) -> serde_json::Value {
    serde_json::json!({
        "executable": "scripts/qa-environment.mjs",
        "args": [phase],
        "timeout_seconds": 10,
        "max_output_bytes": 16384
    })
}

#[allow(clippy::too_many_arguments)]
fn record_story_qualification_attempt(
    repo: &std::path::Path,
    story_id: &str,
    ticket_id: &str,
    source_commit: &str,
    receipt_id: &str,
    result: ReceiptResult,
    outcome: QaCaseOutcome,
    attempt: u32,
    previous_attempt_receipt_id: Option<&str>,
    flaky_waiver: Option<QaFlakyWaiver>,
) -> String {
    let resolution = pulse::qa::resolve_story_cases(repo, story_id).unwrap();
    let manifest = pulse::evidence::manifest::load(repo).unwrap();
    let receipt_id = receipt_id.to_string();
    let receipt = ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 2,
        id: receipt_id.clone(),
        kind: ReceiptKind::QaCheckpoint,
        result,
        actor: ActorRef {
            kind: ActorKind::Human,
            id: "qa-reviewer".to_string(),
        },
        recorded_at: chrono::Utc::now(),
        subject: SubjectRef {
            kind: "work".to_string(),
            id: story_id.to_string(),
        },
        bindings: ReceiptBindings {
            work: vec![],
            source: Some(SourceBinding {
                kind: "git_commit".to_string(),
                commit: source_commit.to_string(),
                repository_id: manifest.repository_id,
            }),
            content: vec![ContentBinding {
                path: resolution.path,
                sha256: resolution.content_hash.clone(),
            }],
            artifacts: vec![],
            graph_fingerprint_observed: None,
        },
        payload: ReceiptPayload::QaCheckpoint(QaCheckpointPayload {
            payload_version: 1,
            qa_scope: QaExecutionScope::StoryClose,
            story_id: story_id.to_string(),
            ticket_id: ticket_id.to_string(),
            baseline_revision: resolution.revision,
            baseline_content_hash: resolution.content_hash,
            cases: resolution
                .cases
                .into_iter()
                .map(|case| QaCaseObservation {
                    case_id: case.id,
                    case_revision: case.revision,
                    outcome,
                })
                .collect(),
            executor: QaExecutor {
                name: "structured-api".to_string(),
                version: "1.0.0".to_string(),
                capabilities: vec!["api".to_string()],
            },
            environment: QaRuntimeEnvironment {
                profile: "fixture".to_string(),
                platform: std::env::consts::OS.to_string(),
                fixture_revision: "minimal-service-1".to_string(),
                lifecycle: None,
            },
            browser: None,
            qualification: Some(QaQualificationContext {
                matrix_entry_id: "default".to_string(),
                attempt,
                previous_attempt_receipt_id: previous_attempt_receipt_id.map(str::to_string),
                flaky_waiver,
            }),
            observations: vec!["Full integrated Story baseline passed.".to_string()],
            cleanup_passed: true,
        }),
    };
    let input = repo.join("story-close-qualification.json");
    std::fs::write(
        &input,
        pulse::canonical_json::to_canonical_bytes(&receipt).unwrap(),
    )
    .unwrap();
    pulse::evidence::record_receipt(repo, None, &input).unwrap();
    std::fs::remove_file(input).unwrap();
    receipt_id
}
