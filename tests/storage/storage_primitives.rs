use pulse::canonical_json::{hash_bytes, to_canonical_bytes};
use pulse::error::PulseError;
use pulse::storage::atomic::atomic_replace;
use pulse::storage::paths::{configured_content_root, resolve_content_path, resolve_repo_relative};
use pulse::storage::transaction::{
    persist_intent, persist_multi_target_intent, recover_prepared_transactions,
    write_event_create_new, write_event_create_new_multi, FileState, MultiTargetTransactionIntent,
    RecoveryAction, TransactionIntent, TransactionTarget,
};
use serde_json::json;
use std::fs;
use std::path::Path;

/// The v2 workgraph `bootstrap()` this crate used to call for its side
/// effect of creating `.pulse/workgraph/nodes` etc. is gone with `graph/*`
/// (plan 0022 P1.3/P1.4). These transaction/atomic-primitive tests never
/// tested workgraph semantics — they just needed *some* nested target path
/// under `.pulse/` to exist before writing "before" state directly with
/// `fs::write`. This creates every such path any test below still uses.
fn ensure_repo_dirs(repo: &Path) {
    for dir in [
        ".pulse/workgraph/nodes",
        ".pulse/runtime/assignment/leases",
        ".pulse/runtime/assignment/tombstones",
        ".pulse/runtime/assignment/workspaces",
        ".pulse/events",
    ] {
        fs::create_dir_all(repo.join(dir)).unwrap();
    }
}

#[test]
fn canonical_json_is_deterministic_and_lf_terminated() {
    let a = json!({"z": 1, "a": {"b": 2, "a": [3, 2, 1]}});
    let b = json!({"a": {"a": [3, 2, 1], "b": 2}, "z": 1});

    let a_bytes = to_canonical_bytes(&a).unwrap();
    let b_bytes = to_canonical_bytes(&b).unwrap();

    assert_eq!(a_bytes, b_bytes);
    assert_eq!(hash_bytes(&a_bytes), hash_bytes(&b_bytes));
    assert!(a_bytes.ends_with(b"\n"));
    assert!(!a_bytes.ends_with(b"\n\n"));
    assert!(!String::from_utf8(a_bytes).unwrap().contains("\r\n"));
}

#[test]
fn canonical_json_rejects_float_numbers() {
    let error = to_canonical_bytes(&json!({"decimal_number": 1.5})).unwrap_err();
    assert!(matches!(error, PulseError::FloatRejected { .. }));
}

#[test]
fn safe_paths_reject_traversal_and_symlink_escape() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    fs::create_dir(repo.join("works")).unwrap();

    let traversal = resolve_repo_relative(repo, "works/../secret").unwrap_err();
    assert!(matches!(traversal, PulseError::PathTraversal { .. }));

    let content_escape = resolve_content_path(repo, ".pulse/workgraph").unwrap_err();
    assert!(matches!(
        content_escape,
        PulseError::ContentRootViolation { .. }
    ));

    let content_root = configured_content_root(repo, "../../works").unwrap();
    assert_eq!(content_root, fs::canonicalize(repo).unwrap().join("works"));
    let manifest_escape = configured_content_root(repo, "../../../outside").unwrap_err();
    assert!(matches!(manifest_escape, PulseError::PathEscape { .. }));

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), repo.join("works/link-out")).unwrap();
        let escape = resolve_content_path(repo, "works/link-out/file.md").unwrap_err();
        assert!(matches!(escape, PulseError::PathEscape { .. }));
    }
}

#[test]
fn atomic_replace_writes_same_directory_and_replaces_existing() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("node.json");
    fs::write(&target, b"before\n").unwrap();

    let report = atomic_replace(&target, b"after\n").unwrap();

    assert_eq!(fs::read(&target).unwrap(), b"after\n");
    assert_eq!(report.temp_path.parent(), target.parent());
    assert!(!report.temp_path.exists());
}

#[test]
fn transaction_recovery_rolls_back_when_target_before_and_event_absent() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    ensure_repo_dirs(repo);

    let target = repo.join(".pulse/workgraph/nodes/TK-001.json");
    let before_bytes = to_canonical_bytes(&json!({"id": "TK-001", "revision": 1})).unwrap();
    fs::write(&target, &before_bytes).unwrap();
    let after_bytes = to_canonical_bytes(&json!({"id": "TK-001", "revision": 2})).unwrap();
    let event_payload = json!({"id": "evt_test_rollback", "event": "node_updated"});
    let intent = TransactionIntent::prepared(
        "evt_test_rollback",
        "node.update",
        "test",
        target.clone(),
        repo.join(".pulse/events/2026-01-01.jsonl"),
        FileState::Present {
            hash: hash_bytes(&before_bytes),
            revision: 1,
        },
        FileState::Present {
            hash: hash_bytes(&after_bytes),
            revision: 2,
        },
        event_payload,
    )
    .unwrap();
    let intent_path = persist_intent(repo, &intent).unwrap();

    let actions = recover_prepared_transactions(repo).unwrap();
    assert_eq!(
        actions,
        vec![RecoveryAction::RolledBack {
            intent_path: intent_path.clone()
        }]
    );
    assert!(!intent_path.exists());
    assert!(!intent.event_path.exists());
}

#[test]
fn transaction_recovery_completes_event_when_target_after_and_event_absent() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    ensure_repo_dirs(repo);

    let target = repo.join(".pulse/workgraph/nodes/TK-001.json");
    let before_bytes = to_canonical_bytes(&json!({"id": "TK-001", "revision": 1})).unwrap();
    let after_bytes = to_canonical_bytes(&json!({"id": "TK-001", "revision": 2})).unwrap();
    fs::write(&target, &after_bytes).unwrap();
    let event_payload = json!({"id": "evt_test_complete", "event": "node_updated"});
    let event_path = repo.join(".pulse/events/2026-01-01.jsonl");
    let intent = TransactionIntent::prepared(
        "evt_test_complete",
        "node.update",
        "test",
        target,
        event_path.clone(),
        FileState::Present {
            hash: hash_bytes(&before_bytes),
            revision: 1,
        },
        FileState::Present {
            hash: hash_bytes(&after_bytes),
            revision: 2,
        },
        event_payload,
    )
    .unwrap();
    let intent_path = persist_intent(repo, &intent).unwrap();

    let actions = recover_prepared_transactions(repo).unwrap();
    assert_eq!(
        actions,
        vec![RecoveryAction::EventCompleted {
            intent_path: intent_path.clone(),
            event_path: event_path.clone()
        }]
    );
    assert!(!intent_path.exists());
    assert!(event_path.exists());
}

#[test]
fn transaction_recovery_hard_fails_ambiguous_state() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    ensure_repo_dirs(repo);

    let target = repo.join(".pulse/workgraph/nodes/TK-001.json");
    let before_bytes = to_canonical_bytes(&json!({"id": "TK-001", "revision": 1})).unwrap();
    let after_bytes = to_canonical_bytes(&json!({"id": "TK-001", "revision": 2})).unwrap();
    fs::write(
        &target,
        to_canonical_bytes(&json!({"id": "TK-001", "revision": 99})).unwrap(),
    )
    .unwrap();
    let intent = TransactionIntent::prepared(
        "evt_test_ambiguous",
        "node.update",
        "test",
        target,
        repo.join(".pulse/events/2026-01-01.jsonl"),
        FileState::Present {
            hash: hash_bytes(&before_bytes),
            revision: 1,
        },
        FileState::Present {
            hash: hash_bytes(&after_bytes),
            revision: 2,
        },
        json!({"id": "evt_test_ambiguous", "event": "node_updated"}),
    )
    .unwrap();
    persist_intent(repo, &intent).unwrap();

    let error = recover_prepared_transactions(repo).unwrap_err();
    assert!(matches!(error, PulseError::AmbiguousTransaction { .. }));
}

#[test]
fn transaction_recovery_hard_fails_event_mismatch() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    ensure_repo_dirs(repo);

    let target = repo.join(".pulse/workgraph/nodes/TK-001.json");
    let before_bytes = to_canonical_bytes(&json!({"id": "TK-001", "revision": 1})).unwrap();
    let after_bytes = to_canonical_bytes(&json!({"id": "TK-001", "revision": 2})).unwrap();
    fs::write(&target, &after_bytes).unwrap();
    let event_path = repo.join(".pulse/events/2026-01-01.jsonl");
    let intent = TransactionIntent::prepared(
        "evt_test_mismatch",
        "node.update",
        "test",
        target,
        event_path.clone(),
        FileState::Present {
            hash: hash_bytes(&before_bytes),
            revision: 1,
        },
        FileState::Present {
            hash: hash_bytes(&after_bytes),
            revision: 2,
        },
        json!({"id": "evt_test_mismatch", "event": "node_updated"}),
    )
    .unwrap();
    persist_intent(repo, &intent).unwrap();
    fs::create_dir_all(event_path.parent().unwrap()).unwrap();
    // Decision 0011: the day file already carries this event id with other
    // content. Identity matches, bytes do not, so recovery must refuse.
    let mut line = pulse::canonical_json::to_canonical_line_bytes(
        &json!({"id": "evt_test_mismatch", "event": "different"}),
    )
    .unwrap();
    line.push(b'\n');
    fs::write(&event_path, &line).unwrap();

    let error = recover_prepared_transactions(repo).unwrap_err();
    assert!(matches!(error, PulseError::EventMismatch { .. }));
    assert!(event_path.exists());
    assert!(intent.intent_path(repo).exists());
}

#[test]
fn transaction_recovery_cleans_after_event_before_intent_cleanup() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    ensure_repo_dirs(repo);

    let target = repo.join(".pulse/workgraph/nodes/TK-001.json");
    let before_bytes = to_canonical_bytes(&json!({"id": "TK-001", "revision": 1})).unwrap();
    let after_bytes = to_canonical_bytes(&json!({"id": "TK-001", "revision": 2})).unwrap();
    fs::write(&target, &after_bytes).unwrap();
    let intent = TransactionIntent::prepared(
        "evt_test_clean",
        "node.update",
        "test",
        target,
        repo.join(".pulse/events/2026-01-01.jsonl"),
        FileState::Present {
            hash: hash_bytes(&before_bytes),
            revision: 1,
        },
        FileState::Present {
            hash: hash_bytes(&after_bytes),
            revision: 2,
        },
        json!({"id": "evt_test_clean", "event": "node_updated"}),
    )
    .unwrap();
    let intent_path = persist_intent(repo, &intent).unwrap();
    write_event_create_new(&intent).unwrap();

    let actions = recover_prepared_transactions(repo).unwrap();
    assert_eq!(
        actions,
        vec![RecoveryAction::CleanedComplete {
            intent_path: intent_path.clone()
        }]
    );
    assert!(!intent_path.exists());
    assert!(intent.event_path.exists());
}

#[test]
fn multi_target_recovery_rolls_back_when_all_targets_before_and_event_absent() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    ensure_repo_dirs(repo);

    // Two create-new targets: lease + workspace (runtime-record-like)
    let lease_path = repo.join(".pulse/runtime/assignment/leases/lease_01JTEST.json");
    let ws_path = repo.join(".pulse/runtime/assignment/workspaces/wt_01JTEST.json");

    let lease_bytes =
        to_canonical_bytes(&json!({"id": "lease_01JTEST", "state": "prepared"})).unwrap();
    let ws_bytes = to_canonical_bytes(&json!({"id": "wt_01JTEST", "state": "bound"})).unwrap();

    let targets = vec![
        TransactionTarget::new(
            lease_path.clone(),
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&lease_bytes),
                revision: 1,
            },
            &lease_bytes,
        ),
        TransactionTarget::new(
            ws_path.clone(),
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&ws_bytes),
                revision: 1,
            },
            &ws_bytes,
        ),
    ];
    let event_path = repo.join(".pulse/events/2026-01-01.jsonl");
    let intent = MultiTargetTransactionIntent::prepared(
        "evt_mt_rollback",
        "assignment.claim",
        "test",
        targets,
        event_path.clone(),
        json!({"id": "evt_mt_rollback", "event": "claim"}),
    )
    .unwrap();
    let intent_path = persist_multi_target_intent(repo, &intent).unwrap();

    // Both targets absent (Before), event absent: rollback.
    let actions = recover_prepared_transactions(repo).unwrap();
    assert_eq!(
        actions,
        vec![RecoveryAction::RolledBack {
            intent_path: intent_path.clone()
        }]
    );
    assert!(!intent_path.exists());
    assert!(!lease_path.exists());
    assert!(!ws_path.exists());
    assert!(!event_path.exists());
}

#[test]
fn multi_target_recovery_completes_first_target_written_rest_absent() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    ensure_repo_dirs(repo);

    // Two create-new targets sorted by path.
    let target_a = repo.join(".pulse/runtime/a_record.json");
    let target_b = repo.join(".pulse/runtime/b_record.json");

    let bytes_a = to_canonical_bytes(&json!({"id": "a", "val": 1})).unwrap();
    let bytes_b = to_canonical_bytes(&json!({"id": "b", "val": 2})).unwrap();

    // Write first target (already at After state), leave second absent.
    fs::create_dir_all(target_a.parent().unwrap()).unwrap();
    fs::write(&target_a, &bytes_a).unwrap();

    let targets = vec![
        TransactionTarget::new(
            target_a.clone(),
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&bytes_a),
                revision: 1,
            },
            &bytes_a,
        ),
        TransactionTarget::new(
            target_b.clone(),
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&bytes_b),
                revision: 1,
            },
            &bytes_b,
        ),
    ];
    let event_path = repo.join(".pulse/events/2026-01-01.jsonl");
    let intent = MultiTargetTransactionIntent::prepared(
        "evt_mt_partial_first",
        "test.multi",
        "test",
        targets,
        event_path.clone(),
        json!({"id": "evt_mt_partial_first", "event": "partial_first"}),
    )
    .unwrap();
    let intent_path = persist_multi_target_intent(repo, &intent).unwrap();

    // First target is After, second is Before, event absent: complete remaining.
    let actions = recover_prepared_transactions(repo).unwrap();
    assert_eq!(actions.len(), 1);
    match &actions[0] {
        RecoveryAction::EventCompleted {
            intent_path: i,
            event_path: e,
        } => {
            assert_eq!(*i, intent_path);
            assert_eq!(*e, event_path);
        }
        other => panic!("expected EventCompleted, got {:?}", other),
    }
    assert!(!intent_path.exists());
    assert!(target_a.exists());
    assert!(target_b.exists());
    assert_eq!(fs::read(&target_b).unwrap(), bytes_b);
    assert!(event_path.exists());
}

#[test]
fn multi_target_recovery_completes_last_target_written_event_absent() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    ensure_repo_dirs(repo);

    let target_a = repo.join(".pulse/runtime/a_record.json");
    let target_b = repo.join(".pulse/runtime/b_record.json");

    let bytes_a = to_canonical_bytes(&json!({"id": "a", "val": 1})).unwrap();
    let bytes_b = to_canonical_bytes(&json!({"id": "b", "val": 2})).unwrap();

    // Both targets already at After state.
    fs::create_dir_all(target_a.parent().unwrap()).unwrap();
    fs::write(&target_a, &bytes_a).unwrap();
    fs::write(&target_b, &bytes_b).unwrap();

    let targets = vec![
        TransactionTarget::new(
            target_a.clone(),
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&bytes_a),
                revision: 1,
            },
            &bytes_a,
        ),
        TransactionTarget::new(
            target_b.clone(),
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&bytes_b),
                revision: 1,
            },
            &bytes_b,
        ),
    ];
    let event_path = repo.join(".pulse/events/2026-01-01.jsonl");
    let intent = MultiTargetTransactionIntent::prepared(
        "evt_mt_last_written",
        "test.multi",
        "test",
        targets,
        event_path.clone(),
        json!({"id": "evt_mt_last_written", "event": "last_written"}),
    )
    .unwrap();
    let intent_path = persist_multi_target_intent(repo, &intent).unwrap();

    // Both targets After, event absent: write event and cleanup.
    let actions = recover_prepared_transactions(repo).unwrap();
    assert_eq!(
        actions,
        vec![RecoveryAction::EventCompleted {
            intent_path: intent_path.clone(),
            event_path: event_path.clone()
        }]
    );
    assert!(!intent_path.exists());
    assert!(event_path.exists());
}

#[test]
fn multi_target_recovery_cleans_when_all_targets_and_event_present() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    ensure_repo_dirs(repo);

    let target_a = repo.join(".pulse/runtime/a_record.json");
    let target_b = repo.join(".pulse/runtime/b_record.json");

    let bytes_a = to_canonical_bytes(&json!({"id": "a", "val": 1})).unwrap();
    let bytes_b = to_canonical_bytes(&json!({"id": "b", "val": 2})).unwrap();

    // Both targets at After state.
    fs::create_dir_all(target_a.parent().unwrap()).unwrap();
    fs::write(&target_a, &bytes_a).unwrap();
    fs::write(&target_b, &bytes_b).unwrap();

    let targets = vec![
        TransactionTarget::new(
            target_a.clone(),
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&bytes_a),
                revision: 1,
            },
            &bytes_a,
        ),
        TransactionTarget::new(
            target_b.clone(),
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&bytes_b),
                revision: 1,
            },
            &bytes_b,
        ),
    ];
    let event_path = repo.join(".pulse/events/2026-01-01.jsonl");
    let intent = MultiTargetTransactionIntent::prepared(
        "evt_mt_clean",
        "test.multi",
        "test",
        targets,
        event_path.clone(),
        json!({"id": "evt_mt_clean", "event": "clean"}),
    )
    .unwrap();
    let intent_path = persist_multi_target_intent(repo, &intent).unwrap();
    write_event_create_new_multi(&intent).unwrap();

    // Both targets After, event Matching: clean up intent.
    let actions = recover_prepared_transactions(repo).unwrap();
    assert_eq!(
        actions,
        vec![RecoveryAction::CleanedComplete {
            intent_path: intent_path.clone()
        }]
    );
    assert!(!intent_path.exists());
    assert!(event_path.exists());
}

#[test]
fn multi_target_recovery_with_remove_target_rolls_back_when_file_still_present() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    ensure_repo_dirs(repo);

    // Simulate a release: remove lease + create tombstone.
    let lease_path = repo.join(".pulse/runtime/assignment/leases/lease_01JTEST.json");
    let tombstone_path = repo.join(".pulse/runtime/assignment/tombstones/lease_01JTEST.json");

    let lease_bytes =
        to_canonical_bytes(&json!({"id": "lease_01JTEST", "state": "prepared"})).unwrap();
    let tombstone_bytes =
        to_canonical_bytes(&json!({"id": "lease_01JTEST", "state": "released"})).unwrap();

    // Lease file present (Before state).
    fs::create_dir_all(lease_path.parent().unwrap()).unwrap();
    fs::write(&lease_path, &lease_bytes).unwrap();

    // Targets sorted by path: lease (remove), tombstone (create).
    // Since leases/ < tombstones/ lexicographically, lease comes first.
    let targets = vec![
        TransactionTarget::new(
            lease_path.clone(),
            FileState::Present {
                hash: hash_bytes(&lease_bytes),
                revision: 1,
            },
            FileState::Absent,
            &[],
        ),
        TransactionTarget::new(
            tombstone_path.clone(),
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&tombstone_bytes),
                revision: 1,
            },
            &tombstone_bytes,
        ),
    ];
    let event_path = repo.join(".pulse/events/2026-01-01.jsonl");
    let intent = MultiTargetTransactionIntent::prepared(
        "evt_mt_remove_rollback",
        "assignment.release",
        "test",
        targets,
        event_path,
        json!({"id": "evt_mt_remove_rollback", "event": "release"}),
    )
    .unwrap();
    let intent_path = persist_multi_target_intent(repo, &intent).unwrap();

    // Both targets in Before state (lease still exists, tombstone absent), event absent: rollback.
    let actions = recover_prepared_transactions(repo).unwrap();
    assert_eq!(
        actions,
        vec![RecoveryAction::RolledBack {
            intent_path: intent_path.clone()
        }]
    );
    assert!(!intent_path.exists());
    // Lease file must still exist (rollback preserves before state).
    assert!(lease_path.exists());
    assert_eq!(fs::read(&lease_path).unwrap(), lease_bytes);
    // Tombstone must not have been created.
    assert!(!tombstone_path.exists());
}

#[test]
fn multi_target_recovery_with_remove_target_completes_when_file_removed() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    ensure_repo_dirs(repo);

    let lease_path = repo.join(".pulse/runtime/assignment/leases/lease_01JTEST.json");
    let tombstone_path = repo.join(".pulse/runtime/assignment/tombstones/lease_01JTEST.json");

    let lease_bytes =
        to_canonical_bytes(&json!({"id": "lease_01JTEST", "state": "prepared"})).unwrap();
    let tombstone_bytes =
        to_canonical_bytes(&json!({"id": "lease_01JTEST", "state": "released"})).unwrap();

    // Lease file removed (After state for remove target).
    // Tombstone absent (Before state).
    // (lease_path deliberately not created)

    let targets = vec![
        TransactionTarget::new(
            lease_path.clone(),
            FileState::Present {
                hash: hash_bytes(&lease_bytes),
                revision: 1,
            },
            FileState::Absent,
            &[],
        ),
        TransactionTarget::new(
            tombstone_path.clone(),
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&tombstone_bytes),
                revision: 1,
            },
            &tombstone_bytes,
        ),
    ];
    let event_path = repo.join(".pulse/events/2026-01-01.jsonl");
    let intent = MultiTargetTransactionIntent::prepared(
        "evt_mt_remove_complete",
        "assignment.release",
        "test",
        targets,
        event_path.clone(),
        json!({"id": "evt_mt_remove_complete", "event": "release"}),
    )
    .unwrap();
    let intent_path = persist_multi_target_intent(repo, &intent).unwrap();

    // First target (remove) is After, second (create) is Before, event absent: complete.
    let actions = recover_prepared_transactions(repo).unwrap();
    assert_eq!(actions.len(), 1);
    match &actions[0] {
        RecoveryAction::EventCompleted {
            intent_path: i,
            event_path: e,
        } => {
            assert_eq!(*i, intent_path);
            assert_eq!(*e, event_path);
        }
        other => panic!("expected EventCompleted, got {:?}", other),
    }
    assert!(!intent_path.exists());
    assert!(!lease_path.exists());
    assert!(tombstone_path.exists());
    assert_eq!(fs::read(&tombstone_path).unwrap(), tombstone_bytes);
    assert!(event_path.exists());
}

#[test]
fn multi_target_recovery_with_remove_target_cleans_when_all_done() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    ensure_repo_dirs(repo);

    let lease_path = repo.join(".pulse/runtime/assignment/leases/lease_01JTEST.json");
    let tombstone_path = repo.join(".pulse/runtime/assignment/tombstones/lease_01JTEST.json");

    let lease_bytes =
        to_canonical_bytes(&json!({"id": "lease_01JTEST", "state": "prepared"})).unwrap();
    let tombstone_bytes =
        to_canonical_bytes(&json!({"id": "lease_01JTEST", "state": "released"})).unwrap();

    // Lease file removed (After), tombstone created (After).
    // (lease_path not created, tombstone created)
    fs::create_dir_all(tombstone_path.parent().unwrap()).unwrap();
    fs::write(&tombstone_path, &tombstone_bytes).unwrap();

    let targets = vec![
        TransactionTarget::new(
            lease_path.clone(),
            FileState::Present {
                hash: hash_bytes(&lease_bytes),
                revision: 1,
            },
            FileState::Absent,
            &[],
        ),
        TransactionTarget::new(
            tombstone_path.clone(),
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&tombstone_bytes),
                revision: 1,
            },
            &tombstone_bytes,
        ),
    ];
    let event_path = repo.join(".pulse/events/2026-01-01.jsonl");
    let intent = MultiTargetTransactionIntent::prepared(
        "evt_mt_remove_clean",
        "assignment.release",
        "test",
        targets,
        event_path.clone(),
        json!({"id": "evt_mt_remove_clean", "event": "release"}),
    )
    .unwrap();
    let intent_path = persist_multi_target_intent(repo, &intent).unwrap();
    write_event_create_new_multi(&intent).unwrap();

    // Both targets After, event Matching: clean up intent.
    let actions = recover_prepared_transactions(repo).unwrap();
    assert_eq!(
        actions,
        vec![RecoveryAction::CleanedComplete {
            intent_path: intent_path.clone()
        }]
    );
    assert!(!intent_path.exists());
    assert!(!lease_path.exists());
    assert!(tombstone_path.exists());
    assert!(event_path.exists());
}

#[test]
fn multi_target_recovery_mixed_create_and_replace_rolls_back() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    ensure_repo_dirs(repo);

    // Runtime record (create-new) + node (atomic-replace).
    let runtime = repo.join(".pulse/runtime/records/rr.json");
    let node = repo.join(".pulse/workgraph/nodes/TK-002.json");

    let runtime_bytes = to_canonical_bytes(&json!({"id": "rr", "val": 1})).unwrap();
    let node_before = to_canonical_bytes(&json!({"id": "TK-002", "revision": 1})).unwrap();
    let node_after = to_canonical_bytes(&json!({"id": "TK-002", "revision": 2})).unwrap();

    // Node exists at before revision.
    fs::create_dir_all(node.parent().unwrap()).unwrap();
    fs::write(&node, &node_before).unwrap();

    // Targets sorted by path: node comes before runtime.
    let targets = vec![
        TransactionTarget::new(
            runtime.clone(),
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&runtime_bytes),
                revision: 1,
            },
            &runtime_bytes,
        ),
        TransactionTarget::new(
            node.clone(),
            FileState::Present {
                hash: hash_bytes(&node_before),
                revision: 1,
            },
            FileState::Present {
                hash: hash_bytes(&node_after),
                revision: 2,
            },
            &node_after,
        ),
    ];
    let event_path = repo.join(".pulse/events/2026-01-01.jsonl");
    let intent = MultiTargetTransactionIntent::prepared(
        "evt_mt_mixed_rollback",
        "test.mixed",
        "test",
        targets,
        event_path,
        json!({"id": "evt_mt_mixed_rollback", "event": "mixed_rollback"}),
    )
    .unwrap();
    let intent_path = persist_multi_target_intent(repo, &intent).unwrap();

    // Both targets Before, event absent: rollback.
    let actions = recover_prepared_transactions(repo).unwrap();
    assert_eq!(
        actions,
        vec![RecoveryAction::RolledBack {
            intent_path: intent_path.clone()
        }]
    );
    assert!(!intent_path.exists());
    assert!(!runtime.exists());
    assert_eq!(fs::read(&node).unwrap(), node_before);
}

#[test]
fn multi_target_recovery_hard_fails_ambiguous_state() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    ensure_repo_dirs(repo);

    let target_a = repo.join(".pulse/runtime/a_record.json");
    let target_b = repo.join(".pulse/runtime/b_record.json");

    let bytes_a = to_canonical_bytes(&json!({"id": "a"})).unwrap();
    let bytes_b = to_canonical_bytes(&json!({"id": "b"})).unwrap();

    // Write unexpected content to target_a so it's neither Before nor After.
    fs::create_dir_all(target_a.parent().unwrap()).unwrap();
    fs::write(
        &target_a,
        to_canonical_bytes(&json!({"id": "a", "unexpected": true})).unwrap(),
    )
    .unwrap();

    let targets = vec![
        TransactionTarget::new(
            target_a,
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&bytes_a),
                revision: 1,
            },
            &bytes_a,
        ),
        TransactionTarget::new(
            target_b,
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&bytes_b),
                revision: 1,
            },
            &bytes_b,
        ),
    ];
    let event_path = repo.join(".pulse/events/2026-01-01.jsonl");
    let intent = MultiTargetTransactionIntent::prepared(
        "evt_mt_ambiguous",
        "test.multi",
        "test",
        targets,
        event_path,
        json!({"id": "evt_mt_ambiguous", "event": "ambiguous"}),
    )
    .unwrap();
    persist_multi_target_intent(repo, &intent).unwrap();

    // Target a has unexpected content: neither Before nor After → ambiguous.
    let error = recover_prepared_transactions(repo).unwrap_err();
    assert!(matches!(error, PulseError::AmbiguousTransaction { .. }));
}

#[test]
fn multi_target_recovery_hard_fails_event_mismatch() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    ensure_repo_dirs(repo);

    let target_a = repo.join(".pulse/runtime/a_record.json");
    let bytes_a = to_canonical_bytes(&json!({"id": "a"})).unwrap();

    fs::create_dir_all(target_a.parent().unwrap()).unwrap();
    fs::write(&target_a, &bytes_a).unwrap();

    let targets = vec![TransactionTarget::new(
        target_a,
        FileState::Absent,
        FileState::Present {
            hash: hash_bytes(&bytes_a),
            revision: 1,
        },
        &bytes_a,
    )];
    let event_path = repo.join(".pulse/events/2026-01-01.jsonl");
    let intent = MultiTargetTransactionIntent::prepared(
        "evt_mt_event_mismatch",
        "test.multi",
        "test",
        targets,
        event_path.clone(),
        json!({"id": "evt_mt_event_mismatch", "event": "mismatch"}),
    )
    .unwrap();
    persist_multi_target_intent(repo, &intent).unwrap();

    // Pre-write the same event id with different content.
    fs::create_dir_all(event_path.parent().unwrap()).unwrap();
    let mut line = pulse::canonical_json::to_canonical_line_bytes(
        &json!({"id": "evt_mt_event_mismatch", "event": "different"}),
    )
    .unwrap();
    line.push(b'\n');
    fs::write(&event_path, &line).unwrap();

    // Event hash doesn't match → EventMismatch.
    let error = recover_prepared_transactions(repo).unwrap_err();
    assert!(matches!(error, PulseError::EventMismatch { .. }));
    assert!(event_path.exists());
}

#[test]
fn multi_target_recovery_rejects_empty_targets() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    ensure_repo_dirs(repo);

    let err = MultiTargetTransactionIntent::prepared(
        "evt_empty",
        "test.empty",
        "test",
        vec![],
        repo.join(".pulse/events/empty.json"),
        json!({"id": "evt_empty", "event": "empty"}),
    )
    .unwrap_err();
    assert!(matches!(err, PulseError::InvalidTransaction { .. }));
}

#[test]
fn multi_target_preparation_sorts_targets_and_rejects_duplicate_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    ensure_repo_dirs(repo);

    let target_a = repo.join(".pulse/runtime/a_record.json");
    let target_b = repo.join(".pulse/runtime/b_record.json");
    let bytes_a = to_canonical_bytes(&json!({"id": "a"})).unwrap();
    let bytes_b = to_canonical_bytes(&json!({"id": "b"})).unwrap();

    let intent = MultiTargetTransactionIntent::prepared(
        "evt_sorted",
        "test.multi",
        "test",
        vec![
            TransactionTarget::new(
                target_b.clone(),
                FileState::Absent,
                FileState::Present {
                    hash: hash_bytes(&bytes_b),
                    revision: 1,
                },
                &bytes_b,
            ),
            TransactionTarget::new(
                target_a.clone(),
                FileState::Absent,
                FileState::Present {
                    hash: hash_bytes(&bytes_a),
                    revision: 1,
                },
                &bytes_a,
            ),
        ],
        repo.join(".pulse/events/sorted.json"),
        json!({"id": "evt_sorted", "event": "sorted"}),
    )
    .unwrap();
    assert_eq!(intent.targets[0].path, target_a);
    assert_eq!(intent.targets[1].path, target_b);

    let err = MultiTargetTransactionIntent::prepared(
        "evt_duplicate",
        "test.multi",
        "test",
        vec![
            TransactionTarget::new(
                target_a.clone(),
                FileState::Absent,
                FileState::Present {
                    hash: hash_bytes(&bytes_a),
                    revision: 1,
                },
                &bytes_a,
            ),
            TransactionTarget::new(
                target_a.clone(),
                FileState::Absent,
                FileState::Present {
                    hash: hash_bytes(&bytes_b),
                    revision: 1,
                },
                &bytes_b,
            ),
        ],
        repo.join(".pulse/events/duplicate.json"),
        json!({"id": "evt_duplicate", "event": "duplicate"}),
    )
    .unwrap_err();
    assert!(matches!(err, PulseError::InvalidTransaction { .. }));
}

#[test]
fn multi_target_prepare_rejects_hash_mismatch_and_remove_payload() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    ensure_repo_dirs(repo);

    let target = repo.join(".pulse/runtime/record.json");
    let bytes = to_canonical_bytes(&json!({"id": "record"})).unwrap();
    let different_bytes = to_canonical_bytes(&json!({"id": "different"})).unwrap();
    let err = MultiTargetTransactionIntent::prepared(
        "evt_hash_mismatch",
        "test.multi",
        "test",
        vec![TransactionTarget::new(
            target.clone(),
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&different_bytes),
                revision: 1,
            },
            &bytes,
        )],
        repo.join(".pulse/events/hash_mismatch.json"),
        json!({"id": "evt_hash_mismatch", "event": "hash_mismatch"}),
    )
    .unwrap_err();
    assert!(matches!(err, PulseError::InvalidTransaction { .. }));

    let err = MultiTargetTransactionIntent::prepared(
        "evt_remove_payload",
        "test.multi",
        "test",
        vec![TransactionTarget {
            path: target,
            before: FileState::Present {
                hash: hash_bytes(&bytes),
                revision: 1,
            },
            after: FileState::Absent,
            private: false,
            after_bytes_base64: Some(base64::Engine::encode(
                &base64::prelude::BASE64_STANDARD,
                b"unexpected",
            )),
        }],
        repo.join(".pulse/events/remove_payload.json"),
        json!({"id": "evt_remove_payload", "event": "remove_payload"}),
    )
    .unwrap_err();
    assert!(matches!(err, PulseError::InvalidTransaction { .. }));
}

#[test]
fn multi_target_recovery_rejects_event_before_targets() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    ensure_repo_dirs(repo);

    let target_a = repo.join(".pulse/runtime/a_record.json");
    let target_b = repo.join(".pulse/runtime/b_record.json");
    let bytes_a = to_canonical_bytes(&json!({"id": "a"})).unwrap();
    let bytes_b = to_canonical_bytes(&json!({"id": "b"})).unwrap();
    fs::create_dir_all(target_a.parent().unwrap()).unwrap();
    fs::write(&target_a, &bytes_a).unwrap();

    let intent = MultiTargetTransactionIntent::prepared(
        "evt_event_before_targets",
        "test.multi",
        "test",
        vec![
            TransactionTarget::new(
                target_a,
                FileState::Absent,
                FileState::Present {
                    hash: hash_bytes(&bytes_a),
                    revision: 1,
                },
                &bytes_a,
            ),
            TransactionTarget::new(
                target_b,
                FileState::Absent,
                FileState::Present {
                    hash: hash_bytes(&bytes_b),
                    revision: 1,
                },
                &bytes_b,
            ),
        ],
        repo.join(".pulse/events/2026-01-01.jsonl"),
        json!({"id": "evt_event_before_targets", "event": "event_before_targets"}),
    )
    .unwrap();
    persist_multi_target_intent(repo, &intent).unwrap();
    write_event_create_new_multi(&intent).unwrap();

    let err = recover_prepared_transactions(repo).unwrap_err();
    assert!(matches!(err, PulseError::AmbiguousTransaction { .. }));
}
