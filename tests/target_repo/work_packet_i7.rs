//! P2S1-I7 hardening coverage for `pulse work packet`.
//!
//! These tests exercise two-fence revalidation, source currentness, hard
//! budgets and deterministic cache/fingerprint behavior against isolated target
//! repository copies. They never run Pulse against the development repository or
//! mutate tracked fixtures in place.

use pulse::canonical_json::to_canonical_bytes;
use pulse::docs::{DocsRegistry, DocumentKind, DocumentRecord, DocumentScope, DocumentStatus};
use pulse::graph::model::contract::{Materialization, Risk, TicketRole};
use pulse::graph::model::edge::EdgeType;
use pulse::graph::model::node::NodeStatus;
use pulse::graph::store::OperationContext;
use pulse::id::WorkKind;
use pulse::identity::actor::ActorKind;
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

/// The Ticket contract bound by the packet: `works/<id>/ticket.md`.
/// Sections mirror the assignment fixture so the markdown ambiguity gate
/// passes for an R1 implementation Ticket.
fn ticket_markdown(ticket_id: &str, require_auth_doc: bool) -> String {
    let docs_section = if require_auth_doc {
        "## Documentation impact\n- Posture: required\n- Rationale: Refresh-token outcomes are public product behavior.\n- Documents: DOC-PRODUCT-AUTH\n"
    } else {
        "## Documentation impact\n- Posture: none\n- Rationale: No durable docs impact.\n- Documents:\n"
    };
    format!(
        "# {ticket_id} Implement refresh token rotation\n\
         \n\
         ## Objective\nRotate refresh tokens atomically.\n\
         \n\
         ## Current behavior\nTokens are long-lived.\n\
         \n\
         ## Target behavior\nTokens rotate on each use.\n\
         \n\
         ## Code anchors\n- src/token.mjs\n\
         \n\
         ## Required changes\n- Add rotation logic.\n\
         \n\
         ## Invariants\n- Concurrent rotation serialized.\n\
         \n\
         ## Implementation freedom\nguided: agent chooses internal structure within this contract.\n\
         \n\
         ## Acceptance\n- AC-1: Tokens rotate without race.\n\
         \n\
         ## Verify\n- node scripts/verify.mjs\n\
         \n\
         {docs_section}\
         \n\
         ## QA impact\n- Owner:\n- Posture: none\n- Cases:\n- Reason: No product QA impact.\n"
    )
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
            tags: vec![],
            id: "DOC-PRODUCT-AUTH".to_string(),
            revision: 1,
            path: "docs/product/authentication.md".to_string(),
            kind: DocumentKind::Product,
            status: DocumentStatus::Approved,
            owner: "team:docs".to_string(),
            summary: "Authentication product behavior.".to_string(),
            scope: DocumentScope {
                paths: vec!["authentication".to_string()],
            },
            generated: None,
            superseded_by: None,
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

    // The Ticket contract is `works/<id>/ticket.md` bound by `brief_hash`:
    // write the markdown, then let `sync` parse it, bump the contract
    // revision, and derive docs/QA metadata.
    let brief_path = root.join(&node.content_dir).join("ticket.md");
    fs::create_dir_all(brief_path.parent().unwrap()).unwrap();
    fs::write(&brief_path, ticket_markdown(&ticket_id, require_auth_doc)).unwrap();

    store
        .sync_ticket_with_context(&ticket_id, node.revision, ctx())
        .unwrap();

    // Markdown ambiguity gate: transition through Shaped to Ready.
    for status in [NodeStatus::Shaped, NodeStatus::Ready] {
        let revision = store.show_node(&ticket_id).unwrap().revision;
        store
            .transition_node_with_context(&ticket_id, status, revision, None, ctx())
            .unwrap();
    }

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

fn git_status(root: &Path, path: &str) -> String {
    git(
        root,
        &[
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--",
            path,
        ],
    )
}

fn assert_packet_rejects_without_side_effects(repo: &TestRepo, ticket_id: &str) {
    let before = tracked_snapshot(repo.path());
    let output = repo.pulse(&["work", "packet", ticket_id, "--json"]);
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
        packet["ticket"]["node"]["title"],
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

    // Baseline: clean worktree packet.
    let clean = packet_ok(&repo, &ticket_id);
    let clean_fp = clean["packet_fingerprint"].as_str().unwrap().to_string();
    assert_eq!(clean["source"]["dirty"], false);
    assert!(clean["source"]["dirty_hash"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));

    // A dirty tracked file is now a valid packet base and is bound into the
    // fingerprint via source.dirty_hash.
    fs::write(repo.path().join("src/token.mjs"), b"dirty tracked\n").unwrap();
    let tracked_dirty = packet_ok(&repo, &ticket_id);
    assert_eq!(tracked_dirty["source"]["dirty"], true);
    let tracked_fp = tracked_dirty["packet_fingerprint"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(tracked_fp, clean_fp);

    // An untracked file mutates the dirty identity again.
    fs::write(repo.path().join("untracked.txt"), b"dirty untracked\n").unwrap();
    let untracked_dirty = packet_ok(&repo, &ticket_id);
    assert_eq!(untracked_dirty["source"]["dirty"], true);
    assert_ne!(
        untracked_dirty["packet_fingerprint"].as_str().unwrap(),
        tracked_fp
    );

    // Ignored cache content does not affect the dirty identity.
    fs::create_dir_all(repo.path().join(".pulse/cache/i7")).unwrap();
    fs::write(repo.path().join(".pulse/cache/i7/ignored"), b"ignored\n").unwrap();
    let still_dirty = packet_ok(&repo, &ticket_id);
    assert_eq!(
        still_dirty["packet_fingerprint"].as_str().unwrap(),
        untracked_dirty["packet_fingerprint"].as_str().unwrap()
    );

    // Restoring the worktree returns to the exact clean packet.
    fs::remove_file(repo.path().join("untracked.txt")).unwrap();
    git(repo.path(), &["checkout", "--", "src/token.mjs"]);
    let restored = packet_ok(&repo, &ticket_id);
    assert_eq!(restored["source"]["dirty"], false);
    assert_eq!(restored["packet_fingerprint"].as_str().unwrap(), clean_fp);

    let head = repo.git_head();
    git(repo.path(), &["checkout", "--detach", &head]);
    let packet = packet_ok(&repo, &ticket_id);
    assert_eq!(packet["source"]["dirty"], false);

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
    let first_size = pulse::canonical_json::to_canonical_bytes(&first)
        .unwrap()
        .len();

    let cache = repo.path().join(".pulse/cache/docs-search");
    if cache.exists() {
        fs::remove_dir_all(&cache).unwrap();
    }
    let second = packet_ok(&repo, &ticket_id);
    assert_eq!(second["packet_fingerprint"].as_str().unwrap(), first_fp);
    assert_eq!(
        pulse::canonical_json::to_canonical_bytes(&second)
            .unwrap()
            .len(),
        first_size
    );
    let canonical = pulse::canonical_json::to_canonical_bytes(&second).unwrap();
    assert_eq!(canonical.len(), first_size);

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
    let first_suggestions = first["docs"]["suggested"].as_array().unwrap();
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
    let changed_suggestions = changed["docs"]["suggested"].as_array().unwrap();
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
fn packet_rejects_non_ignored_absent_operational_paths_before_dirtying_clean_repo() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    fs::write(repo.path().join(".gitignore"), b"node_modules/\n").unwrap();
    commit_all(repo.path(), "stop ignoring pulse operational paths");

    assert_packet_rejects_without_side_effects(&repo, &ticket_id);
}

#[test]
fn packet_rejects_tracked_operational_paths_even_when_status_is_clean() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    fs::create_dir_all(repo.path().join(".pulse/cache/docs-search")).unwrap();
    fs::create_dir_all(repo.path().join(".pulse/runtime/locks")).unwrap();
    fs::write(
        repo.path().join(".pulse/cache/docs-search/CURRENT"),
        b"gen_tracked\n",
    )
    .unwrap();
    fs::write(repo.path().join(".pulse/runtime/locks/workgraph.lock"), b"").unwrap();
    fs::write(
        repo.path().join(".pulse/runtime/locks/docs-search.lock"),
        b"",
    )
    .unwrap();
    git(
        repo.path(),
        &[
            "add",
            "-f",
            ".pulse/cache/docs-search/CURRENT",
            ".pulse/runtime/locks/workgraph.lock",
            ".pulse/runtime/locks/docs-search.lock",
        ],
    );
    commit_all(repo.path(), "track operational paths");
    assert!(repo.git_is_clean());

    assert_packet_rejects_without_side_effects(&repo, &ticket_id);
}

#[test]
fn packet_accepts_nested_gitignore_for_operational_paths() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    fs::write(repo.path().join(".gitignore"), b"node_modules/\n").unwrap();
    fs::write(repo.path().join(".pulse/.gitignore"), b"runtime/\ncache/\n").unwrap();
    commit_all(
        repo.path(),
        "ignore operational paths from nested gitignore",
    );

    let packet = packet_ok(&repo, &ticket_id);
    assert_eq!(packet["code"], "ready_ticket");
    assert!(repo.git_is_clean());
}

#[test]
fn packet_rejects_existing_non_ignored_operational_paths_before_dirtying_clean_repo() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    fs::write(repo.path().join(".gitignore"), b"node_modules/\n").unwrap();
    fs::create_dir_all(repo.path().join(".pulse/cache/docs-search")).unwrap();
    fs::write(
        repo.path().join(".pulse/cache/docs-search/CURRENT"),
        b"gen_untracked\n",
    )
    .unwrap();
    assert!(git_status(repo.path(), ".pulse/cache/docs-search/CURRENT").starts_with("??"));
    fs::write(
        repo.path().join(".gitignore"),
        b".pulse/cache/docs-search/CURRENT\nnode_modules/\n",
    )
    .unwrap();
    commit_all(
        repo.path(),
        "commit source clean before operational path rejection",
    );
    fs::write(repo.path().join(".gitignore"), b"node_modules/\n").unwrap();
    fs::remove_file(repo.path().join(".pulse/cache/docs-search/CURRENT")).unwrap();
    commit_all(repo.path(), "stop ignoring existing cache path");
    fs::create_dir_all(repo.path().join(".pulse/cache/docs-search")).unwrap();
    fs::write(
        repo.path().join(".pulse/cache/docs-search/CURRENT"),
        b"gen_untracked\n",
    )
    .unwrap();
    assert!(git_status(repo.path(), ".pulse/cache/docs-search/CURRENT").starts_with("??"));

    let before = tracked_snapshot(repo.path());
    let output = repo.pulse(&["work", "packet", &ticket_id, "--json"]);
    assert!(!output.status.success());
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["code"], "work_packet_operational_path_not_ignored");
    assert_eq!(before, tracked_snapshot(repo.path()));
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
                pulse::graph::model::contract::PublicCreateClassification {
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

#[test]
fn packet_injects_applicable_validated_knowledge_only() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);

    // A candidate learning must not appear in the packet.
    let capture_draft = repo.path().join("learning.json");
    fs::write(
        &capture_draft,
        serde_json::json!({
            "title": "Freeze the tree between handoff and close",
            "kind": "process_insight",
            "severity": "medium",
            "summary": "Out-of-scope edits after handoff stale the proof chain.",
            "guidance": {
                "do": ["Leave the worktree untouched until close."],
                "avoid": [],
                "required_checks": ["node scripts/verify.mjs"]
            },
            "applicability": {"paths": ["src/**"]},
            "provenance_targets": [],
            "source_commits": [],
            "routing": null,
            "promotion": null,
            "freshness": null,
            "trust": null,
            "content": null
        })
        .to_string(),
    )
    .unwrap();
    let created = repo.pulse_ok(&[
        "knowledge",
        "capture",
        "--from",
        &ticket_id,
        "--file",
        &capture_draft.to_string_lossy(),
        "--actor",
        "human:tester",
        "--json",
    ]);
    assert_eq!(created["value"]["status"], "candidate");
    let before = packet_ok(&repo, &ticket_id);
    assert!(before["knowledge"].as_array().unwrap().is_empty());

    // Validate it: now it applies via the src/** path match and injects.
    fs::create_dir_all(repo.path().join(".pulse/evidence/receipts")).unwrap();
    let receipt = serde_json::json!({
        "schema_version": 1,
        "receipt_version": 2,
        "id": "rcpt_01J00000000000000000000002",
        "kind": "qa_checkpoint",
        "result": "passed",
        "actor": {"kind": "human", "id": "tester"},
        "recorded_at": "2026-09-05T00:00:00Z",
        "subject": {"kind": "work", "id": ticket_id},
        "bindings": {},
        "payload": {
            "payload_version": 1,
            "qa_scope": "ticket_checkpoint",
            "story_id": "ST-000",
            "ticket_id": ticket_id,
            "baseline_revision": 1,
            "baseline_content_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "cases": [{"case_id": "QA-001", "case_revision": 1, "outcome": "passed"}],
            "executor": {"name": "t", "version": "1"},
            "observations": ["observed"]
        }
    });
    fs::write(
        repo.path()
            .join(".pulse/evidence/receipts/rcpt_01J00000000000000000000002.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
    repo.pulse_ok(&[
        "knowledge",
        "validate",
        "LRN-001",
        "--evidence",
        "rcpt_01J00000000000000000000002",
        "--actor",
        "human:tester",
        "--json",
    ]);
    let after = packet_ok(&repo, &ticket_id);
    let knowledge = after["knowledge"].as_array().unwrap();
    assert_eq!(knowledge.len(), 1);
    assert_eq!(knowledge[0]["detail_ref"], "LRN-001");
    assert!(knowledge[0]["why_applicable"]
        .as_str()
        .unwrap()
        .contains("src/**"));
    assert_eq!(knowledge[0]["required_checks"].as_array().unwrap().len(), 1);

    // Packet fingerprint must reflect the injected knowledge (stability).
    let again = packet_ok(&repo, &ticket_id);
    assert_eq!(again["packet_fingerprint"], after["packet_fingerprint"]);
}
