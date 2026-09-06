use pulse::evidence::model::{
    ActorKind, ActorRef, ContentBinding, ReceiptBindings, ReceiptEnvelope, ReceiptKind,
    ReceiptPayload, ReceiptResult, SourceBinding, SubjectRef,
};
use pulse::execution::{
    AcceptanceProof, CloseTicketArgs, CompleteVerificationArgs, Finding, FindingSeverity,
    SubmitHandoffArgs, VerificationCheck, VerificationDisposition, VerificationReceipt,
};
use pulse::graph::model::node::NodeStatus;
use pulse::qa::{
    QaCaseObservation, QaCaseOutcome, QaCheckpointPayload, QaExecutionScope, QaExecutor,
};
use pulse::reservation::{
    AcknowledgeReservationArgs, ActivateReservationArgs, AssignmentAcknowledgement,
    ReservationState, ReserveWorkArgs, RuntimeBinding,
};
use pulse::storage::transaction::TransactionFailpoint;
use pulse::JsonGraphStore;

use super::assignment_fixture::{
    bootstrap_repo, setup_ready_ticket, setup_ready_ticket_with_required_docs,
    setup_ready_ticket_with_required_qa, setup_ready_ticket_with_story_qa, write_policy,
};
use super::common_fixture_repo::TestRepo;
use super::common_git::commit_all;
use std::fs;

fn reserve(
    store: &JsonGraphStore,
    ticket_id: &str,
    key: &str,
) -> pulse::reservation::ReserveWorkOutcome {
    store
        .reserve_work(ReserveWorkArgs {
            ticket_id: ticket_id.to_string(),
            actor: "agent:tester".to_string(),
            assignee: "agent:codex-local".to_string(),
            ttl_seconds: 1800,
            idempotency_key: key.to_string(),
        })
        .unwrap()
}

fn binding() -> RuntimeBinding {
    RuntimeBinding {
        project_id: "prj_test".to_string(),
        workspace_id: "wks_test".to_string(),
        session_id: "ses_test".to_string(),
        provider_id: "codex".to_string(),
    }
}

fn acknowledgement(packet_fingerprint: &str) -> AssignmentAcknowledgement {
    AssignmentAcknowledgement {
        acknowledgement_id: "ack_test".to_string(),
        delivery_id: "delivery_test".to_string(),
        session_id: "ses_test".to_string(),
        packet_fingerprint: packet_fingerprint.to_string(),
        acknowledged_at: chrono::Utc::now().to_rfc3339(),
    }
}

fn acceptance_proofs(check_name: &str) -> Vec<AcceptanceProof> {
    vec![AcceptanceProof {
        acceptance_id: "AC-1".to_string(),
        check_names: vec![check_name.to_string()],
        evidence_receipt_ids: vec![],
    }]
}

struct RequiredDocsCloseFixture {
    repo: TestRepo,
    store: JsonGraphStore,
    ticket_id: String,
    verification: VerificationReceipt,
    source_commit: String,
}

#[derive(Clone, Copy)]
enum DocumentationReceiptMode {
    Missing,
    Current,
    MissingRequiredCoverage,
}

fn required_docs_close_fixture(mode: DocumentationReceiptMode) -> RequiredDocsCloseFixture {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    add_reviewer_policy(repo.path());
    let ticket_id = setup_ready_ticket_with_required_docs(&repo, &store);
    let reserved = reserve(&store, &ticket_id, "reservation-required-docs");
    let active = store
        .activate_reservation(ActivateReservationArgs {
            lease_id: reserved.reservation.lease_id,
            actor: "agent:tester".to_string(),
            runtime_binding: binding(),
            acknowledgement: acknowledgement(&reserved.reservation.packet_fingerprint),
        })
        .unwrap();
    // Decision 0016: the worker owns the docs proof, so a required-docs
    // handoff must carry one. These fixtures exercise the *close* gate, which
    // reads the reviewer's proofs — the worker receipt below only gets the
    // handoff through, and never satisfies close on its own.
    let worker_docs_receipt = pulse::kernel::documentation::run_documentation_validation(
        repo.path(),
        None,
        Some("agent:tester"),
    )
    .unwrap()
    .receipt
    .unwrap()
    .receipt
    .id;
    let handoff = store
        .submit_execution_handoff(SubmitHandoffArgs {
            lease_id: active.lease_id,
            actor: "agent:tester".to_string(),
            session_id: "ses_test".to_string(),
            source_commit: active.source.commit,
            summary: "Required documentation is ready for validation.".to_string(),
            changed_paths: vec!["docs/domain/reservation.md".to_string()],
            evidence_receipt_ids: vec![worker_docs_receipt],
            learning_usage: Vec::new(),
            checks: Vec::new(),
            acceptance_proofs: Vec::new(),

            idempotency_key: "handoff-required-docs".to_string(),
        })
        .unwrap();
    let evidence_receipt_ids = if matches!(mode, DocumentationReceiptMode::Missing) {
        vec![]
    } else {
        let run = pulse::kernel::documentation::run_documentation_validation(
            repo.path(),
            None,
            Some("human:docs-reviewer"),
        )
        .unwrap();
        let receipt = run.receipt.unwrap().receipt;
        if matches!(mode, DocumentationReceiptMode::Current) {
            vec![receipt.id]
        } else {
            let mut derived = receipt;
            derived.id = pulse::evidence::new_receipt_id();
            let pulse::evidence::model::ReceiptPayload::DocumentationValidation(payload) =
                &mut derived.payload
            else {
                unreachable!("docs validate records documentation payload")
            };
            match mode {
                DocumentationReceiptMode::MissingRequiredCoverage => {
                    payload.documents.retain(|document| {
                        document.document_id.as_deref() == Some("DOC-OPERATIONS-GUIDANCE")
                    });
                }
                DocumentationReceiptMode::Missing | DocumentationReceiptMode::Current => {
                    unreachable!("handled before deriving a receipt")
                }
            }
            let recorded =
                pulse::evidence::record_receipt_envelope(repo.path(), None, derived).unwrap();
            vec![recorded.receipt.id]
        }
    };
    let verification = store
        .complete_execution_verification(CompleteVerificationArgs {
            handoff_id: handoff.handoff_id,
            actor: "human:reviewer".to_string(),
            source_commit: handoff.source_commit.clone(),
            disposition: VerificationDisposition::Passed,
            summary: "Implementation verification passed.".to_string(),
            checks: vec![VerificationCheck {
                name: "focused".to_string(),
                command: "true".to_string(),
                exit_code: 0,
                artifact_ids: vec![],
            }],
            acceptance_proofs: vec![AcceptanceProof {
                acceptance_id: "AC-1".to_string(),
                check_names: vec!["focused".to_string()],
                evidence_receipt_ids,
            }],
            idempotency_key: "verification-required-docs".to_string(),
            findings: Vec::new(),
        })
        .unwrap();
    RequiredDocsCloseFixture {
        repo,
        store,
        ticket_id,
        verification,
        source_commit: handoff.source_commit,
    }
}

fn required_docs_close_args(fixture: &RequiredDocsCloseFixture, key: &str) -> CloseTicketArgs {
    CloseTicketArgs {
        verification_id: fixture.verification.verification_id.clone(),
        actor: "human:reviewer".to_string(),
        source_commit: fixture.source_commit.clone(),
        summary: "Required documentation validation is current.".to_string(),
        idempotency_key: key.to_string(),
    }
}

#[test]
fn required_documentation_close_requires_receipt_in_acceptance_proof() {
    let fixture = required_docs_close_fixture(DocumentationReceiptMode::Missing);
    let error = fixture
        .store
        .close_execution_ticket(required_docs_close_args(
            &fixture,
            "close-required-docs-missing",
        ))
        .unwrap_err();
    assert_eq!(error.code(), "close_documentation_receipt_missing");
    assert_eq!(
        fixture.store.show_node(&fixture.ticket_id).unwrap().status,
        NodeStatus::Verifying
    );
}

#[test]
fn required_documentation_close_rejects_incomplete_document_coverage() {
    let fixture = required_docs_close_fixture(DocumentationReceiptMode::MissingRequiredCoverage);
    let error = fixture
        .store
        .close_execution_ticket(required_docs_close_args(
            &fixture,
            "close-required-docs-incomplete",
        ))
        .unwrap_err();
    assert_eq!(error.code(), "close_documentation_coverage_incomplete");
    assert_eq!(
        fixture.store.show_node(&fixture.ticket_id).unwrap().status,
        NodeStatus::Verifying
    );
}

#[test]
fn required_documentation_close_revalidates_profile_before_success() {
    let fixture = required_docs_close_fixture(DocumentationReceiptMode::Current);
    let registry_path = fixture.repo.path().join(".pulse/docs/registry.json");
    let original = std::fs::read(&registry_path).unwrap();
    let mut registry: serde_json::Value = serde_json::from_slice(&original).unwrap();
    registry["documents"][0]["summary"] = serde_json::Value::String("changed summary".to_string());
    std::fs::write(
        &registry_path,
        pulse::canonical_json::to_canonical_bytes(&registry).unwrap(),
    )
    .unwrap();

    let error = fixture
        .store
        .close_execution_ticket(required_docs_close_args(
            &fixture,
            "close-required-docs-current",
        ))
        .unwrap_err();
    assert_eq!(error.code(), "close_documentation_receipt_ineligible");
    assert_eq!(
        fixture.store.show_node(&fixture.ticket_id).unwrap().status,
        NodeStatus::Verifying
    );

    std::fs::write(&registry_path, original).unwrap();
    let close = fixture
        .store
        .close_execution_ticket(required_docs_close_args(
            &fixture,
            "close-required-docs-current",
        ))
        .unwrap();
    assert_eq!(close.ticket_id, fixture.ticket_id);
    assert_eq!(
        fixture.store.show_node(&close.ticket_id).unwrap().status,
        NodeStatus::Done
    );
}

fn record_qa_checkpoint(repo: &std::path::Path, ticket_id: &str, source_commit: &str) -> String {
    let store = JsonGraphStore::new(repo);
    let node = store.show_node(ticket_id).unwrap();
    let resolution = pulse::qa::resolve_ticket_cases(repo, &node).unwrap();
    let manifest = pulse::evidence::manifest::load(repo).unwrap();
    let receipt_id = "rcpt_01J00000000000000000000009".to_string();
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
            id: ticket_id.to_string(),
        },
        bindings: ReceiptBindings {
            work: vec![],
            source: Some(SourceBinding {
                kind: "git_commit".to_string(),
                commit: source_commit.to_string(),
                repository_id: manifest.repository_id,
            }),
            content: vec![ContentBinding {
                path: resolution.path.clone(),
                sha256: resolution.content_hash.clone(),
            }],
            artifacts: vec![],
            graph_fingerprint_observed: None,
        },
        payload: ReceiptPayload::QaCheckpoint(QaCheckpointPayload {
            payload_version: 1,
            qa_scope: QaExecutionScope::TicketCheckpoint,
            story_id: resolution.owner_id,
            ticket_id: ticket_id.to_string(),
            baseline_revision: resolution.revision,
            baseline_content_hash: resolution.content_hash,
            cases: vec![QaCaseObservation {
                case_id: "QA-001".to_string(),
                case_revision: 1,
                outcome: QaCaseOutcome::Passed,
            }],
            executor: QaExecutor {
                name: "structured-api".to_string(),
                version: "1.0.0".to_string(),
                capabilities: vec!["api".to_string()],
            },
            observations: vec!["Repeated reservation returned one stable identity.".to_string()],
            findings: Vec::new(),
        }),
    };
    // Receipt input lands under the gitignored cache plane so recording the
    // receipt does not mutate source state (which would stale the proof
    // binding under test).
    std::fs::create_dir_all(repo.join(".pulse/cache")).unwrap();
    let file = repo.join(".pulse/cache/qa-checkpoint-input.json");
    std::fs::write(
        &file,
        pulse::canonical_json::to_canonical_bytes(&receipt).unwrap(),
    )
    .unwrap();
    pulse::evidence::record_receipt(repo, None, &file).unwrap();
    receipt_id
}

fn record_story_qualification(
    repo: &std::path::Path,
    ticket_id: &str,
    source_commit: &str,
) -> String {
    let store = JsonGraphStore::new(repo);
    let node = store.show_node(ticket_id).unwrap();
    let story_id = node
        .qa
        .as_ref()
        .and_then(|qa| qa.impact.behavioral_owner.as_deref())
        .unwrap();
    let resolution = pulse::qa::resolve_story_cases(repo, story_id).unwrap();
    let manifest = pulse::evidence::manifest::load(repo).unwrap();
    let receipt_id = "rcpt_01J00000000000000000000010".to_string();
    let cases = resolution
        .cases
        .iter()
        .map(|case| QaCaseObservation {
            case_id: case.id.clone(),
            case_revision: case.revision,
            outcome: QaCaseOutcome::Passed,
        })
        .collect();
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
                repository_id: manifest.repository_id,
            }),
            content: vec![ContentBinding {
                path: resolution.path.clone(),
                sha256: resolution.content_hash.clone(),
            }],
            artifacts: vec![],
            graph_fingerprint_observed: None,
        },
        payload: ReceiptPayload::QaCheckpoint(QaCheckpointPayload {
            payload_version: 1,
            qa_scope: QaExecutionScope::StoryClose,
            story_id: resolution.owner_id,
            ticket_id: ticket_id.to_string(),
            baseline_revision: resolution.revision,
            baseline_content_hash: resolution.content_hash,
            cases,
            executor: QaExecutor {
                name: "structured-api".to_string(),
                version: "1.0.0".to_string(),
                capabilities: vec!["api".to_string()],
            },
            observations: vec!["Full applicable Story baseline passed.".to_string()],
            findings: Vec::new(),
        }),
    };
    std::fs::create_dir_all(repo.join(".pulse/cache")).unwrap();
    let file = repo.join(".pulse/cache/story-qualification-input.json");
    std::fs::write(
        &file,
        pulse::canonical_json::to_canonical_bytes(&receipt).unwrap(),
    )
    .unwrap();
    pulse::evidence::record_receipt(repo, None, &file).unwrap();
    receipt_id
}

#[test]
fn required_qa_checkpoint_opens_proof_close_only_with_current_case_coverage() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    add_reviewer_policy(repo.path());
    let ticket_id = setup_ready_ticket_with_required_qa(repo.path(), &store);

    let reserved = reserve(&store, &ticket_id, "reservation-required-qa");
    let active = store
        .activate_reservation(ActivateReservationArgs {
            lease_id: reserved.reservation.lease_id,
            actor: "agent:tester".to_string(),
            runtime_binding: binding(),
            acknowledgement: acknowledgement(&reserved.reservation.packet_fingerprint),
        })
        .unwrap();
    let handoff = store
        .submit_execution_handoff(SubmitHandoffArgs {
            lease_id: active.lease_id,
            actor: "agent:tester".to_string(),
            session_id: "ses_test".to_string(),
            source_commit: active.source.commit,
            summary: "Behavioral change is ready for independent verification.".to_string(),
            changed_paths: vec![],
            evidence_receipt_ids: vec![],
            learning_usage: Vec::new(),
            checks: Vec::new(),
            acceptance_proofs: Vec::new(),

            idempotency_key: "handoff-required-qa".to_string(),
        })
        .unwrap();
    let qa_receipt_id = record_qa_checkpoint(repo.path(), &ticket_id, &handoff.source_commit);
    let verification = store
        .complete_execution_verification(CompleteVerificationArgs {
            handoff_id: handoff.handoff_id,
            actor: "human:reviewer".to_string(),
            source_commit: handoff.source_commit.clone(),
            disposition: VerificationDisposition::Passed,
            summary: "Implementation and behavioral checkpoint passed.".to_string(),
            checks: vec![VerificationCheck {
                name: "focused".to_string(),
                command: "true".to_string(),
                exit_code: 0,
                artifact_ids: vec![],
            }],
            acceptance_proofs: vec![AcceptanceProof {
                acceptance_id: "AC-1".to_string(),
                check_names: vec!["focused".to_string()],
                evidence_receipt_ids: vec![qa_receipt_id],
            }],
            idempotency_key: "verification-required-qa".to_string(),
            findings: Vec::new(),
        })
        .unwrap();
    let close_args = CloseTicketArgs {
        verification_id: verification.verification_id,
        actor: "human:reviewer".to_string(),
        source_commit: handoff.source_commit,
        summary: "Required QA case coverage is current and passed.".to_string(),
        idempotency_key: "close-required-qa".to_string(),
    };
    let node = store.show_node(&ticket_id).unwrap();
    let baseline = pulse::qa::resolve_ticket_cases(repo.path(), &node).unwrap();
    let baseline_path = repo.path().join(&baseline.path);
    let original = std::fs::read(&baseline_path).unwrap();
    let mut stale = original.clone();
    stale.extend_from_slice(b"\nChanged after QA observation.\n");
    std::fs::write(&baseline_path, stale).unwrap();
    let stale_error = store
        .close_execution_ticket(close_args.clone())
        .unwrap_err();
    assert!(
        matches!(
            stale_error.code(),
            "close_qa_checkpoint_stale"
                | "content_binding_stale"
                | "unsupported_source_snapshot"
                | "close_source_stale"
        ),
        "unexpected stale QA error: {}",
        stale_error.code()
    );
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Verifying
    );
    std::fs::write(&baseline_path, original).unwrap();

    let close = store.close_execution_ticket(close_args).unwrap();
    assert_eq!(close.ticket_id, ticket_id);
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Done
    );
}

#[test]
fn story_qualification_opens_covered_ticket_close_on_the_same_source() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    add_reviewer_policy(repo.path());
    let ticket_id = setup_ready_ticket_with_story_qa(repo.path(), &store);
    let story_id = store
        .show_node(&ticket_id)
        .unwrap()
        .qa
        .and_then(|qa| qa.impact.behavioral_owner)
        .unwrap();
    write_story_qa_baseline(repo.path(), &story_id);
    commit_all(repo.path());

    let reserved = reserve(&store, &ticket_id, "reservation-story-qa");
    let active = store
        .activate_reservation(ActivateReservationArgs {
            lease_id: reserved.reservation.lease_id,
            actor: "agent:tester".to_string(),
            runtime_binding: binding(),
            acknowledgement: acknowledgement(&reserved.reservation.packet_fingerprint),
        })
        .unwrap();
    let handoff = store
        .submit_execution_handoff(SubmitHandoffArgs {
            lease_id: active.lease_id,
            actor: "agent:tester".to_string(),
            session_id: "ses_test".to_string(),
            source_commit: active.source.commit,
            summary: "Integrated Story behavior is ready for qualification.".to_string(),
            changed_paths: vec![],
            evidence_receipt_ids: vec![],
            learning_usage: Vec::new(),
            checks: Vec::new(),
            acceptance_proofs: Vec::new(),

            idempotency_key: "handoff-story-qa".to_string(),
        })
        .unwrap();
    let receipt_id = record_story_qualification(repo.path(), &ticket_id, &handoff.source_commit);
    let verification = store
        .complete_execution_verification(CompleteVerificationArgs {
            handoff_id: handoff.handoff_id,
            actor: "human:reviewer".to_string(),
            source_commit: handoff.source_commit.clone(),
            disposition: VerificationDisposition::Passed,
            summary: "Implementation and full Story qualification passed.".to_string(),
            checks: vec![VerificationCheck {
                name: "focused".to_string(),
                command: "true".to_string(),
                exit_code: 0,
                artifact_ids: vec![],
            }],
            acceptance_proofs: vec![AcceptanceProof {
                acceptance_id: "AC-1".to_string(),
                check_names: vec!["focused".to_string()],
                evidence_receipt_ids: vec![receipt_id],
            }],
            idempotency_key: "verification-story-qa".to_string(),
            findings: Vec::new(),
        })
        .unwrap();
    let close = store
        .close_execution_ticket(CloseTicketArgs {
            verification_id: verification.verification_id,
            actor: "human:reviewer".to_string(),
            source_commit: handoff.source_commit,
            summary: "Story qualification covers this deferred Ticket.".to_string(),
            idempotency_key: "close-story-qa".to_string(),
        })
        .unwrap();

    assert_eq!(close.ticket_id, ticket_id);
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Done
    );
}

fn assignment_bytes(repo: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    for directory in [
        repo.join(".pulse/runtime/assignment/reservations"),
        repo.join(".pulse/events"),
    ] {
        if !directory.exists() {
            continue;
        }
        let mut pending = vec![directory];
        while let Some(path) = pending.pop() {
            for entry in std::fs::read_dir(&path).unwrap() {
                let entry = entry.unwrap();
                let entry_path = entry.path();
                if entry_path.is_dir() {
                    pending.push(entry_path);
                } else if entry_path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                    files.push((
                        entry_path
                            .strip_prefix(repo)
                            .unwrap()
                            .to_string_lossy()
                            .to_string(),
                        std::fs::read(entry_path).unwrap(),
                    ));
                }
            }
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

fn pulse_bytes(repo: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let root = repo.join(".pulse");
    let mut files = Vec::new();
    let mut pending = vec![root];
    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(&path).unwrap() {
            let entry = entry.unwrap();
            let entry_path = entry.path();
            if entry_path.is_dir() {
                pending.push(entry_path);
            } else {
                files.push((
                    entry_path
                        .strip_prefix(repo)
                        .unwrap()
                        .to_string_lossy()
                        .to_string(),
                    std::fs::read(entry_path).unwrap(),
                ));
            }
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

fn event_count(repo: &std::path::Path, event_type: &str, lease_id: &str) -> usize {
    assignment_bytes(repo)
        .into_iter()
        .filter(|(path, _bytes)| path.starts_with(".pulse/events/"))
        .filter_map(|(_, bytes)| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .filter(|event| {
            event["event_type"] == event_type && event["payload"]["lease_id"] == lease_id
        })
        .count()
}

fn reservation_state(repo: &std::path::Path, lease_id: &str) -> ReservationState {
    pulse::kernel::reservation::load_reservation(repo, lease_id)
        .unwrap()
        .state
}

#[test]
fn zero_exit_check_without_receipt_keeps_ticket_nonterminal() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    add_reviewer_policy(repo.path());
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    let first = reserve(&store, &ticket_id, "reservation-happy");
    let replay = reserve(&store, &ticket_id, "reservation-happy");
    assert_eq!(first, replay);
    assert_eq!(first.reservation.state, ReservationState::Reserved);
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Ready
    );
    assert!(first.packet.ticket.plan_md.is_none());

    let binding = RuntimeBinding {
        project_id: "prj_test".to_string(),
        workspace_id: "wks_test".to_string(),
        session_id: "ses_test".to_string(),
        provider_id: "codex".to_string(),
    };
    let acknowledgement = AssignmentAcknowledgement {
        acknowledgement_id: "ack_test".to_string(),
        delivery_id: "delivery_test".to_string(),
        session_id: binding.session_id.clone(),
        packet_fingerprint: first.reservation.packet_fingerprint.clone(),
        acknowledged_at: chrono::Utc::now().to_rfc3339(),
    };
    let active = store
        .activate_reservation(ActivateReservationArgs {
            lease_id: first.reservation.lease_id,
            actor: "agent:tester".to_string(),
            runtime_binding: binding,
            acknowledgement,
        })
        .unwrap();
    assert_eq!(active.state, ReservationState::Active);
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Active
    );

    let handoff = store
        .submit_execution_handoff(SubmitHandoffArgs {
            lease_id: active.lease_id,
            actor: "agent:tester".to_string(),
            session_id: "ses_test".to_string(),
            source_commit: active.source.commit.clone(),
            summary: "Implementation completed and ready for verification.".to_string(),
            changed_paths: vec![],
            evidence_receipt_ids: vec![],
            learning_usage: Vec::new(),
            checks: Vec::new(),
            acceptance_proofs: Vec::new(),

            idempotency_key: "handoff-happy".to_string(),
        })
        .unwrap();
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Verifying
    );
    let incomplete = store
        .complete_execution_verification(CompleteVerificationArgs {
            handoff_id: handoff.handoff_id.clone(),
            actor: "human:reviewer".to_string(),
            source_commit: handoff.source_commit.clone(),
            disposition: VerificationDisposition::Passed,
            summary: "Checks passed without acceptance mapping.".to_string(),
            checks: vec![VerificationCheck {
                name: "focused-test".to_string(),
                command: "cargo test --test graph -- reservation".to_string(),
                exit_code: 0,
                artifact_ids: vec![],
            }],
            acceptance_proofs: vec![],
            findings: Vec::new(),
            idempotency_key: "verification-incomplete".to_string(),
        })
        .unwrap_err();
    assert_eq!(
        incomplete.code(),
        "verification_acceptance_coverage_incomplete"
    );
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Verifying
    );
    let verified = store
        .complete_execution_verification(CompleteVerificationArgs {
            handoff_id: handoff.handoff_id.clone(),
            actor: "human:reviewer".to_string(),
            source_commit: handoff.source_commit.clone(),
            disposition: VerificationDisposition::Passed,
            summary: "Independent verification passed.".to_string(),
            checks: vec![VerificationCheck {
                name: "focused-test".to_string(),
                command: "cargo test --test graph -- reservation".to_string(),
                exit_code: 0,
                artifact_ids: vec![],
            }],
            acceptance_proofs: acceptance_proofs("focused-test"),
            findings: Vec::new(),
            idempotency_key: "verification-happy".to_string(),
        })
        .unwrap();
    assert_eq!(verified.resulting_status, "verifying");
    assert_ne!(verified.resulting_status, "done");
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Verifying
    );
    let close_args = CloseTicketArgs {
        verification_id: verified.verification_id,
        actor: "human:reviewer".to_string(),
        source_commit: handoff.source_commit,
        summary: "All low-risk close gates passed.".to_string(),
        idempotency_key: "close-happy".to_string(),
    };
    let crashing =
        JsonGraphStore::with_failpoint(repo.path(), TransactionFailpoint::AfterMultiTargetAll);
    assert!(crashing.close_execution_ticket(close_args.clone()).is_err());
    let closed = store.close_execution_ticket(close_args).unwrap();
    assert_eq!(closed.ticket_id, ticket_id);
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Done
    );
}

#[test]
fn medium_risk_ticket_closes_with_the_same_proof_gates() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    add_reviewer_policy(repo.path());
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let node_path = repo
        .path()
        .join(".pulse/workgraph/nodes")
        .join(format!("{ticket_id}.json"));
    let mut node = store.show_node(&ticket_id).unwrap();
    node.risk = Some(pulse::graph::model::contract::Risk::Medium);
    node.revision += 1;
    node.updated_at = chrono::Utc::now();
    std::fs::write(
        &node_path,
        pulse::canonical_json::to_canonical_bytes(&node).unwrap(),
    )
    .unwrap();

    let reserved = reserve(&store, &ticket_id, "reservation-medium-risk");
    let active = store
        .activate_reservation(ActivateReservationArgs {
            lease_id: reserved.reservation.lease_id,
            actor: "agent:tester".to_string(),
            runtime_binding: binding(),
            acknowledgement: acknowledgement(&reserved.reservation.packet_fingerprint),
        })
        .unwrap();
    let handoff = store
        .submit_execution_handoff(SubmitHandoffArgs {
            lease_id: active.lease_id,
            actor: "agent:tester".to_string(),
            session_id: "ses_test".to_string(),
            source_commit: active.source.commit,
            summary: "Medium-risk handoff awaiting assurance policy.".to_string(),
            changed_paths: vec![],
            evidence_receipt_ids: vec![],
            learning_usage: Vec::new(),
            checks: Vec::new(),
            acceptance_proofs: Vec::new(),

            idempotency_key: "handoff-medium-risk".to_string(),
        })
        .unwrap();
    store
        .complete_execution_verification(CompleteVerificationArgs {
            handoff_id: handoff.handoff_id,
            actor: "human:reviewer".to_string(),
            source_commit: handoff.source_commit.clone(),
            disposition: VerificationDisposition::Passed,
            summary: "Independent verification passed.".to_string(),
            checks: vec![VerificationCheck {
                name: "focused".to_string(),
                command: "true".to_string(),
                exit_code: 0,
                artifact_ids: vec![],
            }],
            acceptance_proofs: acceptance_proofs("focused"),
            findings: Vec::new(),
            idempotency_key: "verification-medium-risk".to_string(),
        })
        .unwrap();
    let close = store
        .close_execution_ticket_for_ticket(
            &ticket_id,
            "human:reviewer".to_string(),
            handoff.source_commit,
            "Medium-risk close gates passed.".to_string(),
            "close-medium-risk".to_string(),
        )
        .unwrap();
    assert_eq!(close.ticket_id, ticket_id);
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Done
    );
}

#[test]
fn high_risk_ticket_requires_a_human_closing_actor() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    add_reviewer_policy(repo.path());
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let node_path = repo
        .path()
        .join(".pulse/workgraph/nodes")
        .join(format!("{ticket_id}.json"));
    let mut node = store.show_node(&ticket_id).unwrap();
    node.risk = Some(pulse::graph::model::contract::Risk::High);
    node.revision += 1;
    node.updated_at = chrono::Utc::now();
    std::fs::write(
        &node_path,
        pulse::canonical_json::to_canonical_bytes(&node).unwrap(),
    )
    .unwrap();

    let reserved = reserve(&store, &ticket_id, "reservation-high-risk");
    let active = store
        .activate_reservation(ActivateReservationArgs {
            lease_id: reserved.reservation.lease_id,
            actor: "agent:tester".to_string(),
            runtime_binding: binding(),
            acknowledgement: acknowledgement(&reserved.reservation.packet_fingerprint),
        })
        .unwrap();
    let handoff = store
        .submit_execution_handoff(SubmitHandoffArgs {
            lease_id: active.lease_id,
            actor: "agent:tester".to_string(),
            session_id: "ses_test".to_string(),
            source_commit: active.source.commit,
            summary: "High-risk handoff is ready for review.".to_string(),
            changed_paths: vec![],
            evidence_receipt_ids: vec![],
            learning_usage: Vec::new(),
            checks: Vec::new(),
            acceptance_proofs: Vec::new(),

            idempotency_key: "handoff-high-risk".to_string(),
        })
        .unwrap();
    let verification = store
        .complete_execution_verification(CompleteVerificationArgs {
            handoff_id: handoff.handoff_id,
            actor: "human:reviewer".to_string(),
            source_commit: handoff.source_commit.clone(),
            disposition: VerificationDisposition::Passed,
            summary: "Independent verification passed.".to_string(),
            checks: vec![VerificationCheck {
                name: "focused".to_string(),
                command: "true".to_string(),
                exit_code: 0,
                artifact_ids: vec![],
            }],
            acceptance_proofs: acceptance_proofs("focused"),
            findings: Vec::new(),
            idempotency_key: "verification-high-risk".to_string(),
        })
        .unwrap();

    let error = store
        .close_execution_ticket(CloseTicketArgs {
            verification_id: verification.verification_id,
            actor: "agent:tester".to_string(),
            source_commit: handoff.source_commit,
            summary: "Agent cannot close high-risk work.".to_string(),
            idempotency_key: "close-high-risk-agent".to_string(),
        })
        .unwrap_err();
    assert_eq!(error.code(), "close_high_risk_human_required");
    assert!(error.to_string().contains("human"));
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Verifying
    );
}

#[test]
fn unauthorized_release_does_not_recover_pending_transaction() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let first = reserve(&store, &ticket_id, "unauthorized-release-pending");
    let crashing =
        JsonGraphStore::with_failpoint(repo.path(), TransactionFailpoint::AfterMultiTargetAll);
    assert!(crashing
        .release_reservation(
            &first.reservation.lease_id,
            "agent:tester",
            "prepare pending release",
        )
        .is_err());
    let before = pulse_bytes(repo.path());

    assert!(store
        .release_reservation(
            &first.reservation.lease_id,
            "agent:intruder",
            "unauthorized release",
        )
        .is_err());
    assert_eq!(pulse_bytes(repo.path()), before);
}

#[test]
fn unauthorized_handoff_does_not_recover_pending_transaction() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    add_reviewer_policy(repo.path());
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let first = reserve(&store, &ticket_id, "unauthorized-handoff-pending");
    let active = store
        .activate_reservation(ActivateReservationArgs {
            lease_id: first.reservation.lease_id.clone(),
            actor: "agent:tester".to_string(),
            runtime_binding: binding(),
            acknowledgement: acknowledgement(&first.reservation.packet_fingerprint),
        })
        .unwrap();
    let args = SubmitHandoffArgs {
        lease_id: active.lease_id,
        actor: "agent:tester".to_string(),
        session_id: "ses_test".to_string(),
        source_commit: active.source.commit,
        summary: "pending handoff".to_string(),
        changed_paths: vec![],
        evidence_receipt_ids: vec![],
        learning_usage: Vec::new(),
        checks: Vec::new(),
        acceptance_proofs: Vec::new(),
        idempotency_key: "unauthorized-handoff-pending-key".to_string(),
    };
    let crashing =
        JsonGraphStore::with_failpoint(repo.path(), TransactionFailpoint::AfterMultiTargetAll);
    assert!(crashing.submit_execution_handoff(args.clone()).is_err());
    let before = pulse_bytes(repo.path());

    let unauthorized = SubmitHandoffArgs {
        actor: "agent:intruder".to_string(),
        ..args
    };
    assert!(store.submit_execution_handoff(unauthorized).is_err());
    assert_eq!(pulse_bytes(repo.path()), before);
}

#[test]
fn unauthorized_verification_does_not_recover_pending_transaction() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    add_reviewer_policy(repo.path());
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let first = reserve(&store, &ticket_id, "unauthorized-verification-pending");
    let active = store
        .activate_reservation(ActivateReservationArgs {
            lease_id: first.reservation.lease_id.clone(),
            actor: "agent:tester".to_string(),
            runtime_binding: binding(),
            acknowledgement: acknowledgement(&first.reservation.packet_fingerprint),
        })
        .unwrap();
    let handoff = store
        .submit_execution_handoff(SubmitHandoffArgs {
            lease_id: active.lease_id,
            actor: "agent:tester".to_string(),
            session_id: "ses_test".to_string(),
            source_commit: active.source.commit,
            summary: "handoff for pending verification".to_string(),
            changed_paths: vec![],
            evidence_receipt_ids: vec![],
            learning_usage: Vec::new(),
            checks: Vec::new(),
            acceptance_proofs: Vec::new(),

            idempotency_key: "unauthorized-verification-handoff".to_string(),
        })
        .unwrap();
    let args = CompleteVerificationArgs {
        handoff_id: handoff.handoff_id,
        actor: "human:reviewer".to_string(),
        source_commit: handoff.source_commit,
        disposition: VerificationDisposition::Passed,
        summary: "pending verification".to_string(),
        checks: vec![VerificationCheck {
            name: "focused".to_string(),
            command: "true".to_string(),
            exit_code: 0,
            artifact_ids: vec![],
        }],
        acceptance_proofs: acceptance_proofs("focused"),
        findings: Vec::new(),
        idempotency_key: "unauthorized-verification-pending-key".to_string(),
    };
    let crashing =
        JsonGraphStore::with_failpoint(repo.path(), TransactionFailpoint::AfterMultiTargetAll);
    assert!(crashing
        .complete_execution_verification(args.clone())
        .is_err());
    let before = pulse_bytes(repo.path());

    let unauthorized = CompleteVerificationArgs {
        actor: "human:intruder".to_string(),
        ..args
    };
    assert!(store.complete_execution_verification(unauthorized).is_err());
    assert_eq!(pulse_bytes(repo.path()), before);
}

#[test]
fn expired_reserved_lease_is_recovered_through_core_and_replaced() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let first = reserve(&store, &ticket_id, "reservation-expiry-recovery");
    let first_packet =
        std::fs::read(packet_file(repo.path(), &first.reservation.lease_id)).unwrap();
    let now = chrono::Utc::now() + chrono::Duration::hours(1);

    let recovered = store
        .recover_expired_reservations("agent:tester", now)
        .unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].lease_id, first.reservation.lease_id);
    assert_eq!(recovered[0].state, ReservationState::Expired);
    assert_eq!(
        event_count(
            repo.path(),
            "work.assignment.expired",
            &first.reservation.lease_id
        ),
        1
    );
    let expired_bytes =
        std::fs::read(reservation_file(repo.path(), &first.reservation.lease_id)).unwrap();

    assert!(store
        .recover_expired_reservations("agent:tester", now)
        .unwrap()
        .is_empty());
    assert_eq!(
        event_count(
            repo.path(),
            "work.assignment.expired",
            &first.reservation.lease_id
        ),
        1
    );

    let replacement = reserve(&store, &ticket_id, "reservation-expiry-recovery");
    assert_ne!(replacement.reservation.lease_id, first.reservation.lease_id);
    assert_eq!(replacement.reservation.state, ReservationState::Reserved);
    assert_eq!(
        std::fs::read(reservation_file(repo.path(), &first.reservation.lease_id)).unwrap(),
        expired_bytes
    );
    assert_eq!(
        std::fs::read(packet_file(repo.path(), &first.reservation.lease_id)).unwrap(),
        first_packet
    );
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Ready
    );
}

#[test]
fn acknowledged_lease_expiry_is_recovered_through_core_api() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let first = reserve(&store, &ticket_id, "reservation-ack-expiry");
    let acknowledged = store
        .acknowledge_reservation(AcknowledgeReservationArgs {
            lease_id: first.reservation.lease_id.clone(),
            actor: "agent:tester".to_string(),
            runtime_binding: binding(),
            acknowledgement: acknowledgement(&first.reservation.packet_fingerprint),
        })
        .unwrap();
    assert_eq!(acknowledged.state, ReservationState::Acknowledged);

    let recovered = store
        .recover_expired_reservations(
            "agent:tester",
            chrono::Utc::now() + chrono::Duration::hours(1),
        )
        .unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].state, ReservationState::Expired);
    assert_eq!(
        event_count(
            repo.path(),
            "work.assignment.expired",
            &first.reservation.lease_id
        ),
        1
    );
    assert_eq!(
        reservation_state(repo.path(), &first.reservation.lease_id),
        ReservationState::Expired
    );
}

#[test]
fn active_lease_is_not_ttl_expired_by_recovery() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let first = reserve(&store, &ticket_id, "reservation-active-no-expiry");
    let active = store
        .activate_reservation(ActivateReservationArgs {
            lease_id: first.reservation.lease_id.clone(),
            actor: "agent:tester".to_string(),
            runtime_binding: binding(),
            acknowledgement: acknowledgement(&first.reservation.packet_fingerprint),
        })
        .unwrap();
    let before = assignment_bytes(repo.path());
    let recovered = store
        .recover_expired_reservations(
            "agent:tester",
            chrono::Utc::now() + chrono::Duration::hours(1),
        )
        .unwrap();
    assert!(recovered.is_empty());
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Active
    );
    assert_eq!(
        reservation_state(repo.path(), &active.lease_id),
        ReservationState::Active
    );
    assert_eq!(assignment_bytes(repo.path()), before);
    assert_eq!(
        event_count(repo.path(), "work.assignment.expired", &active.lease_id),
        0
    );
}

#[test]
fn unauthorized_reserve_activate_and_recover_preserve_expired_lease_bytes() {
    for operation in ["reserve", "activate", "recover"] {
        let repo = TestRepo::from_fixture("minimal-service");
        let store = JsonGraphStore::new(repo.path());
        bootstrap_repo(&repo, &store);
        write_policy(repo.path(), &["work.assignment.release"]);
        let ticket_id = setup_ready_ticket(repo.path(), &store);
        let first = reserve(&store, &ticket_id, &format!("unauthorized-{operation}"));
        let before = assignment_bytes(repo.path());
        let future = chrono::Utc::now() + chrono::Duration::hours(1);

        let error = match operation {
            "reserve" => store
                .reserve_work(ReserveWorkArgs {
                    ticket_id: ticket_id.clone(),
                    actor: "agent:intruder".to_string(),
                    assignee: "agent:codex-local".to_string(),
                    ttl_seconds: 1800,
                    idempotency_key: format!("unauthorized-{operation}-replacement"),
                })
                .map(|_| ()),
            "activate" => store
                .activate_reservation(ActivateReservationArgs {
                    lease_id: first.reservation.lease_id.clone(),
                    actor: "agent:intruder".to_string(),
                    runtime_binding: binding(),
                    acknowledgement: acknowledgement(&first.reservation.packet_fingerprint),
                })
                .map(|_| ()),
            "recover" => store
                .recover_expired_reservations("agent:intruder", future)
                .map(|_| ()),
            _ => unreachable!(),
        };
        assert!(error.is_err(), "{operation} unexpectedly authorized");
        assert_eq!(
            assignment_bytes(repo.path()),
            before,
            "{operation} mutated state"
        );
        assert_eq!(
            reservation_state(repo.path(), &first.reservation.lease_id),
            ReservationState::Reserved
        );
    }
}

#[test]
fn expiry_commit_failpoint_recovers_reservation_and_event_atomically() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let first = reserve(&store, &ticket_id, "reservation-expiry-failpoint");
    let crashing =
        JsonGraphStore::with_failpoint(repo.path(), TransactionFailpoint::AfterMultiTargetAll);
    let result = crashing.recover_expired_reservations(
        "agent:tester",
        chrono::Utc::now() + chrono::Duration::hours(1),
    );
    assert!(result.is_err(), "result={result:?}");

    JsonGraphStore::new(repo.path()).recover().unwrap();
    assert_eq!(
        reservation_state(repo.path(), &first.reservation.lease_id),
        ReservationState::Expired
    );
    assert_eq!(
        event_count(
            repo.path(),
            "work.assignment.expired",
            &first.reservation.lease_id
        ),
        1
    );
    assert_eq!(
        store
            .recover_expired_reservations(
                "agent:tester",
                chrono::Utc::now() + chrono::Duration::hours(1),
            )
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        event_count(
            repo.path(),
            "work.assignment.expired",
            &first.reservation.lease_id
        ),
        1
    );
}

fn write_story_qa_baseline(root: &std::path::Path, story_id: &str) {
    let path = root.join(format!("works/{story_id}/qa.md"));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        format!(
            r#"# Reservation behavioral QA

```pulse-qa
{{
  "schema_version": 1,
  "story_id": "{story_id}",
  "revision": 1,
  "scope": "Reservation behavior remains observable.",
  "risks": ["RISK-DUPLICATE"],
  "cases": [{{
    "id": "QA-001",
    "revision": 1,
    "intent": "Reservation is not duplicated.",
    "priority": "critical",
    "risk_refs": ["RISK-DUPLICATE"],
    "steps": ["reserve twice with one idempotency key"],
    "expected": ["one stable reservation"],
    "surface": "api",
    "applicability": "required"
  }}],
  "exit_criteria": ["The required case passes on the candidate source."]
}}
```
"#
        ),
    )
    .unwrap();
}

fn add_reviewer_policy(root: &std::path::Path) {
    let path = root.join(".pulse/policy/authority.json");
    let mut policy: pulse::policy::AuthorityPolicy =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for principal in &mut policy.principals {
        principal.grants.extend([
            "work.assignment.handoff".to_string(),
            "work.assignment.verify".to_string(),
            "work.close".to_string(),
        ]);
    }
    policy.principals.push(pulse::policy::AuthorityPrincipal {
        kind: pulse::identity::actor::ActorKind::Human,
        id: "reviewer".to_string(),
        grants: vec![
            "work.assignment.verify".to_string(),
            "work.close".to_string(),
        ],
    });
    policy.normalize();
    std::fs::write(
        path,
        pulse::canonical_json::to_canonical_bytes(&policy).unwrap(),
    )
    .unwrap();
}

#[test]
fn terminal_reservation_retry_allocates_a_fresh_immutable_lease() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    let first = reserve(&store, &ticket_id, "reservation-generation");
    let first_path = repo
        .path()
        .join(".pulse/runtime/assignment/reservations")
        .join(format!("{}.json", first.reservation.lease_id));
    let first_bytes = std::fs::read(&first_path).unwrap();
    let released = store
        .release_reservation(
            &first.reservation.lease_id,
            "agent:tester",
            "retry generation test",
        )
        .unwrap();
    assert_eq!(released.state, ReservationState::Released);
    let released_bytes = std::fs::read(&first_path).unwrap();

    let second = reserve(&store, &ticket_id, "reservation-generation");
    assert_ne!(second.reservation.lease_id, first.reservation.lease_id);
    assert!(second.reservation.lease_id.ends_with("_g000002"));
    assert_eq!(second.reservation.state, ReservationState::Reserved);
    assert_eq!(std::fs::read(&first_path).unwrap(), released_bytes);
    assert_ne!(first_bytes, released_bytes);

    let replay = reserve(&store, &ticket_id, "reservation-generation");
    assert_eq!(replay, second);
}

#[test]
fn changed_ticket_revision_rejects_activation_and_compensation_releases() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let outcome = reserve(&store, &ticket_id, "reservation-stale");

    let path = repo
        .path()
        .join(".pulse/workgraph/nodes")
        .join(format!("{ticket_id}.json"));
    let mut node: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    node["revision"] = serde_json::json!(outcome.reservation.subject.ticket_revision + 1);
    std::fs::write(
        &path,
        pulse::canonical_json::to_canonical_bytes(&node).unwrap(),
    )
    .unwrap();

    let error = store
        .activate_reservation(ActivateReservationArgs {
            lease_id: outcome.reservation.lease_id.clone(),
            actor: "agent:tester".to_string(),
            runtime_binding: RuntimeBinding {
                project_id: "prj_test".to_string(),
                workspace_id: "wks_test".to_string(),
                session_id: "ses_test".to_string(),
                provider_id: "codex".to_string(),
            },
            acknowledgement: AssignmentAcknowledgement {
                acknowledgement_id: "ack_test".to_string(),
                delivery_id: "delivery_test".to_string(),
                session_id: "ses_test".to_string(),
                packet_fingerprint: outcome.reservation.packet_fingerprint,
                acknowledged_at: chrono::Utc::now().to_rfc3339(),
            },
        })
        .unwrap_err();
    assert_eq!(error.code(), "assignment_subject_changed");

    let released = store
        .release_reservation(
            &outcome.reservation.lease_id,
            "agent:tester",
            "activation rejected",
        )
        .unwrap();
    assert_eq!(released.state, ReservationState::Released);
}

fn reservation_file(repo: &std::path::Path, lease_id: &str) -> std::path::PathBuf {
    repo.join(".pulse/runtime/assignment/reservations")
        .join(format!("{lease_id}.json"))
}

fn packet_file(repo: &std::path::Path, lease_id: &str) -> std::path::PathBuf {
    repo.join(".pulse/runtime/assignment/packets")
        .join(format!("{lease_id}.json"))
}

fn records_for_key(repo: &std::path::Path, key: &str) -> Vec<pulse::reservation::CoreReservation> {
    let key_hash = pulse::canonical_json::hash_bytes(key.as_bytes());
    let directory = repo.join(".pulse/runtime/assignment/reservations");
    let mut records = std::fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .map(|path| {
            let record: pulse::reservation::CoreReservation =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            record
        })
        .filter(|record| record.idempotency_key_hash == key_hash)
        .collect::<Vec<_>>();
    records.sort_by(|left, right| left.lease_id.cmp(&right.lease_id));
    records
}

fn live_leases_for_key(repo: &std::path::Path, key: &str) -> Vec<String> {
    let mut live = records_for_key(repo, key)
        .into_iter()
        .filter(|record| {
            matches!(
                record.state,
                ReservationState::Reserved
                    | ReservationState::Acknowledged
                    | ReservationState::Active
            )
        })
        .map(|record| record.lease_id)
        .collect::<Vec<_>>();
    live.sort();
    live
}

/// Deterministically move a reservation record into a terminal state the way a
/// future state writer would: set the state and recompute the fingerprint so the
/// record still passes `CoreReservation::validate`.
fn terminalize(repo: &std::path::Path, lease_id: &str, state: ReservationState) -> Vec<u8> {
    let path = reservation_file(repo, lease_id);
    let mut record: pulse::reservation::CoreReservation =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    record.state = state;
    record.reservation_fingerprint = record.compute_fingerprint().unwrap();
    let bytes = pulse::canonical_json::to_canonical_bytes(&record).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    bytes
}

/// Shared shape for terminal-retry coverage: after the first reservation is
/// forced into a terminal state, a retry with the same idempotency key must
/// allocate exactly one fresh immutable generation, preserve the prior terminal
/// record and packet byte-for-byte, and replay to the same fresh outcome.
fn assert_fresh_generation_retry(
    repo: &TestRepo,
    ticket_id: &str,
    key: &str,
    terminal_state: ReservationState,
) {
    let store = JsonGraphStore::new(repo.path());
    let first = reserve(&store, ticket_id, key);
    let first_path = reservation_file(repo.path(), &first.reservation.lease_id);
    let first_reservation_bytes = std::fs::read(&first_path).unwrap();
    let first_packet_path = packet_file(repo.path(), &first.reservation.lease_id);
    let first_packet_bytes = std::fs::read(&first_packet_path).unwrap();

    let terminal_bytes = terminalize(repo.path(), &first.reservation.lease_id, terminal_state);
    assert_ne!(terminal_bytes, first_reservation_bytes);
    let terminal_record: pulse::reservation::CoreReservation =
        serde_json::from_slice(&terminal_bytes).unwrap();
    assert_eq!(terminal_record.state, terminal_state);

    let retry = reserve(&store, ticket_id, key);
    assert_ne!(retry.reservation.lease_id, first.reservation.lease_id);
    assert!(
        retry.reservation.lease_id.ends_with("_g000002"),
        "fresh generation lease id: {}",
        retry.reservation.lease_id
    );
    assert_eq!(retry.reservation.state, ReservationState::Reserved);
    // The prior terminal record and its packet are preserved byte-for-byte.
    assert_eq!(std::fs::read(&first_path).unwrap(), terminal_bytes);
    assert_eq!(
        std::fs::read(&first_packet_path).unwrap(),
        first_packet_bytes
    );
    // The fresh generation carries its own live packet for the same subject.
    let retry_packet =
        std::fs::read(packet_file(repo.path(), &retry.reservation.lease_id)).unwrap();
    let retry_packet: pulse::work_packet::WorkPacket =
        serde_json::from_slice(&retry_packet).unwrap();
    assert_eq!(
        retry_packet.packet_fingerprint,
        retry.reservation.packet_fingerprint
    );
    assert_eq!(retry_packet.ticket.node.id, ticket_id);

    // Exactly one live lease remains for the key: the fresh generation.
    assert_eq!(
        live_leases_for_key(repo.path(), key),
        vec![retry.reservation.lease_id.clone()]
    );
    assert_eq!(records_for_key(repo.path(), key).len(), 2);

    // Replaying the same key returns the fresh generation unchanged.
    let replay = reserve(&store, ticket_id, key);
    assert_eq!(replay, retry);
}

#[test]
fn expired_reservation_retry_allocates_a_fresh_immutable_lease() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    assert_fresh_generation_retry(
        &repo,
        &ticket_id,
        "reservation-expired",
        ReservationState::Expired,
    );
}

#[test]
fn stale_reservation_retry_allocates_a_fresh_immutable_lease() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    assert_fresh_generation_retry(
        &repo,
        &ticket_id,
        "reservation-stale-needs-operator",
        ReservationState::StaleNeedsOperator,
    );
}

#[test]
fn concurrent_terminal_retry_reuses_one_fresh_live_generation() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let key = "reservation-concurrent-retry";

    let first = reserve(&store, &ticket_id, key);
    terminalize(
        repo.path(),
        &first.reservation.lease_id,
        ReservationState::Expired,
    );

    let left_repo = repo.path().to_path_buf();
    let right_repo = repo.path().to_path_buf();
    let left_ticket = ticket_id.clone();
    let right_ticket = ticket_id.clone();
    let left = std::thread::spawn(move || {
        JsonGraphStore::new(left_repo)
            .reserve_work(ReserveWorkArgs {
                ticket_id: left_ticket,
                actor: "agent:tester".to_string(),
                assignee: "agent:codex-local".to_string(),
                ttl_seconds: 1800,
                idempotency_key: key.to_string(),
            })
            .unwrap()
    });
    let right = std::thread::spawn(move || {
        JsonGraphStore::new(right_repo)
            .reserve_work(ReserveWorkArgs {
                ticket_id: right_ticket,
                actor: "agent:tester".to_string(),
                assignee: "agent:codex-local".to_string(),
                ttl_seconds: 1800,
                idempotency_key: key.to_string(),
            })
            .unwrap()
    });
    let left_outcome = left.join().unwrap();
    let right_outcome = right.join().unwrap();

    // Both retries converge on the same fresh live generation instead of
    // allocating duplicate leases.
    assert_eq!(left_outcome, right_outcome);
    assert!(left_outcome.reservation.lease_id.ends_with("_g000002"));
    assert_eq!(left_outcome.reservation.state, ReservationState::Reserved);
    assert_eq!(
        live_leases_for_key(repo.path(), key),
        vec![left_outcome.reservation.lease_id.clone()]
    );
    assert_eq!(records_for_key(repo.path(), key).len(), 2);
    // The prior terminal record is still on disk, untouched.
    let terminal: pulse::reservation::CoreReservation = serde_json::from_slice(
        &std::fs::read(reservation_file(repo.path(), &first.reservation.lease_id)).unwrap(),
    )
    .unwrap();
    assert_eq!(terminal.state, ReservationState::Expired);
}

#[test]
fn close_gate_counts_distinct_reviewer_actors_from_the_profile() {
    // Decision 0012 §5: a profile declaring `reviewers: 2` demands two
    // passed verification receipts on the same handoff from two distinct
    // actors before close; one receipt is not a verdict.
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    add_reviewer_policy(repo.path());
    std::fs::write(
        repo.path().join("PULSE.md"),
        "# Verification Profiles\n\n- `service-change`: `node scripts/verify.mjs`, reviewers: 2\n",
    )
    .unwrap();
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    let reserved = reserve(&store, &ticket_id, "reservation-two-reviewers");
    let binding = RuntimeBinding {
        project_id: "prj_test".to_string(),
        workspace_id: "wks_test".to_string(),
        session_id: "ses_test".to_string(),
        provider_id: "codex".to_string(),
    };
    let active = store
        .activate_reservation(ActivateReservationArgs {
            lease_id: reserved.reservation.lease_id,
            actor: "agent:tester".to_string(),
            runtime_binding: binding,
            acknowledgement: acknowledgement(&reserved.reservation.packet_fingerprint),
        })
        .unwrap();
    let handoff = store
        .submit_execution_handoff(SubmitHandoffArgs {
            lease_id: active.lease_id,
            actor: "agent:tester".to_string(),
            session_id: "ses_test".to_string(),
            source_commit: active.source.commit.clone(),
            summary: "Ready for independent review.".to_string(),
            changed_paths: vec![],
            evidence_receipt_ids: vec![],
            learning_usage: Vec::new(),
            checks: Vec::new(),
            acceptance_proofs: Vec::new(),
            idempotency_key: "handoff-two-reviewers".to_string(),
        })
        .unwrap();

    let verification = |actor: &str, key: &str| {
        store
            .complete_execution_verification(CompleteVerificationArgs {
                handoff_id: handoff.handoff_id.clone(),
                actor: actor.to_string(),
                source_commit: handoff.source_commit.clone(),
                disposition: VerificationDisposition::Passed,
                summary: format!("{actor} re-ran the focused check."),
                checks: vec![VerificationCheck {
                    name: format!("focused-{key}"),
                    command: "node scripts/verify.mjs".to_string(),
                    exit_code: 0,
                    artifact_ids: vec![],
                }],
                acceptance_proofs: acceptance_proofs(&format!("focused-{key}")),
                findings: Vec::new(),
                idempotency_key: key.to_string(),
            })
            .unwrap()
    };
    let first = verification("human:reviewer", "verification-reviewer-1");
    assert_eq!(first.resulting_status, "verifying");

    // The close anchor is reviewer 1's receipt; the gate still counts
    // actors and refuses while only one exists.
    let refused = store.close_execution_ticket(CloseTicketArgs {
        verification_id: first.verification_id.clone(),
        actor: "human:tester".to_string(),
        source_commit: handoff.source_commit.clone(),
        summary: "Too early.".to_string(),
        idempotency_key: "close-one-reviewer".to_string(),
    });
    assert_eq!(refused.unwrap_err().code(), "close_reviewers_missing");

    // A second receipt from the SAME actor does not change the count.
    let replayed = verification("human:reviewer", "verification-reviewer-1b");
    let still_refused = store.close_execution_ticket(CloseTicketArgs {
        verification_id: replayed.verification_id,
        actor: "human:tester".to_string(),
        source_commit: handoff.source_commit.clone(),
        summary: "Still one actor.".to_string(),
        idempotency_key: "close-one-reviewer-2".to_string(),
    });
    assert_eq!(still_refused.unwrap_err().code(), "close_reviewers_missing");

    // A genuinely distinct second actor satisfies the profile.
    let second = verification("human:tester", "verification-reviewer-2");
    assert_eq!(second.resulting_status, "verifying");
    let closed = store
        .close_execution_ticket(CloseTicketArgs {
            verification_id: first.verification_id,
            actor: "human:tester".to_string(),
            source_commit: handoff.source_commit.clone(),
            summary: "Two independent reviewers passed.".to_string(),
            idempotency_key: "close-two-reviewers".to_string(),
        })
        .unwrap();
    assert_eq!(closed.ticket_id, ticket_id);
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Done
    );
}

#[test]
fn close_anchor_is_deterministic_under_duplicate_passed_verifications() {
    // Dogfood regression (TK-003): a reviewer that re-runs `work verify`
    // with a fresh idempotency key seals a second passed receipt on the
    // same handoff and actor. Duplicates are noise, not ambiguity
    // (Decision 0012 §5): close anchors the lowest verification id of the
    // qualifying handoff.
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    add_reviewer_policy(repo.path());
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    let reserved = reserve(&store, &ticket_id, "reservation-duplicate-verify");
    let active = store
        .activate_reservation(ActivateReservationArgs {
            lease_id: reserved.reservation.lease_id,
            actor: "agent:tester".to_string(),
            runtime_binding: binding(),
            acknowledgement: acknowledgement(&reserved.reservation.packet_fingerprint),
        })
        .unwrap();
    let handoff = store
        .submit_execution_handoff(SubmitHandoffArgs {
            lease_id: active.lease_id,
            actor: "agent:tester".to_string(),
            session_id: "ses_test".to_string(),
            source_commit: active.source.commit.clone(),
            summary: "Ready for independent review.".to_string(),
            changed_paths: vec![],
            evidence_receipt_ids: vec![],
            learning_usage: Vec::new(),
            checks: Vec::new(),
            acceptance_proofs: Vec::new(),
            idempotency_key: "handoff-duplicate-verify".to_string(),
        })
        .unwrap();

    let verification = |key: &str| {
        store
            .complete_execution_verification(CompleteVerificationArgs {
                handoff_id: handoff.handoff_id.clone(),
                actor: "human:reviewer".to_string(),
                source_commit: handoff.source_commit.clone(),
                disposition: VerificationDisposition::Passed,
                summary: format!("Re-ran the focused check ({key})."),
                checks: vec![VerificationCheck {
                    name: format!("focused-{key}"),
                    command: "node scripts/verify.mjs".to_string(),
                    exit_code: 0,
                    artifact_ids: vec![],
                }],
                acceptance_proofs: acceptance_proofs(&format!("focused-{key}")),
                findings: Vec::new(),
                idempotency_key: key.to_string(),
            })
            .unwrap()
    };
    let first = verification("verification-a");
    let replay = verification("verification-b");
    assert_ne!(first.verification_id, replay.verification_id);

    let closed = store
        .close_execution_ticket_for_ticket(
            &ticket_id,
            "human:tester".to_string(),
            handoff.source_commit,
            "Duplicate same-actor verifications collapse to one anchor.".to_string(),
            "close-duplicate-verify".to_string(),
        )
        .unwrap();
    assert_eq!(closed.verification_id, first.verification_id);
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Done
    );
}

#[test]
fn rework_dispatch_releases_stale_lease_and_rebuilds_packet_with_observations() {
    // Dogfood regression (TK-004): after a reviewer returns rework, the next
    // `pulse run worker` must release the previous assignment's lease,
    // rebuild the packet with the rework observations and move the Ticket
    // rework -> active. Re-running on the stale pre-rework packet silently
    // hides the findings from the worker.
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    add_reviewer_policy(repo.path());
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    let reserved = reserve(&store, &ticket_id, "reservation-rework-1");
    let active = store
        .activate_reservation(ActivateReservationArgs {
            lease_id: reserved.reservation.lease_id,
            actor: "agent:tester".to_string(),
            runtime_binding: binding(),
            acknowledgement: acknowledgement(&reserved.reservation.packet_fingerprint),
        })
        .unwrap();
    let handoff = store
        .submit_execution_handoff(SubmitHandoffArgs {
            lease_id: active.lease_id.clone(),
            actor: "agent:tester".to_string(),
            session_id: "ses_test".to_string(),
            source_commit: active.source.commit.clone(),
            summary: "Ready for independent review.".to_string(),
            changed_paths: vec![],
            evidence_receipt_ids: vec![],
            learning_usage: Vec::new(),
            checks: vec![VerificationCheck {
                name: "focused".to_string(),
                command: "node scripts/verify.mjs".to_string(),
                exit_code: 0,
                artifact_ids: vec![],
            }],
            acceptance_proofs: acceptance_proofs("focused"),
            idempotency_key: "handoff-rework".to_string(),
        })
        .unwrap();
    let rework = store
        .complete_execution_verification(CompleteVerificationArgs {
            handoff_id: handoff.handoff_id,
            actor: "human:reviewer".to_string(),
            source_commit: handoff.source_commit.clone(),
            disposition: VerificationDisposition::Rework,
            summary: "The docs receipt is missing.".to_string(),
            checks: vec![VerificationCheck {
                name: "docs-receipt-missing".to_string(),
                command: "grep -l missing docs/receipts".to_string(),
                exit_code: 1,
                artifact_ids: vec![],
            }],
            acceptance_proofs: Vec::new(),
            findings: vec![Finding {
                summary: "No documentation_validation receipt covers the updated doc.".to_string(),
                owner: "docs/product/behavior.md".to_string(),
                check: Some("docs-receipt-missing".to_string()),
                severity: FindingSeverity::High,
                acceptance_id: None,
                case_id: None,
                unverifiable: false,
            }],
            idempotency_key: "verification-rework".to_string(),
        })
        .unwrap();
    assert_eq!(rework.disposition, VerificationDisposition::Rework);
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Rework
    );

    // Fresh dispatch on the rework Ticket: new lease, packet carries the
    // rework observations, Ticket returns to active.
    let redispatch = reserve(&store, &ticket_id, "reservation-rework-2");
    assert_ne!(redispatch.reservation.lease_id, active.lease_id.clone());
    assert!(
        !redispatch.packet.rework.is_empty(),
        "fresh packet must embed the rework observations"
    );
    assert_eq!(
        redispatch.packet.rework[0].summary,
        "No documentation_validation receipt covers the updated doc."
    );
    assert_eq!(redispatch.packet.rework[0].actor, "human:reviewer");
    let reactivated = store
        .activate_reservation(ActivateReservationArgs {
            lease_id: redispatch.reservation.lease_id,
            actor: "agent:tester".to_string(),
            runtime_binding: binding(),
            acknowledgement: acknowledgement(&redispatch.reservation.packet_fingerprint),
        })
        .unwrap();
    assert_eq!(reactivated.state, ReservationState::Active);
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Active
    );
}
