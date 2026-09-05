//! P2S1-I6: Target-repository integration tests for `pulse work packet`.
//!
//! Every scenario uses `TestRepo::from_fixture("minimal-service")` to create
//! an isolated working copy.  Never run Pulse against the development
//! repository or tracked fixture in place.

use chrono::Utc;
use pulse::canonical_json::to_canonical_bytes;
use pulse::graph::model::node::NodeStatus;
use pulse::graph::store::OperationContext;
use pulse::identity::actor::ActorKind;
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

/// Commit all pending changes so the worktree is clean for the packet
/// command's source check.
fn commit_all(root: &std::path::Path, message: &str) {
    let add = Command::new("git")
        .current_dir(root)
        .args(["add", "."])
        .output()
        .expect("git add");
    assert!(add.status.success(), "git add failed");
    let commit = Command::new("git")
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
            message,
        ])
        .output()
        .expect("git commit");
    assert!(
        commit.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&commit.stderr)
    );
}

/// The Ticket contract bound by the packet: `works/<id>/ticket.md`.
/// Sections mirror the assignment fixture so the markdown ambiguity gate
/// passes for an R1 implementation Ticket.
fn ticket_markdown(ticket_id: &str) -> String {
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
         ## Documentation impact\n- Posture: none\n- Rationale: No durable docs impact.\n- Documents:\n\
         \n\
         ## QA impact\n- Owner:\n- Posture: none\n- Cases:\n- Reason: No product QA impact.\n"
    )
}

/// Set up a ready implementation Ticket, returning the ticket ID.  The CLI is
/// used for create/bootstrap; the Ticket contract itself is the
/// `works/<id>/ticket.md` markdown bound by `sync`.
fn setup_ready_ticket(repo: &TestRepo) -> String {
    let root = repo.path();
    let store = JsonGraphStore::new(root);

    write_policy(root, &["work.transition.shaped", "work.transition.ready"]);

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
    let node = store
        .show_node(&ticket_id)
        .expect("ticket should exist after CLI create");

    // Write the markdown contract and bind it with `sync`: `sync` parses the
    // brief, bumps the contract revision on hash change, and derives docs/QA
    // metadata.
    let brief_path = root.join(&node.content_dir).join("ticket.md");
    fs::create_dir_all(brief_path.parent().unwrap()).unwrap();
    fs::write(&brief_path, ticket_markdown(&ticket_id)).unwrap();

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

    // Commit so the worktree is clean for the packet source check.
    commit_all(root, "setup ready ticket");

    ticket_id
}

// -----------------------------------------------------------------------
// A. Happy path — target-repo produces the full current packet schema
// -----------------------------------------------------------------------

#[test]
fn target_repo_happy_path_work_packet() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);

    // Use CLI to build the packet.
    let packet_value = repo.pulse_ok(&["work", "packet", &ticket_id, "--json"]);
    assert_eq!(packet_value["schema_version"], 1);
    assert_eq!(packet_value["profile"], "work_packet");
    assert_eq!(packet_value["code"], "ready_ticket");

    // Ticket node and raw contract bind exact revision and status. The ready
    // flow consumes at least create + sync + shaped + ready revisions.
    assert_eq!(packet_value["ticket"]["node"]["id"], ticket_id);
    assert_eq!(packet_value["ticket"]["node"]["status"], "ready");
    assert!(packet_value["ticket"]["node"]["revision"].as_u64().unwrap() >= 4);
    assert!(packet_value["ticket"]["ticket_md"]["content"].is_string());

    // Source binds exact clean HEAD; operational state is not packet content.
    let head = repo.git_head();
    assert_eq!(packet_value["source"]["commit"], head);
    assert_eq!(packet_value["source"]["dirty"], false);
    assert!(packet_value["packet_fingerprint"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert!(packet_value["knowledge"].as_array().unwrap().is_empty());
    assert!(packet_value["notes"].as_array().unwrap().is_empty());
    assert!(packet_value["rework"].as_array().unwrap().is_empty());
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

    // Compare top-level entries. Allowed additions when the target repository
    // explicitly ignores operational state:
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
    commit_all(repo.path(), "enroll target repo");
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
