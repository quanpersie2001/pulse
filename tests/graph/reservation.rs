use pulse::execution::{
    CompleteVerificationArgs, SubmitHandoffArgs, VerificationCheck, VerificationDisposition,
};
use pulse::graph::node::NodeStatus;
use pulse::reservation::{
    ActivateReservationArgs, AssignmentAcknowledgement, ReservationState, ReserveWorkArgs,
    RuntimeBinding,
};
use pulse::JsonGraphStore;

use super::assignment_fixture::{
    bootstrap_repo, setup_ready_ticket, valid_inventory_bytes, write_policy,
};
use super::common_fixture_repo::TestRepo;

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
            capability_inventory_bytes: valid_inventory_bytes("agent:codex-local"),
            ttl_seconds: 1800,
            idempotency_key: key.to_string(),
        })
        .unwrap()
}

#[test]
fn reservation_keeps_core_ready_until_typed_ack_activation() {
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
    assert!(first.packet.workspace.workspace_id.is_none());

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
            idempotency_key: "handoff-happy".to_string(),
        })
        .unwrap();
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Verifying
    );
    let verified = store
        .complete_execution_verification(CompleteVerificationArgs {
            handoff_id: handoff.handoff_id,
            actor: "human:reviewer".to_string(),
            source_commit: handoff.source_commit,
            disposition: VerificationDisposition::Passed,
            summary: "Independent verification passed.".to_string(),
            checks: vec![VerificationCheck {
                name: "focused-test".to_string(),
                command: "cargo test --test graph -- reservation".to_string(),
                exit_code: 0,
                artifact_ids: vec![],
            }],
            idempotency_key: "verification-happy".to_string(),
        })
        .unwrap();
    assert_eq!(verified.resulting_status, "done");
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Done
    );
}

fn add_reviewer_policy(root: &std::path::Path) {
    let path = root.join(".pulse/policy/authority.json");
    let mut policy: pulse::policy::AuthorityPolicy =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for principal in &mut policy.principals {
        principal.grants.extend([
            "work.assignment.handoff".to_string(),
            "work.assignment.verify".to_string(),
        ]);
    }
    policy.principals.push(pulse::policy::AuthorityPrincipal {
        kind: pulse::identity::actor::ActorKind::Human,
        id: "reviewer".to_string(),
        grants: vec!["work.assignment.verify".to_string()],
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

#[cfg(unix)]
#[test]
fn assignment_retry_restores_archived_workspace_and_resumes_closed_session() {
    use pulse::daemon::application::DaemonApplication;
    use pulse::daemon::assignment::AssignmentSagaState;
    use pulse::daemon::persistence::StateStore;
    use pulse::daemon::protocol::{DaemonRequest, DaemonResponse};
    use pulse::daemon::workspace::IsolationMode;

    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let daemon_home = tempfile::tempdir().unwrap();
    let daemon = DaemonApplication::new(StateStore::new(daemon_home.path()), "test").unwrap();
    let project_id = match daemon
        .handle(
            &DaemonRequest::ProjectOpen {
                root: repo.path().to_string_lossy().to_string(),
            },
            "recovery-project",
        )
        .unwrap()
    {
        DaemonResponse::Project { project } => project.project_id,
        other => panic!("unexpected response: {other:?}"),
    };
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fake_codex_provider.mjs");
    let provider_options = serde_json::json!({
        "executable": std::env::var("NODE").unwrap_or_else(|_| "node".to_string()),
        "args": [script.to_string_lossy()],
    });
    let request = DaemonRequest::AssignmentStart {
        project_id: project_id.clone(),
        ticket_id: ticket_id.clone(),
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
        provider_options: provider_options.clone(),
        ttl_seconds: 1800,
    };
    let first = match daemon
        .handle(&request, "recover-original-assignment-key")
        .unwrap()
    {
        DaemonResponse::Assignment { saga } => saga,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(first.state, AssignmentSagaState::BootstrapDelivered);
    let first_lease = first.lease_id.clone().unwrap();
    let workspace_id = first.workspace_id.clone().unwrap();
    let session_id = first.session_id.clone().unwrap();
    let old_process_id = daemon.store().load().unwrap().sessions[&session_id]
        .managed_process_id
        .clone()
        .unwrap();
    daemon
        .handle(
            &DaemonRequest::SessionClose {
                session_id: session_id.clone(),
            },
            "recover-close-session",
        )
        .unwrap();
    daemon
        .handle(
            &DaemonRequest::WorkspaceArchive {
                workspace_id: workspace_id.clone(),
            },
            "recover-archive-workspace",
        )
        .unwrap();
    store
        .release_reservation(&first_lease, "agent:tester", "simulate compensation")
        .unwrap();
    daemon
        .store()
        .with_state(true, |state| {
            let saga = state.assignment_sagas.get_mut(&first.saga_id).unwrap();
            saga.state = AssignmentSagaState::Recoverable;
            saga.last_error = Some("simulated recoverable boundary".to_string());
            Ok(())
        })
        .unwrap();
    drop(daemon);

    let restarted =
        DaemonApplication::new(StateStore::new(daemon_home.path()), "test-restarted").unwrap();
    let recovered = match restarted
        .handle(&request, "recover-original-assignment-key")
        .unwrap()
    {
        DaemonResponse::Assignment { saga } => saga,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(recovered.state, AssignmentSagaState::BootstrapDelivered);
    assert_eq!(
        recovered.workspace_id.as_deref(),
        Some(workspace_id.as_str())
    );
    assert_eq!(recovered.session_id.as_deref(), Some(session_id.as_str()));
    assert_ne!(recovered.lease_id.as_deref(), Some(first_lease.as_str()));
    let state = restarted.store().load().unwrap();
    assert_eq!(
        state.workspaces[&workspace_id].lifecycle,
        pulse::daemon::workspace::WorkspaceLifecycle::Open
    );
    let session = &state.sessions[&session_id];
    assert_eq!(
        session.lifecycle,
        pulse::daemon::session::SessionLifecycle::Running
    );
    assert_ne!(
        session.managed_process_id.as_deref(),
        Some(old_process_id.as_str())
    );
    assert_eq!(
        state
            .processes
            .values()
            .filter(|process| {
                process.owner_id == session_id
                    && process.state == pulse::daemon::process::ManagedProcessState::Running
            })
            .count(),
        1
    );
    assert_eq!(
        state
            .timeline
            .iter()
            .filter(|event| event.event_type == "session.resumed")
            .count(),
        1
    );
}

#[cfg(unix)]
#[test]
fn zero_exit_provider_without_handoff_does_not_complete_ticket() {
    use pulse::daemon::application::DaemonApplication;
    use pulse::daemon::assignment::AssignmentSagaState;
    use pulse::daemon::persistence::StateStore;
    use pulse::daemon::protocol::{DaemonRequest, DaemonResponse};
    use pulse::daemon::workspace::IsolationMode;

    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let daemon_home = tempfile::tempdir().unwrap();
    let daemon = DaemonApplication::new(StateStore::new(daemon_home.path()), "test").unwrap();
    let project_id = match daemon
        .handle(
            &DaemonRequest::ProjectOpen {
                root: repo.path().to_string_lossy().to_string(),
            },
            "zero-exit-project",
        )
        .unwrap()
    {
        DaemonResponse::Project { project } => project.project_id,
        other => panic!("unexpected response: {other:?}"),
    };
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fake_codex_exit_zero.mjs");
    let saga = match daemon
        .handle(
            &DaemonRequest::AssignmentStart {
                project_id,
                ticket_id: ticket_id.clone(),
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
                provider_options: serde_json::json!({
                    "executable": std::env::var("NODE").unwrap_or_else(|_| "node".to_string()),
                    "args": [script.to_string_lossy()],
                }),
                ttl_seconds: 1800,
            },
            "zero-exit-assignment",
        )
        .unwrap()
    {
        DaemonResponse::Assignment { saga } => saga,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(saga.state, AssignmentSagaState::BootstrapDelivered);
    let activated = match daemon
        .handle(
            &DaemonRequest::AssignmentAcknowledge {
                saga_id: saga.saga_id,
                acknowledgement_id: "zero-exit-ack".to_string(),
            },
            "zero-exit-acknowledge",
        )
        .unwrap()
    {
        DaemonResponse::Assignment { saga } => saga,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(activated.state, AssignmentSagaState::Activated);
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Active
    );

    let session_id = activated.session_id.as_deref().unwrap();
    let session = daemon.store().load().unwrap().sessions[session_id].clone();
    let process_id = session.managed_process_id.as_deref().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while daemon.managed_process_is_alive(process_id).unwrap()
        && std::time::Instant::now() < deadline
    {
        std::thread::yield_now();
    }
    assert!(!daemon.managed_process_is_alive(process_id).unwrap());
    drop(daemon);
    let restarted =
        DaemonApplication::new(StateStore::new(daemon_home.path()), "test-restarted").unwrap();
    let recovered = match restarted
        .handle(
            &DaemonRequest::AssignmentInspect {
                saga_id: activated.saga_id,
            },
            "",
        )
        .unwrap()
    {
        DaemonResponse::Assignment { saga } => saga,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(recovered.state, AssignmentSagaState::Activated);
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Active
    );
}

#[cfg(unix)]
#[test]
fn daemon_saga_requires_acknowledgement_before_core_activation() {
    use pulse::daemon::application::DaemonApplication;
    use pulse::daemon::assignment::AssignmentSagaState;
    use pulse::daemon::persistence::StateStore;
    use pulse::daemon::protocol::{DaemonRequest, DaemonResponse};
    use pulse::daemon::workspace::IsolationMode;

    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    write_policy(repo.path(), &["work.assignment.release"]);
    add_reviewer_policy(repo.path());
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let daemon_home = tempfile::tempdir().unwrap();
    let daemon = DaemonApplication::new(StateStore::new(daemon_home.path()), "test").unwrap();

    let project_id = match daemon
        .handle(
            &DaemonRequest::ProjectOpen {
                root: repo.path().to_string_lossy().to_string(),
            },
            "saga-project",
        )
        .unwrap()
    {
        DaemonResponse::Project { project } => project.project_id,
        other => panic!("unexpected response: {other:?}"),
    };
    let saga = match daemon
        .handle(
            &DaemonRequest::AssignmentStart {
                project_id,
                ticket_id: ticket_id.clone(),
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
                provider_options: serde_json::json!({
                    "executable": "/bin/sh",
                    "args": ["-c", "cat"],
                    "protocol_mode": "opaque_test",
                }),
                ttl_seconds: 1800,
            },
            "saga-start",
        )
        .unwrap()
    {
        DaemonResponse::Assignment { saga } => saga,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(saga.state, AssignmentSagaState::BootstrapDelivered);
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Ready
    );

    let activated = match daemon
        .handle(
            &DaemonRequest::AssignmentAcknowledge {
                saga_id: saga.saga_id,
                acknowledgement_id: "ack_worker_loaded_exact_packet".to_string(),
            },
            "saga-ack",
        )
        .unwrap()
    {
        DaemonResponse::Assignment { saga } => saga,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(activated.state, AssignmentSagaState::Activated);
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Active
    );

    let source_commit = pulse::source::head_commit(repo.path()).unwrap();
    let handoff = daemon
        .handle(
            &DaemonRequest::HandoffSubmit {
                saga_id: activated.saga_id.clone(),
                source_commit: source_commit.clone(),
                summary: "Worker handoff for independent verification.".to_string(),
                changed_paths: vec![],
                evidence_receipt_ids: vec![],
            },
            "saga-handoff",
        )
        .unwrap();
    assert!(matches!(handoff, DaemonResponse::Handoff { .. }));
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Verifying
    );
    drop(daemon);
    let restarted =
        DaemonApplication::new(StateStore::new(daemon_home.path()), "test-restarted").unwrap();
    let recovered = restarted
        .handle(
            &DaemonRequest::AssignmentInspect {
                saga_id: activated.saga_id.clone(),
            },
            "",
        )
        .unwrap();
    assert!(matches!(
        recovered,
        DaemonResponse::Assignment {
            saga: pulse::daemon::assignment::AssignmentSagaRecord {
                state: AssignmentSagaState::Verifying,
                ..
            }
        }
    ));
    let verification = restarted
        .handle(
            &DaemonRequest::VerificationComplete {
                saga_id: activated.saga_id.clone(),
                actor: "human:reviewer".to_string(),
                source_commit,
                disposition: VerificationDisposition::Passed,
                summary: "Independent verification passed.".to_string(),
                checks: vec![VerificationCheck {
                    name: "focused".to_string(),
                    command: "cargo test --test graph -- reservation".to_string(),
                    exit_code: 0,
                    artifact_ids: vec![],
                }],
            },
            "saga-verification",
        )
        .unwrap();
    assert!(matches!(verification, DaemonResponse::Verification { .. }));
    assert_eq!(
        store.show_node(&ticket_id).unwrap().status,
        NodeStatus::Done
    );
}
