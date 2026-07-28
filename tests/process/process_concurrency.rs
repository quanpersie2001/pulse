use std::fs;
use std::path::Path;
use std::process::Stdio;

use chrono::Utc;
use pulse::canonical_json::to_canonical_bytes;
use pulse::graph::contract::{
    ContentRef, ContractItem, ContractScope, EffortMetadata, ImplementationContract,
    ImplementationMode, ImplementationSemanticImpact, PlanPolicy, PublicCreateClassification,
    QaImpactPosture, SurfaceRef,
};
use pulse::graph::node::{DocumentationImpactPosture, NodeStatus};
use pulse::graph::store::{
    ContractSetRequest, DocumentationImpactUpdate, OperationContext, QaImpactUpdate,
};
use pulse::id::WorkKind;
use pulse::JsonGraphStore;
use serde_json::Value;
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

fn run_ok(repo: &TempDir, args: &[&str]) -> Value {
    let output = run(repo, args);
    assert!(
        output.status.success(),
        "command failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("json stdout")
}

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

fn write_capability_file(root: &Path, principal: &str) -> std::path::PathBuf {
    // Place inside .pulse/runtime/ which is gitignored so the work packet
    // preflight does not reject it as a dirty untracked path.
    let dir = root.join(".pulse/runtime/claim-test");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("capabilities.json");
    fs::write(&path, valid_inventory_bytes(principal)).unwrap();
    path
}

/// Set up a ready ticket using the library API so process-level concurrent
/// claim tests can focus on subprocess claim invocation.
fn setup_ready_ticket(repo: &Path, store: &JsonGraphStore) -> String {
    // Bootstrap and write policy.
    let mut policy = pulse::policy::AuthorityPolicy {
        schema_version: 1,
        revision: 1,
        principals: vec![
            pulse::policy::AuthorityPrincipal {
                kind: pulse::identity::actor::ActorKind::Agent,
                id: "tester".to_string(),
                grants: vec![
                    "shape.apply".to_string(),
                    "shape.approve.R1".to_string(),
                    "qa.none.approve".to_string(),
                    "work.transition.shaped".to_string(),
                    "work.transition.ready".to_string(),
                    "work.assignment.prepare".to_string(),
                    "work.node.create".to_string(),
                ],
            },
            pulse::policy::AuthorityPrincipal {
                kind: pulse::identity::actor::ActorKind::Human,
                id: "tester".to_string(),
                grants: vec![
                    "shape.apply".to_string(),
                    "shape.approve.R1".to_string(),
                    "qa.none.approve".to_string(),
                    "work.transition.shaped".to_string(),
                    "work.transition.ready".to_string(),
                    "work.assignment.prepare".to_string(),
                    "work.node.create".to_string(),
                ],
            },
        ],
    };
    policy.normalize();
    let policy_path = repo.join(".pulse/policy/authority.json");
    fs::create_dir_all(policy_path.parent().unwrap()).unwrap();
    fs::write(&policy_path, to_canonical_bytes(&policy).unwrap()).unwrap();

    store.bootstrap().unwrap();
    pulse::evidence::manifest::load(repo).unwrap();
    pulse::docs::manifest::bootstrap(repo).unwrap();

    // Create ticket.
    let node = store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Concurrent claim test ticket".to_string(),
            PublicCreateClassification {
                role: Some(pulse::graph::contract::TicketRole::Implementation),
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
    let brief_path = repo.join(&brief_rel);
    fs::create_dir_all(brief_path.parent().unwrap()).unwrap();
    fs::write(&brief_path, b"# Concurrent claim test ticket").unwrap();
    let brief_hash = pulse::canonical_json::hash_bytes(&fs::read(&brief_path).unwrap());

    // Set implementation contract.
    store
        .set_contract_with_context(
            &ticket_id,
            node.revision,
            ContractSetRequest {
                role: pulse::graph::contract::TicketRole::Implementation,
                implementation: Some(ImplementationContract {
                    mode: ImplementationMode::Guided,
                    work_surface: pulse::graph::contract::WorkSurface::Code,
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
    let receipt_file = repo.join("shaping_claim.json");
    fs::write(&receipt_file, to_canonical_bytes(&receipt).unwrap()).unwrap();
    pulse::evidence::receipt::record_receipt(repo, None, &receipt_file).unwrap();

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

    // Commit git baseline.
    commit_all(repo);

    ticket_id
}

fn process_is_running(pid: u32) -> bool {
    let status = Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .expect("probe child process");
    status.success()
}

fn assert_assignment_event_count(repo: &TempDir, expected: usize) {
    let events_dir = repo.path().join(".pulse/events");
    let mut event_count = 0usize;
    if events_dir.exists() {
        for date in fs::read_dir(&events_dir).unwrap() {
            for entry in fs::read_dir(date.unwrap().path()).unwrap() {
                let content = fs::read_to_string(entry.unwrap().path()).unwrap();
                if content.contains("work.assignment.prepared") {
                    event_count += 1;
                }
            }
        }
    }
    assert_eq!(
        event_count, expected,
        "expected exactly {expected} work.assignment.prepared events"
    );
}

fn commit_all(repo: &Path) {
    use std::process::Command as GitCmd;
    // Ensure .pulse/runtime/ and .pulse/cache/ are gitignored so the work
    // packet preflight doesn't reject them as dirty paths.
    let gitignore_path = repo.join(".gitignore");
    if !gitignore_path.exists() {
        fs::write(&gitignore_path, b".pulse/runtime/\n.pulse/cache/\n").unwrap();
    }
    GitCmd::new("git")
        .args(["init"])
        .current_dir(repo)
        .output()
        .ok();
    GitCmd::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo)
        .output()
        .ok();
    GitCmd::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo)
        .output()
        .ok();
    GitCmd::new("git")
        .args(["add", "."])
        .current_dir(repo)
        .output()
        .ok();
    GitCmd::new("git")
        .args(["commit", "--allow-empty", "-m", "snapshot"])
        .current_dir(repo)
        .output()
        .ok();
}

/// Minimal shaping receipt builder.
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

// =========================================================================
// Concurrent claim tests
// =========================================================================

#[test]
fn two_processes_editing_same_revision_have_one_winner() {
    let repo = tempfile::tempdir().unwrap();
    let created = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Original",
            "--role",
            "implementation",
            "--risk",
            "low",
            "--materialization",
            "R0",
            "--json",
        ],
    );
    let id = created["value"]["id"].as_str().unwrap().to_string();

    let mut first = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "work",
            "edit",
            &id,
            "--expected-revision",
            "1",
            "--title",
            "First",
            "--json",
        ])
        .spawn()
        .expect("spawn first");
    let mut second = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "work",
            "edit",
            &id,
            "--expected-revision",
            "1",
            "--title",
            "Second",
            "--json",
        ])
        .spawn()
        .expect("spawn second");

    let first_status = first.wait().unwrap();
    let second_status = second.wait().unwrap();
    assert_ne!(first_status.success(), second_status.success());

    let shown = run_ok(&repo, &["work", "show", &id, "--json"]);
    assert_eq!(shown["node"]["revision"], 2);
    let title = shown["node"]["title"].as_str().unwrap();
    assert!(title == "First" || title == "Second");
}

#[test]
fn two_processes_editing_different_nodes_both_commit() {
    let repo = tempfile::tempdir().unwrap();
    let a = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "A",
            "--role",
            "implementation",
            "--risk",
            "low",
            "--materialization",
            "R0",
            "--json",
        ],
    );
    let b = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "B",
            "--role",
            "implementation",
            "--risk",
            "low",
            "--materialization",
            "R0",
            "--json",
        ],
    );
    let a_id = a["value"]["id"].as_str().unwrap().to_string();
    let b_id = b["value"]["id"].as_str().unwrap().to_string();

    let mut first = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "work",
            "edit",
            &a_id,
            "--expected-revision",
            "1",
            "--title",
            "A2",
            "--json",
        ])
        .spawn()
        .expect("spawn first");
    let mut second = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "work",
            "edit",
            &b_id,
            "--expected-revision",
            "1",
            "--title",
            "B2",
            "--json",
        ])
        .spawn()
        .expect("spawn second");

    assert!(first.wait().unwrap().success());
    assert!(second.wait().unwrap().success());

    assert_eq!(
        run_ok(&repo, &["work", "show", &a_id, "--json"])["node"]["title"],
        "A2"
    );
    assert_eq!(
        run_ok(&repo, &["work", "show", &b_id, "--json"])["node"]["title"],
        "B2"
    );
    assert!(repo
        .path()
        .join(".pulse/workgraph/nodes")
        .join(format!("{a_id}.json"))
        .exists());
    assert!(repo
        .path()
        .join(".pulse/workgraph/nodes")
        .join(format!("{b_id}.json"))
        .exists());
}

#[test]
fn same_ticket_concurrent_claims_have_one_winner() {
    let repo = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(repo.path());
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let cap_file = write_capability_file(repo.path(), "agent:codex-local");

    let first = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "work",
            "claim",
            &ticket_id,
            "--actor",
            "agent:tester",
            "--assignee",
            "agent:codex-local",
            "--capabilities",
            cap_file.to_str().unwrap(),
            "--json",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn first claim");
    let second = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "work",
            "claim",
            &ticket_id,
            "--actor",
            "agent:tester",
            "--assignee",
            "agent:codex-local",
            "--capabilities",
            cap_file.to_str().unwrap(),
            "--json",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn second claim");

    let first_output = first.wait_with_output().unwrap();
    let second_output = second.wait_with_output().unwrap();
    let outputs = [&first_output, &second_output];

    // Exactly one must succeed.
    assert_eq!(
        outputs
            .iter()
            .filter(|output| output.status.success())
            .count(),
        1,
        "expected exactly one successful claim, got stdout: first={} stderr={} second={} stderr={}",
        String::from_utf8_lossy(&first_output.stdout),
        String::from_utf8_lossy(&first_output.stderr),
        String::from_utf8_lossy(&second_output.stdout),
        String::from_utf8_lossy(&second_output.stderr),
    );

    // Loser must receive assignment_live_lease_exists or subject_not_ready
    let loser = outputs
        .iter()
        .find(|output| !output.status.success())
        .expect("one loser");
    let err: Value = serde_json::from_slice(&loser.stderr).expect("loser stderr is json");
    let code = err["code"].as_str().unwrap_or("");
    assert!(
        code == "assignment_claim_failed",
        "expected assignment_claim_failed, got {code}: {}",
        String::from_utf8_lossy(&loser.stderr)
    );
    let cause = err["cause_code"].as_str().unwrap_or("");
    assert!(
        cause == "assignment_live_lease_exists"
            || cause == "assignment_subject_not_ready"
            || cause == "work_packet_status_not_ready",
        "expected live_lease_exists or subject_not_ready, got cause: {cause}"
    );

    // Verify exactly one lease, one prepared, one event
    let leases_dir = repo.path().join(".pulse/runtime/assignment/leases");
    let prepared_dir = repo.path().join(".pulse/runtime/assignment/prepared");
    let events_dir = repo.path().join(".pulse/events");

    let lease_count = if leases_dir.exists() {
        fs::read_dir(&leases_dir).unwrap().count()
    } else {
        0
    };
    let prepared_count = if prepared_dir.exists() {
        fs::read_dir(&prepared_dir).unwrap().count()
    } else {
        0
    };

    assert_eq!(
        lease_count, 1,
        "expected exactly one lease record after one-winner claim"
    );
    assert_eq!(
        prepared_count, 1,
        "expected exactly one prepared record after one-winner claim"
    );

    // Event count for prepared.
    let mut event_count = 0usize;
    if events_dir.exists() {
        for date in fs::read_dir(&events_dir).unwrap() {
            for entry in fs::read_dir(date.unwrap().path()).unwrap() {
                let content = fs::read_to_string(entry.unwrap().path()).unwrap();
                if content.contains("work.assignment.prepared") {
                    event_count += 1;
                }
            }
        }
    }
    assert_eq!(
        event_count, 1,
        "expected exactly one work.assignment.prepared event"
    );
}

#[test]
fn different_ticket_claims_concurrently_both_commit_without_reclaim() {
    // Different ready Tickets must be claimable by two subprocesses under
    // default test threading. The repository fence serializes the commits, but
    // the second claim must not be rejected just because the first claim wrote
    // Pulse metadata (.pulse/workgraph, .pulse/events, .pulse/evidence, etc.).
    let repo = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(repo.path());
    let ticket_a = setup_ready_ticket(repo.path(), &store);
    let ticket_b = setup_ready_ticket(repo.path(), &store);
    let cap_file = write_capability_file(repo.path(), "agent:codex-local");

    let first = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "work",
            "claim",
            &ticket_a,
            "--actor",
            "agent:tester",
            "--assignee",
            "agent:codex-local",
            "--capabilities",
            cap_file.to_str().unwrap(),
            "--json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn first different-ticket claim");
    let second = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "work",
            "claim",
            &ticket_b,
            "--actor",
            "agent:tester",
            "--assignee",
            "agent:codex-local",
            "--capabilities",
            cap_file.to_str().unwrap(),
            "--json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn second different-ticket claim");

    let first_pid = first.id();
    let second_pid = second.id();
    assert_ne!(first_pid, second_pid);
    let first_running_at_spawn = process_is_running(first_pid);
    let second_running_at_spawn = process_is_running(second_pid);

    let first_output = first.wait_with_output().unwrap();
    let second_output = second.wait_with_output().unwrap();
    assert!(
        first_running_at_spawn && second_running_at_spawn,
        "both claim subprocesses should be live immediately after spawn"
    );
    assert!(
        first_output.status.success(),
        "first different-ticket claim failed: stdout={} stderr={}",
        String::from_utf8_lossy(&first_output.stdout),
        String::from_utf8_lossy(&first_output.stderr)
    );
    assert!(
        second_output.status.success(),
        "second different-ticket claim failed: stdout={} stderr={}",
        String::from_utf8_lossy(&second_output.stdout),
        String::from_utf8_lossy(&second_output.stderr)
    );

    let out_a: Value = serde_json::from_slice(&first_output.stdout).unwrap();
    let out_b: Value = serde_json::from_slice(&second_output.stdout).unwrap();
    assert_eq!(out_a["subject"]["id"], ticket_a);
    assert_eq!(out_a["subject"]["status_after"], "active");
    assert_eq!(out_b["subject"]["id"], ticket_b);
    assert_eq!(out_b["subject"]["status_after"], "active");

    // Verify both tickets are Active.
    let shown_a = run_ok(&repo, &["work", "show", &ticket_a, "--json"]);
    assert_eq!(shown_a["node"]["status"], "active");
    let shown_b = run_ok(&repo, &["work", "show", &ticket_b, "--json"]);
    assert_eq!(shown_b["node"]["status"], "active");

    assert_assignment_event_count(&repo, 2);

    let leases_dir = repo.path().join(".pulse/runtime/assignment/leases");
    assert_eq!(
        fs::read_dir(&leases_dir).unwrap().count(),
        2,
        "expected two lease records"
    );
    let prepared_dir = repo.path().join(".pulse/runtime/assignment/prepared");
    assert_eq!(
        fs::read_dir(&prepared_dir).unwrap().count(),
        2,
        "expected two prepared assignment records"
    );
}

#[test]
fn same_ticket_sequential_claim_rejected_after_first_succeeds() {
    // Verify that a second claim for the same ticket (after the first has
    // already completed) is rejected because the node is now Active.
    let repo = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(repo.path());
    let ticket_id = setup_ready_ticket(repo.path(), &store);
    let cap_file = write_capability_file(repo.path(), "agent:codex-local");

    // First claim should succeed.
    let out = run_ok(
        &repo,
        &[
            "work",
            "claim",
            &ticket_id,
            "--actor",
            "agent:tester",
            "--assignee",
            "agent:codex-local",
            "--capabilities",
            cap_file.to_str().unwrap(),
            "--json",
        ],
    );
    assert_eq!(out["subject"]["status_after"], "active");

    // Second claim must fail.
    let second = run(
        &repo,
        &[
            "work",
            "claim",
            &ticket_id,
            "--actor",
            "agent:tester",
            "--assignee",
            "agent:codex-local",
            "--capabilities",
            cap_file.to_str().unwrap(),
            "--json",
        ],
    );
    assert!(!second.status.success(), "second claim must be rejected");
    let err: Value = serde_json::from_slice(&second.stderr).unwrap();
    let cause = err["cause_code"].as_str().unwrap_or("");
    assert!(
        cause == "assignment_live_lease_exists"
            || cause == "work_packet_dirty_source_unsupported"
            || cause == "assignment_subject_not_ready"
            || cause == "work_packet_status_not_ready",
        "expected rejection, got cause: {cause}"
    );
}
