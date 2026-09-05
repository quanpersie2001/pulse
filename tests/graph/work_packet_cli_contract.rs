//! P2S1-I6: CLI contract tests for `pulse work packet`.
//!
//! Covers stable JSON output, error codes, human rendering, and schema
//! validation. The kernel/builder tests in `kernel::packet::tests` cover the
//! library-depth packet construction; this file covers the CLI wiring.

use chrono::Utc;
use pulse::canonical_json::to_canonical_bytes;
use pulse::graph::model::contract::{Materialization, Risk, TicketRole};
use pulse::graph::model::node::NodeStatus;
use pulse::graph::store::OperationContext;
use pulse::id::WorkKind;
use pulse::identity::actor::ActorKind;
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
    // Create a .gitignore that excludes .pulse/ operational state so bootstrap
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

/// The Ticket contract bound by the packet: `works/<id>/ticket.md`.
/// Sections mirror the assignment fixture so the markdown ambiguity gate
/// passes for an R1 implementation Ticket.
fn ticket_markdown(ticket_id: &str) -> String {
    format!(
        "# {ticket_id} Rotate refresh tokens\n\
         \n\
         ## Objective\nRotate refresh tokens atomically.\n\
         \n\
         ## Current behavior\nTokens are long-lived without rotation.\n\
         \n\
         ## Target behavior\nRefresh tokens rotate on each use atomically.\n\
         \n\
         ## Code anchors\n- src/auth.rs\n\
         \n\
         ## Required changes\n- Add rotation logic to the auth module.\n\
         \n\
         ## Invariants\n- Concurrent rotation must be serialized.\n\
         \n\
         ## Implementation freedom\nguided: agent chooses internal structure within this contract.\n\
         \n\
         ## Acceptance\n- AC-1: Tokens rotate without race.\n\
         \n\
         ## Verify\n- cargo test\n\
         \n\
         ## Documentation impact\n- Posture: none\n- Rationale: No durable docs impact.\n- Documents:\n\
         \n\
         ## QA impact\n- Owner:\n- Posture: none\n- Cases:\n- Reason: No product QA impact.\n"
    )
}

/// Build a fully-ready implementation Ticket through the library, returning
/// the ticket ID.  The repo must have been initialized with `init_git_repo`
/// and `bootstrap_repo` already.
fn ready_ticket(repo: &TempDir, store: &JsonGraphStore) -> String {
    write_policy(repo, &["work.transition.shaped", "work.transition.ready"]);
    let node = store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Rotate refresh tokens".to_string(),
            pulse::graph::model::contract::PublicCreateClassification {
                role: Some(TicketRole::Implementation),
                risk: Some(Risk::Low),
                materialization: Some(Materialization::R1),
            },
            ctx(),
        )
        .unwrap()
        .value;
    // The Ticket contract is `works/<id>/ticket.md` bound by `brief_hash`:
    // write the markdown, then let `sync` parse it, bump the contract
    // revision, and derive docs/QA metadata.
    let brief_path = repo.path().join(&node.content_dir).join("ticket.md");
    fs::create_dir_all(brief_path.parent().unwrap()).unwrap();
    fs::write(&brief_path, ticket_markdown(&node.id)).unwrap();
    store
        .sync_ticket_with_context(&node.id, node.revision, ctx())
        .unwrap();
    let node = store
        .transition_node_with_context(
            &node.id,
            NodeStatus::Shaped,
            store.show_node(&node.id).unwrap().revision,
            None,
            ctx(),
        )
        .unwrap()
        .value;
    let ready = store
        .transition_node_with_context(
            &node.id,
            NodeStatus::Ready,
            store.show_node(&node.id).unwrap().revision,
            None,
            ctx(),
        )
        .unwrap()
        .value;
    // Commit the content files written during setup so the worktree is clean
    // for the packet source check.
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
    assert_eq!(packet["profile"], "work_packet");
    assert_eq!(packet["code"], "ready_ticket");
    let typed: pulse::work_packet::WorkPacket = serde_json::from_value(packet.clone()).unwrap();
    typed.validate_schema_contract().unwrap();

    // Ticket node
    assert_eq!(packet["ticket"]["node"]["kind"], "ticket");
    assert_eq!(packet["ticket"]["node"]["role"], "implementation");
    assert_eq!(packet["ticket"]["node"]["status"], "ready");

    // Source and future knowledge/notes sections are explicit and bounded.
    assert!(!packet["source"]["dirty"].as_bool().unwrap());
    assert!(packet["knowledge"].as_array().unwrap().is_empty());
    assert!(packet["notes"].as_array().unwrap().is_empty());
    assert!(packet["handoff"]["commands"].is_array());

    // Packet fingerprint present
    assert!(packet["packet_fingerprint"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));

    // Context includes ticket content, parents, and decisions.
    assert!(packet["ticket"]["ticket_md"]["content"].as_str().is_some());
    assert!(packet["parents"].is_array());
    assert!(packet["decisions"].is_array());
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
        text.contains("ready_ticket"),
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
        text.contains("required docs:"),
        "human output should state docs"
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
            pulse::graph::model::contract::PublicCreateClassification {
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
            pulse::graph::model::contract::PublicCreateClassification {
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
            pulse::graph::model::contract::PublicCreateClassification {
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
