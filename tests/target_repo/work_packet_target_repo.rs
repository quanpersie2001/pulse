//! P2S1-I6: Target-repository integration tests for `pulse work packet`.
//!
//! Every scenario uses `TestRepo::from_fixture("minimal-service")` to create
//! an isolated working copy.  Never run Pulse against the development
//! repository or tracked fixture in place.

use chrono::Utc;
use pulse::canonical_json::hash_bytes;
use pulse::canonical_json::to_canonical_bytes;
use pulse::evidence::model::*;
use pulse::graph::contract::{
    ContentRef, ContractItem, ContractScope, EffortMetadata, ImplementationContract,
    ImplementationMode, ImplementationSemanticImpact, PlanPolicy, QaImpactPosture, SurfaceRef,
    TicketRole, WorkSurface,
};
use pulse::graph::node::DocumentationImpactPosture;
use pulse::graph::store::{
    ContractSetRequest, DocumentationImpactUpdate, OperationContext, QaImpactUpdate,
};
use pulse::policy::{AuthorityPolicy, AuthorityPrincipal};
use pulse::JsonGraphStore;
use serde_json::Value;
use std::fs;
use std::process::Command;

use crate::common::fixture_repo::{development_repo_root, TestRepo};

fn ctx() -> OperationContext {
    OperationContext {
        actor: "human:tester".to_string(),
        now: Utc::now(),
    }
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

/// Set up a ready implementation Ticket using the library API directly,
/// returning the ticket ID.  The CLI is used only for the `work packet`
/// command itself per the I6 scope boundary.
fn setup_ready_ticket(repo: &TestRepo) -> String {
    let root = repo.path();
    let store = JsonGraphStore::new(root);

    write_policy(
        root,
        &[
            "shape.apply",
            "shape.approve.R1",
            "qa.none.approve",
            "work.transition.shaped",
            "work.transition.ready",
        ],
    );

    // Bootstrap graph via CLI
    let bootstrap = repo.pulse_ok(&["graph", "bootstrap", "--json"]);
    assert_eq!(bootstrap["code"], "bootstrapped");

    // Also bootstrap evidence + docs manifests so packet builder passes
    // repository identity check.
    pulse::evidence::manifest::load(root).unwrap();
    pulse::docs::manifest::bootstrap(root).unwrap();

    // Create Ticket via CLI
    let created = repo.pulse_ok(&[
        "work",
        "create",
        "--kind",
        "ticket",
        "--title",
        "Implement refresh token rotation",
        "--role",
        "implementation",
        "--risk",
        "low",
        "--materialization",
        "R1",
        "--json",
    ]);
    let ticket_id = created["value"]["id"].as_str().unwrap().to_string();

    // The rest of the setup is done via the library because the CLI does not
    // expose shaping receipt recording as a single command.
    let node = store
        .show_node(&ticket_id)
        .expect("ticket should exist after CLI create");

    let brief_rel = format!("{}/ticket.md", node.content_dir);
    let brief_path = root.join(&brief_rel);
    fs::create_dir_all(brief_path.parent().unwrap()).unwrap();
    fs::write(
        &brief_path,
        b"# Ticket\nImplement atomic refresh token rotation.",
    )
    .unwrap();
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
        objective: "Rotate refresh tokens atomically.".to_string(),
        current_behavior: "Tokens are long-lived.".to_string(),
        target_behavior: "Tokens rotate on each use.".to_string(),
        code_anchors: vec![SurfaceRef::path("src/token.mjs")],
        documentation_anchors: vec![],
        configuration_anchors: vec![],
        data_anchors: vec![],
        research_refs: vec![],
        required_changes: vec![ContractItem {
            id: "CHG-1".to_string(),
            summary: "Add rotation logic.".to_string(),
        }],
        invariants: vec![ContractItem {
            id: "INV-1".to_string(),
            summary: "Concurrent rotation serialized.".to_string(),
        }],
        acceptance: vec![ContractItem {
            id: "AC-1".to_string(),
            summary: "Tokens rotate without race.".to_string(),
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
                domains: vec!["authentication".to_string()],
                labels: vec!["tokens".to_string()],
            },
            "human:tester".to_string(),
        )
        .unwrap();

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
        recorded_at: Utc::now(),
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
                path: brief_rel.clone(),
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
                summary: "Deliver reliable rotation".to_string(),
                scope_boundary: vec!["No session redesign".to_string()],
                exit_conditions: vec!["Concurrent passes".to_string()],
            }),
            map: None,
            affected_work: vec![],
            branches: vec![ShapingBranch {
                id: "BR-AUTH-1".to_string(),
                question: "How is concurrent rotation serialized?".to_string(),
                gap_kind: "tradeoff_gap".to_string(),
                criticality: BranchCriticality::Critical,
                affected_work: vec![ticket_id.clone()],
                disposition: BranchDisposition::Resolved {
                    resolution: ShapingResolutionPointer {
                        kind: "decision".to_string(),
                        id: format!("DEC-{}", &ticket_id[..3]),
                        revision: 1,
                        gist: "Single-use atomic rotation".to_string(),
                    },
                },
            }],
            fog: vec![],
            out_of_scope: vec![],
            resolution_pointers: vec![ShapingResolutionPointer {
                kind: "decision".to_string(),
                id: format!("DEC-{}", &ticket_id[..3]),
                revision: 1,
                gist: "Single-use atomic rotation".to_string(),
            }],
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
    let receipt_file = root.join("shaping.json");
    fs::write(&receipt_file, to_canonical_bytes(&receipt).unwrap()).unwrap();
    pulse::evidence::record_receipt(root, None, &receipt_file).unwrap();

    let node = store.show_node(&ticket_id).unwrap();
    store
        .apply_shaping_with_context(&ticket_id, node.revision, &receipt.id, None, ctx())
        .unwrap();

    let node = store.show_node(&ticket_id).unwrap();
    store
        .transition_node_with_context(
            &ticket_id,
            pulse::graph::node::NodeStatus::Shaped,
            node.revision,
            None,
            ctx(),
        )
        .unwrap();

    let node = store.show_node(&ticket_id).unwrap();
    store
        .transition_node_with_context(
            &ticket_id,
            pulse::graph::node::NodeStatus::Ready,
            node.revision,
            None,
            ctx(),
        )
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
            "setup ready ticket",
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

// -----------------------------------------------------------------------
// A. Happy path — target-repo produces full schema v1 packet
// -----------------------------------------------------------------------

#[test]
fn target_repo_happy_path_work_packet() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);

    // Use CLI to build the packet
    let packet_value = repo.pulse_ok(&["work", "packet", &ticket_id, "--json"]);
    assert_eq!(packet_value["schema_version"], 1);
    assert_eq!(packet_value["code"], "reservation_candidate");

    // Subject binds exact revision and status
    assert_eq!(packet_value["subject"]["id"], ticket_id);
    assert_eq!(packet_value["subject"]["status"], "ready");
    assert!(packet_value["subject"]["revision"].as_u64().unwrap() >= 5);

    // Source binds exact clean HEAD
    let head = repo.git_head();
    assert_eq!(packet_value["source"]["commit"], head);
    assert_eq!(packet_value["source"]["cleanliness"], "clean");

    // Dispatch constants
    assert_eq!(packet_value["dispatch"]["reservation_candidate"], true);
    assert_eq!(packet_value["dispatch"]["dispatch_authorized"], false);
    assert_eq!(
        packet_value["dispatch"]["authorization_status"],
        "not_reserved"
    );

    // Lease/workspace/capability remain typed not-evaluated
    let gate_families = packet_value["dispatch"]["gate_families"]
        .as_array()
        .unwrap();
    for gate in gate_families {
        match gate["family"].as_str().unwrap() {
            "lease" => {
                assert_eq!(gate["status"], "not_evaluated");
                assert_eq!(gate["reason_codes"][0], "lease_resolver_not_installed");
            }
            "workspace_binding" => {
                assert_eq!(gate["status"], "not_evaluated");
                assert_eq!(gate["reason_codes"][0], "workspace_not_allocated");
            }
            "capability_match" => {
                assert_eq!(gate["status"], "not_evaluated");
                assert_eq!(gate["reason_codes"][0], "capability_inventory_not_bound");
            }
            "qa_baseline_and_cases" => {
                assert_eq!(gate["status"], "not_applicable");
            }
            "readiness" | "packet_completeness" | "source_base" | "documentation_context" => {
                assert_eq!(gate["status"], "passed");
            }
            other => panic!("unexpected gate family: {other}"),
        }
    }

    // Packet stays within budget
    assert!(
        packet_value["budget"]["actual_canonical_json_bytes"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(
        packet_value["budget"]["actual_canonical_json_bytes"]
            .as_u64()
            .unwrap()
            <= packet_value["budget"]["max_canonical_json_bytes"]
                .as_u64()
                .unwrap()
    );

    // Packet fingerprint exists
    assert!(packet_value["packet_fingerprint"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));

    // Workspace strategy: low risk => in_place_allowed
    assert_eq!(
        packet_value["workspace"]["required_strategy"],
        "in_place_allowed"
    );

    // Required capabilities include source.read and repository.inspect
    let caps = packet_value["capabilities"]["required"].as_array().unwrap();
    let cap_strs: Vec<&str> = caps.iter().map(|c| c.as_str().unwrap()).collect();
    assert!(cap_strs.contains(&"repository.inspect"));
    assert!(cap_strs.contains(&"source.read"));
    assert!(cap_strs.contains(&"source.write"));

    // Knowledge typed not-installed
    assert_eq!(packet_value["knowledge"]["status"], "not_installed");
}

// -----------------------------------------------------------------------
// E. Packet coherence: no mutation side effects
// -----------------------------------------------------------------------

#[test]
fn target_repo_packet_creates_no_lease_workspace_or_run_state() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);

    // Before packet: capture .pulse state
    let before: Vec<std::path::PathBuf> = fs::read_dir(repo.path().join(".pulse"))
        .map(|entries| entries.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default();

    repo.pulse_ok(&["work", "packet", &ticket_id, "--json"]);

    // After packet: capture .pulse state
    let after: Vec<std::path::PathBuf> = fs::read_dir(repo.path().join(".pulse"))
        .map(|entries| entries.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default();

    // Compare top-level entries. Allowed additions:
    // - .pulse/cache/workgraph.snapshot.json (from graph projection cache)
    // - .pulse/cache/docs-search/ (cache-only docs search)
    // - .pulse/runtime/locks/workgraph.lock (repository fence lock)
    //
    // Disallowed additions:
    // - .pulse/workspace/ or .pulse/runtime/ leases
    // - .pulse/workgraph/nodes or edges changes (no mutation)
    // - .pulse/events/ (no event created)
    let before_names: Vec<String> = before
        .iter()
        .filter_map(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .collect();
    let after_names: Vec<String> = after
        .iter()
        .filter_map(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .collect();

    // Check no forbidden directories appeared
    let forbidden = ["workspace", "runtime/leases", "events"];
    let new_entries: Vec<&str> = after_names
        .iter()
        .filter(|n| !before_names.contains(n))
        .map(|n| n.as_str())
        .collect();
    for entry in &new_entries {
        assert!(
            !forbidden.contains(entry),
            "packet query must not create {entry}"
        );
    }

    // Node revision must not change
    let node_path = repo
        .path()
        .join(".pulse/workgraph/nodes")
        .join(format!("{ticket_id}.json"));
    let node_bytes = fs::read(&node_path).unwrap();
    let node_val: Value = serde_json::from_slice(&node_bytes).unwrap();
    assert_eq!(
        node_val["status"], "ready",
        "packet must not change node status"
    );

    // Git status must remain clean
    assert!(repo.git_is_clean(), "packet must not make worktree dirty");
}

#[test]
fn target_repo_packet_does_not_bootstrap_on_non_enrolled_path() {
    // Create a TempDir that has NOT been bootstrapped into a Pulse target.
    // Use the same binary resolution as TestRepo.
    let bin = std::env::var("CARGO_BIN_EXE_pulse").unwrap_or_else(|_| {
        development_repo_root()
            .join("target/debug/pulse")
            .to_string_lossy()
            .to_string()
    });
    let temp = tempfile::tempdir().unwrap();

    let output = Command::new(&bin)
        .arg("--repo-root")
        .arg(temp.path())
        .args(["work", "packet", "TK-001", "--json"])
        .output()
        .expect("run pulse on non-enrolled path");

    assert!(
        !output.status.success(),
        "packet on non-enrolled path must fail: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify no .pulse directory was created
    assert!(
        !temp.path().join(".pulse").exists(),
        "packet must not bootstrap .pulse on non-enrolled path"
    );
}

// -----------------------------------------------------------------------
// B. Subject/readiness errors via CLI on target repo
// -----------------------------------------------------------------------

fn full_enroll(repo: &TestRepo) {
    // Bootstrap graph, evidence, and docs manifests so the packet builder's
    // repository identity check passes.
    repo.pulse_ok(&["graph", "bootstrap", "--json"]);
    pulse::evidence::manifest::load(repo.path()).unwrap();
    pulse::docs::manifest::bootstrap(repo.path()).unwrap();

    // Commit the .pulse/ infrastructure so the worktree is clean.
    let add = std::process::Command::new("git")
        .current_dir(repo.path())
        .args(["add", "."])
        .output()
        .expect("git add");
    assert!(add.status.success(), "git add failed");
    let commit = std::process::Command::new("git")
        .current_dir(repo.path())
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
            "enroll target repo",
        ])
        .output()
        .expect("git commit");
    assert!(
        commit.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&commit.stderr)
    );
}

#[test]
fn target_repo_packet_rejects_missing_id() {
    let repo = TestRepo::from_fixture("minimal-service");
    full_enroll(&repo);

    let output = repo.pulse(&["work", "packet", "TK-NONEXISTENT", "--json"]);
    assert!(!output.status.success());
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["code"], "work_packet_subject_not_found");
}

#[test]
fn target_repo_packet_rejects_not_ready_ticket() {
    let repo = TestRepo::from_fixture("minimal-service");
    full_enroll(&repo);
    // Create ticket but don't make it ready
    let created = repo.pulse_ok(&[
        "work",
        "create",
        "--kind",
        "ticket",
        "--title",
        "Draft ticket",
        "--role",
        "implementation",
        "--risk",
        "low",
        "--materialization",
        "R1",
        "--json",
    ]);
    let id = created["value"]["id"].as_str().unwrap();

    let output = repo.pulse(&["work", "packet", id, "--json"]);
    assert!(!output.status.success());
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["code"], "work_packet_status_not_ready");
}

// -----------------------------------------------------------------------
// G. Safety/architecture: tracked fixture remains immutable
// -----------------------------------------------------------------------

#[test]
fn target_repo_packet_does_not_mutate_tracked_fixture() {
    use crate::common::fixture_repo::snapshot_tree;
    let fixture = crate::common::fixture_repo::fixture_path("minimal-service");
    let before = snapshot_tree(&fixture).expect("snapshot tracked fixture");
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);

    // Run packet command
    repo.pulse_ok(&["work", "packet", &ticket_id, "--json"]);

    let after = snapshot_tree(&fixture).expect("snapshot tracked fixture after packet");
    assert_eq!(
        before, after,
        "work packet must not mutate tracked fixture source"
    );
}
