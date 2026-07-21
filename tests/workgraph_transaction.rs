use std::fs;
use std::thread;

use chrono::{TimeZone, Utc};
use pulse::canonical_json::{hash_bytes, to_canonical_bytes};
use pulse::event::EventEnvelope;
use pulse::graph::edge::{Edge, EdgeType};
use pulse::graph::node::{Node, NodeStatus, StatusReason};
use pulse::graph::store::OperationContext;
use pulse::id::WorkKind;
use pulse::storage::atomic::atomic_replace;
use pulse::storage::transaction::{
    persist_intent, persist_multi_target_intent, recover_prepared_transactions, FileState,
    MultiTargetTransactionIntent, RecoveryAction, TransactionIntent, TransactionTarget,
};
use pulse::{JsonGraphStore, PulseError};

fn ctx(actor: &str, sec: i64) -> OperationContext {
    OperationContext {
        actor: actor.to_string(),
        now: Utc.timestamp_opt(sec, 0).unwrap(),
    }
}

#[test]
fn graph_mutations_use_prepared_intent_and_consistent_event_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(dir.path());
    store.bootstrap().unwrap();

    let node = store
        .create_node_with_context(WorkKind::Ticket, "Initial".into(), ctx("test:actor", 1))
        .unwrap()
        .value;
    let updated = store
        .edit_title_with_context(&node.id, 1, "Updated".into(), ctx("test:actor", 2))
        .unwrap()
        .value;
    let blocker = store
        .create_node_with_context(WorkKind::Ticket, "Blocker".into(), ctx("test:actor", 3))
        .unwrap()
        .value;
    store
        .add_edge_with_context(
            EdgeType::BlockedBy,
            updated.id.clone(),
            blocker.id.clone(),
            ctx("test:actor", 4),
        )
        .unwrap();

    assert!(dir.path().join(".pulse/runtime/transactions").is_dir());
    assert_eq!(transaction_count(dir.path()), 0);

    let events = read_events(dir.path());
    assert_eq!(events.len(), 4);
    assert!(events.iter().all(|event| event.schema_version == 1));
    assert!(events.iter().all(|event| event.id.starts_with("evt_")));
    assert_eq!(events[0].event_type, "work.node.created");
    assert_eq!(events[1].event_type, "work.node.updated");
    assert_eq!(events[2].event_type, "work.node.created");
    assert_eq!(events[3].event_type, "work.edge.created");
    assert_eq!(events[1].payload["expected_revision"], 1);
}

#[test]
fn mutation_recovers_prior_after_canonical_intent_before_allocating_next_id() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    let store = JsonGraphStore::new(repo);
    store.bootstrap().unwrap();

    let target = repo.join(".pulse/workgraph/nodes/TK-001.json");
    let node = pulse::graph::node::Node::new(
        "TK-001".to_string(),
        WorkKind::Ticket,
        "Recovered".to_string(),
        Utc.timestamp_opt(1, 0).unwrap(),
    )
    .unwrap();
    let node_bytes = to_canonical_bytes(&node).unwrap();
    let event = EventEnvelope::new(
        "evt_recover_create",
        "work.node.created",
        "test:actor",
        "TK-001",
        serde_json::json!({"node": node}),
        Utc.timestamp_opt(1, 0).unwrap(),
    );
    let event_payload = serde_json::to_value(&event).unwrap();
    let intent = TransactionIntent::prepared(
        event.id.clone(),
        event.event_type.clone(),
        event.actor.clone(),
        target.clone(),
        pulse::event::event_path(repo, &event),
        FileState::Absent,
        FileState::Present {
            hash: hash_bytes(&node_bytes),
            revision: 1,
        },
        event_payload,
    )
    .unwrap();
    persist_intent(repo, &intent).unwrap();
    atomic_replace(&target, &node_bytes).unwrap();

    let next = store
        .create_node_with_context(WorkKind::Ticket, "Next".into(), ctx("test:actor", 2))
        .unwrap()
        .value;

    assert_eq!(next.id, "TK-002");
    assert!(intent.event_path.exists());
    assert_eq!(transaction_count(repo), 0);
    assert_eq!(read_events(repo).len(), 2);
}

#[test]
fn multi_target_recovery_rolls_forward_prefix_after_and_writes_event() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    JsonGraphStore::new(repo).bootstrap().unwrap();

    let old = Node::new(
        "TK-001".to_string(),
        WorkKind::Ticket,
        "Old".to_string(),
        Utc.timestamp_opt(1, 0).unwrap(),
    )
    .unwrap();
    let replacement = Node::new(
        "TK-002".to_string(),
        WorkKind::Ticket,
        "Replacement".to_string(),
        Utc.timestamp_opt(1, 0).unwrap(),
    )
    .unwrap();
    let old_path = repo.join(".pulse/workgraph/nodes/TK-001.json");
    let replacement_path = repo.join(".pulse/workgraph/nodes/TK-002.json");
    let old_before = to_canonical_bytes(&old).unwrap();
    fs::write(&old_path, &old_before).unwrap();
    fs::write(&replacement_path, to_canonical_bytes(&replacement).unwrap()).unwrap();

    let mut old_after = old.clone();
    old_after.status = NodeStatus::Superseded;
    old_after.status_reason = Some(StatusReason::new("superseded", "absorbed", None).unwrap());
    old_after.revision = 2;
    let old_after_bytes = to_canonical_bytes(&old_after).unwrap();
    let edge = Edge::new(
        EdgeType::SupersededBy,
        old.id.clone(),
        replacement.id.clone(),
        "test".into(),
        Utc.timestamp_opt(2, 0).unwrap(),
    )
    .unwrap();
    let edge_path = repo
        .join(".pulse/workgraph/edges")
        .join(format!("{}.json", edge.id));
    let edge_bytes = to_canonical_bytes(&edge).unwrap();
    let event_payload = serde_json::json!({"schema_version":1,"id":"evt_multi","event_type":"work.node.superseded","actor":"test","occurred_at":"1970-01-01T00:00:02Z","subject":"TK-001","payload":{"ok":true}});
    let event_path = repo.join(".pulse/events/1970-01-01/evt_multi.json");
    let intent = MultiTargetTransactionIntent::prepared(
        "evt_multi",
        "work.node.superseded",
        "test",
        vec![
            TransactionTarget::new(
                old_path.clone(),
                FileState::Present {
                    hash: hash_bytes(&old_before),
                    revision: 1,
                },
                FileState::Present {
                    hash: hash_bytes(&old_after_bytes),
                    revision: 2,
                },
                &old_after_bytes,
            ),
            TransactionTarget::new(
                edge_path.clone(),
                FileState::Absent,
                FileState::Present {
                    hash: hash_bytes(&edge_bytes),
                    revision: 1,
                },
                &edge_bytes,
            ),
        ],
        event_path.clone(),
        event_payload,
    )
    .unwrap();
    let first_target_path = intent.targets[0].path.clone();
    let first_target_bytes = intent.targets[0].after_bytes().unwrap();
    let intent_path = persist_multi_target_intent(repo, &intent).unwrap();
    atomic_replace(&first_target_path, &first_target_bytes).unwrap();

    let actions = recover_prepared_transactions(repo).unwrap();
    assert_eq!(
        actions,
        vec![RecoveryAction::EventCompleted {
            intent_path,
            event_path: event_path.clone()
        }]
    );
    assert_eq!(fs::read(&edge_path).unwrap(), edge_bytes);
    assert!(event_path.exists());
}

#[test]
fn multi_target_recovery_stops_on_ambiguous_manual_edit() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    JsonGraphStore::new(repo).bootstrap().unwrap();
    let first = repo.join(".pulse/workgraph/nodes/TK-001.json");
    let second = repo.join(".pulse/workgraph/edges/superseded-by--TK-001--TK-002.json");
    fs::write(&first, b"{\"revision\":999}\n").unwrap();
    let after = b"{\"revision\":1}\n";
    let intent = MultiTargetTransactionIntent::prepared(
        "evt_ambiguous",
        "work.node.superseded",
        "test",
        vec![
            TransactionTarget::new(first.clone(), FileState::Absent, FileState::Present { hash: hash_bytes(after), revision: 1 }, after),
            TransactionTarget::new(second, FileState::Absent, FileState::Present { hash: hash_bytes(after), revision: 1 }, after),
        ],
        repo.join(".pulse/events/1970-01-01/evt_ambiguous.json"),
        serde_json::json!({"schema_version":1,"id":"evt_ambiguous","event_type":"work.node.superseded","actor":"test","occurred_at":"1970-01-01T00:00:01Z","subject":"TK-001","payload":{}}),
    ).unwrap();
    let intent_path = persist_multi_target_intent(repo, &intent).unwrap();

    let err = recover_prepared_transactions(repo).unwrap_err();
    assert_eq!(err.code(), "ambiguous_transaction");
    assert!(intent_path.exists());
}

#[test]
fn recovery_event_create_is_idempotent_for_matching_existing_event() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    pulse::storage::bootstrap(repo).unwrap();

    let target = repo.join(".pulse/workgraph/nodes/TK-001.json");
    let after_bytes = to_canonical_bytes(&serde_json::json!({"id":"TK-001","revision":1})).unwrap();
    fs::write(&target, &after_bytes).unwrap();
    let event_payload = serde_json::json!({"schema_version":1,"id":"evt_idempotent","event_type":"test","actor":"test","occurred_at":"1970-01-01T00:00:01Z","subject":"TK-001","payload":{"ok":true}});
    let event_path = repo.join(".pulse/events/1970-01-01/evt_idempotent.json");
    fs::create_dir_all(event_path.parent().unwrap()).unwrap();
    fs::write(&event_path, to_canonical_bytes(&event_payload).unwrap()).unwrap();
    let intent = TransactionIntent::prepared(
        "evt_idempotent",
        "test",
        "test",
        target,
        event_path.clone(),
        FileState::Absent,
        FileState::Present {
            hash: hash_bytes(&after_bytes),
            revision: 1,
        },
        event_payload,
    )
    .unwrap();
    let intent_path = persist_intent(repo, &intent).unwrap();

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
fn concurrent_same_revision_edit_has_one_success_and_one_cas_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().to_path_buf();
    let store = JsonGraphStore::new(&repo);
    store.bootstrap().unwrap();
    let node = store
        .create_node_with_context(WorkKind::Ticket, "Initial".into(), ctx("test", 1))
        .unwrap()
        .value;

    let left_repo = repo.clone();
    let right_repo = repo.clone();
    let left_id = node.id.clone();
    let right_id = node.id.clone();
    let left = thread::spawn(move || {
        JsonGraphStore::new(left_repo).edit_title_with_context(
            &left_id,
            1,
            "Left".into(),
            ctx("left", 2),
        )
    });
    let right = thread::spawn(move || {
        JsonGraphStore::new(right_repo).edit_title_with_context(
            &right_id,
            1,
            "Right".into(),
            ctx("right", 3),
        )
    });

    let results = [left.join().unwrap(), right.join().unwrap()];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(PulseError::CasConflict { .. })))
            .count(),
        1
    );
    assert_eq!(transaction_count(&repo), 0);
}

#[test]
fn concurrent_different_node_edits_both_succeed_without_shared_canonical_file() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().to_path_buf();
    let store = JsonGraphStore::new(&repo);
    store.bootstrap().unwrap();
    let first = store
        .create_node_with_context(WorkKind::Ticket, "First".into(), ctx("test", 1))
        .unwrap()
        .value;
    let second = store
        .create_node_with_context(WorkKind::Ticket, "Second".into(), ctx("test", 2))
        .unwrap()
        .value;

    let left_repo = repo.clone();
    let right_repo = repo.clone();
    let left = thread::spawn(move || {
        JsonGraphStore::new(left_repo).edit_title_with_context(
            &first.id,
            1,
            "First updated".into(),
            ctx("left", 3),
        )
    });
    let right = thread::spawn(move || {
        JsonGraphStore::new(right_repo).edit_title_with_context(
            &second.id,
            1,
            "Second updated".into(),
            ctx("right", 4),
        )
    });

    assert!(left.join().unwrap().is_ok());
    assert!(right.join().unwrap().is_ok());
    assert_eq!(transaction_count(&repo), 0);
    assert_eq!(read_events(&repo).len(), 4);
}

fn transaction_count(root: &std::path::Path) -> usize {
    let dir = root.join(".pulse/runtime/transactions");
    if !dir.exists() {
        return 0;
    }
    fs::read_dir(dir)
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                == Some("json")
        })
        .count()
}

fn read_events(root: &std::path::Path) -> Vec<EventEnvelope> {
    let events = root.join(".pulse/events");
    if !events.exists() {
        return vec![];
    }
    let mut paths = vec![];
    for date in fs::read_dir(events).unwrap() {
        for entry in fs::read_dir(date.unwrap().path()).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                paths.push(path);
            }
        }
    }
    paths.sort();
    paths
        .into_iter()
        .map(|path| serde_json::from_slice(&fs::read(path).unwrap()).unwrap())
        .collect()
}
