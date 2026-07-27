//! S7-I5 decision/execution frontier projection tests.
//!
//! Covers the S7-61..S7-71 frontier matrix: distinct decision/execution sets
//! over the same coherent snapshot, decision-only / execution-only membership,
//! shaped-but-not-transitioned and stale-ready exclusion, hard-blocker and
//! soft-preference handling, the `--for` destination filter, the explicit
//! `claim_state=not_evaluated` boundary (no persisted claim), deterministic
//! priority-agnostic ID ordering and cache-independence. Also covers the CLI
//! contract: stable JSON, profile validation and empty-frontier success.
//!
//! These tests exercise the harness against temporary target repositories only.
//! They never point Pulse at this development repository.

use chrono::Utc;
use pulse::canonical_json::hash_bytes;
use pulse::evidence::model::*;
use pulse::graph::contract::{
    ContentRef, ContractItem, ContractScope, DecisionWorkContract, DecisionWorkProvenance,
    EffortMetadata, ExpectedEvidence, GapKind, ImplementationContract, ImplementationMode,
    ImplementationSemanticImpact, Materialization, PlanPolicy, PublicCreateClassification,
    QaImpactPosture, ResolutionTarget, ResolutionTargetKind, Risk, SurfaceRef, TicketRole,
    WorkSurface,
};
use pulse::graph::edge::EdgeType;
use pulse::graph::executability::StructuralState;
use pulse::graph::frontier::{
    DecisionFrontierReport, ExecutionFrontierReport, FrontierKind, FrontierReport,
};
use pulse::graph::node::{DocumentationImpactPosture, NodeStatus};
use pulse::graph::store::{
    ContractSetRequest, DocumentationImpactUpdate, OperationContext, QaImpactUpdate,
};
use pulse::id::WorkKind;
use pulse::policy::{AuthorityPolicy, AuthorityPrincipal};
use pulse::JsonGraphStore;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

use crate::common_bin::bin;
use crate::common_canon::write_json;

const FULL_GRANTS: &[&str] = &[
    "shape.apply",
    "shape.approve.R1",
    "qa.none.approve",
    "work.transition.shaped",
    "work.transition.ready",
];

fn write_policy(repo: &std::path::Path, grants: &[&str]) {
    let mut sorted = grants.iter().map(|g| g.to_string()).collect::<Vec<_>>();
    sorted.sort();
    sorted.dedup();
    let policy = AuthorityPolicy {
        schema_version: 1,
        revision: 1,
        principals: vec![AuthorityPrincipal {
            kind: ActorKind::Human,
            id: "tester".to_string(),
            grants: sorted,
        }],
    };
    let path = repo.join(".pulse/policy/authority.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_json(&path, &policy);
}

fn ctx() -> OperationContext {
    OperationContext {
        actor: "human:tester".to_string(),
        now: Utc::now(),
    }
}

fn set_docs_none(
    store: &JsonGraphStore,
    node: &pulse::graph::node::Node,
) -> pulse::graph::node::Node {
    store
        .update_documentation_impact(
            &node.id,
            node.revision,
            DocumentationImpactUpdate {
                posture: DocumentationImpactPosture::None,
                rationale: Some("No public behavior change.".to_string()),
                required_documents: vec![],
                deferred_to: vec![],
                paths: vec![],
                domains: vec![],
                labels: vec![],
            },
            "human:tester".to_string(),
        )
        .unwrap()
        .value
}

fn set_qa_none(
    store: &JsonGraphStore,
    node: &pulse::graph::node::Node,
) -> pulse::graph::node::Node {
    store
        .set_qa_impact_with_context(
            &node.id,
            node.revision,
            QaImpactUpdate {
                posture: QaImpactPosture::None,
                rationale: Some("Internal.".to_string()),
                behavioral_owner: None,
                affected_case_ids: vec![],
            },
            ctx(),
        )
        .unwrap()
        .value
}

fn implementation_contract(
    node: &pulse::graph::node::Node,
    brief_hash: &str,
) -> ImplementationContract {
    ImplementationContract {
        mode: ImplementationMode::Guided,
        work_surface: WorkSurface::Code,
        plan_policy: PlanPolicy::None,
        semantic_impact: ImplementationSemanticImpact::NoBehaviorOrPublicRiskChange,
        effort: EffortMetadata::default(),
        verification_profile: "service-change".to_string(),
        brief: Some(ContentRef {
            path: format!("{}/ticket.md", node.content_dir),
            content_hash: brief_hash.to_string(),
        }),
        objective: "Objective.".to_string(),
        current_behavior: "Current.".to_string(),
        target_behavior: "Target.".to_string(),
        code_anchors: vec![SurfaceRef::path("src/auth.rs")],
        documentation_anchors: vec![],
        configuration_anchors: vec![],
        data_anchors: vec![],
        research_refs: vec![],
        required_changes: vec![ContractItem {
            id: "CHG-1".to_string(),
            summary: "chg".to_string(),
        }],
        invariants: vec![ContractItem {
            id: "INV-1".to_string(),
            summary: "inv".to_string(),
        }],
        acceptance: vec![ContractItem {
            id: "AC-1".to_string(),
            summary: "ac".to_string(),
        }],
        scope: ContractScope::default(),
        implementation_freedom: vec![],
        required_decisions: vec![],
        shared_approach_refs: vec![],
        expected_evidence: vec![],
        expected_handoff: vec![],
    }
}

fn write_brief(repo: &std::path::Path, node: &pulse::graph::node::Node) -> String {
    let rel = format!("{}/ticket.md", node.content_dir);
    let path = repo.join(&rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"# Ticket\nimplementation contract content").unwrap();
    hash_bytes(&fs::read(&path).unwrap())
}

fn record_concise_shaping(
    repo: &std::path::Path,
    node: &pulse::graph::node::Node,
    receipt_id: &str,
    brief_rel: &str,
    brief_hash: &str,
) {
    let receipt = ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 1,
        id: receipt_id.to_string(),
        kind: ReceiptKind::ShapingValidation,
        result: ReceiptResult::Passed,
        actor: ActorRef {
            kind: ActorKind::Human,
            id: "tester".to_string(),
        },
        recorded_at: Utc::now(),
        subject: SubjectRef {
            kind: "work".to_string(),
            id: node.id.clone(),
        },
        bindings: ReceiptBindings {
            work: vec![WorkBinding {
                id: node.id.clone(),
                revision: node.revision,
            }],
            source: None,
            content: vec![ContentBinding {
                path: brief_rel.to_string(),
                sha256: brief_hash.to_string(),
            }],
            artifacts: vec![],
            graph_fingerprint_observed: None,
        },
        payload: ReceiptPayload::ShapingValidation(ShapingValidationPayload {
            payload_version: 1,
            owning_work: ShapingWorkBinding {
                id: node.id.clone(),
                revision_observed: node.revision,
                contract_revision: node.contract_revision,
            },
            materialization: "R1".to_string(),
            shape_mode: ShapeMode::FocusedBranches,
            source_posture: SourcePosture::NotRequiredContentBound,
            destination: None,
            map: None,
            affected_work: vec![],
            branches: vec![],
            fog: vec![],
            out_of_scope: vec![],
            resolution_pointers: vec![],
            approval: ShapingApproval {
                approved_by: ActorRef {
                    kind: ActorKind::Human,
                    id: "tester".to_string(),
                },
                reference: "PULSE.md".to_string(),
            },
            reconciliation: None,
            remaining_uncertainty: vec![],
        }),
    };
    let file = repo.join(format!("{receipt_id}.json"));
    write_json(&file, &receipt);
    pulse::evidence::record_receipt(repo, None, &file).unwrap();
}

/// Build a fully-ready implementation Ticket through the library.
fn ready_implementation_ticket(
    repo: &std::path::Path,
    store: &JsonGraphStore,
    receipt_id: &str,
) -> pulse::graph::node::Node {
    write_policy(repo, FULL_GRANTS);
    let node = store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Implementation ticket".to_string(),
            PublicCreateClassification {
                role: Some(TicketRole::Implementation),
                risk: Some(Risk::Low),
                materialization: Some(Materialization::R1),
            },
            OperationContext::default(),
        )
        .unwrap()
        .value;
    let brief_hash = write_brief(repo, &node);
    let brief_rel = format!("{}/ticket.md", node.content_dir);
    let node = store
        .set_contract_with_context(
            &node.id,
            node.revision,
            ContractSetRequest {
                role: TicketRole::Implementation,
                implementation: Some(implementation_contract(&node, &brief_hash)),
                decision_work: None,
            },
            ctx(),
        )
        .unwrap()
        .value;
    let node = set_qa_none(store, &node);
    let node = set_docs_none(store, &node);
    record_concise_shaping(repo, &node, receipt_id, &brief_rel, &brief_hash);
    let node = store
        .apply_shaping_with_context(&node.id, node.revision, receipt_id, None, ctx())
        .unwrap()
        .value;
    let shaped = store
        .transition_node_with_context(&node.id, NodeStatus::Shaped, node.revision, None, ctx())
        .unwrap()
        .value;
    store
        .transition_node_with_context(&shaped.id, NodeStatus::Ready, shaped.revision, None, ctx())
        .unwrap()
        .value
}

fn create_story(store: &JsonGraphStore, title: &str) -> pulse::graph::node::Node {
    store
        .create_node_public_with_context(
            WorkKind::Story,
            title.to_string(),
            PublicCreateClassification::default(),
            OperationContext::default(),
        )
        .unwrap()
        .value
}

fn decision_work_contract(
    owner: &pulse::graph::node::Node,
    branch_id: &str,
) -> DecisionWorkContract {
    DecisionWorkContract {
        destination_owner: pulse::graph::contract::RevisionedWorkRef {
            id: owner.id.clone(),
            contract_revision: owner.contract_revision,
        },
        branch_id: branch_id.to_string(),
        gap_kind: GapKind::TradeoffGap,
        question: "Which direction should we take?".to_string(),
        expected_output: "A Decision with direction and consequences.".to_string(),
        expected_evidence: vec![ExpectedEvidence::ClientContractInventory],
        resolution_target: Some(ResolutionTarget {
            kind: ResolutionTargetKind::Decision,
            id: "DEC-001".to_string(),
        }),
        provenance: DecisionWorkProvenance {
            shaping_receipt: "rcpt_01J00000000000000000000DEC".to_string(),
            fog_id: None,
        },
    }
}

/// Create a decision-work Ticket serving `owner`, linked via a `parent` edge.
fn decision_work_ticket(
    store: &JsonGraphStore,
    owner: &pulse::graph::node::Node,
    branch_id: &str,
) -> pulse::graph::node::Node {
    let node = store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Decision work".to_string(),
            PublicCreateClassification {
                role: Some(TicketRole::DecisionWork),
                risk: Some(Risk::Low),
                materialization: Some(Materialization::R0),
            },
            OperationContext::default(),
        )
        .unwrap()
        .value;
    let node = store
        .set_contract_with_context(
            &node.id,
            node.revision,
            ContractSetRequest {
                role: TicketRole::DecisionWork,
                implementation: None,
                decision_work: Some(decision_work_contract(owner, branch_id)),
            },
            ctx(),
        )
        .unwrap()
        .value;
    // Link the decision work to its destination owner (parent edge). A Ticket
    // may have at most one live parent; an Epic/Story owner is the norm.
    store
        .add_edge_with_context(EdgeType::Parent, node.id.clone(), owner.id.clone(), ctx())
        .unwrap();
    node
}

/// Record and apply a shaping receipt on an owner (Story) carrying a single
/// branch with the requested disposition, so the decision frontier can evaluate
/// branch relevance against the owner's current shaping receipt.
fn apply_owner_shaping_with_branch(
    repo: &std::path::Path,
    store: &JsonGraphStore,
    owner: &pulse::graph::node::Node,
    receipt_id: &str,
    branch_id: &str,
    disposition: BranchDisposition,
) {
    write_policy(
        repo,
        &[
            "shape.apply",
            "shape.approve.R0",
            "shape.approve.R1",
            "qa.none.approve",
            "work.transition.shaped",
            "work.transition.ready",
        ],
    );
    // Shaping validation requires at least one content binding; create a small
    // owner content file under its content_dir and bind its hash.
    let owner_rel = format!("{}/story.md", owner.content_dir);
    let owner_path = repo.join(&owner_rel);
    fs::create_dir_all(owner_path.parent().unwrap()).unwrap();
    fs::write(&owner_path, b"# Story\nowner shaping context").unwrap();
    let owner_hash = hash_bytes(&fs::read(&owner_path).unwrap());
    let receipt = ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 1,
        id: receipt_id.to_string(),
        kind: ReceiptKind::ShapingValidation,
        result: ReceiptResult::Passed,
        actor: ActorRef {
            kind: ActorKind::Human,
            id: "tester".to_string(),
        },
        recorded_at: Utc::now(),
        subject: SubjectRef {
            kind: "work".to_string(),
            id: owner.id.clone(),
        },
        bindings: ReceiptBindings {
            work: vec![WorkBinding {
                id: owner.id.clone(),
                revision: owner.revision,
            }],
            source: None,
            content: vec![ContentBinding {
                path: owner_rel,
                sha256: owner_hash,
            }],
            artifacts: vec![],
            graph_fingerprint_observed: None,
        },
        payload: ReceiptPayload::ShapingValidation(ShapingValidationPayload {
            payload_version: 1,
            owning_work: ShapingWorkBinding {
                id: owner.id.clone(),
                revision_observed: owner.revision,
                contract_revision: owner.contract_revision,
            },
            materialization: "R0".to_string(),
            shape_mode: ShapeMode::FocusedBranches,
            source_posture: SourcePosture::NotRequiredContentBound,
            destination: None,
            map: None,
            affected_work: vec![],
            branches: vec![ShapingBranch {
                id: branch_id.to_string(),
                question: "Branch question?".to_string(),
                gap_kind: "tradeoff_gap".to_string(),
                criticality: BranchCriticality::Critical,
                affected_work: vec![],
                disposition,
            }],
            fog: vec![],
            out_of_scope: vec![],
            resolution_pointers: vec![],
            approval: ShapingApproval {
                approved_by: ActorRef {
                    kind: ActorKind::Human,
                    id: "tester".to_string(),
                },
                reference: "PULSE.md".to_string(),
            },
            reconciliation: None,
            remaining_uncertainty: vec![],
        }),
    };
    let file = repo.join(format!("{receipt_id}.json"));
    write_json(&file, &receipt);
    pulse::evidence::record_receipt(repo, None, &file).unwrap();
    store
        .apply_shaping_with_context(&owner.id, owner.revision, receipt_id, None, ctx())
        .unwrap();
}

fn decision_report(store: &JsonGraphStore, for_owner: Option<&str>) -> DecisionFrontierReport {
    match store
        .frontier(FrontierKind::Decision, for_owner, None, true)
        .unwrap()
    {
        FrontierReport::Decision(report) => report,
        _ => panic!("expected decision frontier"),
    }
}

fn execution_report(store: &JsonGraphStore, for_owner: Option<&str>) -> ExecutionFrontierReport {
    match store
        .frontier(FrontierKind::Execution, for_owner, None, true)
        .unwrap()
    {
        FrontierReport::Execution(report) => report,
        _ => panic!("expected execution frontier"),
    }
}

fn decision_item_ids(items: &[pulse::graph::frontier::DecisionFrontierItem]) -> Vec<String> {
    items.iter().map(|i| i.id.clone()).collect()
}

fn exec_item_ids(items: &[pulse::graph::frontier::ExecutionFrontierItem]) -> Vec<String> {
    items.iter().map(|i| i.id.clone()).collect()
}

#[test]
fn execution_frontier_includes_only_current_ready_implementation_tickets() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let ready = ready_implementation_ticket(repo, &store, "rcpt_01J00000000000000000000001");

    let report = execution_report(&store, None);
    assert_eq!(report.kind, "execution");
    assert_eq!(report.claim_state, "not_evaluated");
    assert!(!report.dispatch_authorized);
    assert_eq!(report.readiness_profile, "phase1_contract_readiness_v1");
    assert_eq!(exec_item_ids(&report.items), vec![ready.id.clone()]);
    assert!(report.items[0].frontier_eligible);
    assert!(report.items[0].readiness_fingerprint.starts_with("sha256:"));
    assert_eq!(report.items[0].reason_codes, vec!["contract_ready"]);
}

#[test]
fn shaped_but_not_transitioned_ticket_is_not_in_execution_frontier() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    write_policy(repo, FULL_GRANTS);
    // Build a ready ticket, then regress it to `shaped` manually is not allowed
    // by lifecycle; instead build a fresh ticket and stop at `shaped`.
    let node = store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Shaped".to_string(),
            PublicCreateClassification {
                role: Some(TicketRole::Implementation),
                risk: Some(Risk::Low),
                materialization: Some(Materialization::R1),
            },
            OperationContext::default(),
        )
        .unwrap()
        .value;
    let brief_hash = write_brief(repo, &node);
    let brief_rel = format!("{}/ticket.md", node.content_dir);
    let node = store
        .set_contract_with_context(
            &node.id,
            node.revision,
            ContractSetRequest {
                role: TicketRole::Implementation,
                implementation: Some(implementation_contract(&node, &brief_hash)),
                decision_work: None,
            },
            ctx(),
        )
        .unwrap()
        .value;
    let node = set_qa_none(&store, &node);
    let node = set_docs_none(&store, &node);
    record_concise_shaping(
        repo,
        &node,
        "rcpt_01J00000000000000000000002",
        &brief_rel,
        &brief_hash,
    );
    let node = store
        .apply_shaping_with_context(
            &node.id,
            node.revision,
            "rcpt_01J00000000000000000000002",
            None,
            ctx(),
        )
        .unwrap()
        .value;
    let _shaped = store
        .transition_node_with_context(&node.id, NodeStatus::Shaped, node.revision, None, ctx())
        .unwrap()
        .value;

    let report = execution_report(&store, None);
    assert!(report.items.is_empty(), "shaped ticket must not appear");
    // The shaped ticket is excluded with an explicit lifecycle reason.
    let excluded: BTreeMap<String, Vec<String>> = report
        .excluded
        .iter()
        .map(|e| (e.id.clone(), e.reason_codes.clone()))
        .collect();
    assert_eq!(
        excluded.get(&node.id).map(|codes| codes.first().cloned()),
        Some(Some("execution_not_transitioned".to_string()))
    );
}

#[test]
fn decision_frontier_includes_open_unblocked_decision_work() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let owner = create_story(&store, "Owner story");
    let dw = decision_work_ticket(&store, &owner, "BR-OPEN");

    let report = decision_report(&store, Some(&owner.id));
    assert_eq!(report.kind, "decision");
    assert_eq!(report.claim_state, "not_evaluated");
    assert_eq!(report.for_.as_deref(), Some(owner.id.as_str()));
    assert_eq!(decision_item_ids(&report.items), vec![dw.id.clone()]);
    assert_eq!(report.items[0].branch_id, "BR-OPEN");
    assert_eq!(report.items[0].gap_kind, "tradeoff_gap");
    assert_eq!(report.items[0].reason_codes, vec!["open_decision_work"]);
    assert_eq!(report.items[0].structural_state, StructuralState::Candidate);
}

#[test]
fn decision_work_draft_eligible_without_nested_shaping_receipt() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let owner = create_story(&store, "Owner");
    // Decision work stays in `draft` and has no shaping pointer of its own.
    let dw = decision_work_ticket(&store, &owner, "BR-DRAFT");
    assert_eq!(dw.status, NodeStatus::Draft);

    let report = decision_report(&store, Some(&owner.id));
    assert!(report.items.iter().any(|i| i.id == dw.id));
}

#[test]
fn decision_and_execution_frontiers_are_distinct_with_same_fingerprint() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let owner = create_story(&store, "Owner");
    let _dw = decision_work_ticket(&store, &owner, "BR-1");
    // Link the ready implementation ticket to the owner so --for scopes it.
    let ready = ready_implementation_ticket(repo, &store, "rcpt_01J00000000000000000000003");
    store
        .add_edge_with_context(EdgeType::Parent, ready.id.clone(), owner.id.clone(), ctx())
        .unwrap();

    let decision = decision_report(&store, Some(&owner.id));
    let execution = execution_report(&store, Some(&owner.id));

    // Same coherent snapshot -> identical graph fingerprint.
    assert_eq!(decision.graph_fingerprint, execution.graph_fingerprint);
    // Distinct membership: decision work vs implementation work.
    let decision_ids: Vec<String> = decision.items.iter().map(|i| i.id.clone()).collect();
    let exec_ids: Vec<String> = execution.items.iter().map(|i| i.id.clone()).collect();
    assert!(decision_ids.iter().all(|id| id.starts_with("TK")));
    assert!(exec_ids.iter().all(|id| id.starts_with("TK")));
    assert!(
        decision_ids
            .iter()
            .chain(exec_ids.iter())
            .all(|id| !decision_ids.contains(id) || !exec_ids.contains(id)),
        "no ticket may be in both frontiers"
    );
    assert!(!decision_ids.is_empty());
    assert!(!exec_ids.is_empty());
}

#[test]
fn stale_ready_ticket_excluded_from_execution_frontier() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let ready = ready_implementation_ticket(repo, &store, "rcpt_01J00000000000000000000004");

    // Mutate a readiness input (documentation impact back to `unknown`) after
    // the ready transition. Lifecycle stays `ready` but current readiness fails.
    let node = store.show_node(&ready.id).unwrap();
    // Drive docs to unknown by removing the documentation metadata entirely is
    // not a public mutation; instead set posture required without documents via
    // the library edit path through documentation impact update is rejected.
    // Use a direct canonical edit to flip documentation posture to unknown.
    let mut edited = node.clone();
    edited.documentation = None; // missing docs -> posture unknown -> readiness fails
    edited.revision = node.revision + 1;
    edited.updated_at = Utc::now();
    write_json(
        &repo
            .join(".pulse/workgraph/nodes")
            .join(format!("{}.json", node.id)),
        &edited,
    );

    let report = execution_report(&store, None);
    assert!(report.items.is_empty(), "stale-ready must not appear");
    let excluded: BTreeMap<String, Vec<String>> = report
        .excluded
        .iter()
        .map(|e| (e.id.clone(), e.reason_codes.clone()))
        .collect();
    let codes = excluded.get(&ready.id).expect("stale ticket excluded");
    assert!(
        codes.contains(&"ready_state_stale".to_string()),
        "expected ready_state_stale reason, got {codes:?}"
    );
}

#[test]
fn hard_blocker_excludes_from_frontiers() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let owner = create_story(&store, "Owner");
    let blocker = store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Blocker".to_string(),
            PublicCreateClassification {
                role: Some(TicketRole::Implementation),
                risk: Some(Risk::Low),
                materialization: Some(Materialization::R0),
            },
            OperationContext::default(),
        )
        .unwrap()
        .value;
    let dw = decision_work_ticket(&store, &owner, "BR-BLK");
    // decision work blocked_by an open (draft) ticket.
    store
        .add_edge_with_context(
            EdgeType::BlockedBy,
            dw.id.clone(),
            blocker.id.clone(),
            ctx(),
        )
        .unwrap();

    let decision = decision_report(&store, Some(&owner.id));
    assert!(decision.items.is_empty());
    let codes: Vec<String> = decision
        .excluded
        .iter()
        .find(|e| e.id == dw.id)
        .map(|e| e.reason_codes.clone())
        .unwrap_or_default();
    assert!(
        codes.contains(&"decision_work_blocked".to_string()),
        "{codes:?}"
    );
}

#[test]
fn soft_preference_keeps_decision_work_eligible() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let owner = create_story(&store, "Owner");
    let foundation = decision_work_ticket(&store, &owner, "BR-FOUND");
    let later = decision_work_ticket(&store, &owner, "BR-LATER");
    // `later` preferred_after `foundation`: advisory only, not a blocker.
    store
        .add_edge_with_context(
            EdgeType::PreferredAfter,
            later.id.clone(),
            foundation.id.clone(),
            ctx(),
        )
        .unwrap();

    let decision = decision_report(&store, Some(&owner.id));
    let ids = decision_item_ids(&decision.items);
    assert!(ids.contains(&foundation.id));
    assert!(ids.contains(&later.id));
}

#[test]
fn for_filter_excludes_other_destination_owners() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let owner_a = create_story(&store, "Owner A");
    let owner_b = create_story(&store, "Owner B");
    let dw_a = decision_work_ticket(&store, &owner_a, "BR-A");
    let dw_b = decision_work_ticket(&store, &owner_b, "BR-B");

    let for_a = decision_report(&store, Some(&owner_a.id));
    let for_b = decision_report(&store, Some(&owner_b.id));
    assert_eq!(decision_item_ids(&for_a.items), vec![dw_a.id.clone()]);
    assert_eq!(decision_item_ids(&for_b.items), vec![dw_b.id.clone()]);
    assert_eq!(for_a.shaping_context.map(|_| ()), None);
}

#[test]
fn claim_state_not_evaluated_and_not_persisted() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let owner = create_story(&store, "Owner");
    let _dw = decision_work_ticket(&store, &owner, "BR-1");
    let fingerprint_before = store.export().unwrap().graph_fingerprint;

    let decision = decision_report(&store, Some(&owner.id));
    let execution = execution_report(&store, Some(&owner.id));
    assert_eq!(decision.claim_state, "not_evaluated");
    assert_eq!(execution.claim_state, "not_evaluated");

    // A read-only projection must not change the graph and must not persist any
    // frontier/claim state.
    let fingerprint_after = store.export().unwrap().graph_fingerprint;
    assert_eq!(fingerprint_before, fingerprint_after);
    let events_dir = repo.join(".pulse/events");
    let mut pred = |v: &Value| {
        v.get("event_type")
            .and_then(|t| t.as_str())
            .map(|t| t.contains("frontier") || t.contains("claim"))
            .unwrap_or(false)
    };
    let has_frontier_event = walk_json(&events_dir, &mut pred);
    assert!(!has_frontier_event, "frontier must not emit events");
    assert!(
        !repo.join(".pulse/frontiers").exists(),
        "frontier must not persist a frontier store"
    );
}

fn walk_json(dir: &std::path::Path, pred: &mut dyn FnMut(&Value) -> bool) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if walk_json(&path, pred) {
                return true;
            }
        } else if let Ok(bytes) = fs::read(&path) {
            if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                if pred(&v) {
                    return true;
                }
            }
        }
    }
    false
}

#[test]
fn deterministic_id_ordering_is_priority_agnostic() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let owner = create_story(&store, "Owner");
    // Allocate several decision-work tickets; allocation is monotonic so later
    // creations get lexicographically larger IDs. Membership order must follow
    // ID, independent of creation/priority intent.
    let mut ids = Vec::new();
    for branch in ["BR-3", "BR-1", "BR-2"] {
        ids.push(decision_work_ticket(&store, &owner, branch).id);
    }
    let mut sorted = ids.clone();
    sorted.sort();

    let decision = decision_report(&store, Some(&owner.id));
    assert_eq!(decision_item_ids(&decision.items), sorted);

    // Re-running yields identical, stable output.
    let again = decision_report(&store, Some(&owner.id));
    assert_eq!(again.items, decision.items);
}

#[test]
fn cache_corruption_or_deletion_rebuilds_equivalent_semantics() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let owner = create_story(&store, "Owner");
    let _dw = decision_work_ticket(&store, &owner, "BR-1");
    let _ready = ready_implementation_ticket(repo, &store, "rcpt_01J00000000000000000000005");

    let baseline_decision = decision_report(&store, None);
    let baseline_execution = execution_report(&store, None);

    // Corrupt then delete the disposable projection cache; semantics must not
    // depend on it.
    let cache = repo.join(".pulse/cache/workgraph.snapshot.json");
    if cache.exists() {
        fs::write(&cache, b"not json").unwrap();
    }
    let after_corrupt = decision_report(&store, None);
    assert_eq!(after_corrupt, baseline_decision);
    let _ = fs::remove_file(&cache);
    let after_delete_decision = decision_report(&store, None);
    let after_delete_execution = execution_report(&store, None);
    assert_eq!(after_delete_decision, baseline_decision);
    assert_eq!(after_delete_execution, baseline_execution);
}

#[test]
fn decision_work_with_disposed_branch_is_excluded() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let owner = create_story(&store, "Owner");
    // Owner shaping receipt resolves the branch -> decision work is disposed.
    apply_owner_shaping_with_branch(
        repo,
        &store,
        &owner,
        "rcpt_01J00000000000000000000006",
        "BR-RESOLVED",
        BranchDisposition::Resolved {
            resolution: ShapingResolutionPointer {
                kind: "decision".to_string(),
                id: "DEC-001".to_string(),
                revision: 1,
                gist: "Direction chosen.".to_string(),
            },
        },
    );
    // Re-read owner after apply bumped its revision.
    let owner = store.show_node(&owner.id).unwrap();
    let dw = decision_work_ticket(&store, &owner, "BR-RESOLVED");

    let report = decision_report(&store, Some(&owner.id));
    assert!(report.items.is_empty());
    let codes: Vec<String> = report
        .excluded
        .iter()
        .find(|e| e.id == dw.id)
        .map(|e| e.reason_codes.clone())
        .unwrap_or_default();
    assert!(
        codes.contains(&"decision_work_branch_disposed".to_string()),
        "{codes:?}"
    );
    // The owner's current shaping context is surfaced in the report.
    assert!(report.shaping_context.is_some());
    assert_eq!(
        report.shaping_context.as_ref().unwrap().receipt_id,
        "rcpt_01J00000000000000000000006"
    );
}

#[test]
fn decision_work_with_open_blocking_branch_remains_eligible() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let owner = create_story(&store, "Owner");
    apply_owner_shaping_with_branch(
        repo,
        &store,
        &owner,
        "rcpt_01J00000000000000000000007",
        "BR-BLOCKING",
        BranchDisposition::Blocking {
            linked_decision_work: None,
        },
    );
    let owner = store.show_node(&owner.id).unwrap();
    let dw = decision_work_ticket(&store, &owner, "BR-BLOCKING");

    let report = decision_report(&store, Some(&owner.id));
    assert!(report.items.iter().any(|i| i.id == dw.id));
}

#[test]
fn empty_frontier_is_success() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let decision = store
        .frontier(FrontierKind::Decision, None, None, false)
        .unwrap();
    let execution = store
        .frontier(FrontierKind::Execution, None, None, false)
        .unwrap();
    assert!(matches!(decision, FrontierReport::Decision(_)));
    assert!(matches!(execution, FrontierReport::Execution(_)));
}

#[test]
fn execution_frontier_rejects_unsupported_profile() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let err = store
        .frontier(FrontierKind::Execution, None, Some("bogus_v9"), true)
        .unwrap_err();
    assert_eq!(err.code(), "readiness_profile_unsupported");
}

#[test]
fn frontier_rejects_invalid_destination_owner() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    // A Ticket id is not a valid destination owner.
    let err = store
        .frontier(FrontierKind::Decision, Some("TK-999"), None, true)
        .unwrap_err();
    assert_eq!(err.code(), "frontier_destination_invalid");
    // A well-formed but missing owner id is NotFound.
    let err = store
        .frontier(FrontierKind::Decision, Some("ST-999"), None, true)
        .unwrap_err();
    assert_eq!(err.code(), "not_found");
}

// ---------------------------------------------------------------------------
// CLI contract
// ---------------------------------------------------------------------------

fn run(repo: &TempDir, args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args(args)
        .output()
        .expect("run pulse")
}

#[test]
fn cli_decision_frontier_emits_stable_json() {
    let repo = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(repo.path());
    let owner = create_story(&store, "Owner");
    let dw = decision_work_ticket(&store, &owner, "BR-CLI");

    let output = run(
        &repo,
        &[
            "work", "frontier", "--kind", "decision", "--for", &owner.id, "--json",
        ],
    );
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["kind"], "decision");
    assert_eq!(report["code"], "decision_frontier");
    assert_eq!(report["claim_state"], "not_evaluated");
    assert_eq!(report["for"], owner.id);
    assert_eq!(report["items"][0]["id"], dw.id);
    assert_eq!(report["items"][0]["branch_id"], "BR-CLI");
}

#[test]
fn cli_execution_frontier_emits_stable_json() {
    let repo = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(repo.path());
    let ready = ready_implementation_ticket(repo.path(), &store, "rcpt_01J00000000000000000000008");

    let output = run(
        &repo,
        &["work", "frontier", "--kind", "execution", "--json"],
    );
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["kind"], "execution");
    assert_eq!(report["claim_state"], "not_evaluated");
    assert_eq!(report["dispatch_authorized"], false);
    assert_eq!(report["readiness_profile"], "phase1_contract_readiness_v1");
    assert_eq!(report["items"][0]["id"], ready.id);
    assert_eq!(report["items"][0]["frontier_eligible"], true);
}

#[test]
fn cli_empty_frontier_succeeds() {
    let repo = tempfile::tempdir().unwrap();
    let output = run(&repo, &["work", "frontier", "--kind", "decision", "--json"]);
    assert!(output.status.success(), "empty frontier should succeed");
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["items"].as_array().unwrap().is_empty());
}

#[test]
fn cli_include_excluded_populates_excluded() {
    let repo = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(repo.path());
    let owner_a = create_story(&store, "A");
    let owner_b = create_story(&store, "B");
    let _dw_b = decision_work_ticket(&store, &owner_b, "BR-B");

    // Without --include-excluded: excluded is empty.
    let out = run(
        &repo,
        &[
            "work",
            "frontier",
            "--kind",
            "decision",
            "--for",
            &owner_a.id,
            "--json",
        ],
    );
    assert!(out.status.success());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(report["excluded"].as_array().unwrap().is_empty());

    // With --include-excluded: the other-destination decision work appears.
    let out = run(
        &repo,
        &[
            "work",
            "frontier",
            "--kind",
            "decision",
            "--for",
            &owner_a.id,
            "--include-excluded",
            "--json",
        ],
    );
    assert!(out.status.success());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    let excluded = report["excluded"].as_array().unwrap();
    assert!(!excluded.is_empty());
    assert!(excluded.iter().any(|e| e["reason_codes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c == "decision_work_wrong_destination")));
}

#[test]
fn cli_rejects_unsupported_profile() {
    let repo = tempfile::tempdir().unwrap();
    let out = run(
        &repo,
        &[
            "work",
            "frontier",
            "--kind",
            "execution",
            "--profile",
            "bogus_v9",
            "--json",
        ],
    );
    assert!(!out.status.success());
    let err: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(err["code"], "readiness_profile_unsupported");
}
