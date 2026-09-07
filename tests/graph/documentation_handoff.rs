//! Handoff-time documentation gate (Decision 0016).
//!
//! The worker owns recording `documentation_validation`. This gate is what
//! makes that ownership real: a Ticket declaring `Posture: required` cannot
//! hand off without referencing such a receipt, so the omission surfaces at
//! the worker's own step instead of at `close` — after the reviewer has
//! already spent a cycle. Three of six Tickets in Track B round 1 paid that
//! cycle for exactly this gap.

use pulse::execution::SubmitHandoffArgs;
use pulse::graph::model::node::NodeStatus;
use pulse::reservation::{ActivateReservationArgs, AssignmentAcknowledgement, ReserveWorkArgs};
use pulse::JsonGraphStore;

use super::assignment_fixture::{
    bootstrap_repo, setup_ready_ticket, setup_ready_ticket_with_required_docs, write_policy,
};
use super::common_fixture_repo::TestRepo;

/// Reserve, activate and return the lease plus its bound source commit.
fn active_lease(store: &JsonGraphStore, ticket_id: &str, key: &str) -> (String, String) {
    let reserved = store
        .reserve_work(ReserveWorkArgs {
            ticket_id: ticket_id.to_string(),
            actor: "agent:tester".to_string(),
            assignee: "agent:runner:worker".to_string(),
            ttl_seconds: 1800,
            idempotency_key: key.to_string(),
        })
        .unwrap();
    let active = store
        .activate_reservation(ActivateReservationArgs {
            lease_id: reserved.reservation.lease_id.clone(),
            actor: "agent:tester".to_string(),
            runtime_binding: pulse::reservation::RuntimeBinding {
                project_id: "prj_test".to_string(),
                workspace_id: "checkout".to_string(),
                session_id: "ses_test".to_string(),
                provider_id: "runner:worker".to_string(),
            },
            acknowledgement: AssignmentAcknowledgement {
                acknowledgement_id: "ack_test".to_string(),
                delivery_id: "delivery_test".to_string(),
                session_id: "ses_test".to_string(),
                packet_fingerprint: reserved.reservation.packet_fingerprint.clone(),
                acknowledged_at: chrono::Utc::now().to_rfc3339(),
            },
        })
        .unwrap();
    (active.lease_id, active.source.commit)
}

fn handoff_args(
    lease_id: String,
    source_commit: String,
    evidence_receipt_ids: Vec<String>,
    key: &str,
) -> SubmitHandoffArgs {
    SubmitHandoffArgs {
        lease_id,
        actor: "agent:tester".to_string(),
        session_id: "ses_test".to_string(),
        source_commit,
        summary: "Implementation and documentation are ready.".to_string(),
        changed_paths: vec!["docs/domain/reservation.md".to_string()],
        evidence_receipt_ids,
        learning_usage: Vec::new(),
        frictions: Vec::new(),
        checks: Vec::new(),
        acceptance_proofs: Vec::new(),
        idempotency_key: key.to_string(),
    }
}

fn worker_docs_receipt(repo: &TestRepo) -> String {
    pulse::kernel::documentation::run_documentation_validation(
        repo.path(),
        None,
        Some("agent:runner:worker"),
    )
    .unwrap()
    .receipt
    .unwrap()
    .receipt
    .id
}

fn required_docs_repo() -> (TestRepo, JsonGraphStore, String) {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(
        repo.path(),
        &["work.assignment.release", "work.assignment.handoff"],
    );
    let ticket_id = setup_ready_ticket_with_required_docs(&repo, &store);
    (repo, store, ticket_id)
}

#[test]
fn required_docs_handoff_without_a_docs_receipt_is_refused() {
    let (_repo, store, ticket_id) = required_docs_repo();
    let (lease_id, source_commit) = active_lease(&store, &ticket_id, "res-docs-gate-1");

    let error = store
        .submit_execution_handoff(handoff_args(
            lease_id,
            source_commit,
            vec![],
            "handoff-docs-gate-1",
        ))
        .unwrap_err();
    assert_eq!(error.code(), "handoff_documentation_receipt_missing");
    // The message must name the command that fixes it: the worker is the
    // actor who can act on this error, and it only reads the message.
    let message = format!("{error}");
    assert!(
        message.contains("pulse docs validate --record"),
        "message must name the fix: {message}"
    );
    assert!(
        message.contains("--evidence-receipt"),
        "message must name the flag: {message}"
    );

    // The Ticket stays active: a refused handoff moves nothing.
    let node = store.show_node(&ticket_id).unwrap();
    assert_eq!(node.status, NodeStatus::Active);
}

#[test]
fn required_docs_handoff_with_a_docs_receipt_passes() {
    let (repo, store, ticket_id) = required_docs_repo();
    let receipt_id = worker_docs_receipt(&repo);
    let (lease_id, source_commit) = active_lease(&store, &ticket_id, "res-docs-gate-2");

    let handoff = store
        .submit_execution_handoff(handoff_args(
            lease_id,
            source_commit,
            vec![receipt_id.clone()],
            "handoff-docs-gate-2",
        ))
        .unwrap();
    assert_eq!(handoff.evidence_receipt_ids, vec![receipt_id]);
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Verifying
    );
}

/// Record a real, verifiable receipt that is not a documentation proof.
///
/// Referencing *some* receipt must not satisfy the gate; if it did, the whole
/// decision would be defeated silently by any worker that passes an unrelated
/// `--evidence-receipt`.
fn decision_acceptance_receipt(repo: &TestRepo, store: &JsonGraphStore) -> String {
    use pulse::evidence::model::{
        ActorKind, ActorRef, ContentBinding, DecisionAcceptanceDecision, DecisionAcceptancePayload,
        DecisionContentSnapshot, ReceiptBindings, ReceiptEnvelope, ReceiptKind, ReceiptPayload,
        ReceiptResult, SourceBinding, SourcePosture, SubjectRef, WorkBinding,
    };

    let decision = store
        .create_node(pulse::id::WorkKind::Decision, "Docs ownership".to_string())
        .unwrap()
        .value;
    let relative = format!("works/{}/decision.md", decision.id);
    let path = repo.path().join(&relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"# Decision\nThe worker owns the docs receipt.").unwrap();
    let content_hash = pulse::canonical_json::hash_bytes(&std::fs::read(&path).unwrap());
    let source_commit = super::common_git::commit_all(repo.path());
    let manifest = pulse::evidence::manifest::load(repo.path()).unwrap();

    let receipt = ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 1,
        id: "rcpt_01J00000000000000000000416".to_string(),
        kind: ReceiptKind::DecisionAcceptance,
        result: ReceiptResult::Passed,
        actor: ActorRef {
            kind: ActorKind::Human,
            id: "tester".to_string(),
        },
        recorded_at: chrono::Utc::now(),
        subject: SubjectRef {
            kind: "work".to_string(),
            id: decision.id.clone(),
        },
        bindings: ReceiptBindings {
            work: vec![WorkBinding {
                id: decision.id.clone(),
                revision: decision.revision,
            }],
            source: Some(SourceBinding {
                kind: "git_commit".to_string(),
                commit: source_commit,
                repository_id: manifest.repository_id.clone(),
            }),
            content: vec![ContentBinding {
                path: relative.clone(),
                sha256: content_hash.clone(),
            }],
            artifacts: vec![],
            graph_fingerprint_observed: None,
        },
        payload: ReceiptPayload::DecisionAcceptance(DecisionAcceptancePayload {
            payload_version: 1,
            decision: DecisionAcceptanceDecision {
                id: decision.id.clone(),
                revision_observed: decision.revision,
                contract_revision: decision.contract_revision,
                content: DecisionContentSnapshot {
                    path: relative,
                    content_hash,
                },
            },
            accepted_outcome: "Worker records the documentation proof.".to_string(),
            approver: ActorRef {
                kind: ActorKind::Human,
                id: "tester".to_string(),
            },
            source_posture: SourcePosture::CleanGitCommit,
        }),
    };
    pulse::evidence::record_receipt_envelope(repo.path(), None, receipt)
        .unwrap()
        .receipt
        .id
}

#[test]
fn a_receipt_of_the_wrong_kind_does_not_satisfy_the_docs_gate() {
    let (repo, store, ticket_id) = required_docs_repo();
    let unrelated = decision_acceptance_receipt(&repo, &store);
    let (lease_id, source_commit) = active_lease(&store, &ticket_id, "res-docs-gate-3");

    let error = store
        .submit_execution_handoff(handoff_args(
            lease_id,
            source_commit,
            vec![unrelated],
            "handoff-docs-gate-3",
        ))
        .unwrap_err();
    assert_eq!(error.code(), "handoff_documentation_receipt_missing");
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Active
    );
}

#[test]
fn a_ticket_without_required_docs_hands_off_with_no_receipt() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(
        repo.path(),
        &["work.assignment.release", "work.assignment.handoff"],
    );
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let (lease_id, source_commit) = active_lease(&store, &ticket_id, "res-docs-gate-4");

    store
        .submit_execution_handoff(SubmitHandoffArgs {
            changed_paths: vec!["src/token.mjs".to_string()],
            ..handoff_args(lease_id, source_commit, vec![], "handoff-docs-gate-4")
        })
        .unwrap();
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Verifying
    );
}
