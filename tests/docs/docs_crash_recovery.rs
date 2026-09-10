use chrono::{TimeZone, Utc};
use pulse::docs::{
    DocsRegistryStore, DocumentKind, DocumentRecord, DocumentScope, DocumentStatus,
    OperationContext as DocsOperationContext,
};
use pulse::graph::model::node::DocumentationImpactPosture;
use pulse::graph::store::{
    DocumentationImpactUpdate, JsonGraphStore, OperationContext as GraphOperationContext,
};
use pulse::id::WorkKind;
use pulse::storage::transaction::{recover_prepared_transactions, TransactionFailpoint};
use std::fs;

fn write_doc(repo: &std::path::Path, path: &str) {
    let full = repo.join(path);
    fs::create_dir_all(full.parent().expect("doc parent")).expect("create doc parent");
    fs::write(full, format!("# {path}\n")).expect("write doc");
}

fn document(id: &str, path: &str) -> DocumentRecord {
    DocumentRecord {
        id: id.to_string(),
        revision: 1,
        path: path.to_string(),
        kind: DocumentKind::Domain,
        status: DocumentStatus::Approved,
        owner: "team:docs".to_string(),
        summary: format!("Summary for {id}"),
        scope: DocumentScope {
            paths: vec!["src/auth/**".to_string()],
        },
        tags: vec![],
        generated: None,
        superseded_by: None,
    }
}

#[test]
fn registry_register_recovers_missing_event_after_canonical_write() {
    let tmp = tempfile::tempdir().expect("temp repo");
    let repo = tmp.path();
    write_doc(repo, "docs/domain/auth.md");
    let store = DocsRegistryStore::new(repo);
    store.bootstrap().expect("bootstrap docs");

    let failing = DocsRegistryStore::with_failpoint(repo, TransactionFailpoint::AfterCanonical);
    let error = failing
        .register(
            1,
            document("DOC-AUTH-DOMAIN", "docs/domain/auth.md"),
            DocsOperationContext {
                actor: "human:test".to_string(),
                now: Utc.timestamp_opt(2, 0).single().expect("timestamp"),
            },
        )
        .expect_err("failpoint should abort after registry write");
    assert_eq!(error.code(), "failpoint");
    assert_eq!(store.load().expect("load after recovery").revision, 2);
    assert_eq!(
        recover_prepared_transactions(repo).expect("recover").len(),
        0
    );

    let event_count = count_events(repo, "docs.document.registered");
    assert_eq!(event_count, 1);
    let shown = store.show("DOC-AUTH-DOMAIN").expect("show recovered doc");
    assert_eq!(shown.path, "docs/domain/auth.md");
}

#[test]
fn registry_register_rolls_back_intent_before_canonical_write() {
    let tmp = tempfile::tempdir().expect("temp repo");
    let repo = tmp.path();
    write_doc(repo, "docs/domain/auth.md");
    let store = DocsRegistryStore::new(repo);
    store.bootstrap().expect("bootstrap docs");

    let failing = DocsRegistryStore::with_failpoint(repo, TransactionFailpoint::AfterIntent);
    let error = failing
        .register(
            1,
            document("DOC-AUTH-DOMAIN", "docs/domain/auth.md"),
            DocsOperationContext {
                actor: "human:test".to_string(),
                now: Utc.timestamp_opt(2, 0).single().expect("timestamp"),
            },
        )
        .expect_err("failpoint should abort before registry write");
    assert_eq!(error.code(), "failpoint");

    let registry = store.load().expect("load after rollback");
    assert_eq!(registry.revision, 1);
    assert!(registry.documents.is_empty());
    assert_eq!(count_events(repo, "docs.document.registered"), 0);
}

#[test]
fn documentation_impact_recovers_missing_event_after_node_write() {
    let tmp = tempfile::tempdir().expect("temp repo");
    let repo = tmp.path();
    let base = JsonGraphStore::new(repo);
    let ticket = base
        .create_node_with_context(
            WorkKind::Ticket,
            "Ticket".to_string(),
            GraphOperationContext {
                actor: "human:test".to_string(),
                now: Utc.timestamp_opt(1, 0).single().expect("timestamp"),
            },
        )
        .expect("create ticket")
        .value;

    let failing = JsonGraphStore::with_failpoint(repo, TransactionFailpoint::AfterCanonical);
    let error = failing
        .update_documentation_impact_with_context(
            &ticket.id,
            ticket.revision,
            required_update(),
            GraphOperationContext {
                actor: "human:docs".to_string(),
                now: Utc.timestamp_opt(2, 0).single().expect("timestamp"),
            },
        )
        .expect_err("failpoint should abort after node write");
    assert_eq!(error.code(), "failpoint");

    let recovered = base.show_node(&ticket.id).expect("show recovered ticket");
    assert_eq!(recovered.revision, ticket.revision + 1);
    assert_eq!(
        recovered
            .documentation
            .expect("documentation")
            .impact
            .posture,
        DocumentationImpactPosture::Required
    );
    assert_eq!(count_events(repo, "work.documentation_impact.updated"), 1);
}

fn required_update() -> DocumentationImpactUpdate {
    DocumentationImpactUpdate {
        posture: DocumentationImpactPosture::Required,
        rationale: Some("Public auth behavior changes.".to_string()),
        required_documents: vec!["DOC-AUTH-DOMAIN".to_string()],
        deferred_to: Vec::new(),
        paths: vec!["src/auth/**".to_string()],
        domains: vec!["authentication".to_string()],
        labels: vec!["auth".to_string()],
    }
}

fn count_events(repo: &std::path::Path, event_type: &str) -> usize {
    crate::common_events::events_of_type(repo, event_type).len()
}
