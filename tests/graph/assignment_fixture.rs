use std::fs;

use chrono::Utc;
use pulse::canonical_json::to_canonical_bytes;
use pulse::docs::{DocumentKind, DocumentRecord, DocumentScope, DocumentStatus};
use pulse::graph::model::contract::{PublicCreateClassification, TicketRole};
use pulse::graph::model::node::NodeStatus;
use pulse::graph::store::OperationContext;
use pulse::id::WorkKind;
use pulse::JsonGraphStore;

use super::common_fixture_repo::TestRepo;
use super::common_git::commit_all;

fn context() -> OperationContext {
    OperationContext {
        actor: "human:tester".to_string(),
        now: Utc::now(),
    }
}

pub(super) fn write_policy(root: &std::path::Path, extra_grants: &[&str]) {
    let path = root.join(".pulse/policy/authority.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut grants = vec![
        "work.transition.shaped".to_string(),
        "work.transition.ready".to_string(),
        "work.assignment.prepare".to_string(),
        "work.close".to_string(),
        "work.node.create".to_string(),
    ];
    grants.extend(extra_grants.iter().map(|grant| grant.to_string()));
    grants.sort();
    grants.dedup();
    let mut policy = pulse::policy::AuthorityPolicy {
        schema_version: 1,
        revision: 1,
        principals: vec![
            pulse::policy::AuthorityPrincipal {
                kind: pulse::identity::actor::ActorKind::Agent,
                id: "tester".to_string(),
                grants: grants.clone(),
            },
            pulse::policy::AuthorityPrincipal {
                kind: pulse::identity::actor::ActorKind::Human,
                id: "tester".to_string(),
                grants,
            },
        ],
    };
    policy.normalize();
    fs::write(path, to_canonical_bytes(&policy).unwrap()).unwrap();
}

pub(super) fn bootstrap_repo(repo: &TestRepo, _store: &JsonGraphStore) {
    write_policy(repo.path(), &[]);
    repo.pulse_ok(&["graph", "bootstrap", "--json"]);
    pulse::evidence::manifest::load(repo.path()).unwrap();
    pulse::docs::manifest::bootstrap(repo.path()).unwrap();
}

pub(super) fn setup_ready_ticket(root: &std::path::Path, store: &JsonGraphStore) -> String {
    setup_ready_ticket_with_qa(root, store, FixtureQaPosture::None)
}

pub(super) fn setup_ready_ticket_with_required_qa(
    root: &std::path::Path,
    store: &JsonGraphStore,
) -> String {
    setup_ready_ticket_with_qa(root, store, FixtureQaPosture::Required)
}

pub(super) fn setup_ready_ticket_with_story_qa(
    root: &std::path::Path,
    store: &JsonGraphStore,
) -> String {
    setup_ready_ticket_with_qa(root, store, FixtureQaPosture::CoveredByStoryClose)
}

pub(super) fn setup_ready_ticket_with_required_docs(
    repo: &TestRepo,
    store: &JsonGraphStore,
) -> String {
    let path = "docs/domain/reservation.md";
    fs::create_dir_all(repo.path().join("docs/domain")).unwrap();
    fs::write(
        repo.path().join(path),
        b"# Reservation contract\n\nThe reservation remains stable.\n",
    )
    .unwrap();
    pulse::docs::register(
        repo.path(),
        1,
        DocumentRecord {
            tags: vec![],
            id: "DOC-RESERVATION-CONTRACT".to_string(),
            revision: 1,
            path: path.to_string(),
            kind: DocumentKind::Domain,
            status: DocumentStatus::Approved,
            owner: "team:platform".to_string(),
            summary: "Reservation close contract".to_string(),
            scope: DocumentScope {
                paths: vec!["src/**".to_string(), "development".to_string()],
            },
            generated: None,
            superseded_by: None,
        },
        "human:tester",
    )
    .unwrap();
    let optional_path = "docs/domain/operations.md";
    fs::write(
        repo.path().join(optional_path),
        b"# Operations guidance\n\nOperational context.\n",
    )
    .unwrap();
    pulse::docs::register(
        repo.path(),
        2,
        DocumentRecord {
            tags: vec![],
            id: "DOC-OPERATIONS-GUIDANCE".to_string(),
            revision: 1,
            path: optional_path.to_string(),
            kind: DocumentKind::Domain,
            status: DocumentStatus::Approved,
            owner: "team:platform".to_string(),
            summary: "Optional operations guidance".to_string(),
            scope: DocumentScope::default(),
            generated: None,
            superseded_by: None,
        },
        "human:tester",
    )
    .unwrap();
    repo.pulse_ok(&["docs", "index", "--json"]);
    setup_ready_ticket_with_postures(
        repo.path(),
        store,
        FixtureQaPosture::None,
        FixtureDocsPosture::Required,
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FixtureQaPosture {
    None,
    Required,
    CoveredByStoryClose,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FixtureDocsPosture {
    None,
    Required,
}

fn setup_ready_ticket_with_qa(
    root: &std::path::Path,
    store: &JsonGraphStore,
    qa_posture: FixtureQaPosture,
) -> String {
    setup_ready_ticket_with_postures(root, store, qa_posture, FixtureDocsPosture::None)
}

fn setup_ready_ticket_with_postures(
    root: &std::path::Path,
    store: &JsonGraphStore,
    qa_posture: FixtureQaPosture,
    docs_posture: FixtureDocsPosture,
) -> String {
    let node = store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Test reservation ticket".to_string(),
            PublicCreateClassification {
                role: Some(TicketRole::Implementation),
                risk: Some(pulse::graph::model::contract::Risk::Low),
                materialization: Some(pulse::graph::model::contract::Materialization::R1),
            },
            context(),
        )
        .unwrap()
        .value;
    let ticket_id = node.id.clone();

    // QA owner Story is created first when the posture needs one so the
    // ticket.md contract can reference it.
    let behavioral_owner = match qa_posture {
        FixtureQaPosture::None => None,
        FixtureQaPosture::Required | FixtureQaPosture::CoveredByStoryClose => {
            let story = store
                .create_node(WorkKind::Story, "Reservation behavior".to_string())
                .unwrap()
                .value;
            if qa_posture == FixtureQaPosture::Required {
                write_required_qa_baseline(root, &story.id);
            }
            Some(story.id)
        }
    };

    let qa_section = match qa_posture {
        FixtureQaPosture::None => "## QA impact\n- Owner:\n- Posture: none\n- Cases:\n- Reason: No product QA impact.\n".to_string(),
        FixtureQaPosture::Required => format!(
            "## QA impact\n- Owner: {}\n- Posture: required\n- Cases: QA-001\n- Reason: Behavioral checkpoint required.\n",
            behavioral_owner.clone().unwrap()
        ),
        FixtureQaPosture::CoveredByStoryClose => format!(
            "## QA impact\n- Owner: {}\n- Posture: covered_by_story_close\n- Cases:\n- Reason: Integrated Story qualification owns this behavior.\n",
            behavioral_owner.clone().unwrap()
        ),
    };
    let docs_section = match docs_posture {
        FixtureDocsPosture::None => "## Documentation impact\n- Posture: none\n- Rationale: No durable docs impact.\n- Documents:\n".to_string(),
        FixtureDocsPosture::Required => "## Documentation impact\n- Posture: required\n- Rationale: Reservation contract must remain validated.\n- Documents: DOC-RESERVATION-CONTRACT\n".to_string(),
    };
    let ticket_md = format!(
        "# {ticket_id} Test reservation ticket\n\n\
         ## Objective\nTest reservation objective.\n\n\
         ## Current behavior\nCurrent behavior.\n\n\
         ## Target behavior\nTarget behavior.\n\n\
         ## Code anchors\n- src/token.mjs\n\n\
         ## Required changes\n- Make the ticket reservable.\n\n\
         ## Invariants\n- Keep repository semantics Core-owned.\n\n\
         ## Implementation freedom\nguided: agent chooses internal structure within this contract.\n\n\
         ## Acceptance\n- AC-1: Reservation remains ready until acknowledgement.\n\n\
         ## Verify\n- node scripts/verify.mjs\n\n\
         {docs_section}\n\
         {qa_section}\n"
    );
    let brief_path = root.join(&node.content_dir).join("ticket.md");
    fs::create_dir_all(brief_path.parent().unwrap()).unwrap();
    fs::write(&brief_path, ticket_md).unwrap();

    store
        .sync_ticket_with_context(&ticket_id, node.revision, context())
        .unwrap();

    store
        .transition_node_with_context(
            &ticket_id,
            NodeStatus::Shaped,
            store.show_node(&ticket_id).unwrap().revision,
            None,
            context(),
        )
        .unwrap();
    store
        .transition_node_with_context(
            &ticket_id,
            NodeStatus::Ready,
            store.show_node(&ticket_id).unwrap().revision,
            None,
            context(),
        )
        .unwrap();
    commit_all(root);
    ticket_id
}

fn write_required_qa_baseline(root: &std::path::Path, story_id: &str) {
    let path = root.join(format!("works/{story_id}/qa.md"));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        format!(
            r#"# Reservation behavioral QA

```pulse-qa
{{
  "schema_version": 1,
  "story_id": "{story_id}",
  "revision": 1,
  "scope": "Reservation behavior remains observable.",
  "risks": ["RISK-DUPLICATE"],
  "cases": [{{
    "id": "QA-001",
    "revision": 1,
    "intent": "Reservation is not duplicated.",
    "priority": "critical",
    "risk_refs": ["RISK-DUPLICATE"],
    "steps": ["reserve twice with one idempotency key"],
    "expected": ["one stable reservation"],
    "surface": "api",
    "applicability": "required"
  }}],
  "exit_criteria": ["The required case passes on the candidate source."]
}}
```
"#,
        ),
    )
    .unwrap();
}
