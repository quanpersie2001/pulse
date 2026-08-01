use pulse::execution::{
    CompleteVerificationArgs, SubmitHandoffArgs, VerificationCheck, VerificationDisposition,
};
use pulse::graph::node::NodeStatus;
use pulse::reservation::{
    AcknowledgeReservationArgs, ActivateReservationArgs, AssignmentAcknowledgement,
    ReservationState, ReserveWorkArgs, RuntimeBinding,
};
use pulse::storage::transaction::TransactionFailpoint;
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
    assert_eq!(verified.resulting_status, "verifying");
    assert_ne!(verified.resulting_status, "done");
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
                    capability_inventory_bytes: valid_inventory_bytes("agent:codex-local"),
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

fn reservation_file(repo: &std::path::Path, lease_id: &str) -> std::path::PathBuf {
    repo.join(".pulse/runtime/assignment/reservations")
        .join(format!("{lease_id}.json"))
}

fn packet_file(repo: &std::path::Path, lease_id: &str) -> std::path::PathBuf {
    repo.join(".pulse/runtime/assignment/packets")
        .join(format!("{lease_id}.json"))
}

fn records_for_key(
    repo: &std::path::Path,
    key: &str,
) -> Vec<pulse::reservation::CoreReservationV1> {
    let key_hash = pulse::canonical_json::hash_bytes(key.as_bytes());
    let directory = repo.join(".pulse/runtime/assignment/reservations");
    let mut records = std::fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .map(|path| {
            let record: pulse::reservation::CoreReservationV1 =
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
/// record still passes `CoreReservationV1::validate`.
fn terminalize(repo: &std::path::Path, lease_id: &str, state: ReservationState) -> Vec<u8> {
    let path = reservation_file(repo, lease_id);
    let mut record: pulse::reservation::CoreReservationV1 =
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
    let terminal_record: pulse::reservation::CoreReservationV1 =
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
    let retry_packet: pulse::work_packet::WorkPacketV1 =
        serde_json::from_slice(&retry_packet).unwrap();
    assert_eq!(
        retry_packet.packet_fingerprint,
        retry.reservation.packet_fingerprint
    );
    assert_eq!(retry_packet.subject.id, ticket_id);

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
                capability_inventory_bytes: valid_inventory_bytes("agent:codex-local"),
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
                capability_inventory_bytes: valid_inventory_bytes("agent:codex-local"),
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
    let terminal: pulse::reservation::CoreReservationV1 = serde_json::from_slice(
        &std::fs::read(reservation_file(repo.path(), &first.reservation.lease_id)).unwrap(),
    )
    .unwrap();
    assert_eq!(terminal.state, ReservationState::Expired);
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
        NodeStatus::Verifying
    );
}

#[cfg(unix)]
mod delivery_crash_consistency {
    use super::*;
    use pulse::daemon::application::DaemonApplication;
    use pulse::daemon::assignment::{AssignmentSagaRecord, AssignmentSagaState, DeliveryState};
    use pulse::daemon::persistence::{FailpointMode, StateStore};
    use pulse::daemon::protocol::{DaemonRequest, DaemonResponse};
    use pulse::daemon::workspace::IsolationMode;
    use serde_json::json;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::path::Path;
    use std::time::{Duration, Instant};

    /// Opaque provider that appends every received stdin byte to `log_path`,
    /// so a test can count exactly how many bootstrap deliveries reached it.
    /// Pipe writes below PIPE_BUF are atomic, so each bootstrap lands as one
    /// complete append.
    fn provider_options(log_path: &Path) -> serde_json::Value {
        json!({
            "executable": "/bin/sh",
            "args": ["-c", "cat >> \"$1\"", "pulse-test", log_path.to_string_lossy().to_string()],
            "protocol_mode": "opaque_test",
        })
    }

    /// Native-protocol mock provider in POSIX awk: echoes each request's id
    /// back inside a thread/start- or turn/start-shaped result, skips
    /// notifications (no `"id"`), and flushes after every response so the
    /// daemon's JSONL request/response loop never stalls.
    const NATIVE_MOCK_PROVIDER: &str = r#"{ if ($0 !~ /"id":"/) next; id = $0; sub(/.*"id":"/, "", id); sub(/".*/, "", id); if ($0 ~ /"method":"turn\/start"/) { printf "{\"jsonrpc\":\"2.0\",\"id\":\"%s\",\"result\":{\"turn\":{\"id\":\"turn-native-1\"}}}\n", id } else { printf "{\"jsonrpc\":\"2.0\",\"id\":\"%s\",\"result\":{\"thread\":{\"id\":\"thread-native-1\"}}}\n", id } fflush() }"#;

    fn open_project(daemon: &DaemonApplication, root: &Path) -> String {
        match daemon
            .handle(
                &DaemonRequest::ProjectOpen {
                    root: root.to_string_lossy().to_string(),
                },
                "delivery-project",
            )
            .unwrap()
        {
            DaemonResponse::Project { project } => project.project_id,
            other => panic!("unexpected response: {other:?}"),
        }
    }

    fn assignment_start_request(
        project_id: &str,
        ticket_id: &str,
        provider_options: &serde_json::Value,
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
            provider_options: provider_options.clone(),
            ttl_seconds: 1800,
        }
    }

    fn start_assignment(
        daemon: &DaemonApplication,
        request: &DaemonRequest,
        key: &str,
    ) -> AssignmentSagaRecord {
        match daemon.handle(request, key).unwrap() {
            DaemonResponse::Assignment { saga } => saga,
            other => panic!("unexpected response: {other:?}"),
        }
    }

    fn setup_daemon(repo: &TestRepo) -> (tempfile::TempDir, DaemonApplication, String) {
        let daemon_home = tempfile::tempdir().unwrap();
        let daemon = DaemonApplication::new(StateStore::new(daemon_home.path()), "test").unwrap();
        let project_id = open_project(&daemon, repo.path());
        (daemon_home, daemon, project_id)
    }

    fn setup_ticket(repo: &TestRepo, store: &JsonGraphStore) -> String {
        bootstrap_repo(repo, store);
        write_policy(repo.path(), &["work.assignment.release"]);
        setup_ready_ticket(repo.path(), store)
    }

    fn count_lease_lines(path: &Path) -> usize {
        std::fs::read_to_string(path)
            .map(|contents| {
                contents
                    .lines()
                    .filter(|line| line.starts_with("lease="))
                    .count()
            })
            .unwrap_or(0)
    }

    fn wait_until(timeout: Duration, what: &str, mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + timeout;
        while !condition() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn reservation_state(
        root: &Path,
        lease_id: &str,
    ) -> Option<pulse::reservation::ReservationState> {
        pulse::kernel::reservation::list_reservations(root)
            .unwrap()
            .into_iter()
            .find(|reservation| reservation.lease_id == lease_id)
            .map(|reservation| reservation.state)
    }

    #[test]
    fn delivery_intent_is_persisted_before_provider_send() {
        let repo = TestRepo::from_fixture("minimal-service");
        let store = JsonGraphStore::new(repo.path());
        let ticket_id = setup_ticket(&repo, &store);
        let (daemon_home, daemon, project_id) = setup_daemon(&repo);
        let provider_log = daemon_home.path().join("provider-received.log");
        let request =
            assignment_start_request(&project_id, &ticket_id, &provider_options(&provider_log));
        let state_store = StateStore::new(daemon_home.path());
        state_store
            .arm_failpoint("after_delivery_intent", FailpointMode::Panic)
            .unwrap();

        // Crash after the durable intent commit, before any provider I/O.
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            start_assignment(&daemon, &request, "delivery-intent-key")
        }));
        assert!(outcome.is_err(), "injected crash must abort the request");
        assert_eq!(
            count_lease_lines(&provider_log),
            0,
            "no bootstrap may reach the provider before the intent is durable"
        );
        state_store
            .disarm_failpoint("after_delivery_intent", FailpointMode::Panic)
            .unwrap();

        drop(daemon);
        let restarted =
            DaemonApplication::new(StateStore::new(daemon_home.path()), "test-restarted").unwrap();
        let saga = start_assignment(&restarted, &request, "delivery-intent-key");
        assert_eq!(saga.state, AssignmentSagaState::DeliveryPending);
        let delivery_id = saga
            .delivery_id
            .clone()
            .expect("delivery identity is durable");
        assert!(
            saga.last_error
                .as_deref()
                .unwrap_or_default()
                .contains("cannot be proven"),
            "recovery must fail closed with an explicit explanation"
        );

        let state = restarted.store().load().unwrap();
        let delivery = state
            .deliveries
            .get(&delivery_id)
            .expect("delivery record is durable");
        assert_eq!(delivery.state, DeliveryState::Uncertain);
        assert_eq!(delivery.saga_id, saga.saga_id);
        assert_eq!(
            delivery.session_id,
            saga.session_id.as_deref().expect("saga session")
        );
        assert_eq!(delivery.correlation_request_id, None);
        assert_eq!(delivery.correlation_turn_id, None);
        let lease_id = saga.lease_id.as_deref().expect("saga lease");
        assert!(
            delivery.payload.contains(&format!("lease={lease_id}")),
            "payload must be the exact bootstrap for the saga lease"
        );
        assert_eq!(
            count_lease_lines(&provider_log),
            0,
            "crash before send must not deliver the bootstrap"
        );
    }

    #[test]
    fn provider_accepts_then_delivered_commit_failure_does_not_duplicate_or_release() {
        let repo = TestRepo::from_fixture("minimal-service");
        let store = JsonGraphStore::new(repo.path());
        let ticket_id = setup_ticket(&repo, &store);
        let (daemon_home, daemon, project_id) = setup_daemon(&repo);
        let provider_log = daemon_home.path().join("provider-received.log");
        let request =
            assignment_start_request(&project_id, &ticket_id, &provider_options(&provider_log));
        let state_store = StateStore::new(daemon_home.path());
        state_store
            .arm_failpoint("before_delivery_delivered_commit", FailpointMode::Error)
            .unwrap();

        // The provider accepts the bootstrap, but the daemon state commit that
        // would record `BootstrapDelivered` fails. The daemon stays alive.
        let error = daemon
            .handle(&request, "delivery-commit-failure-key")
            .unwrap_err();
        assert_eq!(error.code, "injected_failpoint");
        wait_until(
            Duration::from_secs(5),
            "provider to accept the bootstrap",
            || count_lease_lines(&provider_log) == 1,
        );
        state_store
            .disarm_failpoint("before_delivery_delivered_commit", FailpointMode::Error)
            .unwrap();

        // Retry with the original idempotency key must not re-send and must not
        // release the reservation: it replays the pending saga.
        let saga = start_assignment(&daemon, &request, "delivery-commit-failure-key");
        assert_eq!(saga.state, AssignmentSagaState::DeliveryPending);
        let delivery_id = saga
            .delivery_id
            .clone()
            .expect("delivery identity is durable");
        assert!(
            saga.last_error
                .as_deref()
                .unwrap_or_default()
                .contains("cannot be proven"),
            "retry must explain the pending delivery"
        );
        let lease_id = saga.lease_id.clone().expect("saga lease");

        let state = daemon.store().load().unwrap();
        let delivery = state
            .deliveries
            .get(&delivery_id)
            .expect("delivery record is durable");
        assert_eq!(
            delivery.state,
            DeliveryState::IntentRecorded,
            "no delivered acknowledgement was ever persisted"
        );
        assert_eq!(delivery.correlation_turn_id, None);
        assert_eq!(
            reservation_state(repo.path(), &lease_id),
            Some(pulse::reservation::ReservationState::Reserved),
            "a failed delivered-commit must not release the reservation"
        );
        assert_eq!(
            count_lease_lines(&provider_log),
            1,
            "retry must not re-send the bootstrap"
        );
    }

    #[test]
    fn restart_with_uncertain_delivery_fails_closed_without_duplicate() {
        let repo = TestRepo::from_fixture("minimal-service");
        let store = JsonGraphStore::new(repo.path());
        let ticket_id = setup_ticket(&repo, &store);
        let (daemon_home, daemon, project_id) = setup_daemon(&repo);
        let provider_log = daemon_home.path().join("provider-received.log");
        let request =
            assignment_start_request(&project_id, &ticket_id, &provider_options(&provider_log));
        let state_store = StateStore::new(daemon_home.path());
        state_store
            .arm_failpoint("before_delivery_delivered_commit", FailpointMode::Panic)
            .unwrap();

        // Provider accepts the bootstrap, then the daemon crashes before
        // persisting the delivered acknowledgement.
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            start_assignment(&daemon, &request, "restart-uncertain-key")
        }));
        assert!(outcome.is_err(), "injected crash must abort the request");
        wait_until(
            Duration::from_secs(5),
            "provider to accept the bootstrap",
            || count_lease_lines(&provider_log) == 1,
        );
        state_store
            .disarm_failpoint("before_delivery_delivered_commit", FailpointMode::Panic)
            .unwrap();

        drop(daemon);
        let restarted =
            DaemonApplication::new(StateStore::new(daemon_home.path()), "test-restarted").unwrap();
        let saga = start_assignment(&restarted, &request, "restart-uncertain-key");
        assert_eq!(saga.state, AssignmentSagaState::DeliveryPending);
        let delivery_id = saga
            .delivery_id
            .clone()
            .expect("delivery identity is durable");
        assert!(
            saga.last_error
                .as_deref()
                .unwrap_or_default()
                .contains("cannot be proven"),
            "restart recovery must fail closed with an explicit explanation"
        );
        let lease_id = saga.lease_id.clone().expect("saga lease");

        let state = restarted.store().load().unwrap();
        let delivery = state
            .deliveries
            .get(&delivery_id)
            .expect("delivery record is durable");
        assert_eq!(
            delivery.state,
            DeliveryState::Uncertain,
            "intent without delivered proof must be marked uncertain"
        );
        assert_eq!(
            reservation_state(repo.path(), &lease_id),
            Some(pulse::reservation::ReservationState::Reserved),
            "uncertain delivery must never release a possibly-valid assignment"
        );
        assert_eq!(
            count_lease_lines(&provider_log),
            1,
            "restart retry must not duplicate the bootstrap"
        );
    }

    #[test]
    fn delivered_commit_success_but_response_lost_does_not_duplicate() {
        let repo = TestRepo::from_fixture("minimal-service");
        let store = JsonGraphStore::new(repo.path());
        let ticket_id = setup_ticket(&repo, &store);
        let (daemon_home, daemon, project_id) = setup_daemon(&repo);
        let provider_log = daemon_home.path().join("provider-received.log");
        let request =
            assignment_start_request(&project_id, &ticket_id, &provider_options(&provider_log));
        let state_store = StateStore::new(daemon_home.path());
        state_store
            .arm_failpoint("before_idempotency_result_commit", FailpointMode::Panic)
            .unwrap();

        // The delivered acknowledgement IS persisted (`BootstrapDelivered`),
        // but the daemon crashes before the client response and its idempotent
        // replay record are persisted.
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            start_assignment(&daemon, &request, "response-lost-key")
        }));
        assert!(outcome.is_err(), "injected crash must abort the request");
        wait_until(
            Duration::from_secs(5),
            "provider to accept the bootstrap",
            || count_lease_lines(&provider_log) == 1,
        );
        state_store
            .disarm_failpoint("before_idempotency_result_commit", FailpointMode::Panic)
            .unwrap();

        drop(daemon);
        let restarted =
            DaemonApplication::new(StateStore::new(daemon_home.path()), "test-restarted").unwrap();
        let saga = start_assignment(&restarted, &request, "response-lost-key");
        assert_eq!(
            saga.state,
            AssignmentSagaState::BootstrapDelivered,
            "the durable delivered state must replay"
        );
        let delivery_id = saga
            .delivery_id
            .clone()
            .expect("delivery identity is durable");

        let state = restarted.store().load().unwrap();
        let delivery = state
            .deliveries
            .get(&delivery_id)
            .expect("delivery record is durable");
        assert_eq!(delivery.state, DeliveryState::Delivered);
        assert_eq!(
            count_lease_lines(&provider_log),
            1,
            "replayed retry must not re-send the bootstrap"
        );
    }

    #[test]
    fn native_delivery_record_correlates_provider_request_and_turn_identifiers() {
        let repo = TestRepo::from_fixture("minimal-service");
        let store = JsonGraphStore::new(repo.path());
        let ticket_id = setup_ticket(&repo, &store);
        let (_daemon_home, daemon, project_id) = setup_daemon(&repo);
        let request = assignment_start_request(
            &project_id,
            &ticket_id,
            &json!({
                "executable": "/usr/bin/awk",
                "args": [NATIVE_MOCK_PROVIDER],
            }),
        );
        let saga = start_assignment(&daemon, &request, "native-correlation-key");
        assert_eq!(saga.state, AssignmentSagaState::BootstrapDelivered);
        let delivery_id = saga
            .delivery_id
            .clone()
            .expect("delivery identity is durable");
        let lease_id = saga.lease_id.as_deref().expect("saga lease");

        let state = daemon.store().load().unwrap();
        let delivery = state
            .deliveries
            .get(&delivery_id)
            .expect("delivery record is durable");
        assert_eq!(delivery.state, DeliveryState::Delivered);
        let request_id = delivery
            .correlation_request_id
            .as_deref()
            .expect("native provider request identifier is correlated");
        assert!(
            request_id.starts_with("pulse-turn-start-"),
            "request id: {request_id}"
        );
        assert_eq!(
            delivery.correlation_turn_id.as_deref(),
            Some("turn-native-1")
        );
        assert!(
            delivery.payload.contains(&format!("lease={lease_id}")),
            "payload must be the exact bootstrap for the saga lease"
        );
        let session = state
            .sessions
            .get(&delivery.session_id)
            .expect("delivery session");
        assert_eq!(session.provider_handle.as_deref(), Some("thread-native-1"));
        assert_eq!(session.active_turn_id.as_deref(), Some("turn-native-1"));
    }
}
