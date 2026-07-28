//! P2S1-I7 hardening coverage for `pulse work packet`.
//!
//! These tests exercise two-fence revalidation, source currentness, hard
//! budgets and deterministic cache/fingerprint behavior against isolated target
//! repository copies. They never run Pulse against the development repository or
//! mutate tracked fixtures in place.

use pulse::canonical_json::{hash_bytes, to_canonical_bytes};
use pulse::docs::{
    DocsRegistry, DocumentAuthority, DocumentKind, DocumentLifecycle, DocumentRecord,
    DocumentScope, ReviewPolicy,
};
use pulse::evidence::model::*;
use pulse::graph::contract::{
    ContentRef, ContractItem, ContractScope, EffortMetadata, ImplementationContract,
    ImplementationMode, ImplementationSemanticImpact, Materialization, PlanPolicy, QaImpactPosture,
    Risk, SurfaceRef, TicketRole, WorkSurface,
};
use pulse::graph::edge::EdgeType;
use pulse::graph::node::DocumentationImpactPosture;
use pulse::graph::store::{
    ContractSetRequest, DocumentationImpactUpdate, OperationContext, QaImpactUpdate,
};
use pulse::id::WorkKind;
use pulse::policy::{AuthorityPolicy, AuthorityPrincipal};
use pulse::JsonGraphStore;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use crate::common::fixture_repo::TestRepo;

fn ctx() -> OperationContext {
    OperationContext {
        actor: "human:tester".to_string(),
        now: chrono::Utc::now(),
    }
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .expect("git command");
    assert!(
        output.status.success(),
        "git {:?} failed: stdout={} stderr={}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn commit_all(repo: &Path, message: &str) -> String {
    git(repo, &["add", "."]);
    let output = Command::new("git")
        .current_dir(repo)
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
            message,
        ])
        .output()
        .expect("git commit");
    assert!(
        output.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    git(repo, &["rev-parse", "HEAD"])
}

fn write_policy(root: &Path, revision: u64) {
    let mut grants = vec![
        "shape.apply".to_string(),
        "shape.approve.R1".to_string(),
        "qa.none.approve".to_string(),
        "docs.impact.required".to_string(),
        "work.transition.shaped".to_string(),
        "work.transition.ready".to_string(),
    ];
    grants.sort();
    let policy = AuthorityPolicy {
        schema_version: 1,
        revision,
        principals: vec![AuthorityPrincipal {
            kind: ActorKind::Human,
            id: "tester".to_string(),
            grants,
        }],
    };
    let path = root.join(".pulse/policy/authority.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, to_canonical_bytes(&policy).unwrap()).unwrap();
}

fn setup_ready_ticket(repo: &TestRepo) -> String {
    setup_ready_ticket_with_required_docs(repo, false)
}

fn setup_ready_ticket_with_required_docs(repo: &TestRepo, require_auth_doc: bool) -> String {
    let root = repo.path();
    let store = JsonGraphStore::new(root);

    write_policy(root, 1);
    repo.pulse_ok(&["graph", "bootstrap", "--json"]);
    pulse::evidence::manifest::load(root).unwrap();
    let docs_manifest = pulse::docs::manifest::bootstrap(root).unwrap();
    if require_auth_doc {
        let mut registry = DocsRegistry::empty(docs_manifest.registry.repository_id);
        registry.documents.push(DocumentRecord {
            id: "DOC-PRODUCT-AUTH".to_string(),
            revision: 1,
            path: "docs/product/authentication.md".to_string(),
            kind: DocumentKind::Product,
            authority: DocumentAuthority::Approved,
            lifecycle: DocumentLifecycle::Current,
            owner: "team:docs".to_string(),
            summary: "Authentication product behavior.".to_string(),
            aliases: vec!["auth".to_string()],
            scope: DocumentScope {
                paths: vec![],
                domains: vec!["authentication".to_string()],
                work_labels: vec!["tokens".to_string()],
            },
            review_policy: ReviewPolicy::None,
            verification_profile: "docs-only".to_string(),
            generated: None,
            superseded_by: None,
            retrieval: None,
        });
        registry.normalize();
        let path = root.join(".pulse/docs/registry.json");
        fs::write(path, to_canonical_bytes(&registry).unwrap()).unwrap();
    }

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
    let node = store.show_node(&ticket_id).unwrap();

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
            path: brief_rel.clone(),
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
                posture: if require_auth_doc {
                    DocumentationImpactPosture::Required
                } else {
                    DocumentationImpactPosture::None
                },
                rationale: Some("No docs change.".to_string()),
                required_documents: if require_auth_doc {
                    vec!["DOC-PRODUCT-AUTH".to_string()]
                } else {
                    vec![]
                },
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
                path: brief_rel.clone(),
                sha256: brief_hash,
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
                        id: "DEC-001".to_string(),
                        revision: 1,
                        gist: "Single-use atomic rotation".to_string(),
                    },
                },
            }],
            fog: vec![],
            out_of_scope: vec![],
            resolution_pointers: vec![ShapingResolutionPointer {
                kind: "decision".to_string(),
                id: "DEC-001".to_string(),
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

    commit_all(root, "setup ready ticket");
    ticket_id
}

fn run_packet_with_barrier(repo: &TestRepo, ticket_id: &str, dir: &Path) -> std::process::Child {
    let signal = dir.join("after-first-fence");
    let release = dir.join("release");
    Command::new(
        std::env::var_os("CARGO_BIN_EXE_pulse")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/pulse")
            }),
    )
    .arg("--repo-root")
    .arg(repo.path())
    .arg("--test-work-packet-after-first-fence")
    .args(["work", "packet", ticket_id, "--json"])
    .env("PULSE_WORK_PACKET_AFTER_FIRST_FENCE_SIGNAL", &signal)
    .env("PULSE_WORK_PACKET_AFTER_FIRST_FENCE_WAIT", &release)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .expect("spawn packet with barrier")
}

fn wait_for(path: &Path) {
    let start = Instant::now();
    while !path.exists() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "timed out waiting for {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn finish_child(child: std::process::Child) -> Output {
    child.wait_with_output().expect("wait child")
}

fn error_code(output: &Output) -> String {
    assert!(
        !output.status.success(),
        "expected failure: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    err["code"].as_str().unwrap().to_string()
}

fn packet_ok(repo: &TestRepo, ticket_id: &str) -> Value {
    repo.pulse_ok(&["work", "packet", ticket_id, "--json"])
}

fn tracked_snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let rel = path.strip_prefix(root).unwrap();
            if rel.components().next().and_then(|c| c.as_os_str().to_str()) == Some(".git") {
                continue;
            }
            if entry.file_type().unwrap().is_dir() {
                walk(root, &path, out);
            } else {
                out.insert(
                    rel.to_string_lossy().replace('\\', "/"),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }

    let mut snapshot = BTreeMap::new();
    walk(root, root, &mut snapshot);
    snapshot
}

fn is_allowed_packet_side_effect(path: &str) -> bool {
    path.starts_with(".pulse/cache/") || path.starts_with(".pulse/runtime/locks/")
}

#[test]
fn graph_mutation_during_docs_search_returns_snapshot_changed_and_retry_succeeds() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    let barrier = tempfile::tempdir().unwrap();
    let child = run_packet_with_barrier(&repo, &ticket_id, barrier.path());
    wait_for(&barrier.path().join("after-first-fence"));

    let shown = repo.pulse_ok(&["work", "show", &ticket_id, "--json"]);
    let revision = shown["node"]["revision"].as_u64().unwrap().to_string();
    repo.pulse_ok(&[
        "work",
        "edit",
        &ticket_id,
        "--expected-revision",
        &revision,
        "--title",
        "Implement refresh token rotation changed concurrently",
        "--json",
    ]);
    fs::write(barrier.path().join("release"), b"go").unwrap();
    let output = finish_child(child);
    assert_eq!(error_code(&output), "work_packet_snapshot_changed");

    commit_all(repo.path(), "stable graph mutation");
    let packet = packet_ok(&repo, &ticket_id);
    assert_eq!(
        packet["subject"]["title"],
        "Implement refresh token rotation changed concurrently"
    );
}

#[test]
fn docs_content_mutation_during_docs_search_returns_snapshot_changed_and_no_mixed_packet() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket_with_required_docs(&repo, true);
    let barrier = tempfile::tempdir().unwrap();
    let child = run_packet_with_barrier(&repo, &ticket_id, barrier.path());
    wait_for(&barrier.path().join("after-first-fence"));

    fs::write(
        repo.path().join("docs/product/authentication.md"),
        b"# Authentication\n\nChanged during packet build.\n",
    )
    .unwrap();
    git(repo.path(), &["add", "docs/product/authentication.md"]);
    git(
        repo.path(),
        &["commit", "-q", "-m", "docs mutation during packet"],
    );
    fs::write(barrier.path().join("release"), b"go").unwrap();
    let output = finish_child(child);
    assert_eq!(error_code(&output), "work_packet_snapshot_changed");
}

#[test]
fn authority_policy_mutation_during_docs_search_returns_snapshot_changed() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    let barrier = tempfile::tempdir().unwrap();
    let child = run_packet_with_barrier(&repo, &ticket_id, barrier.path());
    wait_for(&barrier.path().join("after-first-fence"));

    write_policy(repo.path(), 2);
    fs::write(barrier.path().join("release"), b"go").unwrap();
    let output = finish_child(child);
    assert_eq!(error_code(&output), "work_packet_snapshot_changed");
}

#[test]
fn source_change_during_docs_search_returns_source_changed_and_retry_succeeds_after_commit() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    let barrier = tempfile::tempdir().unwrap();
    let child = run_packet_with_barrier(&repo, &ticket_id, barrier.path());
    wait_for(&barrier.path().join("after-first-fence"));

    fs::write(
        repo.path().join("src/token.mjs"),
        b"export const changed = true;\n",
    )
    .unwrap();
    fs::write(barrier.path().join("release"), b"go").unwrap();
    let output = finish_child(child);
    assert_eq!(error_code(&output), "work_packet_source_changed");

    commit_all(repo.path(), "stable source mutation");
    let packet = packet_ok(&repo, &ticket_id);
    assert_eq!(packet["source"]["commit"], repo.git_head());
}

#[test]
fn source_status_matrix_dirty_untracked_ignored_detached_and_operation_state() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);

    fs::write(repo.path().join("src/token.mjs"), b"dirty tracked\n").unwrap();
    let output = repo.pulse(&["work", "packet", &ticket_id, "--json"]);
    assert_eq!(error_code(&output), "work_packet_dirty_source_unsupported");
    git(repo.path(), &["checkout", "--", "src/token.mjs"]);

    fs::write(repo.path().join("untracked.txt"), b"dirty untracked\n").unwrap();
    let output = repo.pulse(&["work", "packet", &ticket_id, "--json"]);
    assert_eq!(error_code(&output), "work_packet_dirty_source_unsupported");
    fs::remove_file(repo.path().join("untracked.txt")).unwrap();

    fs::create_dir_all(repo.path().join(".pulse/cache/i7")).unwrap();
    fs::write(repo.path().join(".pulse/cache/i7/ignored"), b"ignored\n").unwrap();
    let packet = packet_ok(&repo, &ticket_id);
    assert_eq!(packet["source"]["cleanliness"], "clean");

    let head = repo.git_head();
    git(repo.path(), &["checkout", "--detach", &head]);
    let packet = packet_ok(&repo, &ticket_id);
    assert!(packet["source"]["head_ref"].is_null());

    let git_path = git(
        repo.path(),
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "MERGE_HEAD",
        ],
    );
    fs::write(git_path, b"0123456789abcdef0123456789abcdef01234567\n").unwrap();
    let output = repo.pulse(&["work", "packet", &ticket_id, "--json"]);
    assert_eq!(
        error_code(&output),
        "work_packet_source_operation_in_progress"
    );
}

#[test]
fn same_inputs_cache_rebuild_and_required_doc_hash_have_exact_fingerprint_behavior() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket_with_required_docs(&repo, true);
    let first = packet_ok(&repo, &ticket_id);
    let first_fp = first["packet_fingerprint"].as_str().unwrap().to_string();
    let first_size = first["budget"]["actual_canonical_json_bytes"]
        .as_u64()
        .unwrap();

    let cache = repo.path().join(".pulse/cache/docs-search");
    if cache.exists() {
        fs::remove_dir_all(&cache).unwrap();
    }
    let second = packet_ok(&repo, &ticket_id);
    assert_eq!(second["packet_fingerprint"].as_str().unwrap(), first_fp);
    assert_eq!(
        second["budget"]["actual_canonical_json_bytes"]
            .as_u64()
            .unwrap(),
        first_size
    );
    let canonical = pulse::canonical_json::to_canonical_bytes(&second).unwrap();
    assert_eq!(canonical.len() as u64, first_size);

    fn assert_no_float(value: &Value) {
        match value {
            Value::Number(n) => assert!(!n.is_f64(), "packet JSON must not contain floats"),
            Value::Array(items) => items.iter().for_each(assert_no_float),
            Value::Object(map) => map.values().for_each(assert_no_float),
            _ => {}
        }
    }
    assert_no_float(&second);

    fs::write(
        repo.path().join("docs/product/authentication.md"),
        b"# Authentication\n\nRequired document hash changed.\n",
    )
    .unwrap();
    commit_all(repo.path(), "change required doc hash");
    let changed = packet_ok(&repo, &ticket_id);
    assert_ne!(
        changed["packet_fingerprint"].as_str().unwrap(),
        first_fp,
        "required document content hash must participate in packet fingerprint"
    );
}

#[test]
fn selected_suggestion_hash_rank_and_score_changes_affect_fingerprint() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket_with_required_docs(&repo, true);
    let first = packet_ok(&repo, &ticket_id);
    let first_fp = first["packet_fingerprint"].as_str().unwrap().to_string();
    let first_suggestions = first["documentation"]["suggested_sections"]
        .as_array()
        .unwrap();
    assert!(
        !first_suggestions.is_empty(),
        "fixture must produce at least one selected suggestion"
    );
    let first_refs: Vec<String> = first_suggestions
        .iter()
        .map(|section| section["section_ref"].as_str().unwrap().to_string())
        .collect();
    let first_scores: Vec<u64> = first_suggestions
        .iter()
        .map(|section| section["score_micros"].as_u64().unwrap())
        .collect();

    fs::write(
        repo.path().join("docs/product/authentication.md"),
        b"# Refresh-token failure contract\n\nRefresh token rotation atomic acceptance TokenExpired InvalidToken.\n\n## Outcomes\n\nTokenExpired InvalidToken refresh token rotation acceptance.\n",
    )
    .unwrap();
    commit_all(repo.path(), "change selected suggestion content");
    let changed = packet_ok(&repo, &ticket_id);
    assert_ne!(
        changed["packet_fingerprint"].as_str().unwrap(),
        first_fp,
        "selected suggestion identity/hash/rank/score must participate in fingerprint"
    );
    let changed_suggestions = changed["documentation"]["suggested_sections"]
        .as_array()
        .unwrap();
    let changed_refs: Vec<String> = changed_suggestions
        .iter()
        .map(|section| section["section_ref"].as_str().unwrap().to_string())
        .collect();
    let changed_scores: Vec<u64> = changed_suggestions
        .iter()
        .map(|section| section["score_micros"].as_u64().unwrap())
        .collect();
    assert!(
        changed_refs != first_refs || changed_scores != first_scores,
        "test mutation must alter selected suggestion ordering or quantized score"
    );
}

#[test]
fn packet_side_effects_are_limited_to_ignored_cache_and_lock_paths() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    let before = tracked_snapshot(repo.path());

    packet_ok(&repo, &ticket_id);

    let after = tracked_snapshot(repo.path());
    for path in before.keys().chain(after.keys()) {
        if is_allowed_packet_side_effect(path) {
            continue;
        }
        assert_eq!(
            before.get(path),
            after.get(path),
            "packet query changed forbidden path {path}"
        );
    }
    assert!(
        after.keys().any(|path| path.starts_with(".pulse/cache/")),
        "packet query should only materialize disposable cache state when cache paths are ignored"
    );
    assert!(
        repo.git_is_clean(),
        "allowed side effects must be git-ignored"
    );
}

#[test]
fn packet_rejects_non_ignored_operational_paths_before_dirtying_clean_repo() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    fs::write(repo.path().join(".gitignore"), b"node_modules/\n").unwrap();
    commit_all(repo.path(), "stop ignoring pulse operational paths");
    let before = tracked_snapshot(repo.path());

    let output = repo.pulse(&["work", "packet", &ticket_id, "--json"]);
    assert!(!output.status.success());
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["code"], "work_packet_operational_path_not_ignored");

    let after = tracked_snapshot(repo.path());
    assert_eq!(
        before, after,
        "failed packet must not create cache or lock files"
    );
    assert!(repo.git_is_clean(), "failed packet must leave source clean");
}

#[test]
fn relation_overflow_rejects_more_than_128_incident_edges() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    let store = JsonGraphStore::new(repo.path());
    for i in 0..=pulse::work_packet::MAX_INCIDENT_RELATIONS {
        let created = store
            .create_node_public_with_context(
                WorkKind::Ticket,
                format!("Related {i}"),
                pulse::graph::contract::PublicCreateClassification {
                    role: Some(TicketRole::Implementation),
                    risk: Some(Risk::Low),
                    materialization: Some(Materialization::R0),
                },
                ctx(),
            )
            .unwrap()
            .value;
        store
            .add_edge_with_context(EdgeType::Related, ticket_id.clone(), created.id, ctx())
            .unwrap();
    }
    commit_all(repo.path(), "add overflowing incident edges");
    let output = repo.pulse(&["work", "packet", &ticket_id, "--json"]);
    assert_eq!(error_code(&output), "work_packet_relation_overflow");
}

#[test]
fn budget_exceeded_rejects_without_truncating_required_context() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    let store = JsonGraphStore::new(repo.path());
    let node = store.show_node(&ticket_id).unwrap();
    store
        .edit_title_with_context(
            &ticket_id,
            node.revision,
            format!("{}{}", "Budget pressure ", "x".repeat(140_000)),
            ctx(),
        )
        .unwrap();
    commit_all(repo.path(), "oversize packet title");
    let output = repo.pulse(&["work", "packet", &ticket_id, "--json"]);
    assert_eq!(error_code(&output), "work_packet_budget_exceeded");
}
