//! P2S1-I6: CLI contract tests for `pulse work packet`.
//!
//! Covers stable JSON output, error codes, human rendering, dispatch
//! constants, and schema validation. The kernel/builder tests in
//! `kernel::packet::tests` cover the library-depth packet construction;
//! this file covers the CLI wiring.

use chrono::Utc;
use pulse::canonical_json::hash_bytes;
use pulse::canonical_json::to_canonical_bytes;
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

use crate::common_bin::bin;

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

/// Initialize a minimal Git repository with a baseline commit so that the
/// packet source snapshot passes.
fn init_git_repo(repo: &TempDir) {
    use std::process::Command as GitCmd;
    GitCmd::new("git")
        .current_dir(repo.path())
        .arg("init")
        .arg("-q")
        .output()
        .expect("git init");
    // Create a .gitignore that excludes .pulse/ runtime state so bootstrap
    // and packet creation don't leave untracked non-ignored files.
    fs::write(repo.path().join(".gitignore"), b".pulse/\n").unwrap();
    // Create an initial file so the commit has content
    let readme = repo.path().join("README.md");
    if !readme.exists() {
        fs::write(&readme, b"# Test\n").unwrap();
    }
    let add = GitCmd::new("git")
        .current_dir(repo.path())
        .args(["add", "."])
        .output()
        .expect("git add");
    assert!(add.status.success(), "git add failed");
    let commit = GitCmd::new("git")
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
            "-m",
            "initial",
        ])
        .output()
        .expect("git commit");
    assert!(commit.status.success(), "git commit failed");
}

/// Bootstrap the Pulse graph store via CLI for a freshly initialized repo.
fn bootstrap_repo(repo: &TempDir) {
    let output = run(repo, &["graph", "bootstrap", "--json"]);
    assert!(
        output.status.success(),
        "bootstrap failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Build a fully-ready implementation Ticket through the library, returning
/// the ticket ID.  The repo must have been initialized with `init_git_repo`
/// and `bootstrap_repo` already.
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
            "Rotate refresh tokens".to_string(),
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
    fs::write(&brief_path, b"# Ticket\nImplement atomic rotation.").unwrap();
    let brief_hash = hash_bytes(&fs::read(&brief_path).unwrap());
    let contract = ImplementationContract {
        mode: ImplementationMode::Guided,
        work_surface: WorkSurface::Code,
        plan_policy: PlanPolicy::None,
        semantic_impact: ImplementationSemanticImpact::NoBehaviorOrPublicRiskChange,
        effort: EffortMetadata {
            multi_session: false,
            multiple_dependent_decisions: false,
            resume_or_audit_continuity: false,
        },
        verification_profile: "service-change".to_string(),
        brief: Some(ContentRef {
            path: format!("{}/ticket.md", node.content_dir),
            content_hash: brief_hash.clone(),
        }),
        objective: "Rotate refresh tokens atomically.".to_string(),
        current_behavior: "Tokens are long-lived without rotation.".to_string(),
        target_behavior: "Refresh tokens rotate on each use atomically.".to_string(),
        code_anchors: vec![SurfaceRef::path("src/auth.rs")],
        documentation_anchors: vec![],
        configuration_anchors: vec![],
        data_anchors: vec![],
        research_refs: vec![],
        required_changes: vec![ContractItem {
            id: "CHG-1".to_string(),
            summary: "Add rotation logic to auth module.".to_string(),
        }],
        invariants: vec![ContractItem {
            id: "INV-1".to_string(),
            summary: "Concurrent rotation must be serialized.".to_string(),
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
                rationale: Some("No behavior change for end users.".to_string()),
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
                rationale: Some("No docs change.".to_string()),
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
            destination: Some(ShapingDestination {
                summary: "Deliver reliable refresh-token rotation".to_string(),
                scope_boundary: vec!["No session UI redesign".to_string()],
                exit_conditions: vec!["Concurrent rotation acceptance passes".to_string()],
            }),
            map: None,
            affected_work: vec![],
            branches: vec![ShapingBranch {
                id: "BR-AUTH-1".to_string(),
                question: "How is concurrent rotation serialized?".to_string(),
                gap_kind: "tradeoff_gap".to_string(),
                criticality: BranchCriticality::Critical,
                affected_work: vec!["TK-001".to_string()],
                disposition: BranchDisposition::Resolved {
                    resolution: ShapingResolutionPointer {
                        kind: "decision".to_string(),
                        id: "DEC-001".to_string(),
                        revision: 2,
                        gist: "Single-use atomic rotation".to_string(),
                    },
                },
            }],
            fog: vec![],
            out_of_scope: vec![],
            resolution_pointers: vec![ShapingResolutionPointer {
                kind: "decision".to_string(),
                id: "DEC-001".to_string(),
                revision: 2,
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
            remaining_uncertainty: vec![RemainingUncertainty {
                summary: "Telemetry naming remains open".to_string(),
                trigger: "Telemetry implementation starts".to_string(),
            }],
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
    // Commit the content and receipt files written during setup so the
    // worktree is clean for the packet source check.
    commit_all(repo);
    ready.id
}

/// Commit all pending changes so the repo is clean for the packet command.
fn commit_all(repo: &TempDir) {
    use std::process::Command as GitCmd;
    let add = GitCmd::new("git")
        .current_dir(repo.path())
        .args(["add", "."])
        .output()
        .expect("git add");
    assert!(add.status.success(), "git add failed");
    // Allow commit to be a no-op if there are no changes (e.g. when
    // all new files are gitignored).
    let output = GitCmd::new("git")
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
            "setup",
        ])
        .output()
        .expect("git commit");
    assert!(
        output.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Set up a minimal git + bootstrapped Pulse repo for tests that query
/// `work packet`.  Returns the store for further setup.
fn setup_repo(repo: &TempDir) -> JsonGraphStore {
    init_git_repo(repo);
    bootstrap_repo(repo);
    // The packet flow also needs the evidence manifest and docs registry.
    // Bootstrap them using the library (not available as separate CLI commands).
    pulse::evidence::manifest::load(repo.path()).unwrap();
    pulse::docs::manifest::bootstrap(repo.path()).unwrap();
    // Commit .pulse/ infrastructure so the worktree is clean.
    commit_all(repo);
    JsonGraphStore::new(repo.path())
}

// -----------------------------------------------------------------------
// A. Happy path — packet emitted with stable JSON
// -----------------------------------------------------------------------

#[test]
fn work_packet_emits_stable_json_for_ready_ticket() {
    let repo = tempfile::tempdir().unwrap();
    let store = setup_repo(&repo);
    let id = ready_ticket(&repo, &store);

    let output = run(&repo, &["work", "packet", &id, "--json"]);
    assert!(
        output.status.success(),
        "work packet failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let packet: Value = serde_json::from_slice(&output.stdout).unwrap();

    // Top-level shape
    assert_eq!(packet["schema_version"], 1);
    assert_eq!(packet["profile"], "phase2_work_packet_preview_v1");
    assert_eq!(packet["code"], "reservation_candidate");

    // Subject
    assert_eq!(packet["subject"]["kind"], "ticket");
    assert_eq!(packet["subject"]["role"], "implementation");
    assert_eq!(packet["subject"]["status"], "ready");

    // Dispatch constants per P2S1-D1
    assert_eq!(packet["dispatch"]["reservation_candidate"], true);
    assert_eq!(packet["dispatch"]["dispatch_authorized"], false);
    assert_eq!(packet["dispatch"]["authorization_status"], "not_reserved");

    // Snapshot has revalidation fingerprints
    assert!(packet["snapshot"]["readiness_fingerprint"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));

    // Source
    assert_eq!(packet["source"]["kind"], "git_commit");
    assert_eq!(packet["source"]["cleanliness"], "clean");

    // Capabilities
    let caps = packet["capabilities"]["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(caps.contains(&"repository.inspect"));
    assert!(caps.contains(&"source.read"));

    // Knowledge is typed not-installed per P2S1-D10
    assert_eq!(packet["knowledge"]["status"], "not_installed");
    assert_eq!(packet["knowledge"]["owner_phase"], 4);
    assert!(packet["knowledge"]["knowledge_fingerprint"].is_null());

    // Workspace
    assert_eq!(packet["workspace"]["binding_status"], "not_allocated");
    assert!(packet["workspace"]["workspace_id"].is_null());

    // Budget
    assert!(
        packet["budget"]["actual_canonical_json_bytes"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(
        packet["budget"]["actual_canonical_json_bytes"]
            .as_u64()
            .unwrap()
            <= packet["budget"]["max_canonical_json_bytes"]
                .as_u64()
                .unwrap()
    );

    // Packet fingerprint present
    assert!(packet["packet_fingerprint"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));

    // Context includes shaping, parents, decisions
    assert!(packet["shaping"]["owning_work"]["id"].as_str().is_some());
    assert!(packet["graph"]["structural_state"].as_str().is_some());
}

#[test]
fn work_packet_produces_json_on_stdout_errors_on_stderr() {
    let repo = tempfile::tempdir().unwrap();
    let store = setup_repo(&repo);
    let id = ready_ticket(&repo, &store);

    let output = run(&repo, &["work", "packet", &id, "--json"]);

    // JSON output on stdout
    assert!(!output.stdout.is_empty());
    let packet: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(packet["schema_version"], 1);
    // stderr may be empty for success
}

#[test]
fn work_packet_human_output_contains_key_fields() {
    let repo = tempfile::tempdir().unwrap();
    let store = setup_repo(&repo);
    let id = ready_ticket(&repo, &store);

    let output = run(&repo, &["work", "packet", &id]);
    assert!(
        output.status.success(),
        "work packet failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);

    // Human rendering should contain ticket ID, source commit, readiness,
    // workspace strategy and fingerprint.
    assert!(text.contains(&id), "human output should contain ticket ID");
    assert!(
        text.contains("reservation_candidate"),
        "human output should contain packet code"
    );
    assert!(
        text.contains("source:") && text.contains("clean"),
        "human output should contain source status"
    );
    assert!(
        text.contains("packet fingerprint:"),
        "human output should contain fingerprint"
    );
    assert!(
        text.contains("dispatch authorized: no"),
        "human output should state no dispatch authorization"
    );
}

// -----------------------------------------------------------------------
// B. Subject/readiness errors
// -----------------------------------------------------------------------

#[test]
fn work_packet_rejects_missing_id() {
    let repo = tempfile::tempdir().unwrap();
    let store = setup_repo(&repo);
    ready_ticket(&repo, &store);

    let output = run(&repo, &["work", "packet", "TK-NONEXISTENT", "--json"]);
    assert!(!output.status.success());
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["code"], "work_packet_subject_not_found");
}

#[test]
fn work_packet_rejects_draft_ticket() {
    let repo = tempfile::tempdir().unwrap();
    let store = setup_repo(&repo);
    write_policy(&repo, &["work.transition.shaped", "work.transition.ready"]);
    let node = store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Draft ticket".to_string(),
            pulse::graph::contract::PublicCreateClassification {
                role: Some(TicketRole::Implementation),
                risk: Some(Risk::Low),
                materialization: Some(Materialization::R1),
            },
            OperationContext::default(),
        )
        .unwrap()
        .value;

    let output = run(&repo, &["work", "packet", &node.id, "--json"]);
    assert!(!output.status.success());
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["code"], "work_packet_status_not_ready");
}

#[test]
fn work_packet_rejects_story_kind() {
    let repo = tempfile::tempdir().unwrap();
    let store = setup_repo(&repo);
    let node = store
        .create_node_public_with_context(
            WorkKind::Story,
            "A story".to_string(),
            pulse::graph::contract::PublicCreateClassification {
                role: None,
                risk: None,
                materialization: None,
            },
            OperationContext::default(),
        )
        .unwrap()
        .value;

    let output = run(&repo, &["work", "packet", &node.id, "--json"]);
    assert!(!output.status.success());
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["code"], "work_packet_subject_not_ticket");
}

#[test]
fn work_packet_rejects_non_implementation_role() {
    let repo = tempfile::tempdir().unwrap();
    let store = setup_repo(&repo);
    let node = store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Decision work ticket".to_string(),
            pulse::graph::contract::PublicCreateClassification {
                role: Some(TicketRole::DecisionWork),
                risk: Some(Risk::Low),
                materialization: Some(Materialization::R1),
            },
            OperationContext::default(),
        )
        .unwrap()
        .value;

    let output = run(&repo, &["work", "packet", &node.id, "--json"]);
    assert!(!output.status.success());
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["code"], "work_packet_role_unsupported");
}

#[test]
fn work_packet_error_envelope_is_stable() {
    let repo = tempfile::tempdir().unwrap();
    let store = setup_repo(&repo);
    ready_ticket(&repo, &store);

    let output = run(&repo, &["work", "packet", "TK-NONEXISTENT", "--json"]);
    assert!(!output.status.success());
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["schema_version"], 1);
    assert_eq!(err["code"], "work_packet_subject_not_found");
    assert!(err["message"].as_str().is_some());
}

#[test]
fn work_packet_rejects_unsupported_flags() {
    let repo = tempfile::tempdir().unwrap();
    let store = setup_repo(&repo);
    let id = ready_ticket(&repo, &store);

    for flag in [
        "--force",
        "--allow-dirty",
        "--include-not-ready",
        "--full-docs",
        "--claim",
    ] {
        let output = run(&repo, &["work", "packet", &id, flag, "--json"]);
        assert!(!output.status.success(), "{flag} must be unsupported");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("unexpected argument") || stderr.contains("unrecognized"),
            "{flag} should be rejected by CLI parser, got {stderr}"
        );
    }
}

#[test]
fn work_packet_maps_missing_docs_registry_before_graph_bootstrap() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(&repo);
    pulse::evidence::manifest::load(repo.path()).unwrap();
    commit_all(&repo);

    let output = run(&repo, &["work", "packet", "TK-001", "--json"]);
    assert!(!output.status.success());
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["code"], "work_packet_docs_registry_missing");
    assert!(
        !repo.path().join(".pulse/workgraph").exists(),
        "packet must not bootstrap workgraph after docs-registry rejection"
    );
}

#[test]
fn work_packet_rejects_missing_workgraph_without_bootstrap() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(&repo);
    pulse::evidence::manifest::load(repo.path()).unwrap();
    pulse::docs::manifest::bootstrap(repo.path()).unwrap();
    commit_all(&repo);

    let output = run(&repo, &["work", "packet", "TK-001", "--json"]);
    assert!(!output.status.success());
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["code"], "work_packet_graph_invalid");
    assert!(
        !repo.path().join(".pulse/workgraph").exists(),
        "packet must not bootstrap missing workgraph"
    );
}
