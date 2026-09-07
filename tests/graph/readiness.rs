//! Readiness composition, narrow fingerprint, stale-ready semantics and
//! lifecycle gate tests.
//!
//! The Ticket contract is `works/<id>/ticket.md`, bound by `brief_hash` and
//! synced via `sync_ticket`; QA and documentation postures are derived from its
//! markdown sections. There is no separate implementation/decision contract and
//! no shaping receipt ceremony. These tests exercise the harness against
//! temporary target repositories only; they never point Pulse at this
//! development repository.

use chrono::Utc;
use pulse::graph::model::contract::{
    Materialization, PublicCreateClassification, Risk, TicketRole,
};
use pulse::graph::model::lifecycle::{installed_gate, GateProfile, TransitionReason};
use pulse::graph::model::node::NodeStatus;
use pulse::graph::read::readiness::{
    self, GateFamilyReport, GateStatus, ReadinessReport, ReadinessStatus, READINESS_PROFILE,
};
use pulse::graph::store::OperationContext;
use pulse::id::WorkKind;
use pulse::policy::{AuthorityPolicy, AuthorityPrincipal};
use pulse::storage::transaction::TransactionFailpoint;
use pulse::JsonGraphStore;
use std::fs;

use crate::common_canon::write_json;
use pulse::docs::{DocumentKind, DocumentRecord, DocumentScope, DocumentStatus};

fn write_policy(repo: &std::path::Path, grants: &[&str]) {
    let mut sorted_grants = grants.iter().map(|g| g.to_string()).collect::<Vec<_>>();
    sorted_grants.sort();
    sorted_grants.dedup();
    let policy = AuthorityPolicy {
        schema_version: 1,
        revision: 1,
        principals: vec![AuthorityPrincipal {
            kind: pulse::identity::actor::ActorKind::Human,
            id: "tester".to_string(),
            grants: sorted_grants,
        }],
    };
    let path = repo.join(".pulse/policy/authority.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_json(&path, &policy);
}

fn full_grants() -> &'static [&'static str] {
    &["work.transition.shaped", "work.transition.ready"]
}

fn ctx() -> OperationContext {
    OperationContext {
        actor: "human:tester".to_string(),
        now: Utc::now(),
    }
}

fn reason(code: &str, summary: &str) -> Option<TransitionReason> {
    Some(TransitionReason {
        code: code.to_string(),
        summary: summary.to_string(),
        reference: None,
    })
}

fn create_ticket(store: &JsonGraphStore) -> pulse::graph::model::node::Node {
    let classification = PublicCreateClassification {
        role: Some(TicketRole::Implementation),
        risk: Some(Risk::Low),
        materialization: Some(Materialization::R1),
    };
    store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Sample ticket".to_string(),
            classification,
            OperationContext::default(),
        )
        .unwrap()
        .value
}

const DOCS_NONE_SECTION: &str = "## Documentation impact\n- Posture: none\n- Rationale: No durable docs impact.\n- Documents:\n";
const QA_NONE_SECTION: &str =
    "## QA impact\n- Owner:\n- Posture: none\n- Cases:\n- Reason: No product QA impact.\n";

/// Render the full `ticket.md` contract. Omitted sections are left out entirely
/// (so `## Documentation impact` / `## QA impact` absence leaves the derived
/// posture at its default `unknown`).
fn ticket_md(
    id: &str,
    docs_section: Option<&str>,
    qa_section: Option<&str>,
    open_questions_body: Option<&str>,
) -> String {
    let docs = docs_section
        .map(|section| format!("{section}\n"))
        .unwrap_or_default();
    let qa = qa_section
        .map(|section| format!("{section}\n"))
        .unwrap_or_default();
    let questions = open_questions_body
        .map(|body| format!("## Open questions\n{body}\n"))
        .unwrap_or_default();
    format!(
        "# {id} Test ticket\n\n\
         ## Objective\nDistinguish expired and invalid tokens.\n\n\
         ## Current behavior\nBoth map to InvalidToken.\n\n\
         ## Target behavior\nExpired maps to TokenExpired.\n\n\
         ## Code anchors\n- src/auth.rs\n\n\
         ## Required changes\n- Introduce the expired-token error.\n\n\
         ## Invariants\n- Do not leak secrets.\n\n\
         ## Implementation freedom\nguided: agent chooses internal structure within this contract.\n\n\
         ## Acceptance\n- AC-1: Expired token is classified.\n\n\
         ## Verify\n- cargo test\n\n\
         {docs}\
         {qa}\
         {questions}"
    )
}

/// Write `ticket.md` and bind it with `sync_ticket` so `brief_hash` and the
/// derived docs/QA metadata match the markdown. Returns the updated node.
fn bind_brief(
    repo: &std::path::Path,
    store: &JsonGraphStore,
    node: &pulse::graph::model::node::Node,
    docs_section: Option<&str>,
    qa_section: Option<&str>,
    open_questions_body: Option<&str>,
) -> pulse::graph::model::node::Node {
    let path = repo.join(format!("{}/ticket.md", node.content_dir));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        ticket_md(&node.id, docs_section, qa_section, open_questions_body),
    )
    .unwrap();
    let revision = store.show_node(&node.id).unwrap().revision;
    store
        .sync_ticket_with_context(&node.id, revision, ctx())
        .unwrap()
        .value
}

/// A Draft ticket whose `ticket.md` is bound (brief_hash current) with docs/QA
/// posture `none`.
fn bound_ticket(repo: &std::path::Path, store: &JsonGraphStore) -> pulse::graph::model::node::Node {
    let node = create_ticket(store);
    bind_brief(
        repo,
        store,
        &node,
        Some(DOCS_NONE_SECTION),
        Some(QA_NONE_SECTION),
        None,
    )
}

fn transition(
    store: &JsonGraphStore,
    id: &str,
    to: NodeStatus,
    reason: Option<TransitionReason>,
) -> pulse::graph::model::node::Node {
    let revision = store.show_node(id).unwrap().revision;
    store
        .transition_node_with_context(id, to, revision, reason, ctx())
        .unwrap()
        .value
}

fn ready_ticket(repo: &std::path::Path, store: &JsonGraphStore) -> pulse::graph::model::node::Node {
    write_policy(repo, full_grants());
    let node = bound_ticket(repo, store);
    let shaped = transition(store, &node.id, NodeStatus::Shaped, None);
    transition(store, &shaped.id, NodeStatus::Ready, None)
}

fn write_qa_baseline(repo: &std::path::Path, story_id: &str, case_id: &str) {
    let path = repo.join(format!("works/{story_id}/qa.md"));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        format!(
            r#"# {story_id} Behavioral QA

## Scope
Authentication recovery remains observable and bounded.

## Posture
automated

## Risks
- RISK-LOOP: expired credentials retry forever.

## Exit criteria
- All required cases pass on the candidate source.

## Cases

### {case_id} Expired credentials recover without a loop
- Intent: Expired credentials recover without a loop.
- Surface: api
- Priority: critical
- Risks: RISK-LOOP
- Steps:
  1. invoke the protected operation
- Expected:
  - one refresh and a successful retry
"#,
        ),
    )
    .unwrap();
}

fn family<'a>(report: &'a ReadinessReport, name: &str) -> &'a GateFamilyReport {
    report
        .gate_families
        .iter()
        .find(|f| f.family == name)
        .unwrap_or_else(|| panic!("missing gate family {name}"))
}

fn collect_events(repo: &std::path::Path, event_type: &str) -> Vec<serde_json::Value> {
    let events = repo.join(".pulse/events");
    let mut out = Vec::new();
    if let Ok(days) = fs::read_dir(&events) {
        for day in days.flatten() {
            if !day.path().is_dir() {
                continue;
            }
            for entry in fs::read_dir(day.path()).unwrap().flatten() {
                let v: serde_json::Value =
                    serde_json::from_slice(&fs::read(entry.path()).unwrap()).unwrap();
                if v.get("event_type").and_then(|v| v.as_str()) == Some(event_type) {
                    out.push(v);
                }
            }
        }
    }
    out
}

#[test]
fn ready_ticket_reports_ready_with_full_gate_families() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = ready_ticket(repo, &store);

    let report = store.readiness(&node.id).unwrap();
    assert_eq!(report.profile, READINESS_PROFILE);
    assert_eq!(report.status, ReadinessStatus::Ready);
    assert!(report.transition_eligible);
    assert!(!report.dispatch_authorized);
    assert!(report.readiness_fingerprint.starts_with("sha256:"));
    // The gate families appear in the fixed evaluation order, with no legacy
    // implementation-contract/shaping families present.
    let expected_order = [
        "graph_validity",
        "work_kind_and_role",
        "lifecycle_eligibility",
        "structural_executability",
        "ticket_ambiguity",
        "authority",
        "documentation_impact",
        "applicable_documents",
        "qa_impact",
        "content_reference_integrity",
    ];
    assert_eq!(report.gate_families.len(), expected_order.len());
    for (expected, actual) in expected_order.iter().zip(&report.gate_families) {
        assert_eq!(&actual.family, expected);
    }
    assert_eq!(
        family(&report, "structural_executability").status,
        GateStatus::Passed
    );
    assert_eq!(
        family(&report, "ticket_ambiguity").status,
        GateStatus::Passed
    );
    assert_eq!(family(&report, "qa_impact").status, GateStatus::Passed);
    assert_eq!(
        family(&report, "documentation_impact").status,
        GateStatus::Passed
    );
    assert_eq!(
        family(&report, "applicable_documents").status,
        GateStatus::NotApplicable
    );
    assert_eq!(
        family(&report, "content_reference_integrity").status,
        GateStatus::Passed
    );
    assert_eq!(family(&report, "authority").status, GateStatus::Passed);
    assert_eq!(report.future_gate_families.len(), 3);
    assert!(report
        .future_gate_families
        .iter()
        .all(|f| f.status == GateStatus::NotEvaluated));
}

#[test]
fn fingerprint_stable_across_unrelated_mutation_and_status_transition() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = ready_ticket(repo, &store);
    let before = store.readiness(&node.id).unwrap().readiness_fingerprint;

    // Unrelated graph mutation (another ticket) does not stale readiness.
    let _other = create_ticket(&store);
    let after_unrelated = store.readiness(&node.id).unwrap();
    assert_eq!(before, after_unrelated.readiness_fingerprint);

    // Status-only transition (ready -> shaped) leaves the fingerprint unchanged.
    let reshaped = store
        .transition_node_with_context(
            &node.id,
            NodeStatus::Shaped,
            node.revision,
            reason("rework_needed", "back to shaped"),
            ctx(),
        )
        .unwrap()
        .value;
    let after_status = store.readiness(&reshaped.id).unwrap();
    assert_eq!(before, after_status.readiness_fingerprint);
}

#[test]
fn fingerprint_changes_on_content_and_policy_inputs() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = ready_ticket(repo, &store);
    let baseline = store.readiness(&node.id).unwrap().readiness_fingerprint;

    // Mutating ticket.md bytes without re-syncing makes the bound brief hash
    // stale, staling readiness and changing the fingerprint.
    let brief_path = repo.join(format!("{}/ticket.md", node.content_dir));
    let original = fs::read(&brief_path).unwrap();
    fs::write(&brief_path, b"# Ticket\nchanged content").unwrap();
    let changed = store.readiness(&node.id).unwrap();
    assert_eq!(changed.status, ReadinessStatus::Stale);
    assert!(family(&changed, "content_reference_integrity")
        .reason_codes
        .contains(&"implementation_brief_hash_stale".to_string()));
    assert_ne!(changed.readiness_fingerprint, baseline);

    // Restore content -> fingerprint returns to baseline.
    fs::write(&brief_path, original).unwrap();
    let restored = store.readiness(&node.id).unwrap();
    assert_eq!(restored.status, ReadinessStatus::Ready);
    assert_eq!(restored.readiness_fingerprint, baseline);

    // Policy change participates in the fingerprint.
    write_policy(repo, &["work.transition.shaped"]);
    let policy_changed = store.readiness(&node.id).unwrap();
    assert_ne!(policy_changed.readiness_fingerprint, baseline);
}

#[test]
fn ready_state_stale_does_not_silently_mutate_status() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = ready_ticket(repo, &store);
    assert_eq!(node.status, NodeStatus::Ready);

    let brief_path = repo.join(format!("{}/ticket.md", node.content_dir));
    fs::write(&brief_path, b"# Ticket\nchanged content").unwrap();

    let report = store.readiness(&node.id).unwrap();
    assert_eq!(report.status, ReadinessStatus::Stale);
    assert!(report
        .reason_codes
        .contains(&"ready_state_stale".to_string()));
    assert!(!report.transition_eligible);

    let retained = store.show_node(&node.id).unwrap();
    assert_eq!(retained.status, NodeStatus::Ready);
}

#[test]
fn qa_unknown_blocks_ready_and_required_resolves_current_story_cases() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    // QA left at default unknown: the brief has no `## QA impact` section, so
    // sync derives nothing and the node keeps its unknown default.
    let node = create_ticket(&store);
    let node = bind_brief(repo, &store, &node, Some(DOCS_NONE_SECTION), None, None);
    let report = store.readiness(&node.id).unwrap();
    assert_eq!(family(&report, "qa_impact").status, GateStatus::Failed);
    assert!(family(&report, "qa_impact")
        .reason_codes
        .contains(&"qa_impact_unknown".to_string()));
    assert_ne!(report.status, ReadinessStatus::Ready);

    // QA=required fails closed until the behavioral owner's baseline exists.
    let story = store
        .create_node(WorkKind::Story, "Behavioral owner".to_string())
        .unwrap()
        .value;
    let node = bind_brief(
        repo,
        &store,
        &node,
        Some(DOCS_NONE_SECTION),
        Some(&format!(
            "## QA impact\n- Owner: {}\n- Posture: required\n- Cases: QA-001\n- Reason: Behavioral checkpoint required.\n",
            story.id
        )),
        None,
    );
    let report = store.readiness(&node.id).unwrap();
    let qa = family(&report, "qa_impact");
    assert_eq!(qa.status, GateStatus::Failed);
    assert!(qa.reason_codes.contains(&"qa_baseline_missing".to_string()));

    write_qa_baseline(repo, &story.id, "QA-001");
    let report = store.readiness(&node.id).unwrap();
    assert_eq!(family(&report, "qa_impact").status, GateStatus::Passed);

    // A stale/nonexistent case ID cannot silently pass against the same owner.
    let node = bind_brief(
        repo,
        &store,
        &node,
        Some(DOCS_NONE_SECTION),
        Some(&format!(
            "## QA impact\n- Owner: {}\n- Posture: required\n- Cases: QA-999\n- Reason: Changed case selection.\n",
            story.id
        )),
        None,
    );
    let report = store.readiness(&node.id).unwrap();
    assert_eq!(family(&report, "qa_impact").status, GateStatus::Failed);
    assert!(family(&report, "qa_impact")
        .reason_codes
        .contains(&"qa_case_missing".to_string()));
}

#[test]
fn missing_authority_policy_makes_authority_gate_unavailable() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = ready_ticket(repo, &store);
    // Now remove the policy entirely.
    let policy_path = repo.join(".pulse/policy/authority.json");
    fs::remove_file(&policy_path).unwrap();
    let report = store.readiness(&node.id).unwrap();
    let authority = family(&report, "authority");
    assert_eq!(authority.status, GateStatus::Unavailable);
    assert!(authority
        .reason_codes
        .contains(&"readiness_policy_missing".to_string()));
    assert_ne!(report.status, ReadinessStatus::Ready);
}

#[test]
fn documentation_impact_unknown_fails_none_passes() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    // No `## Documentation impact` section -> derived posture stays unknown.
    let node = create_ticket(&store);
    let node = bind_brief(repo, &store, &node, None, Some(QA_NONE_SECTION), None);
    let report = store.readiness(&node.id).unwrap();
    assert_eq!(
        family(&report, "documentation_impact").status,
        GateStatus::Failed
    );
    assert_ne!(report.status, ReadinessStatus::Ready);

    // Deriving the none posture from the brief passes the family.
    let node = bind_brief(
        repo,
        &store,
        &node,
        Some(DOCS_NONE_SECTION),
        Some(QA_NONE_SECTION),
        None,
    );
    let report = store.readiness(&node.id).unwrap();
    assert_eq!(
        family(&report, "documentation_impact").status,
        GateStatus::Passed
    );
    assert_eq!(
        family(&report, "applicable_documents").status,
        GateStatus::NotApplicable
    );
}

#[test]
fn applicable_documents_gate_requires_registered_current_documents() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    write_policy(repo, full_grants());
    let node = create_ticket(&store);
    let node = bind_brief(
        repo,
        &store,
        &node,
        Some(
            "## Documentation impact\n- Posture: required\n- Rationale: Reservation contract must remain validated.\n- Documents: DOC-RESERVATION-CONTRACT\n",
        ),
        Some(QA_NONE_SECTION),
        None,
    );

    // The required document is not registered yet -> gate fails closed.
    let report = store.readiness(&node.id).unwrap();
    let applicable = family(&report, "applicable_documents");
    assert_eq!(applicable.status, GateStatus::Failed);
    assert!(applicable
        .reason_codes
        .contains(&"required_document_missing".to_string()));
    assert_ne!(report.status, ReadinessStatus::Ready);

    // Registering the approved document with present content passes the gate.
    let doc_path = repo.join("docs/domain/reservation.md");
    fs::create_dir_all(doc_path.parent().unwrap()).unwrap();
    fs::write(
        &doc_path,
        b"# Reservation contract\n\nThe reservation remains stable.\n",
    )
    .unwrap();
    pulse::evidence::bootstrap(repo).unwrap();
    pulse::docs::manifest::bootstrap(repo).unwrap();
    pulse::docs::register(
        repo,
        1,
        DocumentRecord {
            id: "DOC-RESERVATION-CONTRACT".to_string(),
            revision: 1,
            path: "docs/domain/reservation.md".to_string(),
            summary: "Reservation close contract".to_string(),
            owner: "team:platform".to_string(),
            kind: DocumentKind::Domain,
            status: DocumentStatus::Approved,
            scope: DocumentScope {
                paths: vec!["src/**".to_string()],
            },
            tags: vec![],
            generated: None,
            superseded_by: None,
        },
        "human:tester",
    )
    .unwrap();
    let report = store.readiness(&node.id).unwrap();
    assert_eq!(
        family(&report, "applicable_documents").status,
        GateStatus::Passed
    );
}

#[test]
fn draft_to_shaped_transition_records_shaped_gate_profile() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    write_policy(repo, &["work.transition.shaped"]);
    let node = bound_ticket(repo, &store);
    let shaped = transition(&store, &node.id, NodeStatus::Shaped, None);
    assert_eq!(shaped.status, NodeStatus::Shaped);
    let events = collect_events(repo, "work.node.transitioned");
    let last = events.last().unwrap();
    assert_eq!(
        last["payload"]["gate_profile"].as_str(),
        Some(readiness::SHAPED_GATE_PROFILE)
    );
    assert!(last["payload"]["input_fingerprint"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
}

#[test]
fn shaped_to_ready_requires_authority_and_passing_gate() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    // Grants without work.transition.ready.
    write_policy(repo, &["work.transition.shaped"]);
    let node = bound_ticket(repo, &store);
    let shaped = transition(&store, &node.id, NodeStatus::Shaped, None);

    // Missing transition-ready grant -> denied before gate evaluation.
    let err = store
        .transition_node_with_context(&shaped.id, NodeStatus::Ready, shaped.revision, None, ctx())
        .unwrap_err();
    assert_eq!(err.code(), "readiness_authority_denied");

    // Grant it -> succeeds.
    write_policy(repo, full_grants());
    let ready = transition(&store, &shaped.id, NodeStatus::Ready, None);
    assert_eq!(ready.status, NodeStatus::Ready);
    let events = collect_events(repo, "work.node.transitioned");
    let ready_events: Vec<_> = events
        .iter()
        .filter(|e| e["payload"]["to"] == "ready")
        .collect();
    assert_eq!(ready_events.len(), 1);
    assert_eq!(
        ready_events[0]["payload"]["gate_profile"].as_str(),
        Some(READINESS_PROFILE)
    );
    assert!(ready_events[0]["payload"]["input_fingerprint"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
}

#[test]
fn expected_readiness_fingerprint_mismatch_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    write_policy(repo, full_grants());
    let node = bound_ticket(repo, &store);
    let shaped = transition(&store, &node.id, NodeStatus::Shaped, None);

    let err = store
        .transition_node_gated_with_context(
            &shaped.id,
            NodeStatus::Ready,
            shaped.revision,
            None,
            Some("sha256:deadbeef"),
            ctx(),
        )
        .unwrap_err();
    assert_eq!(err.code(), "readiness_fingerprint_mismatch");

    // Correct fingerprint succeeds.
    let report = store.readiness(&shaped.id).unwrap();
    let _ready = store
        .transition_node_gated_with_context(
            &shaped.id,
            NodeStatus::Ready,
            shaped.revision,
            None,
            Some(&report.readiness_fingerprint),
            ctx(),
        )
        .unwrap();
}

#[test]
fn decision_work_ticket_is_not_ready_under_implementation_profile() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let classification = PublicCreateClassification {
        role: Some(TicketRole::DecisionWork),
        risk: Some(Risk::Low),
        materialization: Some(Materialization::R0),
    };
    let node = store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Decision work".to_string(),
            classification,
            OperationContext::default(),
        )
        .unwrap()
        .value;
    let report = store.readiness(&node.id).unwrap();
    assert_eq!(
        family(&report, "work_kind_and_role").status,
        GateStatus::NotApplicable
    );
    assert_ne!(report.status, ReadinessStatus::Ready);
}

#[test]
fn shaped_gate_requires_only_a_parseable_unambiguous_ticket_brief() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    write_policy(repo, &["work.transition.shaped"]);

    // A parseable brief with only resolved dispositions is enough: the
    // draft -> shaped gate is the markdown ambiguity gate and needs no
    // receipts.
    let node = create_ticket(&store);
    let node = bind_brief(
        repo,
        &store,
        &node,
        Some(DOCS_NONE_SECTION),
        Some(QA_NONE_SECTION),
        Some("- (delegated) Internal naming is safe to delegate.\n"),
    );
    let shaped = transition(&store, &node.id, NodeStatus::Shaped, None);
    assert_eq!(shaped.status, NodeStatus::Shaped);

    // A blocking disposition fails the ambiguity gate.
    let node = create_ticket(&store);
    let path = repo.join(format!("{}/ticket.md", node.content_dir));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        ticket_md(
            &node.id,
            Some(DOCS_NONE_SECTION),
            Some(QA_NONE_SECTION),
            Some("- (blocking) Which API?\n"),
        ),
    )
    .unwrap();
    let error = store
        .transition_node_with_context(&node.id, NodeStatus::Shaped, node.revision, None, ctx())
        .unwrap_err();
    assert_eq!(error.code(), "readiness_not_ready");
    let report = store.readiness(&node.id).unwrap();
    let ambiguity = family(&report, "ticket_ambiguity");
    assert_eq!(ambiguity.status, GateStatus::Failed);
    assert!(ambiguity
        .reason_codes
        .contains(&"ticket_brief_open_question_blocking".to_string()));
}

#[test]
fn shaped_to_ready_requires_no_evidence_receipts() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    write_policy(repo, full_grants());
    let node = bound_ticket(repo, &store);
    let shaped = transition(&store, &node.id, NodeStatus::Shaped, None);
    let ready = transition(&store, &shaped.id, NodeStatus::Ready, None);
    assert_eq!(ready.status, NodeStatus::Ready);
}

#[test]
fn installed_gate_profiles_match_directions() {
    assert_eq!(
        installed_gate(NodeStatus::Draft, NodeStatus::Shaped),
        Some(GateProfile::Shaped)
    );
    assert_eq!(
        installed_gate(NodeStatus::Shaped, NodeStatus::Ready),
        Some(GateProfile::Ready)
    );
    assert_eq!(installed_gate(NodeStatus::Blocked, NodeStatus::Ready), None);
    assert_eq!(installed_gate(NodeStatus::Ready, NodeStatus::Shaped), None);
}

#[test]
fn ready_transition_crash_recovers_coherent_event() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    write_policy(repo, full_grants());
    let prep = JsonGraphStore::new(repo);
    let node = bound_ticket(repo, &prep);
    let shaped = transition(&prep, &node.id, NodeStatus::Shaped, None);

    let crashing = JsonGraphStore::with_failpoint(repo, TransactionFailpoint::AfterCanonical);
    let _ = crashing
        .transition_node_with_context(&shaped.id, NodeStatus::Ready, shaped.revision, None, ctx())
        .unwrap_err();

    JsonGraphStore::new(repo).recover().unwrap();
    let recovered = JsonGraphStore::new(repo).show_node(&shaped.id).unwrap();
    assert_eq!(recovered.status, NodeStatus::Ready);
    let events = collect_events(repo, "work.node.transitioned");
    let ready_events: Vec<_> = events
        .iter()
        .filter(|e| e["payload"]["to"] == "ready")
        .collect();
    assert_eq!(ready_events.len(), 1);
    assert_eq!(
        ready_events[0]["payload"]["gate_profile"].as_str(),
        Some(READINESS_PROFILE)
    );
}

#[test]
fn readiness_query_does_not_mutate_status() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = ready_ticket(repo, &store);
    let rev_before = node.revision;
    let _ = store.readiness(&node.id).unwrap();
    let after = store.show_node(&node.id).unwrap();
    assert_eq!(after.revision, rev_before);
    assert_eq!(after.status, NodeStatus::Ready);
}

#[test]
fn readiness_query_does_not_bootstrap_docs_or_evidence_plane() {
    // Read-only readiness/frontier projections must never bootstrap or rewrite
    // the docs registry (or, transitively, the evidence manifest) as a side
    // effect of a query. Slice 7 contract: read-only commands never bootstrap
    // canonical planes beyond the accepted workgraph ensure-baseline.
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    // Bootstrap ONLY the workgraph baseline (what every op does). Do not touch
    // docs/evidence.
    pulse::storage::bootstrap(repo).unwrap();
    let docs_registry = repo.join(".pulse/docs/registry.json");
    let evidence_manifest = repo.join(".pulse/evidence/manifest.json");
    assert!(!docs_registry.exists());
    assert!(!evidence_manifest.exists());

    let store = JsonGraphStore::new(repo);
    let node = create_ticket(&store);

    // A read-only readiness query on a workgraph-only repository must succeed
    // (or at worst report not_ready) without materializing canonical docs/
    // evidence state.
    let _ = store.readiness(&node.id).unwrap();

    assert!(
        !docs_registry.exists(),
        "read-only readiness bootstrapped the docs registry"
    );
    assert!(
        !evidence_manifest.exists(),
        "read-only readiness bootstrapped the evidence manifest"
    );

    // The frontier projection must observe the same invariant.
    let _ = store.frontier(None, None, false).unwrap();
    assert!(
        !docs_registry.exists(),
        "read-only execution frontier bootstrapped the docs registry"
    );
    assert!(
        !evidence_manifest.exists(),
        "read-only execution frontier bootstrapped the evidence manifest"
    );
}

#[test]
fn blocked_resume_goes_via_shaped_not_direct_ready() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let ready = ready_ticket(repo, &store);

    // ready -> blocked (supported, reason required). Lifecycle eligibility
    // fails while blocked.
    let blocked = transition(
        &store,
        &ready.id,
        NodeStatus::Blocked,
        reason("dependency_unavailable", "blocked"),
    );
    assert_eq!(blocked.status, NodeStatus::Blocked);
    let report = store.readiness(&blocked.id).unwrap();
    assert_eq!(
        family(&report, "lifecycle_eligibility").status,
        GateStatus::Failed
    );
    assert!(family(&report, "lifecycle_eligibility")
        .reason_codes
        .contains(&"lifecycle_blocked".to_string()));
    assert_eq!(report.status, ReadinessStatus::NotReady);

    // Direct blocked -> ready is intentionally NOT installed.
    let err = store
        .transition_node_with_context(
            &blocked.id,
            NodeStatus::Ready,
            blocked.revision,
            None,
            ctx(),
        )
        .unwrap_err();
    assert_eq!(err.code(), "transition_gate_unavailable");

    // Blocked -> shaped (supported resume, reason required) then shaped -> ready.
    let reshaped = transition(
        &store,
        &blocked.id,
        NodeStatus::Shaped,
        reason("dependency_restored", "resume"),
    );
    assert_eq!(reshaped.status, NodeStatus::Shaped);
    let resumed = transition(&store, &reshaped.id, NodeStatus::Ready, None);
    assert_eq!(resumed.status, NodeStatus::Ready);
}
