use std::fs;

use chrono::{TimeZone, Utc};
use pulse::canonical_json::{hash_bytes, to_canonical_bytes};
use pulse::graph::edge::{deterministic_edge_id, Edge, EdgeType};
use pulse::graph::node::NodeStatus;
use pulse::graph::store::{
    OperationContext, SupersessionAssertion, SupersessionClaim, SupersessionTarget,
};
use pulse::graph::validate::ValidationReport;
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

const SLICE1_NODE_SCHEMA: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Pulse Work Graph Node",
  "type": "object",
  "required": [
    "schema_version",
    "id",
    "kind",
    "revision",
    "title",
    "status",
    "content_dir",
    "created_at",
    "updated_at"
  ],
  "properties": {
    "content_dir": {
      "type": "string"
    },
    "created_at": {
      "format": "date-time",
      "type": "string"
    },
    "id": {
      "pattern": "^(EP|ST|TK|DEC)-[0-9]{3,}$",
      "type": "string"
    },
    "kind": {
      "enum": ["epic", "story", "ticket", "decision"],
      "type": "string"
    },
    "revision": {
      "minimum": 1,
      "type": "integer"
    },
    "schema_version": {
      "const": 1
    },
    "status": {
      "enum": ["draft", "shaped", "ready", "active", "verifying", "done", "rework", "blocked", "superseded"],
      "type": "string"
    },
    "title": {
      "minLength": 1,
      "type": "string"
    },
    "updated_at": {
      "format": "date-time",
      "type": "string"
    }
  },
  "additionalProperties": false
}
"#;

fn assertion(source: &str, references: Vec<String>) -> SupersessionAssertion {
    SupersessionAssertion {
        assertion_version: 1,
        asserted_by: "human:test".to_string(),
        source_revisions: vec![source.to_string()],
        claim: SupersessionClaim::Absorbed,
        references,
    }
}

#[test]
fn bootstrap_upgrades_exact_slice1_node_schema_and_emits_migration_event() {
    let dir = tempfile::tempdir().unwrap();
    let wg = dir.path().join(".pulse/workgraph");
    fs::create_dir_all(wg.join("schemas")).unwrap();
    fs::create_dir_all(wg.join("nodes")).unwrap();
    fs::create_dir_all(wg.join("edges")).unwrap();
    fs::write(
        wg.join("manifest.json"),
        to_canonical_bytes(&pulse::graph::manifest::Manifest::default()).unwrap(),
    )
    .unwrap();
    fs::write(wg.join("schemas/node.schema.json"), SLICE1_NODE_SCHEMA).unwrap();

    let store = JsonGraphStore::new(dir.path());
    store.bootstrap().unwrap();

    let upgraded = fs::read(wg.join("schemas/node.schema.json")).unwrap();
    assert_eq!(upgraded, pulse::graph::manifest::NODE_SCHEMA.as_bytes());
    let events_dir = dir.path().join(".pulse/events/1970-01-01");
    assert!(
        !events_dir.exists(),
        "migration event should use wall-clock date, not fixture date"
    );
    let event_count = walkdir_count(dir.path().join(".pulse/events"));
    assert_eq!(event_count, 1);
}

#[test]
fn bootstrap_refuses_unknown_node_schema_without_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let wg = dir.path().join(".pulse/workgraph");
    fs::create_dir_all(wg.join("schemas")).unwrap();
    fs::write(wg.join("schemas/node.schema.json"), b"{\"unknown\":true}\n").unwrap();
    let before_hash = hash_bytes(&fs::read(wg.join("schemas/node.schema.json")).unwrap());

    let store = JsonGraphStore::new(dir.path());
    let err = store.bootstrap().unwrap_err();
    assert_eq!(err.code(), "node_schema_upgrade_refused");
    let after_hash = hash_bytes(&fs::read(wg.join("schemas/node.schema.json")).unwrap());
    assert_eq!(after_hash, before_hash);
}

fn walkdir_count(path: impl AsRef<std::path::Path>) -> usize {
    let path = path.as_ref();
    if !path.exists() {
        return 0;
    }
    let mut count = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                stack.push(entry.path());
            } else {
                count += 1;
            }
        }
    }
    count
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
        .add_edge_with_context(
            EdgeType::Parent,
            st.id.clone(),
            ep.id.clone(),
            ctx("test", 4),
        )
        .unwrap();
    store
        .add_edge_with_context(
            EdgeType::Parent,
            tk.id.clone(),
            st.id.clone(),
            ctx("test", 5),
        )
        .unwrap();
    let projection = store.export().unwrap();
    assert_eq!(projection.schema_version, 2);
    assert_eq!(projection.nodes.len(), 3);
    assert_eq!(
        projection.inverse.children.get(&ep.id).unwrap(),
        &vec![st.id.clone()]
    );
    assert_eq!(
        projection.inverse.children.get(&st.id).unwrap(),
        &vec![tk.id.clone()]
    );
    assert!(projection.lifecycle.rollups.contains_key(&ep.id));
    assert!(projection
        .lifecycle
        .structural_executability
        .contains_key(&tk.id));
    assert_eq!(
        projection
            .lifecycle
            .status_classes
            .get(&pulse::graph::lifecycle::StatusClass::Preparation)
            .unwrap(),
        &vec![ep.id.clone(), st.id.clone(), tk.id.clone()]
    );
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
        .add_edge_with_context(
            EdgeType::BlockedBy,
            a.id.clone(),
            b.id.clone(),
            ctx("test", 3),
        )
        .unwrap();
    let after_create_events = event_count(dir.path());
    let retry = store
        .add_edge_with_context(
            EdgeType::BlockedBy,
            a.id.clone(),
            b.id.clone(),
            ctx("test", 4),
        )
        .unwrap();
    assert_eq!(retry.code, "unchanged");
    assert_eq!(event_count(dir.path()), after_create_events);
    assert_eq!(after_create_events, before_events + 1);
}

#[test]
fn supersede_by_replacement_updates_node_edge_and_one_event() {
    let (dir, store) = repo();
    let old = store
        .create_node_with_context(WorkKind::Ticket, "Old".into(), ctx("test", 1))
        .unwrap()
        .value;
    let new = store
        .create_node_with_context(WorkKind::Ticket, "New".into(), ctx("test", 2))
        .unwrap()
        .value;
    let before_events = event_count(dir.path());

    let out = store
        .supersede_work_with_context(
            &old.id,
            SupersessionTarget::Replacement { id: new.id.clone() },
            1,
            "absorbed by replacement".into(),
            assertion("TK-001@1", vec![new.id.clone()]),
            ctx("human:test", 3),
        )
        .unwrap();

    assert_eq!(out.code, "superseded");
    assert_eq!(out.value.node.status, NodeStatus::Superseded);
    assert_eq!(out.value.node.revision, 2);
    assert!(out
        .value
        .node
        .status_reason
        .as_ref()
        .unwrap()
        .reference
        .is_none());
    let edge = out.value.edge.unwrap();
    assert_eq!(edge.edge_type, EdgeType::SupersededBy);
    assert_eq!(edge.from, old.id);
    assert_eq!(edge.to, new.id);
    assert_eq!(event_count(dir.path()), before_events + 1);
    assert!(store.validate().unwrap().valid);
}

#[test]
fn supersede_by_decision_uses_status_reason_without_edge() {
    let (dir, store) = repo();
    let old = store
        .create_node_with_context(WorkKind::Ticket, "Old".into(), ctx("test", 1))
        .unwrap()
        .value;
    let decision = store
        .create_node_with_context(WorkKind::Decision, "Decision".into(), ctx("test", 2))
        .unwrap()
        .value;
    let before_events = event_count(dir.path());

    let out = store
        .supersede_work_with_context(
            &old.id,
            SupersessionTarget::Decision {
                id: decision.id.clone(),
            },
            1,
            "explained by decision".into(),
            assertion("TK-001@1", vec![decision.id.clone()]),
            ctx("human:test", 3),
        )
        .unwrap();

    assert!(out.value.edge.is_none());
    assert_eq!(
        out.value.node.status_reason.unwrap().reference,
        Some(decision.id)
    );
    assert_eq!(edge_count(dir.path()), 0);
    assert_eq!(event_count(dir.path()), before_events + 1);
}

#[test]
fn supersede_retry_same_is_unchanged_and_different_conflicts() {
    let (dir, store) = repo();
    let old = store
        .create_node_with_context(WorkKind::Ticket, "Old".into(), ctx("test", 1))
        .unwrap()
        .value;
    let new = store
        .create_node_with_context(WorkKind::Ticket, "New".into(), ctx("test", 2))
        .unwrap()
        .value;
    let other = store
        .create_node_with_context(WorkKind::Ticket, "Other".into(), ctx("test", 3))
        .unwrap()
        .value;
    let first_assertion = assertion("TK-001@1", vec![new.id.clone()]);
    store
        .supersede_work_with_context(
            &old.id,
            SupersessionTarget::Replacement { id: new.id.clone() },
            1,
            "absorbed".into(),
            first_assertion.clone(),
            ctx("human:test", 4),
        )
        .unwrap();
    let events = event_count(dir.path());

    let retry = store
        .supersede_work_with_context(
            &old.id,
            SupersessionTarget::Replacement { id: new.id },
            1,
            "absorbed".into(),
            first_assertion,
            ctx("human:test", 5),
        )
        .unwrap();
    assert_eq!(retry.code, "unchanged");
    assert_eq!(event_count(dir.path()), events);

    let err = store
        .supersede_work_with_context(
            &old.id,
            SupersessionTarget::Replacement { id: other.id },
            2,
            "different".into(),
            assertion("TK-001@2", vec![]),
            ctx("human:test", 6),
        )
        .unwrap_err();
    assert_eq!(err.code(), "supersession_conflict");
}

#[test]
fn supersede_rejects_self_cycle_terminal_and_bad_assertion() {
    let (_dir, store) = repo();
    let old = store
        .create_node_with_context(WorkKind::Ticket, "Old".into(), ctx("test", 1))
        .unwrap()
        .value;
    let err = store
        .supersede_work_with_context(
            &old.id,
            SupersessionTarget::Replacement { id: old.id.clone() },
            1,
            "self".into(),
            assertion("TK-001@1", vec![]),
            ctx("human:test", 2),
        )
        .unwrap_err();
    assert_eq!(err.code(), "supersession_cycle");

    let replacement = store
        .create_node_with_context(WorkKind::Ticket, "Replacement".into(), ctx("test", 3))
        .unwrap()
        .value;
    let err = store
        .supersede_work_with_context(
            &old.id,
            SupersessionTarget::Replacement { id: replacement.id },
            1,
            "stale".into(),
            assertion("TK-001@999", vec![]),
            ctx("human:test", 4),
        )
        .unwrap_err();
    assert_eq!(err.code(), "assertion_revision_mismatch");

    store
        .transition_node_with_context(
            &old.id,
            NodeStatus::Cancelled,
            1,
            Some(pulse::graph::lifecycle::TransitionReason {
                code: "cancelled".into(),
                summary: "not needed".into(),
                reference: None,
            }),
            ctx("human:test", 5),
        )
        .unwrap();
    let next = store
        .create_node_with_context(WorkKind::Ticket, "Next".into(), ctx("test", 6))
        .unwrap()
        .value;
    let err = store
        .supersede_work_with_context(
            &old.id,
            SupersessionTarget::Replacement { id: next.id },
            2,
            "terminal".into(),
            assertion("TK-001@2", vec![]),
            ctx("human:test", 7),
        )
        .unwrap_err();
    assert_eq!(err.code(), "supersession_unavailable");
}

#[test]
fn generic_superseded_by_edge_add_is_rejected() {
    let (_dir, store) = repo();
    let a = store
        .create_node_with_context(WorkKind::Ticket, "A".into(), ctx("test", 1))
        .unwrap()
        .value;
    let b = store
        .create_node_with_context(WorkKind::Ticket, "B".into(), ctx("test", 2))
        .unwrap()
        .value;
    let err = store
        .add_edge_with_context(EdgeType::SupersededBy, a.id, b.id, ctx("test", 3))
        .unwrap_err();
    assert_eq!(err.code(), "superseded_by_lifecycle_owned");
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
        .add_edge_with_context(
            EdgeType::Related,
            b.id.clone(),
            a.id.clone(),
            ctx("test", 3),
        )
        .unwrap();
    assert_eq!(first.value.from, a.id);
    assert_eq!(first.value.to, b.id);
    let retry = store
        .add_edge_with_context(
            EdgeType::Related,
            a.id.clone(),
            b.id.clone(),
            ctx("test", 4),
        )
        .unwrap();
    assert_eq!(retry.code, "unchanged");
    let projection = store.export().unwrap();
    assert_eq!(
        projection.inverse.related.get(&a.id).unwrap(),
        &vec![b.id.clone()]
    );
    assert_eq!(
        projection.inverse.related.get(&b.id).unwrap(),
        &vec![a.id.clone()]
    );
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

#[test]
fn corrupt_cache_rebuilds_without_changing_projection() {
    let (dir, store) = repo();
    store
        .create_node_with_context(WorkKind::Ticket, "A".into(), ctx("test", 1))
        .unwrap();
    let first = store.export().unwrap();
    let cache = dir.path().join(".pulse/cache/workgraph.snapshot.json");
    fs::write(&cache, b"not json").unwrap();
    let second = store.export().unwrap();
    assert_eq!(first, second);
    assert_ne!(fs::read(&cache).unwrap(), b"not json");
}

#[test]
fn schema_template_mismatch_refuses_unknown_upgrade_without_overwrite() {
    let (dir, store) = repo();
    let schema_path = dir.path().join(".pulse/workgraph/schemas/node.schema.json");
    fs::write(&schema_path, b"{\"schema_version\":999}\n").unwrap();
    let before = fs::read(&schema_path).unwrap();
    let err = store.validate().unwrap_err();
    assert_eq!(err.code(), "node_schema_upgrade_refused");
    assert_eq!(fs::read(&schema_path).unwrap(), before);
}

#[test]
fn canonical_drift_is_validation_warning() {
    let (dir, store) = repo();
    let tk = store
        .create_node_with_context(WorkKind::Ticket, "A".into(), ctx("test", 1))
        .unwrap()
        .value;
    let path = dir
        .path()
        .join(".pulse/workgraph/nodes")
        .join(format!("{}.json", tk.id));
    let compact = serde_json::to_vec(&tk).unwrap();
    fs::write(&path, compact).unwrap();
    let report = store.validate().unwrap();
    assert!(report.valid, "{report:?}");
    assert!(report
        .warnings
        .iter()
        .any(|finding| finding.code == "node_canonical_drift"));
}

#[test]
fn edge_file_payload_mismatch_invalidates_graph() {
    let (dir, store) = repo();
    let a = store
        .create_node_with_context(WorkKind::Ticket, "A".into(), ctx("test", 1))
        .unwrap()
        .value;
    let b = store
        .create_node_with_context(WorkKind::Ticket, "B".into(), ctx("test", 2))
        .unwrap()
        .value;
    let c = store
        .create_node_with_context(WorkKind::Ticket, "C".into(), ctx("test", 3))
        .unwrap()
        .value;
    let mut edge = Edge::new(
        EdgeType::BlockedBy,
        a.id.clone(),
        b.id,
        "test".into(),
        ctx("test", 4).now,
    )
    .unwrap();
    let file_id = edge.id.clone();
    edge.to = c.id;
    edge.id = file_id.clone();
    fs::write(
        dir.path()
            .join(".pulse/workgraph/edges")
            .join(format!("{file_id}.json")),
        to_canonical_bytes(&edge).unwrap(),
    )
    .unwrap();

    let report = store.validate().unwrap();
    assert_finding(&report, "edge_id_mismatch");
    assert_eq!(
        deterministic_edge_id(EdgeType::BlockedBy, &edge.from, &edge.to),
        format!("blocked-by--{}--{}", edge.from, edge.to)
    );
}

fn assert_finding(report: &ValidationReport, code: &str) {
    assert!(
        report.errors.iter().any(|finding| finding.code == code),
        "expected finding {code}, got {report:?}"
    );
}

fn edge_count(root: &std::path::Path) -> usize {
    fs::read_dir(root.join(".pulse/workgraph/edges"))
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .path()
                .extension()
                .and_then(|s| s.to_str())
                == Some("json")
        })
        .count()
}
