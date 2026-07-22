use chrono::{TimeZone, Utc};
use pulse::event::EventEnvelope;
use pulse::graph::lifecycle::TransitionReason;
use pulse::graph::node::NodeStatus;
use pulse::graph::store::{
    OperationContext, SupersessionAssertion, SupersessionClaim, SupersessionTarget,
};
use pulse::id::WorkKind;
use pulse::JsonGraphStore;
use std::fs;

fn ctx(actor: &str, sec: i64) -> OperationContext {
    OperationContext {
        actor: actor.to_string(),
        now: Utc.timestamp_opt(sec, 0).unwrap(),
    }
}

fn reason(code: &str) -> TransitionReason {
    TransitionReason {
        code: code.to_string(),
        summary: format!("summary for {code}"),
        reference: None,
    }
}

fn assertion(source: String, reference: String) -> SupersessionAssertion {
    SupersessionAssertion {
        assertion_version: 1,
        asserted_by: "human:test".to_string(),
        source_revisions: vec![source],
        claim: SupersessionClaim::Absorbed,
        references: vec![reference],
    }
}

fn events(root: &std::path::Path, event_type: &str) -> Vec<EventEnvelope> {
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
        .map(|path| serde_json::from_slice::<EventEnvelope>(&fs::read(path).unwrap()).unwrap())
        .filter(|event| event.event_type == event_type)
        .collect()
}

#[test]
fn lifecycle_transition_event_records_graph_fingerprints_before_and_after() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(dir.path());
    store.bootstrap().unwrap();
    let ticket = store
        .create_node_with_context(WorkKind::Ticket, "Ticket".into(), ctx("test", 1))
        .unwrap()
        .value;
    let before = store.export().unwrap().graph_fingerprint;

    store
        .transition_node_with_context(
            &ticket.id,
            NodeStatus::Cancelled,
            1,
            Some(reason("obsolete")),
            ctx("test", 2),
        )
        .unwrap();
    let after = store.export().unwrap().graph_fingerprint;

    let transitioned = events(dir.path(), "work.node.transitioned");
    assert_eq!(transitioned.len(), 1);
    let payload = &transitioned[0].payload;
    assert_eq!(payload["graph_fingerprint_before"], before);
    assert_eq!(payload["graph_fingerprint_after"], after);
    assert_ne!(before, after);
}

#[test]
fn supersession_event_records_graph_fingerprints_before_and_after() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(dir.path());
    store.bootstrap().unwrap();
    let old = store
        .create_node_with_context(WorkKind::Ticket, "Old".into(), ctx("test", 1))
        .unwrap()
        .value;
    let replacement = store
        .create_node_with_context(WorkKind::Ticket, "Replacement".into(), ctx("test", 2))
        .unwrap()
        .value;
    let before = store.export().unwrap().graph_fingerprint;

    store
        .supersede_work_with_context(
            &old.id,
            SupersessionTarget::Replacement {
                id: replacement.id.clone(),
            },
            1,
            "absorbed by replacement".into(),
            assertion(format!("{}@1", old.id), replacement.id),
            ctx("human:test", 3),
        )
        .unwrap();
    let after = store.export().unwrap().graph_fingerprint;

    let superseded = events(dir.path(), "work.node.superseded");
    assert_eq!(superseded.len(), 1);
    let payload = &superseded[0].payload;
    assert_eq!(payload["graph_fingerprint_before"], before);
    assert_eq!(payload["graph_fingerprint_after"], after);
    assert_ne!(before, after);
}
