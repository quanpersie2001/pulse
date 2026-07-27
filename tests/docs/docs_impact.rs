use std::fs;
use std::process::Command;

use chrono::{TimeZone, Utc};
use pulse::event::EventEnvelope;
use pulse::graph::node::DocumentationImpactPosture;
use pulse::graph::store::{DocumentationImpactUpdate, OperationContext};
use pulse::id::WorkKind;
use pulse::{JsonGraphStore, PulseError};
use tempfile::TempDir;

use crate::common_bin::bin;

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

fn update(posture: DocumentationImpactPosture) -> DocumentationImpactUpdate {
    DocumentationImpactUpdate {
        posture,
        rationale: None,
        required_documents: vec![],
        deferred_to: vec![],
        paths: vec![],
        domains: vec![],
        labels: vec![],
    }
}

fn events(repo_root: &std::path::Path) -> Vec<EventEnvelope> {
    let root = repo_root.join(".pulse/events");
    if !root.exists() {
        return vec![];
    }
    let mut out: Vec<EventEnvelope> = vec![];
    let mut stack = vec![root];
    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                stack.push(entry.path());
            } else {
                let bytes = fs::read(entry.path()).unwrap();
                out.push(serde_json::from_slice(&bytes).unwrap());
            }
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

#[test]
fn missing_documentation_metadata_derives_unknown() {
    let (_dir, store) = repo();
    let ticket = store
        .create_node_with_context(WorkKind::Ticket, "Ticket".into(), ctx("human:test", 1))
        .unwrap()
        .value;

    assert!(ticket.documentation.is_none());
    assert_eq!(
        ticket.documentation_posture(),
        DocumentationImpactPosture::Unknown
    );
    let json = serde_json::to_string(&ticket).unwrap();
    assert!(!json.contains("documentation"));
    let round_trip: pulse::graph::node::Node = serde_json::from_str(&json).unwrap();
    assert_eq!(
        round_trip.documentation_posture(),
        DocumentationImpactPosture::Unknown
    );
}

#[test]
fn required_impact_updates_ticket_revision_and_event_but_not_status() {
    let (dir, store) = repo();
    let ticket = store
        .create_node_with_context(WorkKind::Ticket, "Ticket".into(), ctx("human:test", 1))
        .unwrap()
        .value;
    let status = ticket.status;
    let mut req = update(DocumentationImpactPosture::Required);
    req.rationale = Some("Public token behavior changes.".into());
    req.required_documents = vec!["DOC-AUTH-DOMAIN".into()];
    req.paths = vec!["src/auth/**".into()];
    req.domains = vec!["authentication".into()];
    req.labels = vec!["auth".into()];

    let out = store
        .update_documentation_impact_with_context(
            &ticket.id,
            ticket.revision,
            req,
            ctx("human:docs", 2),
        )
        .unwrap();

    assert_eq!(out.value.revision, ticket.revision + 1);
    assert_eq!(out.value.status, status);
    let docs = out.value.documentation.unwrap();
    assert_eq!(docs.impact.posture, DocumentationImpactPosture::Required);
    assert_eq!(docs.impact.required_documents, vec!["DOC-AUTH-DOMAIN"]);
    assert_eq!(docs.routing.paths, vec!["src/auth/**"]);

    let events = events(dir.path());
    let event = events
        .iter()
        .find(|event| event.event_type == "work.documentation_impact.updated")
        .unwrap_or_else(|| panic!("documentation impact event missing"));
    assert_eq!(event.actor.kind, pulse::event::EventActorKind::Human);
    assert_eq!(event.actor.id, "docs");
    assert_eq!(event.subject.kind, "ticket");
    assert_eq!(event.subject.id, ticket.id);
    assert_eq!(event.subject.revision, Some(ticket.revision + 1));
    assert_eq!(event.payload["expected_revision"], ticket.revision);
    assert_eq!(event.payload["new_revision"], ticket.revision + 1);
}

#[test]
fn required_impact_uses_node_revision_cas() {
    let (_dir, store) = repo();
    let ticket = store
        .create_node_with_context(WorkKind::Ticket, "Ticket".into(), ctx("human:test", 1))
        .unwrap()
        .value;
    let mut req = update(DocumentationImpactPosture::Required);
    req.required_documents = vec!["DOC-AUTH-DOMAIN".into()];

    let err = store
        .update_documentation_impact_with_context(
            &ticket.id,
            ticket.revision + 1,
            req,
            ctx("human:docs", 2),
        )
        .unwrap_err();

    match err {
        PulseError::CasConflict {
            subject,
            expected_revision,
            current_revision,
        } => {
            assert_eq!(subject, ticket.id);
            assert_eq!(expected_revision, ticket.revision + 1);
            assert_eq!(current_revision, ticket.revision);
        }
        other => panic!("expected CAS conflict, got {other:?}"),
    }
}

#[test]
fn none_impact_missing_rationale_is_rejected() {
    let (_dir, store) = repo();
    let ticket = store
        .create_node_with_context(WorkKind::Ticket, "Ticket".into(), ctx("human:test", 1))
        .unwrap()
        .value;

    let err = store
        .update_documentation_impact_with_context(
            &ticket.id,
            ticket.revision,
            update(DocumentationImpactPosture::None),
            ctx("human:docs", 2),
        )
        .unwrap_err();

    assert_eq!(err.code(), "documentation_rationale_required");
}

#[test]
fn deferred_impact_missing_target_is_rejected() {
    let (_dir, store) = repo();
    let ticket = store
        .create_node_with_context(WorkKind::Ticket, "Ticket".into(), ctx("human:test", 1))
        .unwrap()
        .value;
    let mut deferred = update(DocumentationImpactPosture::Deferred);
    deferred.rationale = Some("Follow-up docs work split out.".into());
    deferred.deferred_to = vec!["TK-999".into()];

    let err = store
        .update_documentation_impact_with_context(
            &ticket.id,
            ticket.revision,
            deferred,
            ctx("human:docs", 2),
        )
        .unwrap_err();

    assert_eq!(err.code(), "documentation_defer_target_missing");
}

#[test]
fn non_ticket_impact_is_rejected() {
    let (_dir, store) = repo();
    let story = store
        .create_node_with_context(WorkKind::Story, "Story".into(), ctx("human:test", 1))
        .unwrap()
        .value;
    let mut req = update(DocumentationImpactPosture::Required);
    req.required_documents = vec!["DOC-AUTH-DOMAIN".into()];

    let err = store
        .update_documentation_impact_with_context(
            &story.id,
            story.revision,
            req,
            ctx("human:docs", 2),
        )
        .unwrap_err();

    assert_eq!(err.code(), "documentation_impact_requires_ticket");
}

#[test]
fn cli_docs_impact_json_contract() {
    let (dir, store) = repo();
    let ticket = store
        .create_node_with_context(WorkKind::Ticket, "Ticket".into(), ctx("human:test", 1))
        .unwrap()
        .value;

    let output = Command::new(bin())
        .arg("--repo-root")
        .arg(dir.path())
        .args([
            "docs",
            "impact",
            &ticket.id,
            "--expected-revision",
            &ticket.revision.to_string(),
            "--posture",
            "required",
            "--rationale",
            "Public auth behavior changes.",
            "--required-doc",
            "DOC-AUTH-DOMAIN",
            "--path",
            "src/auth/**",
            "--domain",
            "authentication",
            "--label",
            "auth",
            "--actor",
            "human:docs",
            "--json",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["code"], "updated");
    assert_eq!(value["value"]["id"], ticket.id);
    assert_eq!(value["value"]["revision"], ticket.revision + 1);
    assert_eq!(
        value["value"]["documentation"]["impact"]["posture"],
        "required"
    );
    assert_eq!(
        value["value"]["documentation"]["impact"]["required_documents"],
        serde_json::json!(["DOC-AUTH-DOMAIN"])
    );
}
