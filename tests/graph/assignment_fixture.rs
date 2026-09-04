use std::fs;

use chrono::Utc;
use pulse::canonical_json::to_canonical_bytes;
use pulse::docs::{
    DocumentAuthority, DocumentKind, DocumentLifecycle, DocumentRecord, DocumentScope, ReviewPolicy,
};
use pulse::graph::model::contract::{
    ContentRef, ContractItem, ContractScope, EffortMetadata, ImplementationContract,
    ImplementationMode, ImplementationSemanticImpact, PlanPolicy, PublicCreateClassification,
    QaImpactPosture, SurfaceRef, TicketRole, WorkSurface,
};
use pulse::graph::model::node::{DocumentationImpactPosture, NodeStatus};
use pulse::graph::store::{
    ContractSetRequest, DocumentationImpactUpdate, OperationContext, QaImpactUpdate,
};
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

pub(super) fn valid_inventory_bytes(principal: &str) -> Vec<u8> {
    serde_json::json!({
        "schema_version": 1,
        "principal": principal,
        "inventory_id": "test-inventory",
        "capabilities": [
            "repository.inspect",
            "source.read",
            "source.write",
            "test.run",
            "workspace.worktree"
        ]
    })
    .to_string()
    .into_bytes()
}

pub(super) fn write_policy(root: &std::path::Path, extra_grants: &[&str]) {
    let path = root.join(".pulse/policy/authority.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut grants = vec![
        "shape.apply".to_string(),
        "shape.approve.R1".to_string(),
        "qa.none.approve".to_string(),
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
            id: "DOC-RESERVATION-CONTRACT".to_string(),
            revision: 1,
            path: path.to_string(),
            kind: DocumentKind::Domain,
            authority: DocumentAuthority::Approved,
            lifecycle: DocumentLifecycle::Current,
            owner: "team:platform".to_string(),
            summary: "Reservation close contract".to_string(),
            aliases: vec![],
            scope: DocumentScope {
                paths: vec!["src/**".to_string()],
                domains: vec!["development".to_string()],
                work_labels: vec!["reservation".to_string()],
            },
            review_policy: ReviewPolicy::None,
            verification_profile: "domain-doc".to_string(),
            generated: None,
            superseded_by: None,
            retrieval: None,
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
            id: "DOC-OPERATIONS-GUIDANCE".to_string(),
            revision: 1,
            path: optional_path.to_string(),
            kind: DocumentKind::Domain,
            authority: DocumentAuthority::Approved,
            lifecycle: DocumentLifecycle::Current,
            owner: "team:platform".to_string(),
            summary: "Optional operations guidance".to_string(),
            aliases: vec![],
            scope: DocumentScope::default(),
            review_policy: ReviewPolicy::None,
            verification_profile: "domain-doc".to_string(),
            generated: None,
            superseded_by: None,
            retrieval: None,
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
    let behavioral_qa = qa_posture != FixtureQaPosture::None;
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
    let brief_relative = format!("{}/ticket.md", node.content_dir);
    let brief_path = root.join(&brief_relative);
    fs::create_dir_all(brief_path.parent().unwrap()).unwrap();
    fs::write(&brief_path, b"# Ticket\nReservation test brief.").unwrap();
    let brief_hash = pulse::canonical_json::hash_bytes(&fs::read(&brief_path).unwrap());
    store
        .set_contract_with_context(
            &ticket_id,
            node.revision,
            ContractSetRequest {
                role: TicketRole::Implementation,
                implementation: Some(ImplementationContract {
                    mode: ImplementationMode::Guided,
                    work_surface: WorkSurface::Code,
                    plan_policy: PlanPolicy::None,
                    semantic_impact: if behavioral_qa {
                        ImplementationSemanticImpact::BehaviorOrPublicRiskChange
                    } else {
                        ImplementationSemanticImpact::NoBehaviorOrPublicRiskChange
                    },
                    effort: EffortMetadata::default(),
                    verification_profile: "service-change".to_string(),
                    brief: Some(ContentRef {
                        path: brief_relative,
                        content_hash: brief_hash.clone(),
                    }),
                    objective: "Test reservation objective.".to_string(),
                    current_behavior: "Current behavior.".to_string(),
                    target_behavior: "Target behavior.".to_string(),
                    code_anchors: vec![SurfaceRef::path("src/token.mjs")],
                    documentation_anchors: vec![],
                    configuration_anchors: vec![],
                    data_anchors: vec![],
                    research_refs: vec![],
                    required_changes: vec![ContractItem {
                        id: "CHG-1".to_string(),
                        summary: "Make the ticket reservable.".to_string(),
                    }],
                    invariants: vec![ContractItem {
                        id: "INV-1".to_string(),
                        summary: "Keep repository semantics Core-owned.".to_string(),
                    }],
                    acceptance: vec![ContractItem {
                        id: "AC-1".to_string(),
                        summary: "Reservation remains ready until acknowledgement.".to_string(),
                    }],
                    scope: ContractScope::default(),
                    implementation_freedom: vec![],
                    required_decisions: vec![],
                    shared_approach_refs: vec![],
                    expected_evidence: vec![],
                    expected_handoff: vec![],
                }),
                decision_work: None,
            },
            context(),
        )
        .unwrap();
    let behavioral_owner = if behavioral_qa {
        let story = store
            .create_node(WorkKind::Story, "Reservation behavior".to_string())
            .unwrap()
            .value;
        write_required_qa_baseline(root, &story.id);
        Some(story.id)
    } else {
        None
    };
    let current = store.show_node(&ticket_id).unwrap();
    store
        .set_qa_impact_with_context(
            &ticket_id,
            current.revision,
            QaImpactUpdate {
                posture: match qa_posture {
                    FixtureQaPosture::None => QaImpactPosture::None,
                    FixtureQaPosture::Required => QaImpactPosture::Required,
                    FixtureQaPosture::CoveredByStoryClose => QaImpactPosture::CoveredByStoryClose,
                },
                rationale: Some(match qa_posture {
                    FixtureQaPosture::None => "No product QA impact.".to_string(),
                    FixtureQaPosture::Required => "Behavioral checkpoint required.".to_string(),
                    FixtureQaPosture::CoveredByStoryClose => {
                        "Integrated Story qualification owns this behavior.".to_string()
                    }
                }),
                behavioral_owner,
                affected_case_ids: if qa_posture == FixtureQaPosture::Required {
                    vec!["QA-001".to_string()]
                } else {
                    vec![]
                },
            },
            context(),
        )
        .unwrap();
    let current = store.show_node(&ticket_id).unwrap();
    store
        .update_documentation_impact(
            &ticket_id,
            current.revision,
            DocumentationImpactUpdate {
                posture: match docs_posture {
                    FixtureDocsPosture::None => DocumentationImpactPosture::None,
                    FixtureDocsPosture::Required => DocumentationImpactPosture::Required,
                },
                rationale: Some(match docs_posture {
                    FixtureDocsPosture::None => "No durable docs impact.".to_string(),
                    FixtureDocsPosture::Required => {
                        "Reservation contract must remain validated.".to_string()
                    }
                }),
                required_documents: if docs_posture == FixtureDocsPosture::Required {
                    vec!["DOC-RESERVATION-CONTRACT".to_string()]
                } else {
                    vec![]
                },
                deferred_to: vec![],
                paths: vec![],
                domains: vec!["development".to_string()],
                labels: vec!["reservation".to_string()],
            },
            "human:tester".to_string(),
        )
        .unwrap();
    let current = store.show_node(&ticket_id).unwrap();
    let receipt = shaping_receipt(
        &ticket_id,
        current.revision,
        current.contract_revision,
        &current.content_dir,
        &brief_hash,
    );
    let receipt_file = root.join("shaping_reservation.json");
    fs::write(&receipt_file, to_canonical_bytes(&receipt).unwrap()).unwrap();
    pulse::evidence::receipt::record_receipt(root, None, &receipt_file).unwrap();
    let current = store.show_node(&ticket_id).unwrap();
    store
        .apply_shaping_with_context(&ticket_id, current.revision, &receipt.id, None, context())
        .unwrap();
    let current = store.show_node(&ticket_id).unwrap();
    store
        .transition_node_with_context(
            &ticket_id,
            NodeStatus::Shaped,
            current.revision,
            None,
            context(),
        )
        .unwrap();
    let current = store.show_node(&ticket_id).unwrap();
    store
        .transition_node_with_context(
            &ticket_id,
            NodeStatus::Ready,
            current.revision,
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

fn shaping_receipt(
    id: &str,
    revision: u64,
    contract_revision: u64,
    content_dir: &str,
    content_hash: &str,
) -> pulse::evidence::model::ReceiptEnvelope {
    use pulse::evidence::model::*;
    ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 1,
        id: format!("rcpt_{:0<26}", &id[3..]),
        kind: ReceiptKind::ShapingValidation,
        result: ReceiptResult::Passed,
        actor: ActorRef {
            kind: pulse::identity::actor::ActorKind::Human,
            id: "tester".to_string(),
        },
        recorded_at: Utc::now(),
        subject: SubjectRef {
            kind: "work".to_string(),
            id: id.to_string(),
        },
        bindings: ReceiptBindings {
            work: vec![WorkBinding {
                id: id.to_string(),
                revision,
            }],
            source: None,
            content: vec![ContentBinding {
                path: format!("{content_dir}/ticket.md"),
                sha256: content_hash.to_string(),
            }],
            artifacts: vec![],
            graph_fingerprint_observed: None,
        },
        payload: ReceiptPayload::ShapingValidation(ShapingValidationPayload {
            payload_version: 1,
            owning_work: ShapingWorkBinding {
                id: id.to_string(),
                revision_observed: revision,
                contract_revision,
            },
            materialization: "R1".to_string(),
            shape_mode: ShapeMode::FocusedBranches,
            source_posture: SourcePosture::NotRequiredContentBound,
            destination: Some(ShapingDestination {
                summary: "Reservation target".to_string(),
                scope_boundary: vec!["test".to_string()],
                exit_conditions: vec!["reservation proven".to_string()],
            }),
            map: None,
            affected_work: vec![],
            branches: vec![],
            fog: vec![],
            out_of_scope: vec![],
            resolution_pointers: vec![],
            approval: ShapingApproval {
                approved_by: ActorRef {
                    kind: pulse::identity::actor::ActorKind::Human,
                    id: "tester".to_string(),
                },
                reference: "test".to_string(),
            },
            reconciliation: None,
            remaining_uncertainty: vec![],
        }),
    }
}
