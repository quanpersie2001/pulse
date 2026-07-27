//! S7-I4 readiness composition, narrow fingerprint, stale-ready semantics and
//! lifecycle gate tests.
//!
//! These tests exercise the harness against temporary target repositories only.
//! They never point Pulse at this development repository.

use chrono::Utc;
use pulse::canonical_json::{hash_bytes, to_canonical_bytes};
use pulse::evidence::model::*;
use pulse::graph::contract::{
    ContentRef, ContractItem, ContractScope, EffortMetadata, ImplementationContract,
    ImplementationMode, ImplementationSemanticImpact, Materialization, PlanPolicy, QaImpactPosture,
    Risk, SurfaceRef, TicketRole, WorkSurface,
};
use pulse::graph::lifecycle::{installed_gate, GateProfile};
use pulse::graph::node::{DocumentationImpactPosture, NodeStatus};
use pulse::graph::readiness::{
    self, GateFamilyReport, GateStatus, ReadinessReport, ReadinessStatus, READINESS_PROFILE,
};
use pulse::graph::store::{
    ContractSetRequest, DocumentationImpactUpdate, OperationContext, QaImpactUpdate,
};
use pulse::id::WorkKind;
use pulse::policy::{AuthorityPolicy, AuthorityPrincipal};
use pulse::storage::transaction::TransactionFailpoint;
use pulse::{JsonGraphStore, PulseError};
use std::fs;

fn write_json(path: &std::path::Path, value: &impl serde::Serialize) {
    fs::write(path, to_canonical_bytes(value).unwrap()).unwrap();
}

fn write_policy(repo: &std::path::Path, grants: &[&str]) {
    let mut sorted_grants = grants.iter().map(|g| g.to_string()).collect::<Vec<_>>();
    sorted_grants.sort();
    sorted_grants.dedup();
    let policy = AuthorityPolicy {
        schema_version: 1,
        revision: 1,
        principals: vec![AuthorityPrincipal {
            kind: ActorKind::Human,
            id: "tester".to_string(),
            grants: sorted_grants,
        }],
    };
    let path = repo.join(".pulse/policy/authority.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_json(&path, &policy);
}

fn full_grants() -> &'static [&'static str] {
    &[
        "shape.apply",
        "shape.approve.R1",
        "qa.none.approve",
        "work.transition.shaped",
        "work.transition.ready",
    ]
}

fn ctx() -> OperationContext {
    OperationContext {
        actor: "human:tester".to_string(),
        now: Utc::now(),
    }
}

fn create_ticket(store: &JsonGraphStore) -> pulse::graph::node::Node {
    let classification = pulse::graph::contract::PublicCreateClassification {
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

fn write_brief(repo: &std::path::Path, node: &pulse::graph::node::Node) -> String {
    let rel = format!("{}/ticket.md", node.content_dir);
    let path = repo.join(&rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"# Ticket\nimplementation contract content").unwrap();
    hash_bytes(&fs::read(&path).unwrap())
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
        objective: "Distinguish expired and invalid tokens.".to_string(),
        current_behavior: "Both map to InvalidToken.".to_string(),
        target_behavior: "Expired maps to TokenExpired.".to_string(),
        code_anchors: vec![SurfaceRef::path("src/auth.rs")],
        documentation_anchors: vec![],
        configuration_anchors: vec![],
        data_anchors: vec![],
        research_refs: vec![],
        required_changes: vec![ContractItem {
            id: "CHG-1".to_string(),
            summary: "Introduce expired-token error.".to_string(),
        }],
        invariants: vec![ContractItem {
            id: "INV-1".to_string(),
            summary: "Do not leak secrets.".to_string(),
        }],
        acceptance: vec![ContractItem {
            id: "AC-1".to_string(),
            summary: "Expired token is classified.".to_string(),
        }],
        scope: ContractScope::default(),
        implementation_freedom: vec![ContractItem {
            id: "FREE-1".to_string(),
            summary: "Helper structure is free.".to_string(),
        }],
        required_decisions: vec![],
        shared_approach_refs: vec![],
        expected_evidence: vec![],
        expected_handoff: vec![],
    }
}

fn set_contract(
    store: &JsonGraphStore,
    node: &pulse::graph::node::Node,
    contract: ImplementationContract,
) -> pulse::graph::node::Node {
    store
        .set_contract_with_context(
            &node.id,
            node.revision,
            ContractSetRequest {
                role: TicketRole::Implementation,
                implementation: Some(contract),
                decision_work: None,
            },
            ctx(),
        )
        .unwrap()
        .value
}

fn set_qa(
    store: &JsonGraphStore,
    node: &pulse::graph::node::Node,
    posture: QaImpactPosture,
) -> pulse::graph::node::Node {
    store
        .set_qa_impact_with_context(
            &node.id,
            node.revision,
            QaImpactUpdate {
                posture,
                rationale: Some("Internal refactor; behavior unchanged.".to_string()),
                behavioral_owner: None,
                affected_case_ids: vec![],
            },
            ctx(),
        )
        .unwrap()
        .value
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

fn record_shaping(
    repo: &std::path::Path,
    node: &pulse::graph::node::Node,
    receipt_id: &str,
    brief_hash: &str,
) {
    let manifest = pulse::evidence::bootstrap(repo).unwrap().manifest;
    let rel = format!("{}/ticket.md", node.content_dir);
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
                path: rel,
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
                reference: "PULSE.md#human-judgment-boundaries".to_string(),
            },
            reconciliation: None,
            remaining_uncertainty: vec![],
        }),
    };
    let _ = manifest;
    let file = repo.join(format!("{receipt_id}.json"));
    write_json(&file, &receipt);
    pulse::evidence::record_receipt(repo, None, &file).unwrap();
}

fn apply_shaping(
    store: &JsonGraphStore,
    node: &pulse::graph::node::Node,
    receipt_id: &str,
) -> pulse::graph::node::Node {
    store
        .apply_shaping_with_context(&node.id, node.revision, receipt_id, None, ctx())
        .unwrap()
        .value
}

/// Apply all contract-revision-bumping mutations, then record + apply a shaping
/// receipt binding the final contract revision. Returns a node with the shaping
/// pointer set (still in its original lifecycle status).
fn prepare_with_shaping(
    repo: &std::path::Path,
    store: &JsonGraphStore,
    receipt_id: &str,
    with_qa_none: bool,
    with_docs_none: bool,
) -> pulse::graph::node::Node {
    let mut node = create_ticket(store);
    let brief_hash = write_brief(repo, &node);
    node = set_contract(store, &node, implementation_contract(&node, &brief_hash));
    if with_qa_none {
        node = set_qa(store, &node, QaImpactPosture::None);
    }
    if with_docs_none {
        node = set_docs_none(store, &node);
    }
    record_shaping(repo, &node, receipt_id, &brief_hash);
    apply_shaping(store, &node, receipt_id)
}

fn ready_ticket(repo: &std::path::Path, store: &JsonGraphStore) -> pulse::graph::node::Node {
    write_policy(repo, full_grants());
    let node = prepare_with_shaping(repo, store, "rcpt_01J00000000000000000000001", true, true);
    let shaped = store
        .transition_node_with_context(&node.id, NodeStatus::Shaped, node.revision, None, ctx())
        .unwrap()
        .value;
    store
        .transition_node_with_context(&shaped.id, NodeStatus::Ready, shaped.revision, None, ctx())
        .unwrap()
        .value
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
    assert_eq!(
        family(&report, "structural_executability").status,
        GateStatus::Passed
    );
    assert_eq!(
        family(&report, "implementation_contract").status,
        GateStatus::Passed
    );
    assert_eq!(
        family(&report, "shaping_receipt_integrity").status,
        GateStatus::Passed
    );
    assert_eq!(family(&report, "qa_impact").status, GateStatus::Passed);
    assert_eq!(
        family(&report, "documentation_impact").status,
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
            Some(pulse::graph::lifecycle::TransitionReason {
                code: "rework_needed".to_string(),
                summary: "back to shaped".to_string(),
                reference: None,
            }),
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

    // Brief content byte change stales readiness.
    let brief_path = repo.join(format!("{}/ticket.md", node.content_dir));
    fs::write(&brief_path, b"# Ticket\nchanged content").unwrap();
    let changed = store.readiness(&node.id).unwrap();
    assert_eq!(changed.status, ReadinessStatus::Stale);
    assert_ne!(changed.readiness_fingerprint, baseline);

    // Restore content -> fingerprint returns to baseline.
    fs::write(&brief_path, b"# Ticket\nimplementation contract content").unwrap();
    let restored = store.readiness(&node.id).unwrap();
    assert_eq!(restored.readiness_fingerprint, baseline);

    // Policy change participates in the fingerprint.
    write_policy(
        repo,
        &[
            "shape.apply",
            "shape.approve.R1",
            "qa.none.approve",
            "work.transition.shaped",
        ],
    );
    let policy_changed = store.readiness(&node.id).unwrap();
    assert_ne!(policy_changed.readiness_fingerprint, baseline);
}

#[test]
fn required_decisions_gate_consumes_decision_acceptance_proof() {
    // S7-57: a required Decision reference must resolve to a current accepted
    // Decision acceptance proof. Without the proof the required_decisions gate
    // is `unavailable` (not passed); with a current proof it passes; stale
    // Decision prose makes it stale.
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    write_policy(repo, full_grants());
    let store = JsonGraphStore::new(repo);

    // Decision node + content + acceptance receipt.
    let decision = store
        .create_node(WorkKind::Decision, "Token compatibility".to_string())
        .unwrap()
        .value;
    let decision_rel = format!("works/{}/decision.md", decision.id);
    let decision_path = repo.join(&decision_rel);
    fs::create_dir_all(decision_path.parent().unwrap()).unwrap();
    fs::write(&decision_path, b"# Decision\nPreserve compatibility.").unwrap();
    let decision_hash = hash_bytes(&fs::read(&decision_path).unwrap());
    let manifest = pulse::evidence::bootstrap(repo).unwrap().manifest;
    let acceptance_id = "rcpt_01J00000000000000000000077";
    let acceptance_receipt = ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 1,
        id: acceptance_id.to_string(),
        kind: ReceiptKind::DecisionAcceptance,
        result: ReceiptResult::Passed,
        actor: ActorRef {
            kind: ActorKind::Human,
            id: "tester".to_string(),
        },
        recorded_at: Utc::now(),
        subject: SubjectRef {
            kind: "work".to_string(),
            id: decision.id.clone(),
        },
        bindings: ReceiptBindings {
            work: vec![WorkBinding {
                id: decision.id.clone(),
                revision: decision.revision,
            }],
            source: None,
            content: vec![ContentBinding {
                path: decision_rel.clone(),
                sha256: decision_hash.clone(),
            }],
            artifacts: vec![],
            graph_fingerprint_observed: None,
        },
        payload: ReceiptPayload::DecisionAcceptance(DecisionAcceptancePayload {
            payload_version: 1,
            decision: DecisionAcceptanceDecision {
                id: decision.id.clone(),
                revision_observed: decision.revision,
                contract_revision: decision.contract_revision,
                content: DecisionContentSnapshot {
                    path: decision_rel,
                    content_hash: decision_hash,
                },
            },
            accepted_outcome: "Accept compatibility direction.".to_string(),
            approver: ActorRef {
                kind: ActorKind::Human,
                id: "tester".to_string(),
            },
            source_posture: SourcePosture::NotRequiredContentBound,
        }),
    };
    let _ = manifest;

    // Locked ticket referencing the decision, but the acceptance receipt is
    // not recorded yet. The contract is structurally valid (receipt ref is a
    // well-formed id+hash placeholder); readiness resolves the proof.
    let ticket = create_ticket(&store);
    let brief_hash = write_brief(repo, &ticket);
    let mut contract = implementation_contract(&ticket, &brief_hash);
    contract.mode = ImplementationMode::Locked;
    contract.required_decisions = vec![pulse::graph::contract::RequiredDecisionRef {
        id: decision.id.clone(),
        contract_revision: decision.contract_revision,
        acceptance_receipt: pulse::graph::contract::ReceiptRef {
            id: acceptance_id.to_string(),
            hash: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
        },
    }];
    let ticket = set_contract(&store, &ticket, contract);
    let ticket = set_qa(&store, &ticket, QaImpactPosture::None);
    let ticket = set_docs_none(&store, &ticket);
    record_shaping(
        repo,
        &ticket,
        "rcpt_01J00000000000000000000078",
        &brief_hash,
    );
    let ticket = apply_shaping(&store, &ticket, "rcpt_01J00000000000000000000078");

    let report = store.readiness(&ticket.id).unwrap();
    let gate = family(&report, "required_decisions");
    assert_eq!(gate.status, GateStatus::Unavailable);
    assert!(gate
        .reason_codes
        .contains(&"decision_acceptance_missing".to_string()));

    // Record the acceptance proof and re-bind the contract to its real hash.
    let file = repo.join("acceptance.json");
    write_json(&file, &acceptance_receipt);
    let outcome = pulse::evidence::record_receipt(repo, None, &file).unwrap();
    let real_hash = outcome.receipt_hash;

    let mut contract = store
        .show_contract(&ticket.id)
        .unwrap()
        .implementation
        .unwrap();
    contract.required_decisions[0].acceptance_receipt.hash = real_hash;
    // Bump contract revision via a fresh set; re-record shaping for the new
    // contract revision so the shaping pointer stays current.
    let ticket = set_contract(&store, &ticket, contract);
    record_shaping(
        repo,
        &ticket,
        "rcpt_01J00000000000000000000079",
        &brief_hash,
    );
    let ticket = apply_shaping(&store, &ticket, "rcpt_01J00000000000000000000079");

    let report = store.readiness(&ticket.id).unwrap();
    let gate = family(&report, "required_decisions");
    assert_eq!(gate.status, GateStatus::Passed);

    // Stale Decision prose makes the proof stale.
    fs::write(&decision_path, b"# Decision\nChanged direction.").unwrap();
    let report = store.readiness(&ticket.id).unwrap();
    let gate = family(&report, "required_decisions");
    assert_eq!(gate.status, GateStatus::Stale);
    assert!(gate
        .reason_codes
        .contains(&"decision_acceptance_stale".to_string()));
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
fn qa_unknown_blocks_ready_and_required_is_unavailable() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    write_policy(
        repo,
        &[
            "shape.apply",
            "shape.approve.R1",
            "qa.none.approve",
            "work.transition.shaped",
            "work.transition.ready",
        ],
    );
    // QA left at default unknown.
    let node = prepare_with_shaping(repo, &store, "rcpt_01J00000000000000000000002", false, true);
    let report = store.readiness(&node.id).unwrap();
    assert_eq!(family(&report, "qa_impact").status, GateStatus::Failed);
    assert!(family(&report, "qa_impact")
        .reason_codes
        .contains(&"qa_impact_unknown".to_string()));
    assert_ne!(report.status, ReadinessStatus::Ready);

    // QA=required is unavailable until the Phase 3 baseline resolver.
    let story = store
        .create_node(WorkKind::Story, "Behavioral owner".to_string())
        .unwrap()
        .value;
    let node = store
        .set_qa_impact_with_context(
            &node.id,
            node.revision,
            QaImpactUpdate {
                posture: QaImpactPosture::Required,
                rationale: Some("Behavioral change needs baseline.".to_string()),
                behavioral_owner: Some(story.id.clone()),
                affected_case_ids: vec!["CASE-LOGIN-001".to_string()],
            },
            ctx(),
        )
        .unwrap()
        .value;
    let report = store.readiness(&node.id).unwrap();
    let qa = family(&report, "qa_impact");
    assert_eq!(qa.status, GateStatus::Unavailable);
    assert!(qa
        .reason_codes
        .contains(&"qa_baseline_resolver_unavailable".to_string()));
}

#[test]
fn missing_authority_policy_makes_authority_gate_unavailable() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    // Apply shaping needs policy, so grant apply/approve but NOT transition grants.
    write_policy(
        repo,
        &["shape.apply", "shape.approve.R1", "qa.none.approve"],
    );
    let node = prepare_with_shaping(repo, &store, "rcpt_01J00000000000000000000003", true, true);
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
    write_policy(
        repo,
        &["shape.apply", "shape.approve.R1", "qa.none.approve"],
    );
    // Docs left unknown.
    let node = prepare_with_shaping(repo, &store, "rcpt_01J00000000000000000000004", true, false);
    let report = store.readiness(&node.id).unwrap();
    assert_eq!(
        family(&report, "documentation_impact").status,
        GateStatus::Failed
    );
    assert_ne!(report.status, ReadinessStatus::Ready);

    // Setting docs to none passes the family.
    let node = set_docs_none(&store, &node);
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
fn draft_to_shaped_transition_records_shaped_gate_profile() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    write_policy(
        repo,
        &[
            "shape.apply",
            "shape.approve.R1",
            "qa.none.approve",
            "work.transition.shaped",
        ],
    );
    let node = prepare_with_shaping(repo, &store, "rcpt_01J00000000000000000000005", true, true);
    let shaped = store
        .transition_node_with_context(&node.id, NodeStatus::Shaped, node.revision, None, ctx())
        .unwrap()
        .value;
    assert_eq!(shaped.status, NodeStatus::Shaped);
    let events = collect_events(repo, "work.node.transitioned");
    let last = events.last().unwrap();
    assert_eq!(last["payload"]["gate_profile"], "phase1_shaped_v1");
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
    write_policy(
        repo,
        &[
            "shape.apply",
            "shape.approve.R1",
            "qa.none.approve",
            "work.transition.shaped",
        ],
    );
    let node = prepare_with_shaping(repo, &store, "rcpt_01J00000000000000000000006", true, true);
    let shaped = store
        .transition_node_with_context(&node.id, NodeStatus::Shaped, node.revision, None, ctx())
        .unwrap()
        .value;

    // Missing transition-ready grant -> denied before gate evaluation.
    let err = store
        .transition_node_with_context(&shaped.id, NodeStatus::Ready, shaped.revision, None, ctx())
        .unwrap_err();
    assert_eq!(err.code(), "readiness_authority_denied");

    // Grant it -> succeeds.
    write_policy(repo, full_grants());
    let ready = store
        .transition_node_with_context(&shaped.id, NodeStatus::Ready, shaped.revision, None, ctx())
        .unwrap()
        .value;
    assert_eq!(ready.status, NodeStatus::Ready);
    let events = collect_events(repo, "work.node.transitioned");
    let ready_events: Vec<_> = events
        .iter()
        .filter(|e| e["payload"]["to"] == "ready")
        .collect();
    assert_eq!(ready_events.len(), 1);
    assert_eq!(
        ready_events[0]["payload"]["gate_profile"],
        "phase1_contract_readiness_v1"
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
    let node = prepare_with_shaping(repo, &store, "rcpt_01J00000000000000000000007", true, true);
    let shaped = store
        .transition_node_with_context(&node.id, NodeStatus::Shaped, node.revision, None, ctx())
        .unwrap()
        .value;

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
    let classification = pulse::graph::contract::PublicCreateClassification {
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
fn shaped_gate_fails_without_shaping_pointer() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    write_policy(repo, &["work.transition.shaped"]);
    let node = create_ticket(&store);
    let err = store
        .transition_node_with_context(&node.id, NodeStatus::Shaped, node.revision, None, ctx())
        .unwrap_err();
    assert_eq!(err.code(), "readiness_not_ready");
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
    let node = prepare_with_shaping(repo, &prep, "rcpt_01J00000000000000000000008", true, true);
    let shaped = prep
        .transition_node_with_context(&node.id, NodeStatus::Shaped, node.revision, None, ctx())
        .unwrap()
        .value;

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
        ready_events[0]["payload"]["gate_profile"],
        "phase1_contract_readiness_v1"
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
    let _ = store
        .frontier(
            pulse::graph::frontier::FrontierKind::Execution,
            None,
            None,
            false,
        )
        .unwrap();
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

    // ready -> blocked (supported, reason required).
    let blocked = store
        .transition_node_with_context(
            &ready.id,
            NodeStatus::Blocked,
            ready.revision,
            Some(pulse::graph::lifecycle::TransitionReason {
                code: "dependency_unavailable".to_string(),
                summary: "blocked".to_string(),
                reference: None,
            }),
            ctx(),
        )
        .unwrap()
        .value;
    assert_eq!(blocked.status, NodeStatus::Blocked);

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
    let reshaped = store
        .transition_node_with_context(
            &blocked.id,
            NodeStatus::Shaped,
            blocked.revision,
            Some(pulse::graph::lifecycle::TransitionReason {
                code: "dependency_restored".to_string(),
                summary: "resume".to_string(),
                reference: None,
            }),
            ctx(),
        )
        .unwrap()
        .value;
    assert_eq!(reshaped.status, NodeStatus::Shaped);
    let _ = PulseError::Validation {
        code: "ready",
        message: String::new(),
    }; // keep import used
    let _ = readiness::SHAPED_GATE_PROFILE;
}
