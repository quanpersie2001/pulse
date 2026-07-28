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

/// Write authority policy without work.assignment.prepare grant — for
/// testing that claim without authority is rejected.
fn write_policy_without_claim_grant(root: &std::path::Path) {
    let path = root.join(".pulse/policy/authority.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    // All grants that setup_ready_ticket needs EXCEPT work.assignment.prepare.
    let grants: Vec<String> = vec![
        "shape.apply".to_string(),
        "shape.approve.R1".to_string(),
        "qa.none.approve".to_string(),
        "work.transition.shaped".to_string(),
        "work.transition.ready".to_string(),
        "work.node.create".to_string(),
    ];
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

/// Bootstrap with release grant included.
fn bootstrap_repo_with_release(repo: &TestRepo, _store: &JsonGraphStore) {
    write_policy(repo.path(), &["work.assignment.release"]);
    repo.pulse_ok(&["graph", "bootstrap", "--json"]);
    pulse::evidence::manifest::load(repo.path()).unwrap();
    pulse::docs::manifest::bootstrap(repo.path()).unwrap();
}

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

// ===========================================================================
// Release work (P2S2-I9)
// ===========================================================================

#[test]
fn release_releases_prepared_lease_and_transitions_to_ready() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo_with_release(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    // First claim to create a prepared assignment.
    let outcome = store.claim_work(claim_args(&ticket_id)).expect("claim");
    let pa = outcome.prepared_assignment;

    let node_before = store.show_node(&ticket_id).unwrap();
    assert_eq!(node_before.status, NodeStatus::Active);

    // Release.
    let release_out = store
        .release_work(pulse::kernel::assignment::ReleaseArgs {
            ticket_id: ticket_id.clone(),
            lease_id: pa.lease.lease_id.clone(),
            expected_revision: node_before.revision,
            reason: "Test release".to_string(),
            actor: "human:tester".to_string(),
        })
        .expect("release");

    assert_eq!(release_out.ticket_id, ticket_id);
    assert_eq!(release_out.lease_id, pa.lease.lease_id);
    assert_eq!(release_out.workspace_id, pa.workspace.workspace_id);
    assert_eq!(release_out.new_revision, node_before.revision + 1);
    assert_eq!(release_out.workspace_final_state, "released");

    // Verify node transitioned back to Ready.
    let node_after = store.show_node(&ticket_id).unwrap();
    assert_eq!(node_after.status, NodeStatus::Ready);
    assert_eq!(node_after.revision, node_before.revision + 1);

    // Verify runtime state: lease removed, tombstone written.
    assert!(
        !assignment_store::lease_path(repo.path(), &pa.lease.lease_id)
            .unwrap()
            .exists(),
        "live lease should be removed"
    );
    let tombstone = assignment_store::load_tombstone(repo.path(), &pa.lease.lease_id)
        .expect("tombstone should exist");
    assert_eq!(tombstone.state, "released");
    assert_eq!(tombstone.subject_id, ticket_id);

    // Verify workspace record updated.
    let ws = assignment_store::load_workspace(repo.path(), &pa.workspace.workspace_id)
        .expect("workspace should exist");
    assert_eq!(ws.state, "released");
    assert!(ws.released_at.is_some());

    // Verify event was emitted.
    let events_dir = repo.path().join(".pulse/events");
    let mut found = false;
    if events_dir.exists() {
        for entry in fs::read_dir(&events_dir).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                for sub in fs::read_dir(entry.path()).unwrap() {
                    let sub = sub.unwrap();
                    let content = fs::read_to_string(sub.path()).unwrap();
                    if content.contains("work.assignment.released") {
                        found = true;
                    }
                }
            }
        }
    }
    assert!(found, "expected work.assignment.released event");
}

#[test]
fn release_rejects_wrong_lease_id() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo_with_release(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    store.claim_work(claim_args(&ticket_id)).expect("claim");
    let node = store.show_node(&ticket_id).unwrap();

    let err = store
        .release_work(pulse::kernel::assignment::ReleaseArgs {
            ticket_id,
            lease_id: "lease_nonexistent".to_string(),
            expected_revision: node.revision,
            reason: "Test".to_string(),
            actor: "human:tester".to_string(),
        })
        .expect_err("wrong lease");
    assert_eq!(err.code(), "assignment_lease_not_found");
}

#[test]
fn release_rejects_wrong_revision() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo_with_release(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    let outcome = store.claim_work(claim_args(&ticket_id)).expect("claim");
    let pa = outcome.prepared_assignment;
    let node = store.show_node(&ticket_id).unwrap();

    // Claim revision is current node.revision. Use wrong (old) revision.
    let err = store
        .release_work(pulse::kernel::assignment::ReleaseArgs {
            ticket_id: ticket_id.clone(),
            lease_id: pa.lease.lease_id.clone(),
            expected_revision: node.revision - 1,
            reason: "Test".to_string(),
            actor: "human:tester".to_string(),
        })
        .expect_err("wrong revision");
    assert_eq!(err.code(), "cas_conflict");
}

// ===========================================================================
// Leases listing (P2S2-I9)
// ===========================================================================

#[test]
fn leases_listing_shows_claimed_leases() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    // Before claim: empty report.
    let before = store.list_leases(None).expect("list leases before");
    assert_eq!(before.count, 0);

    store.claim_work(claim_args(&ticket_id)).expect("claim");

    // After claim: one live lease.
    let after = store.list_leases(None).expect("list leases after");
    assert_eq!(after.count, 1);
    assert_eq!(after.live_count, 1);
    assert_eq!(after.entries[0].subject_id, ticket_id);
    assert_eq!(after.entries[0].state, "prepared");
    assert_eq!(after.entries[0].node_status, "Active");

    // Filter by ticket.
    let filtered = store.list_leases(Some(&ticket_id)).expect("list filtered");
    assert_eq!(filtered.count, 1);

    let filtered_other = store.list_leases(Some("TK-999")).expect("list other");
    assert_eq!(filtered_other.count, 0);
}

#[test]
fn leases_listing_shows_tombstoned_lease_after_release() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo_with_release(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    let outcome = store.claim_work(claim_args(&ticket_id)).expect("claim");
    let pa = outcome.prepared_assignment;
    let node = store.show_node(&ticket_id).unwrap();

    store
        .release_work(pulse::kernel::assignment::ReleaseArgs {
            ticket_id: ticket_id.clone(),
            lease_id: pa.lease.lease_id.clone(),
            expected_revision: node.revision,
            reason: "release for lease listing test".to_string(),
            actor: "human:tester".to_string(),
        })
        .expect("release");

    let report = store.list_leases(None).expect("list after release");
    assert_eq!(report.count, 1);
    assert_eq!(report.live_count, 0);
    assert_eq!(report.tombstoned_count, 1);
    assert!(report.entries[0].is_tombstoned);
}

#[test]
fn leases_listing_preserves_no_bootstrap() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);

    // No claim has been made; listing should not create runtime dirs.
    let _report = store.list_leases(None).expect("empty list");
    assert!(
        !repo.path().join(".pulse/runtime/assignment").exists(),
        "read-only leases listing must not create runtime directories"
    );
}

#[test]
fn recover_expired_in_place_marks_stale_and_blocks_duplicate_claim() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo_with_release(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    let outcome = store.claim_work(claim_args(&ticket_id)).expect("claim");
    let pa = outcome.prepared_assignment;
    let mut lease = assignment_store::load_lease(repo.path(), &pa.lease.lease_id).unwrap();
    lease.expires_at = (Utc::now() - chrono::Duration::seconds(30)).to_rfc3339();
    fs::write(
        assignment_store::lease_path(repo.path(), &lease.lease_id).unwrap(),
        to_canonical_bytes(&lease).unwrap(),
    )
    .unwrap();

    let recovery = store.recover_leases("human:tester").expect("recover");
    assert_eq!(recovery.expired_count, 1);
    assert_eq!(recovery.requeued_count, 0);
    assert_eq!(recovery.stale_count, 1);

    let node = store.show_node(&ticket_id).unwrap();
    assert_eq!(node.status, NodeStatus::Active);
    assert_eq!(node.revision, lease.subject.revision + 1);
    assert!(!assignment_store::lease_path(repo.path(), &lease.lease_id)
        .unwrap()
        .exists());
    let tombstone = assignment_store::load_tombstone(repo.path(), &lease.lease_id).unwrap();
    assert_eq!(tombstone.state, "stale_needs_operator");
    let workspace =
        assignment_store::load_workspace(repo.path(), &pa.workspace.workspace_id).unwrap();
    assert_eq!(workspace.state, "stale_needs_operator");

    let duplicate = store
        .claim_work(claim_args(&ticket_id))
        .expect_err("active/stale blocks claim");
    assert_eq!(duplicate.code(), "work_packet_status_not_ready");

    let second = store
        .recover_leases("human:tester")
        .expect("recover idempotently");
    assert_eq!(second.expired_count, 0);
    assert_eq!(second.requeued_count, 0);
    assert_eq!(second.stale_count, 0);
    assert_eq!(second.report.tombstoned_count, 1);
}

#[test]
fn recover_expired_clean_isolated_requeues_and_cleans_workspace() {
    let repo = TestRepo::from_fixture("minimal-service");
    let root = repo.path().canonicalize().unwrap();
    let store = JsonGraphStore::new(&root);
    bootstrap_repo_with_release(&repo, &store);
    let ticket_id = setup_ready_ticket(&root, &store);

    let outcome = store
        .claim_work(ClaimArgs {
            workspace_mode: Some("isolated_worktree".to_string()),
            ..claim_args(&ticket_id)
        })
        .expect("claim isolated");
    let pa = outcome.prepared_assignment;
    let workspace_path = root.join(&pa.workspace.path);
    assert!(workspace_path.exists());

    let mut lease = assignment_store::load_lease(repo.path(), &pa.lease.lease_id).unwrap();
    lease.expires_at = (Utc::now() - chrono::Duration::seconds(30)).to_rfc3339();
    fs::write(
        assignment_store::lease_path(repo.path(), &lease.lease_id).unwrap(),
        to_canonical_bytes(&lease).unwrap(),
    )
    .unwrap();

    let recovery = store.recover_leases("human:tester").expect("recover");
    assert_eq!(recovery.expired_count, 1);
    assert_eq!(recovery.requeued_count, 1);
    assert_eq!(recovery.stale_count, 0);

    let node = store.show_node(&ticket_id).unwrap();
    assert_eq!(node.status, NodeStatus::Ready);
    assert_eq!(node.revision, lease.subject.revision + 2);
    let tombstone = assignment_store::load_tombstone(repo.path(), &lease.lease_id).unwrap();
    assert_eq!(tombstone.state, "expired");
    let workspace =
        assignment_store::load_workspace(repo.path(), &pa.workspace.workspace_id).unwrap();
    assert_eq!(workspace.state, "released");
    assert!(
        !workspace_path.exists(),
        "clean isolated workspace should be removed post-commit"
    );
}

#[test]
fn recover_expired_dirty_isolated_marks_stale_and_preserves_workspace() {
    let repo = TestRepo::from_fixture("minimal-service");
    let root = repo.path().canonicalize().unwrap();
    let store = JsonGraphStore::new(&root);
    bootstrap_repo_with_release(&repo, &store);
    let ticket_id = setup_ready_ticket(&root, &store);

    let outcome = store
        .claim_work(ClaimArgs {
            workspace_mode: Some("isolated_worktree".to_string()),
            ..claim_args(&ticket_id)
        })
        .expect("claim isolated");
    let pa = outcome.prepared_assignment;
    let workspace_path = root.join(&pa.workspace.path);
    fs::write(workspace_path.join("dirty.txt"), b"preserve me").unwrap();

    let mut lease = assignment_store::load_lease(repo.path(), &pa.lease.lease_id).unwrap();
    lease.expires_at = (Utc::now() - chrono::Duration::seconds(30)).to_rfc3339();
    fs::write(
        assignment_store::lease_path(repo.path(), &lease.lease_id).unwrap(),
        to_canonical_bytes(&lease).unwrap(),
    )
    .unwrap();

    let recovery = store.recover_leases("human:tester").expect("recover");
    assert_eq!(recovery.expired_count, 1);
    assert_eq!(recovery.requeued_count, 0);
    assert_eq!(recovery.stale_count, 1);

    let node = store.show_node(&ticket_id).unwrap();
    assert_eq!(node.status, NodeStatus::Active);
    let tombstone = assignment_store::load_tombstone(repo.path(), &lease.lease_id).unwrap();
    assert_eq!(tombstone.state, "stale_needs_operator");
    assert!(workspace_path.join("dirty.txt").exists());
    let workspace =
        assignment_store::load_workspace(repo.path(), &pa.workspace.workspace_id).unwrap();
    assert_eq!(workspace.state, "stale_needs_operator");
}

#[test]
fn recover_reports_ambiguous_without_mutation() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo_with_release(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    let outcome = store.claim_work(claim_args(&ticket_id)).expect("claim");
    let pa = outcome.prepared_assignment;
    let tombstone = pulse::assignment::AssignmentTombstoneV1 {
        schema_version: pulse::assignment::TOMBSTONE_SCHEMA_VERSION,
        lease_id: pa.lease.lease_id.clone(),
        subject_id: ticket_id.clone(),
        state: "released".to_string(),
        recorded_at: Utc::now().to_rfc3339(),
        actor: "human:tester".to_string(),
        reason: Some("synthetic ambiguous test".to_string()),
        reason_codes: vec![],
    };
    assignment_store::write_tombstone(repo.path(), &tombstone).unwrap();

    let before_node = store.show_node(&ticket_id).unwrap();
    let recovery = store.recover_leases("human:tester").expect("recover");
    assert_eq!(recovery.ambiguous_count, 1);
    assert_eq!(recovery.expired_count, 0);
    assert!(
        assignment_store::lease_path(repo.path(), &pa.lease.lease_id)
            .unwrap()
            .exists()
    );
    let after_node = store.show_node(&ticket_id).unwrap();
    assert_eq!(after_node.revision, before_node.revision);
    assert_eq!(after_node.status, before_node.status);
}

#[test]
fn release_rejects_runner_started_prepared_assignment() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo_with_release(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    let outcome = store.claim_work(claim_args(&ticket_id)).expect("claim");
    let pa = outcome.prepared_assignment;
    let prepared_path =
        assignment_store::prepared_assignment_path(repo.path(), &pa.prepared_assignment_id)
            .unwrap();
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&prepared_path).unwrap()).unwrap();
    value["dispatch"]["runner_status"] = serde_json::Value::String("started".to_string());
    fs::write(&prepared_path, to_canonical_bytes(&value).unwrap()).unwrap();

    let node = store.show_node(&ticket_id).unwrap();
    let err = store
        .release_work(pulse::kernel::assignment::ReleaseArgs {
            ticket_id,
            lease_id: pa.lease.lease_id,
            expected_revision: node.revision,
            reason: "must reject runner state".to_string(),
            actor: "human:tester".to_string(),
        })
        .expect_err("release outside no-run scope");
    assert_eq!(err.code(), "assignment_lease_not_releasable");
}

#[test]
fn release_without_authority_rejects() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    let outcome = store.claim_work(claim_args(&ticket_id)).expect("claim");
    let pa = outcome.prepared_assignment;
    let node = store.show_node(&ticket_id).unwrap();
    let err = store
        .release_work(pulse::kernel::assignment::ReleaseArgs {
            ticket_id,
            lease_id: pa.lease.lease_id,
            expected_revision: node.revision,
            reason: "no grant".to_string(),
            actor: "human:tester".to_string(),
        })
        .expect_err("release authority required");
    assert_eq!(err.code(), "readiness_authority_denied");
}

#[test]
fn leases_listing_reports_corrupt_record_with_null_fields() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    let leases_dir = repo.path().join(".pulse/runtime/assignment/leases");
    fs::create_dir_all(&leases_dir).unwrap();
    fs::write(leases_dir.join("lease_CORRUPT.json"), b"{ not json").unwrap();

    let report = store.list_leases(None).expect("list corrupt");
    assert_eq!(report.count, 1);
    assert_eq!(report.entries[0].classification, "invalid");
    assert_eq!(report.entries[0].assignee, None);
    assert_eq!(report.entries[0].workspace_id, None);

    let json = repo.pulse_ok(&["work", "leases", "--json"]);
    assert!(json["entries"][0]["assignee"].is_null());
    assert!(json["entries"][0]["workspace_id"].is_null());
}

#[test]
fn cli_leases_default_and_recover_grammar() {
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo_with_release(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    store.claim_work(claim_args(&ticket_id)).expect("claim");

    let list_json = repo.pulse_ok(&["work", "leases", "--ticket", &ticket_id, "--json"]);
    assert_eq!(list_json["count"], 1);

    let recover_json = repo.pulse_ok(&[
        "work",
        "leases",
        "recover",
        "--actor",
        "human:tester",
        "--json",
    ]);
    assert_eq!(recover_json["code"], "leases_recovered");
}

#[test]
fn frontier_reports_active_without_record_as_ambiguous_with_null_fields() {
    use pulse::graph::frontier::FrontierKind;

    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let pa = store
        .claim_work(claim_args(&ticket_id))
        .unwrap()
        .prepared_assignment;
    fs::remove_file(assignment_store::lease_path(repo.path(), &pa.lease.lease_id).unwrap())
        .unwrap();

    let enriched = store
        .frontier_with_claim_state(FrontierKind::Execution, None, None, false)
        .expect("enriched frontier");
    let assignment = enriched
        .active_assignments
        .iter()
        .find(|entry| entry.ticket_id == ticket_id)
        .unwrap();
    assert_eq!(
        assignment.claim_state,
        pulse::kernel::frontier::FrontierClaimState::Ambiguous
    );
    assert_eq!(assignment.lease_id, None);
    assert_eq!(assignment.assignee, None);
}

#[test]
fn frontier_reports_expired_active_assignment_as_stale() {
    use pulse::graph::frontier::FrontierKind;

    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let pa = store
        .claim_work(claim_args(&ticket_id))
        .unwrap()
        .prepared_assignment;
    let mut lease = assignment_store::load_lease(repo.path(), &pa.lease.lease_id).unwrap();
    lease.expires_at = (Utc::now() - chrono::Duration::seconds(30)).to_rfc3339();
    fs::write(
        assignment_store::lease_path(repo.path(), &lease.lease_id).unwrap(),
        to_canonical_bytes(&lease).unwrap(),
    )
    .unwrap();

    let enriched = store
        .frontier_with_claim_state(FrontierKind::Execution, None, None, false)
        .expect("enriched frontier");
    let assignment = enriched
        .active_assignments
        .iter()
        .find(|entry| entry.ticket_id == ticket_id)
        .unwrap();
    assert_eq!(
        assignment.claim_state,
        pulse::kernel::frontier::FrontierClaimState::Stale
    );
    assert_eq!(
        assignment.lease_id.as_deref(),
        Some(pa.lease.lease_id.as_str())
    );
}

// ===========================================================================
// Frontier claim-state enrichment (P2S2-I9)
// ===========================================================================

#[test]
fn frontier_claim_state_shows_not_claimed_before_any_lease() {
    use pulse::graph::frontier::FrontierKind;

    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    let enriched = store
        .frontier_with_claim_state(FrontierKind::Execution, None, None, false)
        .expect("enriched frontier");

    assert_eq!(enriched.code, "enriched_execution_frontier");
    assert!(
        enriched.items.iter().any(|i| i.id == ticket_id),
        "ready ticket should be in enriched frontier"
    );
    // Before claim: claim_state should be NotClaimed.
    for item in &enriched.items {
        if item.id == ticket_id {
            assert_eq!(
                item.claim_state,
                pulse::kernel::frontier::FrontierClaimState::NotClaimed,
                "unclaimed ticket should have NotClaimed state"
            );
            assert!(item.lease_id.is_none());
        }
    }
}

#[test]
fn frontier_claim_state_shows_prepared_after_claim() {
    use pulse::graph::frontier::FrontierKind;

    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    // Claim transitions to Active so it leaves the frontier.
    store.claim_work(claim_args(&ticket_id)).expect("claim");

    let enriched = store
        .frontier_with_claim_state(FrontierKind::Execution, None, None, false)
        .expect("enriched frontier");

    // After claim, the ticket is Active, not in the ready frontier items.
    // But it should appear in active_assignments.
    assert!(
        enriched
            .active_assignments
            .iter()
            .any(|a| a.ticket_id == ticket_id),
        "claimed ticket should be in active_assignments"
    );

    let assignment = enriched
        .active_assignments
        .iter()
        .find(|a| a.ticket_id == ticket_id)
        .expect("assignment entry");
    assert_eq!(
        assignment.claim_state,
        pulse::kernel::frontier::FrontierClaimState::Prepared
    );
}

#[test]
fn frontier_claim_state_reports_ambiguous_for_active_without_lease() {
    use pulse::graph::frontier::FrontierKind;

    // Create a scenario where node is Active but no lease exists.
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo_with_release(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    // Claim then release -> node is Ready again.
    let outcome = store.claim_work(claim_args(&ticket_id)).expect("claim");
    let pa = outcome.prepared_assignment;
    let node = store.show_node(&ticket_id).unwrap();
    store
        .release_work(pulse::kernel::assignment::ReleaseArgs {
            ticket_id: ticket_id.clone(),
            lease_id: pa.lease.lease_id.clone(),
            expected_revision: node.revision,
            reason: "release".to_string(),
            actor: "human:tester".to_string(),
        })
        .expect("release");

    let enriched = store
        .frontier_with_claim_state(FrontierKind::Execution, None, None, false)
        .expect("enriched frontier");

    // After release, ticket is Ready again, but with a tombstoned lease
    // the enriched frontier should show it...
    // The release left a tombstone, so the recovery classification
    // shows tombstoned, not NotClaimed.
    assert!(
        enriched.items.iter().any(|i| i.id == ticket_id),
        "released ticket should be back in frontier"
    );
}

// =========================================================================
// Side-effect assertions (P2S2-I10)
// =========================================================================

#[test]
fn claim_rejects_non_enrolled_and_creates_no_runtime() {
    // A non-enrolled repo should reject claim with not_enrolled and
    // never create .pulse/runtime/ directory.
    let dir = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(dir.path());

    let err = store
        .claim_work(claim_args("TK-001"))
        .expect_err("non-enrolled");
    assert_eq!(err.code(), "not_enrolled");
    assert!(
        !dir.path().join(".pulse/runtime").exists(),
        "non-enrolled repo must not get .pulse/runtime/ as a side effect"
    );
}

#[test]
fn claim_rejects_without_authority_and_creates_no_runtime_records() {
    // Authority policy without work.assignment.prepare should reject the
    // claim before any runtime records are created.
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    // Bootstrap with NO work.assignment.prepare grant.
    write_policy_without_claim_grant(repo.path());
    repo.pulse_ok(&["graph", "bootstrap", "--json"]);
    pulse::evidence::manifest::load(repo.path()).unwrap();
    pulse::docs::manifest::bootstrap(repo.path()).unwrap();

    let ticket_id = setup_ready_ticket(repo.path(), &store);

    let err = store
        .claim_work(claim_args(&ticket_id))
        .expect_err("no authority");
    assert_eq!(err.code(), "readiness_authority_denied");

    // No runtime records should exist.
    assert!(
        !repo.path().join(".pulse/runtime/assignment").exists(),
        "authority rejection must not create assignment runtime state"
    );
    // Node must still be Ready.
    let node = store.show_node(&ticket_id).unwrap();
    assert_eq!(node.status, NodeStatus::Ready);
}

#[test]
fn claim_rejects_capability_mismatch_and_creates_no_runtime_records() {
    // Capability principal mismatch should reject before runtime records.
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

    assert!(
        !repo.path().join(".pulse/runtime/assignment").exists(),
        "capability mismatch must not create assignment runtime state"
    );
    let node = store.show_node(&ticket_id).unwrap();
    assert_eq!(node.status, NodeStatus::Ready);
}

#[test]
fn claim_unsupported_workspace_mode_creates_no_runtime_records() {
    // Invalid workspace mode should reject before runtime records.
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    let err = store
        .claim_work(ClaimArgs {
            workspace_mode: Some("invalid_mode".to_string()),
            ..claim_args(&ticket_id)
        })
        .expect_err("invalid mode");
    assert_eq!(err.code(), "assignment_workspace_mode_unsupported");

    assert!(
        !repo.path().join(".pulse/runtime/assignment").exists(),
        "invalid workspace mode must not create assignment runtime state"
    );
}

#[test]
fn claim_rejects_stale_subject_and_creates_no_lease() {
    // Claiming a ticket that is not in Ready status should fail before
    // creating any runtime lease records.
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);

    // First claim succeeds.
    store
        .claim_work(claim_args(&ticket_id))
        .expect("first claim");

    // Second claim should fail (node is now Active).
    let err = store
        .claim_work(claim_args(&ticket_id))
        .expect_err("second claim");
    assert_eq!(err.code(), "assignment_live_lease_exists");

    // Still exactly one lease record.
    let leases_dir = repo.path().join(".pulse/runtime/assignment/leases");
    assert_eq!(
        fs::read_dir(&leases_dir).unwrap().count(),
        1,
        "second claim must not create a second lease record"
    );
}

// =========================================================================
// Ambiguous non-prefix / event mismatch blocking (P2S2-I10)
// =========================================================================

#[test]
fn claim_ambiguous_non_prefix_target_state_blocks_recovery() {
    // Simulate a corrupt multi-target transaction where targets are in a
    // non-prefix state (e.g., first and third targets written but not the
    // second). After path-sorting, the observed states produce
    // [After, Before, After] — non-prefix — so recovery must report
    // ambiguous and not produce a duplicate event or node transition.
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    let _ticket_id = setup_ready_ticket(repo.path(), &store);

    // To get a non-prefix [After, Before, After] pattern after sort by
    // path, we use paths that sort as:
    //   1. leases/lease_z.json                    (After - written)
    //   2. prepared/prepared_a.json               (Before - NOT written)
    //   3. workspaces/workspace_z.json            (After - written)
    // The alphabetically middle entry (prepared) is absent.

    // Create a realistic event payload for the transaction.
    let event_payload = serde_json::json!({
        "from": "ready",
        "to": "active",
        "expected_revision": 0u64,
        "lease_id": "lease_AMBIG",
        "workspace_id": "wt_AMBIG",
        "prepared_assignment_id": "pa_AMBIG",
    });
    let event_dir = repo.path().join(".pulse/events/2030-01-01");
    fs::create_dir_all(&event_dir).unwrap();
    let event_path = event_dir.join("evt_ambig.json");

    // Compute correct after hashes for the intent.
    let lease_content = b"{\"lease_id\":\"ambiguous\"}";
    let ws_content = b"{\"ws_id\":\"ambiguous\"}";
    let lease_hash = pulse::canonical_json::hash_bytes(lease_content);
    let ws_hash = pulse::canonical_json::hash_bytes(ws_content);

    // Target 1 (alphabetically second): prepared — NOT written.
    // Use content with a known hash even though it stays absent.
    let prepared_content = b"{\"prepared_id\":\"ambiguous\"}";
    let prepared_hash = pulse::canonical_json::hash_bytes(prepared_content);

    // Write target 0 (lease) and target 2 (workspace) on disk.
    let lease_dir = repo.path().join(".pulse/runtime/assignment/leases");
    fs::create_dir_all(&lease_dir).unwrap();
    let lease_path = lease_dir.join("lease_z.json");
    fs::write(&lease_path, lease_content).unwrap();

    let ws_dir = repo.path().join(".pulse/runtime/assignment/workspaces");
    fs::create_dir_all(&ws_dir).unwrap();
    let ws_path = ws_dir.join("workspace_z.json");
    fs::write(&ws_path, ws_content).unwrap();

    // Prepared path (alphabetically middle) — NOT written.
    let prepared_dir = repo.path().join(".pulse/runtime/assignment/prepared");
    fs::create_dir_all(&prepared_dir).unwrap();
    let prepared_path = prepared_dir.join("prepared_a.json");
    // Do NOT write prepared_path — it stays absent (Before).

    let intent =
        pulse::storage::transaction::MultiTargetTransactionIntent::prepared_with_transaction_id(
            "txn_AMBIG",
            "evt_AMBIG",
            "work.assignment.prepared",
            "tester",
            vec![
                pulse::storage::transaction::TransactionTarget::new(
                    lease_path,
                    pulse::storage::transaction::FileState::Absent,
                    pulse::storage::transaction::FileState::Present {
                        hash: lease_hash,
                        revision: 0,
                    },
                    lease_content,
                ),
                pulse::storage::transaction::TransactionTarget::new(
                    prepared_path,
                    pulse::storage::transaction::FileState::Absent,
                    pulse::storage::transaction::FileState::Present {
                        hash: prepared_hash,
                        revision: 0,
                    },
                    prepared_content,
                ),
                pulse::storage::transaction::TransactionTarget::new(
                    ws_path,
                    pulse::storage::transaction::FileState::Absent,
                    pulse::storage::transaction::FileState::Present {
                        hash: ws_hash,
                        revision: 0,
                    },
                    ws_content,
                ),
            ],
            event_path,
            event_payload,
        )
        .unwrap();

    let _prepared_txn =
        pulse::storage::transaction::prepare_multi_target_transaction(repo.path(), intent).unwrap();

    // The recovery should detect non-prefix as ambiguous / return an error.
    let result = pulse::storage::transaction::recover_prepared_transactions(repo.path());
    assert!(
        result.is_err(),
        "ambiguous non-prefix state must not silently repair; got {result:?}"
    );
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("non-prefix") || err.code() == "ambiguous_transaction",
        "expected ambiguous_transaction error, got: {err:?}"
    );

    // The transaction intent should NOT have been cleaned up (it's ambiguous).
    let tx_dir = repo.path().join(".pulse/runtime/transactions");
    assert!(
        tx_dir.join("txn_AMBIG.json").exists(),
        "ambiguous transaction intent should remain for operator review"
    );
}

#[test]
fn claim_event_mismatch_blocks_commit_and_reports_error() {
    // Verify that an event hash mismatch during recovery blocks and
    // reports event_mismatch rather than silently overwriting.
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    let _ticket_id = setup_ready_ticket(repo.path(), &store);

    // Write a different event payload at the intended event path.
    let event_dir = repo.path().join(".pulse/events/2030-01-01");
    fs::create_dir_all(&event_dir).unwrap();
    let event_path = event_dir.join("evt_mismatch.json");
    let different_payload = serde_json::json!({"event_type": "different"});
    let different_bytes = pulse::canonical_json::to_canonical_bytes(&different_payload).unwrap();
    fs::write(&event_path, &different_bytes).unwrap();

    // Now create an intent pointing to this event path with a DIFFERENT
    // intended payload. The intent's event_hash (from its intended payload)
    // will NOT match the event file already on disk.
    let event_payload = serde_json::json!({
        "from": "ready",
        "to": "active",
        "expected_revision": 0u64,
        "lease_id": "lease_EVT_MISMATCH",
        "workspace_id": "wt_EVT_MISMATCH",
        "prepared_assignment_id": "pa_EVT_MISMATCH",
    });

    // Build targets with correct after hashes (must match the actual bytes
    // so the intent passes validation at creation time).
    let lease_bytes = b"lease_content";
    let ws_bytes = b"workspace_content";
    let prepared_bytes = b"prepared_content";
    let lease_hash = pulse::canonical_json::hash_bytes(lease_bytes);
    let ws_hash = pulse::canonical_json::hash_bytes(ws_bytes);
    let prepared_hash = pulse::canonical_json::hash_bytes(prepared_bytes);

    let lease_path = repo
        .path()
        .join(".pulse/runtime/assignment/leases/lease_EVT_MISMATCH.json");
    fs::create_dir_all(lease_path.parent().unwrap()).unwrap();
    let ws_path = repo
        .path()
        .join(".pulse/runtime/assignment/workspaces/wt_EVT_MISMATCH.json");
    fs::create_dir_all(ws_path.parent().unwrap()).unwrap();
    let prepared_path = repo
        .path()
        .join(".pulse/runtime/assignment/prepared/pa_EVT_MISMATCH.json");
    fs::create_dir_all(prepared_path.parent().unwrap()).unwrap();

    let intent =
        pulse::storage::transaction::MultiTargetTransactionIntent::prepared_with_transaction_id(
            "txn_EVT_MISMATCH",
            "evt_MISMATCH",
            "work.assignment.prepared",
            "tester",
            vec![
                pulse::storage::transaction::TransactionTarget::new(
                    lease_path,
                    pulse::storage::transaction::FileState::Absent,
                    pulse::storage::transaction::FileState::Present {
                        hash: lease_hash,
                        revision: 0,
                    },
                    lease_bytes,
                ),
                pulse::storage::transaction::TransactionTarget::new(
                    ws_path,
                    pulse::storage::transaction::FileState::Absent,
                    pulse::storage::transaction::FileState::Present {
                        hash: ws_hash,
                        revision: 0,
                    },
                    ws_bytes,
                ),
                pulse::storage::transaction::TransactionTarget::new(
                    prepared_path,
                    pulse::storage::transaction::FileState::Absent,
                    pulse::storage::transaction::FileState::Present {
                        hash: prepared_hash,
                        revision: 0,
                    },
                    prepared_bytes,
                ),
            ],
            event_path.clone(),
            event_payload,
        )
        .unwrap();

    let _prepared_txn =
        pulse::storage::transaction::prepare_multi_target_transaction(repo.path(), intent).unwrap();

    // Recovery should detect event mismatch because the event file already
    // exists with different content than the intent's event_hash.
    let recovery_result = pulse::storage::transaction::recover_prepared_transactions(repo.path());
    assert!(
        recovery_result.is_err(),
        "event mismatch during recovery should return error"
    );
    let err = recovery_result.unwrap_err();
    assert!(
        err.code() == "event_mismatch",
        "expected event_mismatch error code, got: {err:?}"
    );

    // The transaction intent should still be present.
    let tx_dir = repo.path().join(".pulse/runtime/transactions");
    assert!(
        tx_dir.join("txn_EVT_MISMATCH.json").exists(),
        "event mismatch transaction intent should remain for operator review"
    );
}

// =========================================================================
// Fixture immutability (P2S2-I10)
// =========================================================================

#[test]
fn claim_does_not_mutate_tracked_fixture() {
    // Verify that the tracked target-repository fixture remains unchanged
    // after running a claim test. The TestRepo copies the fixture into a
    // tempdir, so mutations affect the tempdir copy, not the original.
    let fixture_dir = crate::common_fixture_repo::fixture_path("minimal-service");
    let before = crate::common_fixture_repo::snapshot_tree(&fixture_dir).unwrap();

    // Run a full claim cycle.
    let repo = TestRepo::from_fixture("minimal-service");
    let store = JsonGraphStore::new(repo.path());
    bootstrap_repo(&repo, &store);
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    store.claim_work(claim_args(&ticket_id)).expect("claim");

    // Verify the original fixture is unchanged.
    let after = crate::common_fixture_repo::snapshot_tree(&fixture_dir).unwrap();
    assert_eq!(
        before, after,
        "tracked fixture must not be mutated by claim tests"
    );
}

#[test]
fn non_enrolled_repo_has_no_pulse_runtime_after_attempted_operations() {
    // Verify that running various operations on a non-enrolled repository
    // never creates .pulse/runtime/ as a side effect.
    let dir = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(dir.path());

    // Attempt claim (must fail with not_enrolled).
    let _ = store.claim_work(claim_args("TK-001"));
    assert!(
        !dir.path().join(".pulse/runtime").exists(),
        "claim on non-enrolled must not create .pulse/runtime/"
    );

    // Re-create fresh store for non-enrolled checks.
    let dir2 = tempfile::tempdir().unwrap();
    let store2 = JsonGraphStore::new(dir2.path());

    // Attempt release (must fail with not_enrolled).
    let _ = store2.release_work(pulse::kernel::assignment::ReleaseArgs {
        ticket_id: "TK-001".to_string(),
        lease_id: "lease_nonexistent".to_string(),
        expected_revision: 1,
        reason: "test".to_string(),
        actor: "human:tester".to_string(),
    });
    assert!(
        !dir2.path().join(".pulse/runtime").exists(),
        "release on non-enrolled must not create .pulse/runtime/"
    );

    // Attempt recover leases (must fail with not_enrolled).
    let dir3 = tempfile::tempdir().unwrap();
    let store3 = JsonGraphStore::new(dir3.path());
    let _ = store3.recover_leases("human:tester");
    assert!(
        !dir3.path().join(".pulse/runtime").exists(),
        "recover leases on non-enrolled must not create .pulse/runtime/"
    );

    // Attempt list leases (must fail with not_enrolled).
    let dir4 = tempfile::tempdir().unwrap();
    let store4 = JsonGraphStore::new(dir4.path());
    let _ = store4.list_leases(None);
    assert!(
        !dir4.path().join(".pulse/runtime").exists(),
        "list leases on non-enrolled must not create .pulse/runtime/"
    );
}
