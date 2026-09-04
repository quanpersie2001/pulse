use pulse::evidence::model::{
    ActorKind, ActorRef, ContentBinding, ReceiptBindings, ReceiptEnvelope, ReceiptKind,
    ReceiptPayload, ReceiptResult, SourceBinding, SubjectRef,
};
use pulse::execution::CloseStoryArgs;
use pulse::graph::edge::EdgeType;
use pulse::graph::node::{Node, NodeStatus};
use pulse::qa::{
    QaCaseObservation, QaCaseOutcome, QaCheckpointPayload, QaExecutionScope, QaExecutor,
};
use pulse::storage::transaction::TransactionFailpoint;
use pulse::JsonGraphStore;

use super::assignment_fixture::{bootstrap_repo, setup_ready_ticket_with_story_qa, write_policy};
use super::common_fixture_repo::TestRepo;
use super::common_git::commit_all;

#[test]
fn ready_story_closes_with_terminal_children_and_full_current_qualification() {
    let (repo, store, story_id, ticket_id, source_commit) = ready_story_fixture(true);
    let qualification_receipt_id = record_story_qualification(
        repo.path(),
        &story_id,
        &ticket_id,
        &source_commit,
        "rcpt_01J00000000000000000000020",
        ReceiptResult::Passed,
        QaCaseOutcome::Passed,
    );
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
    let qualification_receipt_id = record_story_qualification(
        repo.path(),
        &story_id,
        &ticket_id,
        &source_commit,
        "rcpt_01J00000000000000000000020",
        ReceiptResult::Passed,
        QaCaseOutcome::Passed,
    );
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
    let qualification_receipt_id = record_story_qualification(
        repo.path(),
        &story_id,
        &ticket_id,
        &source_commit,
        "rcpt_01J00000000000000000000020",
        ReceiptResult::Passed,
        QaCaseOutcome::Passed,
    );
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
fn story_close_rejects_qualification_that_is_not_passed() {
    let (repo, store, story_id, ticket_id, source_commit) = ready_story_fixture(true);
    let qualification_receipt_id = record_story_qualification(
        repo.path(),
        &story_id,
        &ticket_id,
        &source_commit,
        "rcpt_01J00000000000000000000021",
        ReceiptResult::Failed,
        QaCaseOutcome::ProductFailure,
    );
    let error = store
        .close_story(CloseStoryArgs {
            story_id: story_id.clone(),
            qualification_receipt_ids: vec![qualification_receipt_id],
            actor: "human:tester".to_string(),
            source_commit,
            summary: "Attempt close on a failing qualification.".to_string(),
            idempotency_key: "close-story-failed-qualification".to_string(),
        })
        .unwrap_err();

    assert_eq!(error.code(), "story_close_qualification_invalid");
    assert_eq!(
        store.show_node(&story_id).unwrap().status,
        NodeStatus::Ready
    );
}

#[test]
fn story_close_requires_a_single_qualification_receipt() {
    let (repo, store, story_id, ticket_id, source_commit) = ready_story_fixture(true);
    let first = record_story_qualification(
        repo.path(),
        &story_id,
        &ticket_id,
        &source_commit,
        "rcpt_01J00000000000000000000022",
        ReceiptResult::Passed,
        QaCaseOutcome::Passed,
    );
    let second = record_story_qualification(
        repo.path(),
        &story_id,
        &ticket_id,
        &source_commit,
        "rcpt_01J00000000000000000000023",
        ReceiptResult::Passed,
        QaCaseOutcome::Passed,
    );
    let error = store
        .close_story(CloseStoryArgs {
            story_id: story_id.clone(),
            qualification_receipt_ids: vec![first, second],
            actor: "human:tester".to_string(),
            source_commit,
            summary: "Attempt close with multiple qualification heads.".to_string(),
            idempotency_key: "close-story-multiple-qualifications".to_string(),
        })
        .unwrap_err();

    assert_eq!(error.code(), "story_close_qualification_required");
    assert_eq!(
        store.show_node(&story_id).unwrap().status,
        NodeStatus::Ready
    );
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
    receipt_id: &str,
    result: ReceiptResult,
    outcome: QaCaseOutcome,
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
            observations: vec!["Full integrated Story baseline passed.".to_string()],
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
