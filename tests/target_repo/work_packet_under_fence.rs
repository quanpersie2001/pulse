//! P2S2-I6: Fence-aware packet revalidation integration tests.
//!
//! Verifies that the public `work_packet` method still builds identical
//! packets after the I6 phase extraction refactoring, and that preview
//! semantics remain unchanged.
//!
//! The `work_packet_under_fence` internal method is `pub(crate)` and tested
//! in `src/kernel/packet.rs`'s in-module tests.

use pulse::canonical_json::to_canonical_bytes;
use pulse::graph::store::OperationContext;
use pulse::JsonGraphStore;

use crate::common::fixture_repo::TestRepo;

fn ctx() -> OperationContext {
    OperationContext {
        actor: "human:tester".to_string(),
        now: chrono::Utc::now(),
    }
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

#[test]
fn public_work_packet_builds_after_refactoring() {
    // Use the existing setup from the target_repo test fixture. We rely on
    // `work_packet_target_repo` tests to validate the full setup/contract;
    // here we just verify the public API still works after the I6 phase
    // extraction refactoring.
    let repo = TestRepo::from_fixture("minimal-service");

    // Run a full CLI setup via the existing test pattern: bootstrap, create
    // ticket, set contract, set qa, set docs impact, transition to ready.
    let ticket_id = test_setup_ready_ticket(&repo);

    let store = JsonGraphStore::new(repo.path());
    let packet = store.work_packet(&ticket_id).expect("work packet builds");

    // Verify preview semantics (P2S2-D1)
    assert_eq!(packet.code, "reservation_candidate");
    assert_eq!(packet.schema_version, 1);
    assert_eq!(packet.profile, "phase2_work_packet_preview_v1");
    assert!(!packet.dispatch.dispatch_authorized);
    assert_eq!(packet.dispatch.authorization_status, "not_reserved");
    assert!(packet.dispatch.reservation_candidate);
    assert_eq!(packet.workspace.binding_status, "not_allocated");
    assert!(packet.workspace.workspace_id.is_none());
    assert_eq!(packet.capabilities.evaluation_status, "not_evaluated");
    assert!(packet.capabilities.inventory_identity.is_none());
    assert!(!packet.packet_fingerprint.is_empty());

    assert_eq!(packet.subject.id, ticket_id);
    assert_eq!(packet.subject.role, "implementation");
    assert_eq!(packet.subject.status, "ready");
}

#[test]
fn two_packets_from_same_state_produce_identical_canonical_bytes() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = test_setup_ready_ticket(&repo);

    let store = JsonGraphStore::new(repo.path());
    let p1 = store.work_packet(&ticket_id).expect("first packet");
    let p2 = store.work_packet(&ticket_id).expect("second packet");

    let canonical1 = to_canonical_bytes(&p1).unwrap();
    let canonical2 = to_canonical_bytes(&p2).unwrap();
    assert_eq!(
        canonical1, canonical2,
        "two work_packet calls must produce identical canonical bytes"
    );
}

// -----------------------------------------------------------------------
// Test setup helper (minimal ready ticket for packet tests)
// -----------------------------------------------------------------------

use pulse::canonical_json::hash_bytes;
use pulse::evidence::model::*;
use pulse::evidence::record_receipt;
use pulse::graph::contract::{
    ContentRef, ContractItem, ContractScope, EffortMetadata, ImplementationContract,
    ImplementationMode, ImplementationSemanticImpact, PlanPolicy, QaImpactPosture, SurfaceRef,
    TicketRole, WorkSurface,
};
use pulse::graph::node::DocumentationImpactPosture;
use pulse::graph::node::NodeStatus;
use pulse::graph::store::{ContractSetRequest, DocumentationImpactUpdate, QaImpactUpdate};
use pulse::identity::ActorKind;
use pulse::policy::{AuthorityPolicy, AuthorityPrincipal};
use std::fs;

fn test_setup_ready_ticket(repo: &TestRepo) -> String {
    let root = repo.path();
    let store = JsonGraphStore::new(root);

    // Policy grants
    write_policy(
        root,
        &[
            "shape.apply",
            "shape.approve.R1",
            "qa.none.approve",
            "work.transition.shaped",
            "work.transition.ready",
            "work.node.create",
        ],
    );

    // Bootstrap graph
    let bootstrap = repo.pulse_ok(&["graph", "bootstrap", "--json"]);
    assert_eq!(bootstrap["code"], "bootstrapped");

    // Bootstrap evidence + docs manifests
    pulse::evidence::manifest::load(root).unwrap();
    pulse::docs::manifest::bootstrap(root).unwrap();

    // Create Ticket via CLI
    let created = repo.pulse_ok(&[
        "work",
        "create",
        "--kind",
        "ticket",
        "--title",
        "Implement fence-aware packet revalidation",
        "--role",
        "implementation",
        "--risk",
        "low",
        "--materialization",
        "R1",
        "--json",
    ]);
    let ticket_id = created["value"]["id"].as_str().unwrap().to_string();

    let node = store.show_node(&ticket_id).expect("ticket should exist");

    let brief_rel = format!("{}/ticket.md", node.content_dir);
    let brief_path = root.join(&brief_rel);
    fs::create_dir_all(brief_path.parent().unwrap()).unwrap();
    fs::write(
        &brief_path,
        b"# Ticket\nImplement fence-aware packet builder.",
    )
    .unwrap();
    let brief_hash = hash_bytes(&fs::read(&brief_path).unwrap());

    // Set implementation contract
    let contract = ImplementationContract {
        mode: ImplementationMode::Guided,
        work_surface: WorkSurface::Code,
        plan_policy: PlanPolicy::None,
        semantic_impact: ImplementationSemanticImpact::NoBehaviorOrPublicRiskChange,
        effort: EffortMetadata::default(),
        verification_profile: "service-change".to_string(),
        brief: Some(ContentRef {
            path: brief_rel,
            content_hash: brief_hash.clone(),
        }),
        objective: "Extract fence-aware packet builder.".to_string(),
        current_behavior: "No fence-aware builder.".to_string(),
        target_behavior: "Extracted builder available.".to_string(),
        code_anchors: vec![SurfaceRef::path("src/kernel/packet.rs")],
        documentation_anchors: vec![],
        configuration_anchors: vec![],
        data_anchors: vec![],
        research_refs: vec![],
        required_changes: vec![ContractItem {
            id: "CHG-1".to_string(),
            summary: "Extract phase builder.".to_string(),
        }],
        invariants: vec![ContractItem {
            id: "INV-1".to_string(),
            summary: "Public bytes unchanged.".to_string(),
        }],
        acceptance: vec![ContractItem {
            id: "AC-1".to_string(),
            summary: "Fence-aware builder works.".to_string(),
        }],
        scope: ContractScope::default(),
        implementation_freedom: vec![],
        required_decisions: vec![],
        shared_approach_refs: vec![],
        expected_evidence: vec![],
        expected_handoff: vec![],
    };
    store
        .set_contract_with_context(
            &ticket_id,
            node.revision,
            ContractSetRequest {
                role: TicketRole::Implementation,
                implementation: Some(contract),
                decision_work: None,
            },
            ctx(),
        )
        .unwrap();

    // Set QA impact
    let node = store.show_node(&ticket_id).unwrap();
    store
        .set_qa_impact_with_context(
            &ticket_id,
            node.revision,
            QaImpactUpdate {
                posture: QaImpactPosture::None,
                rationale: Some("No behavior change.".to_string()),
                behavioral_owner: None,
                affected_case_ids: vec![],
            },
            ctx(),
        )
        .unwrap();

    // Set docs impact
    let node = store.show_node(&ticket_id).unwrap();
    store
        .update_documentation_impact(
            &ticket_id,
            node.revision,
            DocumentationImpactUpdate {
                posture: DocumentationImpactPosture::None,
                rationale: Some("No docs change.".to_string()),
                required_documents: vec![],
                deferred_to: vec![],
                paths: vec![],
                domains: vec!["development".to_string()],
                labels: vec!["packet".to_string()],
            },
            "human:tester".to_string(),
        )
        .unwrap();

    // Record shaping receipt
    let node = store.show_node(&ticket_id).unwrap();
    let receipt = ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 1,
        id: format!("rcpt_{:0<26}", &ticket_id[3..]),
        kind: ReceiptKind::ShapingValidation,
        result: ReceiptResult::Passed,
        actor: ActorRef {
            kind: ActorKind::Human,
            id: "tester".to_string(),
        },
        recorded_at: chrono::Utc::now(),
        subject: SubjectRef {
            kind: "work".to_string(),
            id: ticket_id.clone(),
        },
        bindings: ReceiptBindings {
            work: vec![WorkBinding {
                id: ticket_id.clone(),
                revision: node.revision,
            }],
            source: None,
            content: vec![ContentBinding {
                path: format!("{}/ticket.md", node.content_dir),
                sha256: brief_hash.clone(),
            }],
            artifacts: vec![],
            graph_fingerprint_observed: None,
        },
        payload: ReceiptPayload::ShapingValidation(ShapingValidationPayload {
            payload_version: 1,
            owning_work: ShapingWorkBinding {
                id: ticket_id.clone(),
                revision_observed: node.revision,
                contract_revision: node.contract_revision,
            },
            materialization: "R1".to_string(),
            shape_mode: ShapeMode::FocusedBranches,
            source_posture: SourcePosture::NotRequiredContentBound,
            destination: Some(ShapingDestination {
                summary: "Deliver fence-aware packet".to_string(),
                scope_boundary: vec!["No dispatch".to_string()],
                exit_conditions: vec!["Packet coherent".to_string()],
            }),
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
                reference: "test".to_string(),
            },
            reconciliation: None,
            remaining_uncertainty: vec![],
        }),
    };
    let receipt_file = root.join("shaping.json");
    fs::write(&receipt_file, to_canonical_bytes(&receipt).unwrap()).unwrap();
    record_receipt(root, None, &receipt_file).unwrap();

    // Apply shaping and transition to Shaped
    let node = store.show_node(&ticket_id).unwrap();
    store
        .apply_shaping_with_context(&ticket_id, node.revision, &receipt.id, None, ctx())
        .unwrap();

    let node = store.show_node(&ticket_id).unwrap();
    store
        .transition_node_with_context(&ticket_id, NodeStatus::Shaped, node.revision, None, ctx())
        .unwrap();

    // Transition to Ready
    let node = store.show_node(&ticket_id).unwrap();
    store
        .transition_node_with_context(&ticket_id, NodeStatus::Ready, node.revision, None, ctx())
        .unwrap();

    // Commit so the worktree is clean for the packet source check.
    let add = std::process::Command::new("git")
        .current_dir(root)
        .args(["add", "."])
        .output()
        .expect("git add");
    assert!(add.status.success(), "git add failed");
    let commit = std::process::Command::new("git")
        .current_dir(root)
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
        .args([
            "-c",
            "user.name=Pulse Test",
            "-c",
            "user.email=pulse@example.test",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "setup ready ticket for I6",
        ])
        .output()
        .expect("git commit");
    assert!(
        commit.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&commit.stderr)
    );

    ticket_id
}

fn write_policy(root: &std::path::Path, grants: &[&str]) {
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
    let policy_path = root.join(".pulse/policy/authority.json");
    fs::create_dir_all(policy_path.parent().unwrap()).unwrap();
    fs::write(&policy_path, to_canonical_bytes(&policy).unwrap()).unwrap();
}
