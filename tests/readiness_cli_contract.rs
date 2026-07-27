//! S7-I4 CLI contract smoke test for `pulse work ready`. The library-depth
//! readiness/gate logic lives in `readiness.rs`; this file covers the CLI
//! wiring: stable JSON output, profile validation and the documented non-zero
//! gate exit for not-ready work.

use chrono::Utc;
use pulse::canonical_json::{hash_bytes, to_canonical_bytes};
use pulse::evidence::model::*;
use pulse::graph::contract::{
    ContentRef, ContractItem, ContractScope, EffortMetadata, ImplementationContract,
    ImplementationMode, ImplementationSemanticImpact, Materialization, PlanPolicy, QaImpactPosture,
    Risk, SurfaceRef, TicketRole, WorkSurface,
};
use pulse::graph::node::DocumentationImpactPosture;
use pulse::graph::store::{
    ContractSetRequest, DocumentationImpactUpdate, OperationContext, QaImpactUpdate,
};
use pulse::id::WorkKind;
use pulse::policy::{AuthorityPolicy, AuthorityPrincipal};
use pulse::JsonGraphStore;
use serde_json::Value;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn bin() -> String {
    std::env::var("CARGO_BIN_EXE_pulse").unwrap_or_else(|_| "target/debug/pulse".to_string())
}

fn run(repo: &TempDir, args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args(args)
        .output()
        .expect("run pulse")
}

fn ctx() -> OperationContext {
    OperationContext {
        actor: "human:tester".to_string(),
        now: Utc::now(),
    }
}

fn write_policy(repo: &TempDir, grants: &[&str]) {
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
    let path = repo.path().join(".pulse/policy/authority.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, to_canonical_bytes(&policy).unwrap()).unwrap();
}

/// Build a fully-ready ticket through the library so the CLI test focuses on the
/// `work ready` command surface. Contract-revision bumps precede the shaping
/// receipt so the receipt binds the final contract revision.
fn ready_ticket(repo: &TempDir, store: &JsonGraphStore) -> String {
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
    let node = store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Ready ticket".to_string(),
            pulse::graph::contract::PublicCreateClassification {
                role: Some(TicketRole::Implementation),
                risk: Some(Risk::Low),
                materialization: Some(Materialization::R1),
            },
            OperationContext::default(),
        )
        .unwrap()
        .value;
    let brief_rel = format!("{}/ticket.md", node.content_dir);
    let brief_path = repo.path().join(&brief_rel);
    fs::create_dir_all(brief_path.parent().unwrap()).unwrap();
    fs::write(&brief_path, b"# Ticket\ncontent").unwrap();
    let brief_hash = hash_bytes(&fs::read(&brief_path).unwrap());
    let contract = ImplementationContract {
        mode: ImplementationMode::Guided,
        work_surface: WorkSurface::Code,
        plan_policy: PlanPolicy::None,
        semantic_impact: ImplementationSemanticImpact::NoBehaviorOrPublicRiskChange,
        effort: EffortMetadata::default(),
        verification_profile: "service-change".to_string(),
        brief: Some(ContentRef {
            path: format!("{}/ticket.md", node.content_dir),
            content_hash: brief_hash.clone(),
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
    };
    let node = store
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
        .value;
    let node = store
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
        .value;
    let node = store
        .update_documentation_impact(
            &node.id,
            node.revision,
            DocumentationImpactUpdate {
                posture: DocumentationImpactPosture::None,
                rationale: Some("None.".to_string()),
                required_documents: vec![],
                deferred_to: vec![],
                paths: vec![],
                domains: vec![],
                labels: vec![],
            },
            "human:tester".to_string(),
        )
        .unwrap()
        .value;
    // Record + apply a shaping receipt binding the final contract revision.
    let receipt = ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 1,
        id: "rcpt_01J00000000000000000000010".to_string(),
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
                path: brief_rel,
                sha256: brief_hash,
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
    let file = repo.path().join("shaping.json");
    fs::write(&file, to_canonical_bytes(&receipt).unwrap()).unwrap();
    pulse::evidence::record_receipt(repo.path(), None, &file).unwrap();
    let node = store
        .apply_shaping_with_context(&node.id, node.revision, &receipt.id, None, ctx())
        .unwrap()
        .value;
    let node = store
        .transition_node_with_context(
            &node.id,
            pulse::graph::node::NodeStatus::Shaped,
            node.revision,
            None,
            ctx(),
        )
        .unwrap()
        .value;
    let ready = store
        .transition_node_with_context(
            &node.id,
            pulse::graph::node::NodeStatus::Ready,
            node.revision,
            None,
            ctx(),
        )
        .unwrap()
        .value;
    ready.id
}

#[test]
fn work_ready_emits_stable_json_for_ready_ticket() {
    let repo = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(repo.path());
    let id = ready_ticket(&repo, &store);

    let output = run(&repo, &["work", "ready", &id, "--json"]);
    assert!(
        output.status.success(),
        "ready query failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["profile"], "phase1_contract_readiness_v1");
    assert_eq!(report["status"], "ready");
    assert_eq!(report["transition_eligible"], true);
    assert_eq!(report["dispatch_authorized"], false);
    assert!(report["readiness_fingerprint"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert!(report["gate_families"].is_array());
    assert!(report["future_gate_families"].is_array());
}

#[test]
fn work_ready_returns_nonzero_for_not_ready_work() {
    let repo = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(repo.path());
    // A fresh draft ticket is not ready (no contract, no shaping, qa unknown).
    let node = store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Draft".to_string(),
            pulse::graph::contract::PublicCreateClassification {
                role: Some(TicketRole::Implementation),
                risk: Some(Risk::Low),
                materialization: Some(Materialization::R1),
            },
            OperationContext::default(),
        )
        .unwrap()
        .value;

    let output = run(&repo, &["work", "ready", &node.id, "--json"]);
    assert!(
        !output.status.success(),
        "not-ready query should exit non-zero"
    );
    // The report is still rendered on stdout for CI/automation.
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_ne!(report["status"], "ready");
    // The error envelope is on stderr with a stable code.
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["code"], "readiness_not_ready");
}

#[test]
fn work_ready_rejects_unsupported_profile() {
    let repo = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(repo.path());
    let id = ready_ticket(&repo, &store);
    let output = run(
        &repo,
        &["work", "ready", &id, "--profile", "bogus_v9", "--json"],
    );
    assert!(!output.status.success());
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["code"], "readiness_profile_unsupported");
}
