use pulse::canonical_json::{hash_bytes, to_canonical_bytes};
use pulse::error::PulseError;
use pulse::storage::atomic::atomic_replace;
use pulse::storage::paths::{configured_content_root, resolve_content_path, resolve_repo_relative};
use pulse::storage::transaction::{
    persist_intent, recover_prepared_transactions, write_event_create_new, FileState, RecoveryAction,
    TransactionIntent,
};
use pulse::storage::{bootstrap, MANIFEST_JSON};
use serde_json::json;
use std::fs;

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
    let error = to_canonical_bytes(&json!({"slice_number": 1.5})).unwrap_err();
    assert!(matches!(error, PulseError::FloatRejected { .. }));
}

#[test]
fn bootstrap_is_idempotent_and_does_not_overwrite_user_files() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();

    let first = bootstrap(repo).unwrap();
    assert!(repo.join(".pulse/workgraph/manifest.json").exists());
    assert!(repo.join(".pulse/workgraph/schemas/node.schema.json").exists());
    assert!(repo.join(".pulse/runtime/transactions").is_dir());
    assert_eq!(
        first.proposed_ignore_entries,
        vec![".pulse/runtime/".to_string(), ".pulse/cache/".to_string()]
    );

    let manifest_path = repo.join(".pulse/workgraph/manifest.json");
    fs::write(&manifest_path, b"user-owned manifest\n").unwrap();
    let second = bootstrap(repo).unwrap();
    assert_eq!(fs::read(&manifest_path).unwrap(), b"user-owned manifest\n");
    assert!(second.preserved.contains(&manifest_path));

    let template_value: serde_json::Value = serde_json::from_str(MANIFEST_JSON).unwrap();
    assert!(to_canonical_bytes(&template_value).unwrap().ends_with(b"\n"));
}

#[test]
fn safe_paths_reject_traversal_and_symlink_escape() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    fs::create_dir(repo.join("works")).unwrap();

    let traversal = resolve_repo_relative(repo, "works/../secret").unwrap_err();
    assert!(matches!(traversal, PulseError::PathTraversal { .. }));

    let content_escape = resolve_content_path(repo, ".pulse/workgraph").unwrap_err();
    assert!(matches!(content_escape, PulseError::ContentRootViolation { .. }));

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
    bootstrap(repo).unwrap();

    let target = repo.join(".pulse/workgraph/nodes/TK-001.json");
    let before_bytes = to_canonical_bytes(&json!({"id": "TK-001", "revision": 1})).unwrap();
    fs::write(&target, &before_bytes).unwrap();
    let after_bytes = to_canonical_bytes(&json!({"id": "TK-001", "revision": 2})).unwrap();
    let event_payload = json!({"event": "node_updated", "id": "TK-001"});
    let intent = TransactionIntent::prepared(
        "evt_test_rollback",
        "node.update",
        "test",
        target.clone(),
        repo.join(".pulse/events/2026-01-01/evt_test_rollback.json"),
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
    bootstrap(repo).unwrap();

    let target = repo.join(".pulse/workgraph/nodes/TK-001.json");
    let before_bytes = to_canonical_bytes(&json!({"id": "TK-001", "revision": 1})).unwrap();
    let after_bytes = to_canonical_bytes(&json!({"id": "TK-001", "revision": 2})).unwrap();
    fs::write(&target, &after_bytes).unwrap();
    let event_payload = json!({"event": "node_updated", "id": "TK-001"});
    let event_path = repo.join(".pulse/events/2026-01-01/evt_test_complete.json");
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
    bootstrap(repo).unwrap();

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
        repo.join(".pulse/events/2026-01-01/evt_test_ambiguous.json"),
        FileState::Present {
            hash: hash_bytes(&before_bytes),
            revision: 1,
        },
        FileState::Present {
            hash: hash_bytes(&after_bytes),
            revision: 2,
        },
        json!({"event": "node_updated", "id": "TK-001"}),
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
    bootstrap(repo).unwrap();

    let target = repo.join(".pulse/workgraph/nodes/TK-001.json");
    let before_bytes = to_canonical_bytes(&json!({"id": "TK-001", "revision": 1})).unwrap();
    let after_bytes = to_canonical_bytes(&json!({"id": "TK-001", "revision": 2})).unwrap();
    fs::write(&target, &after_bytes).unwrap();
    let event_path = repo.join(".pulse/events/2026-01-01/evt_test_mismatch.json");
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
        json!({"event": "node_updated", "id": "TK-001"}),
    )
    .unwrap();
    persist_intent(repo, &intent).unwrap();
    fs::create_dir_all(event_path.parent().unwrap()).unwrap();
    fs::write(&event_path, to_canonical_bytes(&json!({"event": "different"})).unwrap()).unwrap();

    let error = recover_prepared_transactions(repo).unwrap_err();
    assert!(matches!(error, PulseError::EventMismatch { .. }));
    assert!(event_path.exists());
    assert!(intent.intent_path(repo).exists());
}

#[test]
fn transaction_recovery_cleans_after_event_before_intent_cleanup() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    bootstrap(repo).unwrap();

    let target = repo.join(".pulse/workgraph/nodes/TK-001.json");
    let before_bytes = to_canonical_bytes(&json!({"id": "TK-001", "revision": 1})).unwrap();
    let after_bytes = to_canonical_bytes(&json!({"id": "TK-001", "revision": 2})).unwrap();
    fs::write(&target, &after_bytes).unwrap();
    let intent = TransactionIntent::prepared(
        "evt_test_clean",
        "node.update",
        "test",
        target,
        repo.join(".pulse/events/2026-01-01/evt_test_clean.json"),
        FileState::Present {
            hash: hash_bytes(&before_bytes),
            revision: 1,
        },
        FileState::Present {
            hash: hash_bytes(&after_bytes),
            revision: 2,
        },
        json!({"event": "node_updated", "id": "TK-001"}),
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
