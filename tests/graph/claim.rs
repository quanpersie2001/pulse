//! Integration tests for the claim pipeline (P2S2-I8).
//!
//! Tests cover:
//! - Happy path: claim a ready Ticket with valid capability inventory
//! - Subject not found
//! - Live lease exists (duplicate claim)
//! - Capability principal mismatch / invalid inventory
//! - Workspace mode validation
//! - Assert node transitions to Active and runtime records are created
//! - Event emission

use std::fs;

use chrono::Utc;
use pulse::canonical_json::to_canonical_bytes;
use pulse::graph::contract::{
    ContentRef, ContractItem, ContractScope, EffortMetadata, ImplementationContract,
    ImplementationMode, ImplementationSemanticImpact, PlanPolicy, PublicCreateClassification,
    QaImpactPosture, SurfaceRef, TicketRole, WorkSurface,
};
use pulse::graph::node::{DocumentationImpactPosture, NodeStatus};
use pulse::graph::store::{
    ClaimArgs, ContractSetRequest, DocumentationImpactUpdate, OperationContext, QaImpactUpdate,
};
use pulse::id::WorkKind;
use pulse::kernel::assignment_store;
use pulse::JsonGraphStore;

use super::common_fixture_repo::TestRepo;
use super::common_git::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ctx() -> OperationContext {
    OperationContext {
        actor: "human:tester".to_string(),
        now: Utc::now(),
    }
}

fn valid_inventory_bytes(principal: &str) -> Vec<u8> {
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

fn write_policy(root: &std::path::Path, extra_grants: &[&str]) {
    let path = root.join(".pulse/policy/authority.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut grants: Vec<String> = vec![
        "shape.apply".to_string(),
        "shape.approve.R1".to_string(),
        "qa.none.approve".to_string(),
        "work.transition.shaped".to_string(),
        "work.transition.ready".to_string(),
        "work.assignment.prepare".to_string(),
        "work.node.create".to_string(),
    ];
    for g in extra_grants {
        grants.push(g.to_string());
    }
    grants.sort();
    grants.dedup();
    // Write the policy as canonical JSON using the struct so it matches
    // exactly what the policy loader expects after normalize().
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
    fs::write(&path, to_canonical_bytes(&policy).unwrap()).unwrap();
}

fn setup_ready_ticket(root: &std::path::Path, store: &JsonGraphStore) -> String {
    let node = store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Test claim ticket".to_string(),
            PublicCreateClassification {
                role: Some(TicketRole::Implementation),
                risk: Some(pulse::graph::contract::Risk::Low),
                materialization: Some(pulse::graph::contract::Materialization::R1),
            },
            ctx(),
        )
        .unwrap()
        .value;

    let ticket_id = node.id.clone();

    // Write brief.
    let brief_rel = format!("{}/ticket.md", node.content_dir);
    let brief_path = root.join(&brief_rel);
    fs::create_dir_all(brief_path.parent().unwrap()).unwrap();
    fs::write(&brief_path, b"# Ticket\nTest claim ticket brief.").unwrap();
    let brief_hash = pulse::canonical_json::hash_bytes(&fs::read(&brief_path).unwrap());

    // Set implementation contract.
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
                    semantic_impact: ImplementationSemanticImpact::NoBehaviorOrPublicRiskChange,
                    effort: EffortMetadata::default(),
                    verification_profile: "service-change".to_string(),
                    brief: Some(ContentRef {
                        path: brief_rel,
                        content_hash: brief_hash.clone(),
                    }),
                    objective: "Test claim objective.".to_string(),
                    current_behavior: "Current behavior.".to_string(),
                    target_behavior: "Target behavior.".to_string(),
                    code_anchors: vec![SurfaceRef::path("src/main.rs")],
                    documentation_anchors: vec![],
                    configuration_anchors: vec![],
                    data_anchors: vec![],
                    research_refs: vec![],
                    required_changes: vec![ContractItem {
                        id: "CHG-1".to_string(),
                        summary: "Make test claimable.".to_string(),
                    }],
                    invariants: vec![ContractItem {
                        id: "INV-1".to_string(),
                        summary: "Invariant holds.".to_string(),
                    }],
                    acceptance: vec![ContractItem {
                        id: "AC-1".to_string(),
                        summary: "Claim works.".to_string(),
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
            ctx(),
        )
        .unwrap();

    // QA impact.
    let node = store.show_node(&ticket_id).unwrap();
    store
        .set_qa_impact_with_context(
            &ticket_id,
            node.revision,
            QaImpactUpdate {
                posture: QaImpactPosture::None,
                rationale: Some("No QA needed.".to_string()),
                behavioral_owner: None,
                affected_case_ids: vec![],
            },
            ctx(),
        )
        .unwrap();

    // Docs impact.
    let node = store.show_node(&ticket_id).unwrap();
    store
        .update_documentation_impact(
            &ticket_id,
            node.revision,
            DocumentationImpactUpdate {
                posture: DocumentationImpactPosture::None,
                rationale: Some("No docs impact.".to_string()),
                required_documents: vec![],
                deferred_to: vec![],
                paths: vec![],
                domains: vec!["development".to_string()],
                labels: vec!["claim".to_string()],
            },
            "human:tester".to_string(),
        )
        .unwrap();

    // Shaping receipt.
    let node = store.show_node(&ticket_id).unwrap();
    let receipt = build_shaping_receipt(
        &ticket_id,
        node.revision,
        node.contract_revision,
        &node.content_dir,
        &brief_hash,
    );
    let receipt_file = root.join("shaping_claim.json");
    fs::write(&receipt_file, to_canonical_bytes(&receipt).unwrap()).unwrap();
    pulse::evidence::receipt::record_receipt(root, None, &receipt_file).unwrap();

    // Apply shaping.
    let node = store.show_node(&ticket_id).unwrap();
    store
        .apply_shaping_with_context(&ticket_id, node.revision, &receipt.id, None, ctx())
        .unwrap();

    // Transition to Shaped.
    let node = store.show_node(&ticket_id).unwrap();
    store
        .transition_node_with_context(&ticket_id, NodeStatus::Shaped, node.revision, None, ctx())
        .unwrap();

    // Transition to Ready.
    let node = store.show_node(&ticket_id).unwrap();
    store
        .transition_node_with_context(&ticket_id, NodeStatus::Ready, node.revision, None, ctx())
        .unwrap();

    commit_all(root);

    ticket_id
}

/// Minimal shaping receipt for test setups.
/// `content_dir` is the node's content_dir (e.g., "works/TK-001").
fn build_shaping_receipt(
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
        recorded_at: chrono::Utc::now(),
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
            content: vec![pulse::evidence::model::ContentBinding {
                path: format!("{}/ticket.md", content_dir),
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
                summary: "Test shaping".to_string(),
                scope_boundary: vec!["test".to_string()],
                exit_conditions: vec!["condition met".to_string()],
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

fn claim_args(ticket_id: &str) -> ClaimArgs {
    ClaimArgs {
        ticket_id: ticket_id.to_string(),
        actor: "agent:tester".to_string(),
        assignee: "agent:codex-local".to_string(),
        capability_inventory_bytes: valid_inventory_bytes("agent:codex-local"),
        ttl_seconds: 1800,
        workspace_mode: Some("in_place".to_string()),
    }
}

fn bootstrap_repo(repo: &TestRepo, _store: &JsonGraphStore) {
    write_policy(repo.path(), &[]);
    repo.pulse_ok(&["graph", "bootstrap", "--json"]);
    pulse::evidence::manifest::load(repo.path()).unwrap();
    pulse::docs::manifest::bootstrap(repo.path()).unwrap();
}

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

#[test]
fn claim_ready_ticket_succeeds_and_returns_prepared_assignment() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    let outcome = store
        .claim_work(claim_args(&ticket_id))
        .expect("claim should succeed");
    let pa = outcome.prepared_assignment;

    assert_eq!(pa.schema_version, 1);
    assert_eq!(pa.profile, "phase2_prepared_assignment_v1");
    assert_eq!(pa.code, "prepared_assignment");
    assert!(pa.prepared_assignment_id.starts_with("pa_"));
    assert!(pa.prepared_assignment_fingerprint.starts_with("sha256:"));
    assert!(pa.lease.lease_id.starts_with("lease_"));
    assert!(pa.workspace.workspace_id.starts_with("wt_"));
    assert!(pa.lifecycle.event_id.starts_with("evt_"));

    assert!(pa.dispatch.dispatch_authorized);
    assert_eq!(pa.dispatch.authorization_status, "prepared_assignment");

    assert_eq!(pa.subject.id, ticket_id);
    assert_eq!(pa.subject.status_before, "ready");
    assert_eq!(pa.subject.status_after, "active");

    assert_eq!(pa.capability_match.status, "matched");
    assert!(pa.capability_match.missing.is_empty());

    assert_eq!(pa.lifecycle.transition, "ready_to_active");
    assert_eq!(pa.lifecycle.gate_profile, "phase2_prepared_assignment_v1");
    assert_eq!(pa.lifecycle.gate_status, "passed");

    assert_eq!(pa.lease.assignee, "agent:codex-local");
    assert_eq!(pa.lease.state, "prepared");
    assert!(pa.lease.exclusive);

    assert_eq!(pa.workspace.binding_status, "bound");
    assert_eq!(pa.workspace.mode, "in_place");

    assert_eq!(pa.packet.code, "reservation_candidate");
    assert!(!pa.packet.dispatch.dispatch_authorized);

    assert!(pa.transaction.transaction_id.starts_with("txn_"));
    assert_eq!(pa.transaction.committed_targets.len(), 4);

    let computed = pa.compute_fingerprint().expect("recompute fingerprint");
    assert_eq!(pa.prepared_assignment_fingerprint, computed);
}

#[test]
fn claim_transitions_node_to_active() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let node_before = store.show_node(&ticket_id).unwrap();

    store.claim_work(claim_args(&ticket_id)).expect("claim");

    let reloaded = store.show_node(&ticket_id).unwrap();
    assert_eq!(reloaded.status, NodeStatus::Active);
    assert_eq!(reloaded.revision, node_before.revision + 1);
}

#[test]
fn claim_creates_runtime_records() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    let outcome = store.claim_work(claim_args(&ticket_id)).expect("claim");
    let pa = outcome.prepared_assignment;
    let root = repo.path();

    let lease_path = assignment_store::lease_path(root, &pa.lease.lease_id).unwrap();
    assert!(lease_path.exists(), "lease record should exist");
    let loaded_lease = assignment_store::load_lease(root, &pa.lease.lease_id).unwrap();
    assert_eq!(loaded_lease.state, "prepared");
    assert_eq!(loaded_lease.assignee.principal, "agent:codex-local");
    assert_eq!(loaded_lease.subject.id, ticket_id);

    let ws_path = assignment_store::workspace_path(root, &pa.workspace.workspace_id).unwrap();
    assert!(ws_path.exists(), "workspace record should exist");
    let loaded_ws = assignment_store::load_workspace(root, &pa.workspace.workspace_id).unwrap();
    assert_eq!(loaded_ws.state, "bound");
    assert_eq!(loaded_ws.mode, "in_place");

    let pa_path =
        assignment_store::prepared_assignment_path(root, &pa.prepared_assignment_id).unwrap();
    assert!(pa_path.exists(), "prepared record should exist");
    let loaded_pa = assignment_store::load_prepared(root, &pa.prepared_assignment_id).unwrap();
    assert_eq!(loaded_pa.code, "prepared_assignment");
    assert!(loaded_pa.dispatch.dispatch_authorized);
    assert_eq!(loaded_pa.transaction, pa.transaction);
    assert_eq!(
        loaded_pa.prepared_assignment_fingerprint,
        pa.prepared_assignment_fingerprint
    );
    assert_eq!(
        loaded_pa.compute_fingerprint().unwrap(),
        pa.prepared_assignment_fingerprint,
        "committed prepared record bytes must be fingerprint-lossless with response"
    );
}

// ---------------------------------------------------------------------------
// Failure modes
// ---------------------------------------------------------------------------

#[test]
fn claim_rejects_subject_not_found() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);

    let err = store
        .claim_work(claim_args("TK-999"))
        .expect_err("missing subject");
    assert!(
        err.code() == "not_found" || err.code().contains("work_packet"),
        "expected not_found or work_packet error, got {}",
        err.code()
    );
}

#[test]
fn claim_rejects_live_lease() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    store
        .claim_work(claim_args(&ticket_id))
        .expect("first claim");
    let err = store
        .claim_work(claim_args(&ticket_id))
        .expect_err("duplicate claim");
    assert_eq!(err.code(), "assignment_live_lease_exists");
}

#[test]
fn claim_rejects_capability_principal_mismatch() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    let err = store
        .claim_work(ClaimArgs {
            capability_inventory_bytes: valid_inventory_bytes("agent:wrong-person"),
            assignee: "agent:codex-local".to_string(),
            ..claim_args(&ticket_id)
        })
        .expect_err("principal mismatch");
    assert_eq!(err.code(), "assignment_capability_principal_mismatch");
}

#[test]
fn claim_rejects_invalid_capability_inventory() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    let err = store
        .claim_work(ClaimArgs {
            capability_inventory_bytes: b"not valid json".to_vec(),
            ..claim_args(&ticket_id)
        })
        .expect_err("invalid inventory");
    assert_eq!(err.code(), "assignment_capability_inventory_invalid");
}

#[test]
fn claim_rejects_unsupported_workspace_mode() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    let err = store
        .claim_work(ClaimArgs {
            workspace_mode: Some("invalid_mode".to_string()),
            ..claim_args(&ticket_id)
        })
        .expect_err("unsupported mode");
    assert_eq!(err.code(), "assignment_workspace_mode_unsupported");
}

#[test]
fn claim_rejects_in_place_when_isolated_required() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);

    // Medium risk -> isolated_worktree_required.
    let node = store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Med risk".to_string(),
            PublicCreateClassification {
                role: Some(TicketRole::Implementation),
                risk: Some(pulse::graph::contract::Risk::Medium),
                materialization: Some(pulse::graph::contract::Materialization::R1),
            },
            ctx(),
        )
        .unwrap()
        .value;

    let tid = node.id;
    let brief_rel = format!("{}/ticket.md", node.content_dir);
    let brief_path = repo.path().join(&brief_rel);
    fs::create_dir_all(brief_path.parent().unwrap()).unwrap();
    fs::write(&brief_path, b"# Medium").unwrap();

    store
        .set_contract_with_context(
            &tid,
            node.revision,
            ContractSetRequest {
                role: TicketRole::Implementation,
                implementation: Some(ImplementationContract {
                    mode: ImplementationMode::Guided,
                    work_surface: WorkSurface::Code,
                    plan_policy: PlanPolicy::None,
                    semantic_impact: ImplementationSemanticImpact::NoBehaviorOrPublicRiskChange,
                    effort: EffortMetadata::default(),
                    verification_profile: "service-change".to_string(),
                    brief: Some(ContentRef {
                        path: brief_rel,
                        content_hash: pulse::canonical_json::hash_bytes(
                            &fs::read(&brief_path).unwrap(),
                        ),
                    }),
                    objective: "Test".to_string(),
                    current_behavior: "Cur".to_string(),
                    target_behavior: "Tgt".to_string(),
                    code_anchors: vec![SurfaceRef::path("src/main.rs")],
                    documentation_anchors: vec![],
                    configuration_anchors: vec![],
                    data_anchors: vec![],
                    research_refs: vec![],
                    required_changes: vec![ContractItem {
                        id: "CHG-1".to_string(),
                        summary: "Make change.".to_string(),
                    }],
                    invariants: vec![ContractItem {
                        id: "INV-1".to_string(),
                        summary: "Invariant holds.".to_string(),
                    }],
                    acceptance: vec![ContractItem {
                        id: "AC-1".to_string(),
                        summary: "Acceptance.".to_string(),
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
            ctx(),
        )
        .unwrap();

    let node = store.show_node(&tid).unwrap();
    store
        .set_qa_impact_with_context(
            &tid,
            node.revision,
            QaImpactUpdate {
                posture: QaImpactPosture::None,
                rationale: Some("none".to_string()),
                behavioral_owner: None,
                affected_case_ids: vec![],
            },
            ctx(),
        )
        .unwrap();

    let node = store.show_node(&tid).unwrap();
    store
        .update_documentation_impact(
            &tid,
            node.revision,
            DocumentationImpactUpdate {
                posture: DocumentationImpactPosture::None,
                rationale: Some("none".to_string()),
                required_documents: vec![],
                deferred_to: vec![],
                paths: vec![],
                domains: vec!["development".to_string()],
                labels: vec!["claim".to_string()],
            },
            "human:tester".to_string(),
        )
        .unwrap();

    let node = store.show_node(&tid).unwrap();
    let receipt = build_shaping_receipt(
        &tid,
        node.revision,
        node.contract_revision,
        &node.content_dir,
        &pulse::canonical_json::hash_bytes(&fs::read(&brief_path).unwrap()),
    );
    let rpath = repo.path().join("shaping_med.json");
    fs::write(&rpath, to_canonical_bytes(&receipt).unwrap()).unwrap();
    pulse::evidence::receipt::record_receipt(repo.path(), None, &rpath).unwrap();

    let node = store.show_node(&tid).unwrap();
    store
        .apply_shaping_with_context(&tid, node.revision, &receipt.id, None, ctx())
        .unwrap();

    let node = store.show_node(&tid).unwrap();
    store
        .transition_node_with_context(&tid, NodeStatus::Shaped, node.revision, None, ctx())
        .unwrap();
    let node = store.show_node(&tid).unwrap();
    store
        .transition_node_with_context(&tid, NodeStatus::Ready, node.revision, None, ctx())
        .unwrap();

    commit_all(repo.path());

    let err = store
        .claim_work(ClaimArgs {
            workspace_mode: Some("in_place".to_string()),
            ..claim_args(&tid)
        })
        .expect_err("in_place on medium risk");
    assert_eq!(err.code(), "assignment_workspace_worktree_required");
}

#[test]
fn claim_rejects_when_not_enrolled() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(dir.path());

    let err = store
        .claim_work(claim_args("TK-001"))
        .expect_err("non-enrolled");
    assert_eq!(err.code(), "not_enrolled");
}

// ---------------------------------------------------------------------------
// Event emission
// ---------------------------------------------------------------------------

#[test]
fn claim_creates_exactly_one_assignment_event() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    store.claim_work(claim_args(&ticket_id)).expect("claim");

    let events_dir = repo.path().join(".pulse/events");
    let mut count = 0usize;
    if events_dir.exists() {
        for entry in fs::read_dir(&events_dir).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                for sub in fs::read_dir(entry.path()).unwrap() {
                    let sub = sub.unwrap();
                    let content = fs::read_to_string(sub.path()).unwrap();
                    if content.contains("work.assignment.prepared") {
                        count += 1;
                    }
                }
            }
        }
    }
    assert_eq!(
        count, 1,
        "expected exactly one work.assignment.prepared event"
    );
}

// ---------------------------------------------------------------------------
// TTL validation
// ---------------------------------------------------------------------------

#[test]
fn claim_rejects_ttl_outside_valid_range() {
    for ttl_seconds in [5, 86_401] {
        let repo = TestRepo::from_fixture("minimal-service");
        let store = JsonGraphStore::new(repo.path());
        bootstrap_repo(&repo, &store);
        let ticket_id = setup_ready_ticket(repo.path(), &store);

        let err = store
            .claim_work(ClaimArgs {
                ttl_seconds,
                ticket_id: ticket_id.clone(),
                ..claim_args(&ticket_id)
            })
            .expect_err("TTL outside bounds rejects");
        assert_eq!(err.code(), "assignment_ttl_out_of_range");
        assert!(
            !repo.path().join(".pulse/runtime/assignment").exists(),
            "TTL rejection must not create assignment runtime state"
        );
    }
}
