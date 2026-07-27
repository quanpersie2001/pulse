//! S7-I3 contract/QA/shaping mutation API tests: CAS, authority, idempotency,
//! contract_revision semantics, transaction recovery and read-only behavior.
//!
//! These tests exercise the harness against temporary target repositories only.
//! They never point Pulse at this development repository. Pure contract model
//! validation lives in `shaping_contract.rs`; this file covers the store
//! mutation API and CLI wiring added by S7-I3.

use chrono::Utc;
use pulse::canonical_json::hash_bytes;
use pulse::evidence::model::*;
use pulse::graph::contract::{
    ContentRef, ContractItem, ContractScope, DecisionWorkContract, DecisionWorkProvenance,
    EffortMetadata, GapKind, ImplementationContract, ImplementationMode,
    ImplementationSemanticImpact, Materialization, PlanPolicy, QaImpactPosture, ResolutionTarget,
    ResolutionTargetKind, RevisionedWorkRef, Risk, SurfaceRef, TicketRole, WorkSurface,
};
use pulse::graph::store::{ContractSetRequest, OperationContext, QaImpactUpdate};
use pulse::id::WorkKind;
use pulse::policy::{AuthorityPolicy, AuthorityPrincipal};
use pulse::storage::transaction::TransactionFailpoint;
use pulse::JsonGraphStore;
use std::fs;
use std::process::Command;

fn git(repo: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn commit_all(repo: &std::path::Path) -> String {
    if !repo.join(".git").exists() {
        git(repo, &["init"]);
        git(repo, &["config", "user.email", "test@example.com"]);
        git(repo, &["config", "user.name", "Test User"]);
    }
    git(repo, &["add", "."]);
    git(repo, &["commit", "--allow-empty", "-m", "snapshot"]);
    git(repo, &["rev-parse", "HEAD"])
}

fn write_json(path: &std::path::Path, value: &impl serde::Serialize) {
    fs::write(
        path,
        pulse::canonical_json::to_canonical_bytes(value).unwrap(),
    )
    .unwrap();
}

fn write_policy(repo: &std::path::Path, grants: &[&str]) {
    let policy = AuthorityPolicy {
        schema_version: 1,
        revision: 1,
        principals: vec![AuthorityPrincipal {
            kind: ActorKind::Human,
            id: "tester".to_string(),
            grants: grants.iter().map(|g| g.to_string()).collect(),
        }],
    };
    let path = repo.join(".pulse/policy/authority.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_json(&path, &policy);
}

fn create_ticket(
    store: &JsonGraphStore,
    role: TicketRole,
    materialization: Materialization,
) -> pulse::graph::node::Node {
    let classification = pulse::graph::contract::PublicCreateClassification {
        role: Some(role),
        risk: Some(Risk::Low),
        materialization: Some(materialization),
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

fn make_implementation_contract(node: &pulse::graph::node::Node) -> ImplementationContract {
    let brief_path = format!("{}/ticket.md", node.content_dir);
    ImplementationContract {
        mode: ImplementationMode::Guided,
        work_surface: WorkSurface::Code,
        plan_policy: PlanPolicy::None,
        semantic_impact: ImplementationSemanticImpact::NoBehaviorOrPublicRiskChange,
        effort: EffortMetadata::default(),
        verification_profile: "service-change".to_string(),
        brief: Some(ContentRef {
            path: brief_path,
            content_hash: "sha256:".to_string() + &"a".repeat(64),
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

fn make_decision_work_contract(owner: &pulse::graph::node::Node) -> DecisionWorkContract {
    DecisionWorkContract {
        destination_owner: RevisionedWorkRef {
            id: owner.id.clone(),
            contract_revision: owner.contract_revision,
        },
        branch_id: "BR-COMPAT".to_string(),
        gap_kind: GapKind::TradeoffGap,
        question: "Must legacy clients retain the generic invalid-token path?".to_string(),
        expected_output: "A Decision with compatibility direction.".to_string(),
        expected_evidence: vec![],
        resolution_target: Some(ResolutionTarget {
            kind: ResolutionTargetKind::Decision,
            id: "DEC-001".to_string(),
        }),
        provenance: DecisionWorkProvenance {
            shaping_receipt: "rcpt_01J00000000000000000000010".to_string(),
            fog_id: None,
        },
    }
}

fn make_shaping_receipt(
    id: &str,
    node: &pulse::graph::node::Node,
    manifest: &pulse::evidence::manifest::EvidenceManifest,
    content_rel: &str,
    content_hash: String,
    source_commit: String,
    materialization: &str,
) -> ReceiptEnvelope {
    ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 1,
        id: id.to_string(),
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
            source: Some(SourceBinding {
                kind: "git_commit".to_string(),
                commit: source_commit,
                repository_id: manifest.repository_id.clone(),
            }),
            content: vec![ContentBinding {
                path: content_rel.to_string(),
                sha256: content_hash,
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
            materialization: materialization.to_string(),
            shape_mode: ShapeMode::FocusedBranches,
            source_posture: SourcePosture::CleanGitCommit,
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
    }
}

fn record_shaping(
    repo: &std::path::Path,
    node: &pulse::graph::node::Node,
    receipt_id: &str,
    materialization: &str,
) -> String {
    let manifest = pulse::evidence::bootstrap(repo).unwrap().manifest;
    let rel = format!("{}/ticket.md", node.content_dir);
    let path = repo.join(&rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"# Ticket\ncontract content").unwrap();
    let content_hash = hash_bytes(&fs::read(&path).unwrap());
    let source_commit = commit_all(repo);
    record_shaping_at(
        repo,
        node,
        receipt_id,
        materialization,
        &manifest,
        &rel,
        content_hash,
        source_commit,
    )
}

/// Record a shaping receipt binding an explicit source commit, without creating
/// a new commit. Lets multiple competing receipts share the same current source
/// state for CAS/idempotency tests.
#[allow(clippy::too_many_arguments)]
fn record_shaping_at(
    repo: &std::path::Path,
    node: &pulse::graph::node::Node,
    receipt_id: &str,
    materialization: &str,
    manifest: &pulse::evidence::manifest::EvidenceManifest,
    content_rel: &str,
    content_hash: String,
    source_commit: String,
) -> String {
    let receipt = make_shaping_receipt(
        receipt_id,
        node,
        manifest,
        content_rel,
        content_hash,
        source_commit,
        materialization,
    );
    let file = repo.join(format!("{receipt_id}.json"));
    write_json(&file, &receipt);
    pulse::evidence::record_receipt(repo, None, &file)
        .unwrap()
        .receipt_hash
}

fn ctx() -> OperationContext {
    OperationContext {
        actor: "human:tester".to_string(),
        now: Utc::now(),
    }
}

#[test]
fn contract_set_bumps_both_revisions_and_replaces_typed_contract() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = create_ticket(&store, TicketRole::Implementation, Materialization::R1);
    let expected = node.revision;

    let contract = make_implementation_contract(&node);
    let request = ContractSetRequest {
        role: TicketRole::Implementation,
        implementation: Some(contract),
        decision_work: None,
    };
    let out = store
        .set_contract_with_context(&node.id, expected, request, ctx())
        .unwrap();
    assert_eq!(out.status, pulse::graph::store::MutationStatus::Updated);
    assert_eq!(out.value.revision, expected + 1);
    assert_eq!(out.value.contract_revision, node.contract_revision + 1);
    assert!(out.value.implementation.is_some());
    assert!(out.value.decision_work.is_none());

    let shown = store.show_contract(&node.id).unwrap();
    assert_eq!(shown.role, Some(TicketRole::Implementation));
    assert!(shown.implementation.is_some());
}

#[test]
fn contract_set_rejects_role_mismatch_and_non_ticket() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = create_ticket(&store, TicketRole::Implementation, Materialization::R1);

    let request = ContractSetRequest {
        role: TicketRole::DecisionWork,
        implementation: None,
        decision_work: Some(make_decision_work_contract(&node)),
    };
    let err = store
        .set_contract_with_context(&node.id, node.revision, request, ctx())
        .unwrap_err();
    assert_eq!(err.code(), "work_role_invalid");

    let epic = store
        .create_node(WorkKind::Epic, "Epic".to_string())
        .unwrap()
        .value;
    let err = store
        .set_contract_with_context(
            &epic.id,
            epic.revision,
            ContractSetRequest {
                role: TicketRole::Implementation,
                implementation: Some(make_implementation_contract(&node)),
                decision_work: None,
            },
            ctx(),
        )
        .unwrap_err();
    assert_eq!(err.code(), "work_role_invalid");
}

#[test]
fn contract_set_cas_conflict_and_decision_work_contract() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = create_ticket(&store, TicketRole::DecisionWork, Materialization::R0);
    let owner = store
        .create_node(WorkKind::Story, "Owner".to_string())
        .unwrap()
        .value;

    let dw = DecisionWorkContract {
        destination_owner: RevisionedWorkRef {
            id: owner.id.clone(),
            contract_revision: owner.contract_revision,
        },
        ..make_decision_work_contract(&node)
    };
    let request = ContractSetRequest {
        role: TicketRole::DecisionWork,
        implementation: None,
        decision_work: Some(dw),
    };
    let out = store
        .set_contract_with_context(&node.id, node.revision, request.clone(), ctx())
        .unwrap();
    assert!(out.value.decision_work.is_some());

    // CAS conflict: stale expected revision.
    let err = store
        .set_contract_with_context(&node.id, node.revision, request, ctx())
        .unwrap_err();
    assert_eq!(err.code(), "cas_conflict");
}

#[test]
fn qa_impact_set_bumps_both_revisions_and_requires_authority_for_none() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = create_ticket(&store, TicketRole::Implementation, Materialization::R1);

    // Missing policy → default deny for authority-gated posture.
    let err = store
        .set_qa_impact_with_context(
            &node.id,
            node.revision,
            QaImpactUpdate {
                posture: QaImpactPosture::None,
                rationale: Some("Internal refactor.".to_string()),
                behavioral_owner: None,
                affected_case_ids: vec![],
            },
            ctx(),
        )
        .unwrap_err();
    assert_eq!(err.code(), "readiness_policy_missing");

    // Grant the qa.none.approve grant.
    write_policy(repo, &["qa.none.approve"]);
    let out = store
        .set_qa_impact_with_context(
            &node.id,
            node.revision,
            QaImpactUpdate {
                posture: QaImpactPosture::None,
                rationale: Some("Internal refactor.".to_string()),
                behavioral_owner: None,
                affected_case_ids: vec![],
            },
            ctx(),
        )
        .unwrap();
    assert_eq!(out.value.revision, node.revision + 1);
    assert_eq!(out.value.contract_revision, node.contract_revision + 1);
    let shown = store.show_qa_impact(&node.id).unwrap();
    assert_eq!(shown.qa.unwrap().impact.posture, QaImpactPosture::None);
}

#[test]
fn qa_impact_covered_by_story_close_requires_specific_grant() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = create_ticket(&store, TicketRole::Implementation, Materialization::R1);
    let story = store
        .create_node(WorkKind::Story, "Behavioral owner".to_string())
        .unwrap()
        .value;

    // Wrong grant → denied.
    write_policy(repo, &["qa.none.approve"]);
    let err = store
        .set_qa_impact_with_context(
            &node.id,
            node.revision,
            QaImpactUpdate {
                posture: QaImpactPosture::CoveredByStoryClose,
                rationale: Some("Targeted checkpoint later.".to_string()),
                behavioral_owner: Some(story.id.clone()),
                affected_case_ids: vec![],
            },
            ctx(),
        )
        .unwrap_err();
    assert_eq!(err.code(), "readiness_authority_denied");

    // Correct grant.
    write_policy(repo, &["qa.defer_to_story_close"]);
    let out = store
        .set_qa_impact_with_context(
            &node.id,
            node.revision,
            QaImpactUpdate {
                posture: QaImpactPosture::CoveredByStoryClose,
                rationale: Some("Targeted checkpoint later.".to_string()),
                behavioral_owner: Some(story.id),
                affected_case_ids: vec![],
            },
            ctx(),
        )
        .unwrap();
    assert_eq!(
        out.value.qa.as_ref().unwrap().impact.posture,
        QaImpactPosture::CoveredByStoryClose
    );
}

#[test]
fn shaping_apply_bumps_normal_revision_only_and_emits_event() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = create_ticket(&store, TicketRole::Implementation, Materialization::R1);
    write_policy(repo, &["shape.apply", "shape.approve.R1"]);

    let receipt_hash = record_shaping(repo, &node, "rcpt_01J00000000000000000000001", "R1");
    let expected = node.revision;
    let contract_revision_before = node.contract_revision;

    let out = store
        .apply_shaping_with_context(
            &node.id,
            expected,
            "rcpt_01J00000000000000000000001",
            None,
            ctx(),
        )
        .unwrap();
    assert_eq!(out.code, "applied");
    assert_eq!(out.value.revision, expected + 1);
    // Contract revision must NOT change on a pointer-only apply.
    assert_eq!(out.value.contract_revision, contract_revision_before);
    let shaping = out.value.shaping.as_ref().unwrap();
    assert_eq!(shaping.receipt.hash, receipt_hash);
    assert!(shaping.map.is_none());
}

#[test]
fn shaping_apply_idempotent_same_receipt_no_duplicate_event() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = create_ticket(&store, TicketRole::Implementation, Materialization::R1);
    write_policy(repo, &["shape.apply", "shape.approve.R1"]);
    record_shaping(repo, &node, "rcpt_01J00000000000000000000002", "R1");

    store
        .apply_shaping_with_context(
            &node.id,
            node.revision,
            "rcpt_01J00000000000000000000002",
            None,
            ctx(),
        )
        .unwrap();

    // Retry with a deliberately stale expected revision → unchanged.
    let retry = store
        .apply_shaping_with_context(
            &node.id,
            node.revision,
            "rcpt_01J00000000000000000000002",
            None,
            ctx(),
        )
        .unwrap();
    assert_eq!(retry.code, "unchanged");
    assert_eq!(retry.status, pulse::graph::store::MutationStatus::Unchanged);

    let events = repo.join(".pulse/events");
    let mut count = 0;
    for day in fs::read_dir(&events).unwrap() {
        let day = day.unwrap().path();
        if !day.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&day).unwrap() {
            let v: serde_json::Value =
                serde_json::from_slice(&fs::read(entry.unwrap().path()).unwrap()).unwrap();
            if v.get("event_type").and_then(|v| v.as_str()) == Some("work.shaping.applied") {
                count += 1;
            }
        }
    }
    assert_eq!(count, 1, "idempotent retry must not emit a duplicate event");
}

#[test]
fn shaping_apply_cas_conflict_and_expected_current_receipt_protection() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = create_ticket(&store, TicketRole::Implementation, Materialization::R1);
    write_policy(repo, &["shape.apply", "shape.approve.R1"]);

    // Record two competing receipts binding the same current source state so
    // neither is source-stale relative to the other.
    let manifest = pulse::evidence::bootstrap(repo).unwrap().manifest;
    let rel = format!("{}/ticket.md", node.content_dir);
    let path = repo.join(&rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"# Ticket\ncontract content").unwrap();
    let content_hash = hash_bytes(&fs::read(&path).unwrap());
    let source_commit = commit_all(repo);
    record_shaping_at(
        repo,
        &node,
        "rcpt_01J00000000000000000000003",
        "R1",
        &manifest,
        &rel,
        content_hash.clone(),
        source_commit.clone(),
    );
    record_shaping_at(
        repo,
        &node,
        "rcpt_01J00000000000000000000004",
        "R1",
        &manifest,
        &rel,
        content_hash,
        source_commit,
    );

    let first = store
        .apply_shaping_with_context(
            &node.id,
            node.revision,
            "rcpt_01J00000000000000000000003",
            None,
            ctx(),
        )
        .unwrap();
    let new_revision = first.value.revision;

    // Competing apply with stale expected revision → CAS conflict.
    let err = store
        .apply_shaping_with_context(
            &node.id,
            node.revision,
            "rcpt_01J00000000000000000000004",
            Some("rcpt_01J00000000000000000000003"),
            ctx(),
        )
        .unwrap_err();
    assert_eq!(err.code(), "cas_conflict");

    // Wrong expected-current-receipt → lost-reconciliation conflict.
    let err = store
        .apply_shaping_with_context(
            &node.id,
            new_revision,
            "rcpt_01J00000000000000000000004",
            Some("rcpt_01J000000000000000000099"),
            ctx(),
        )
        .unwrap_err();
    assert_eq!(err.code(), "shaping_expected_current_receipt_conflict");
}

#[test]
fn shaping_apply_authority_default_deny_and_grant_derivation() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = create_ticket(&store, TicketRole::Implementation, Materialization::R1);
    record_shaping(repo, &node, "rcpt_01J00000000000000000000005", "R1");

    // Missing policy → caller denied.
    let err = store
        .apply_shaping_with_context(
            &node.id,
            node.revision,
            "rcpt_01J00000000000000000000005",
            None,
            ctx(),
        )
        .unwrap_err();
    assert_eq!(err.code(), "readiness_policy_missing");

    // Caller has shape.apply but approver lacks shape.approve.R1 → denied.
    write_policy(repo, &["shape.apply", "shape.approve.R0"]);
    let err = store
        .apply_shaping_with_context(
            &node.id,
            node.revision,
            "rcpt_01J00000000000000000000005",
            None,
            ctx(),
        )
        .unwrap_err();
    assert_eq!(err.code(), "readiness_authority_denied");

    // Both grants present → success.
    write_policy(repo, &["shape.apply", "shape.approve.R1"]);
    store
        .apply_shaping_with_context(
            &node.id,
            node.revision,
            "rcpt_01J00000000000000000000005",
            None,
            ctx(),
        )
        .unwrap();
}

#[test]
fn shaping_apply_rejects_wrong_subject_and_stale_contract_revision() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = create_ticket(&store, TicketRole::Implementation, Materialization::R1);
    write_policy(repo, &["shape.apply", "shape.approve.R1"]);
    let other = create_ticket(&store, TicketRole::Implementation, Materialization::R1);
    record_shaping(repo, &other, "rcpt_01J00000000000000000000006", "R1");

    // Wrong subject: receipt is bound to `other`, applied to `node`.
    let err = store
        .apply_shaping_with_context(
            &node.id,
            node.revision,
            "rcpt_01J00000000000000000000006",
            None,
            ctx(),
        )
        .unwrap_err();
    assert_eq!(err.code(), "shaping_receipt_subject_mismatch");

    // Stale contract revision: bump the contract, then the old receipt is stale.
    record_shaping(repo, &node, "rcpt_01J00000000000000000000007", "R1");
    let contract = make_implementation_contract(&node);
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
        .unwrap();
    let refreshed = store.show_node(&node.id).unwrap();
    let err = store
        .apply_shaping_with_context(
            &node.id,
            refreshed.revision,
            "rcpt_01J00000000000000000000007",
            None,
            ctx(),
        )
        .unwrap_err();
    assert_eq!(err.code(), "shaping_receipt_stale");
}

#[test]
fn shaping_invalidate_clears_pointer_normal_revision_only() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let node = create_ticket(&store, TicketRole::Implementation, Materialization::R1);
    write_policy(
        repo,
        &["shape.apply", "shape.approve.R1", "shape.invalidate"],
    );
    record_shaping(repo, &node, "rcpt_01J00000000000000000000008", "R1");
    let applied = store
        .apply_shaping_with_context(
            &node.id,
            node.revision,
            "rcpt_01J00000000000000000000008",
            None,
            ctx(),
        )
        .unwrap();
    let contract_revision = applied.value.contract_revision;

    // Missing grant → denied.
    write_policy(repo, &["shape.apply", "shape.approve.R1"]);
    let err = store
        .invalidate_shaping_with_context(
            &node.id,
            applied.value.revision,
            "readiness changed".to_string(),
            ctx(),
        )
        .unwrap_err();
    assert_eq!(err.code(), "readiness_authority_denied");

    write_policy(repo, &["shape.invalidate"]);
    let out = store
        .invalidate_shaping_with_context(
            &node.id,
            applied.value.revision,
            "readiness changed".to_string(),
            ctx(),
        )
        .unwrap();
    assert_eq!(out.code, "invalidated");
    assert!(out.value.shaping.is_none());
    // Pointer-only mutation: contract revision unchanged.
    assert_eq!(out.value.contract_revision, contract_revision);
    assert_eq!(out.value.revision, applied.value.revision + 1);
}

#[test]
fn shaping_apply_crash_after_node_recovers_to_coherent_state() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::with_failpoint(repo, TransactionFailpoint::AfterCanonical);
    let node = create_ticket(
        &JsonGraphStore::new(repo),
        TicketRole::Implementation,
        Materialization::R1,
    );
    write_policy(repo, &["shape.apply", "shape.approve.R1"]);
    record_shaping(repo, &node, "rcpt_01J00000000000000000000009", "R1");

    let _ = store
        .apply_shaping_with_context(
            &node.id,
            node.revision,
            "rcpt_01J00000000000000000000009",
            None,
            ctx(),
        )
        .unwrap_err();

    // Recovery rolls the prepared transaction forward to a coherent state.
    JsonGraphStore::new(repo).recover().unwrap();
    let recovered = JsonGraphStore::new(repo).show_node(&node.id).unwrap();
    assert!(recovered.shaping.is_some());
    let events = repo.join(".pulse/events");
    let mut applied = 0;
    for day in fs::read_dir(&events).unwrap() {
        let day = day.unwrap().path();
        if !day.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&day).unwrap() {
            let v: serde_json::Value =
                serde_json::from_slice(&fs::read(entry.unwrap().path()).unwrap()).unwrap();
            if v.get("event_type").and_then(|v| v.as_str()) == Some("work.shaping.applied") {
                applied += 1;
            }
        }
    }
    assert_eq!(applied, 1, "recovery commits exactly one apply event");
}
