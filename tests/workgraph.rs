use std::fs;

use chrono::{TimeZone, Utc};
use pulse::graph::edge::EdgeType;
use pulse::graph::store::OperationContext;
use pulse::id::WorkKind;
use pulse::{JsonGraphStore, PulseError};
use tempfile::TempDir;

fn repo() -> (TempDir, JsonGraphStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(dir.path());
    store.bootstrap().unwrap();
    (dir, store)
}

fn ctx(actor: &str, sec: i64) -> OperationContext {
    OperationContext {
        actor: actor.to_string(),
        now: Utc.timestamp_opt(sec, 0).unwrap(),
    }
}

#[test]
fn standalone_ticket_validates() {
    let (_dir, store) = repo();
    let ticket = store
        .create_node_with_context(WorkKind::Ticket, "Standalone".into(), ctx("test", 1))
        .unwrap()
        .value;
    assert_eq!(ticket.id, "TK-001");
    assert_eq!(ticket.content_dir, "works/TK-001");
    let report = store.validate().unwrap();
    assert!(report.valid, "{report:?}");
    assert!(report
        .warnings
        .iter()
        .any(|w| w.code == "missing_draft_content_dir"));
}

#[test]
fn epic_story_ticket_projection_has_inverse_children() {
    let (_dir, store) = repo();
    let ep = store
        .create_node_with_context(WorkKind::Epic, "Epic".into(), ctx("test", 1))
        .unwrap()
        .value;
    let st = store
        .create_node_with_context(WorkKind::Story, "Story".into(), ctx("test", 2))
        .unwrap()
        .value;
    let tk = store
        .create_node_with_context(WorkKind::Ticket, "Ticket".into(), ctx("test", 3))
        .unwrap()
        .value;
    store
        .add_edge_with_context(EdgeType::Parent, st.id.clone(), ep.id.clone(), ctx("test", 4))
        .unwrap();
    store
        .add_edge_with_context(EdgeType::Parent, tk.id.clone(), st.id.clone(), ctx("test", 5))
        .unwrap();
    let projection = store.export().unwrap();
    assert_eq!(projection.nodes.len(), 3);
    assert_eq!(projection.inverse.children.get(&ep.id).unwrap(), &vec![st.id.clone()]);
    assert_eq!(projection.inverse.children.get(&st.id).unwrap(), &vec![tk.id.clone()]);
}

#[test]
fn cas_conflict_reports_current_revision() {
    let (_dir, store) = repo();
    let tk = store
        .create_node_with_context(WorkKind::Ticket, "Old".into(), ctx("test", 1))
        .unwrap()
        .value;
    store
        .edit_title_with_context(&tk.id, 1, "New".into(), ctx("test", 2))
        .unwrap();
    let err = store
        .edit_title_with_context(&tk.id, 1, "Stale".into(), ctx("test", 3))
        .unwrap_err();
    match err {
        PulseError::CasConflict {
            subject,
            expected_revision,
            current_revision,
        } => {
            assert_eq!(subject, tk.id);
            assert_eq!(expected_revision, 1);
            assert_eq!(current_revision, 2);
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn edge_retry_is_unchanged_and_emits_no_event() {
    let (dir, store) = repo();
    let a = store
        .create_node_with_context(WorkKind::Ticket, "A".into(), ctx("test", 1))
        .unwrap()
        .value;
    let b = store
        .create_node_with_context(WorkKind::Ticket, "B".into(), ctx("test", 2))
        .unwrap()
        .value;
    let before_events = event_count(dir.path());
    store
        .add_edge_with_context(EdgeType::BlockedBy, a.id.clone(), b.id.clone(), ctx("test", 3))
        .unwrap();
    let after_create_events = event_count(dir.path());
    let retry = store
        .add_edge_with_context(EdgeType::BlockedBy, a.id.clone(), b.id.clone(), ctx("test", 4))
        .unwrap();
    assert_eq!(retry.code, "unchanged");
    assert_eq!(event_count(dir.path()), after_create_events);
    assert_eq!(after_create_events, before_events + 1);
}

#[test]
fn dangling_edge_rejected_before_commit() {
    let (dir, store) = repo();
    let a = store
        .create_node_with_context(WorkKind::Ticket, "A".into(), ctx("test", 1))
        .unwrap()
        .value;
    let err = store
        .add_edge_with_context(EdgeType::BlockedBy, a.id, "TK-999".into(), ctx("test", 2))
        .unwrap_err();
    assert_eq!(err.code(), "dangling_edge");
    assert_eq!(edge_count(dir.path()), 0);
}

#[test]
fn parent_cycle_rejected() {
    let (_dir, store) = repo();
    let a = store
        .create_node_with_context(WorkKind::Ticket, "A".into(), ctx("test", 1))
        .unwrap()
        .value;
    let b = store
        .create_node_with_context(WorkKind::Ticket, "B".into(), ctx("test", 2))
        .unwrap()
        .value;
    store
        .add_edge_with_context(EdgeType::Parent, a.id.clone(), b.id.clone(), ctx("test", 3))
        .unwrap();
    let err = store
        .add_edge_with_context(EdgeType::Parent, b.id.clone(), a.id.clone(), ctx("test", 4))
        .unwrap_err();
    assert_eq!(err.code(), "invalid_graph");
    assert!(err.to_string().contains("cycle_detected"));
}

#[test]
fn related_is_canonical_and_symmetric() {
    let (_dir, store) = repo();
    let a = store
        .create_node_with_context(WorkKind::Ticket, "A".into(), ctx("test", 1))
        .unwrap()
        .value;
    let b = store
        .create_node_with_context(WorkKind::Ticket, "B".into(), ctx("test", 2))
        .unwrap()
        .value;
    let first = store
        .add_edge_with_context(EdgeType::Related, b.id.clone(), a.id.clone(), ctx("test", 3))
        .unwrap();
    assert_eq!(first.value.from, a.id);
    assert_eq!(first.value.to, b.id);
    let retry = store
        .add_edge_with_context(EdgeType::Related, a.id.clone(), b.id.clone(), ctx("test", 4))
        .unwrap();
    assert_eq!(retry.code, "unchanged");
    let projection = store.export().unwrap();
    assert_eq!(projection.inverse.related.get(&a.id).unwrap(), &vec![b.id.clone()]);
    assert_eq!(projection.inverse.related.get(&b.id).unwrap(), &vec![a.id.clone()]);
}

#[test]
fn cache_delete_rebuilds_equivalent_projection() {
    let (dir, store) = repo();
    store
        .create_node_with_context(WorkKind::Ticket, "A".into(), ctx("test", 1))
        .unwrap();
    let first = store.export().unwrap();
    let cache = dir.path().join(".pulse/cache/workgraph.snapshot.json");
    assert!(cache.exists());
    fs::remove_file(&cache).unwrap();
    let second = store.export().unwrap();
    assert_eq!(first, second);
    assert!(cache.exists());
}

fn event_count(root: &std::path::Path) -> usize {
    let events = root.join(".pulse/events");
    if !events.exists() {
        return 0;
    }
    let mut count = 0;
    for date in fs::read_dir(events).unwrap() {
        for entry in fs::read_dir(date.unwrap().path()).unwrap() {
            if entry.unwrap().path().extension().and_then(|s| s.to_str()) == Some("json") {
                count += 1;
            }
        }
    }
    count
}

fn edge_count(root: &std::path::Path) -> usize {
    fs::read_dir(root.join(".pulse/workgraph/edges"))
        .unwrap()
        .filter(|e| e.as_ref().unwrap().path().extension().and_then(|s| s.to_str()) == Some("json"))
        .count()
}
