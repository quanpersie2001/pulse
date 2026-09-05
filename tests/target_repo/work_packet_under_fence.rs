//! P2S2-I6: Fence-aware packet revalidation integration tests.
//!
//! Verifies that the public `work_packet` method still builds identical
//! packets after the I6 phase extraction refactoring, and that preview
//! semantics remain unchanged.
//!
//! The `work_packet_under_fence` internal method is `pub(crate)` and tested
//! in `src/kernel/packet.rs`'s in-module tests.

use pulse::canonical_json::to_canonical_bytes;
use pulse::graph::model::node::NodeStatus;
use pulse::graph::store::OperationContext;
use pulse::identity::ActorKind;
use pulse::policy::{AuthorityPolicy, AuthorityPrincipal};
use pulse::storage::transaction::{
    persist_multi_target_intent, FileState, MultiTargetTransactionIntent, TransactionTarget,
};
use pulse::JsonGraphStore;
use serde_json::json;
use std::fs;
use std::path::Path;

use crate::common::fixture_repo::TestRepo;

fn ctx() -> OperationContext {
    OperationContext {
        actor: "human:tester".to_string(),
        now: chrono::Utc::now(),
    }
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

#[test]
fn public_work_packet_builds_after_refactoring() -> TestResult {
    // Use the existing setup from the target_repo test fixture. We rely on
    // `work_packet_target_repo` tests to validate the full setup/contract;
    // here we just verify the public API still works after the I6 phase
    // extraction refactoring.
    let repo = TestRepo::from_fixture("minimal-service");

    // Run a full CLI setup via the existing test pattern: bootstrap, create
    // ticket, write ticket.md, sync, transition to ready.
    let ticket_id = test_setup_ready_ticket(&repo)?;

    let store = JsonGraphStore::new(repo.path());
    let packet = store.work_packet(&ticket_id)?;

    // Verify preview semantics (P2S2-D1)
    assert_eq!(packet.code, "ready_ticket");
    assert_eq!(packet.schema_version, 1);
    assert_eq!(packet.profile, "work_packet");
    assert!(!packet.packet_fingerprint.is_empty());
    assert_eq!(packet.ticket.node.id, ticket_id);
    assert_eq!(packet.ticket.node.role, "implementation");
    assert_eq!(packet.ticket.node.status, "ready");
    Ok(())
}

#[test]
fn two_packets_from_same_state_produce_identical_canonical_bytes() -> TestResult {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = test_setup_ready_ticket(&repo)?;

    let store = JsonGraphStore::new(repo.path());
    let p1 = store.work_packet(&ticket_id)?;
    let p2 = store.work_packet(&ticket_id)?;

    let canonical1 = to_canonical_bytes(&p1)?;
    let canonical2 = to_canonical_bytes(&p2)?;
    assert_eq!(
        canonical1, canonical2,
        "two work_packet calls must produce identical canonical bytes"
    );
    Ok(())
}

#[test]
fn packet_identity_observation_does_not_recover_until_packet_fence() -> TestResult {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = test_setup_ready_ticket(&repo)?;
    let root = repo.path();

    let mut pending = Vec::new();
    for (name, relative) in [
        ("docs", ".pulse/docs/registry.json"),
        ("evidence", ".pulse/evidence/manifest.json"),
    ] {
        let path = root.join(relative);
        let bytes = fs::read(&path)?;
        let target = TransactionTarget::new(
            path,
            FileState::Present {
                hash: pulse::canonical_json::hash_bytes(&bytes),
                revision: 1,
            },
            FileState::Present {
                hash: pulse::canonical_json::hash_bytes(&bytes),
                revision: 1,
            },
            &bytes,
        );
        let intent = MultiTargetTransactionIntent::prepared(
            format!("evt_packet_{name}"),
            format!("{name}.test"),
            "test",
            vec![target],
            root.join(format!(".pulse/events/packet-{name}.json")),
            json!({"event": format!("packet-{name}")}),
        )?;
        let intent_path = persist_multi_target_intent(root, &intent)?;
        pending.push(intent_path);
    }

    let docs_before = fs::read(root.join(".pulse/docs/registry.json"))?;
    pulse::source::check_repository_identity(root)?;
    assert!(pending.iter().all(|path| path.exists()));
    assert_eq!(
        docs_before,
        fs::read(root.join(".pulse/docs/registry.json"))?
    );

    JsonGraphStore::new(root).work_packet(&ticket_id)?;

    assert!(pending.iter().all(|path| !path.exists()));
    Ok(())
}

// -----------------------------------------------------------------------
// Test setup helper (minimal ready ticket for packet tests)
// -----------------------------------------------------------------------

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn test_error(message: impl Into<String>) -> std::io::Error {
    std::io::Error::other(message.into())
}

/// The Ticket contract bound by the packet: `works/<id>/ticket.md`.
/// Sections mirror the assignment fixture so the markdown ambiguity gate
/// passes for an R1 implementation Ticket.
fn ticket_markdown(ticket_id: &str) -> String {
    format!(
        "# {ticket_id} Implement fence-aware packet revalidation\n\
         \n\
         ## Objective\nExtract and validate the fence-aware packet builder.\n\
         \n\
         ## Current behavior\nThe packet builder phases are not fence-aware.\n\
         \n\
         ## Target behavior\nThe packet builder revalidates under the fence.\n\
         \n\
         ## Code anchors\n- src/kernel/packet.rs\n\
         \n\
         ## Required changes\n- Extract the fence-aware phase builder.\n\
         \n\
         ## Invariants\n- Public packet bytes remain unchanged.\n\
         \n\
         ## Implementation freedom\nguided: agent chooses internal structure within this contract.\n\
         \n\
         ## Acceptance\n- AC-1: Fence-aware packet builder works.\n\
         \n\
         ## Verify\n- cargo test\n\
         \n\
         ## Documentation impact\n- Posture: none\n- Rationale: No durable docs impact.\n- Documents:\n\
         \n\
         ## QA impact\n- Owner:\n- Posture: none\n- Cases:\n- Reason: No product QA impact.\n"
    )
}

fn test_setup_ready_ticket(repo: &TestRepo) -> TestResult<String> {
    let root = repo.path();
    let store = JsonGraphStore::new(root);

    // Policy grants
    write_policy(root, &["work.transition.shaped", "work.transition.ready"])?;

    // Bootstrap graph
    let bootstrap = repo.pulse_ok(&["graph", "bootstrap", "--json"]);
    assert_eq!(bootstrap["code"], "bootstrapped");

    // Bootstrap evidence + docs manifests
    pulse::evidence::manifest::load(root)?;
    pulse::docs::manifest::bootstrap(root)?;

    // Create Ticket via CLI
    let created = repo.pulse_ok(&[
        "work",
        "create",
        "--kind",
        "ticket",
        "--title",
        "Implement fence-aware packet revalidation",
        "--role",
        "implementation",
        "--risk",
        "low",
        "--materialization",
        "R1",
        "--json",
    ]);
    let ticket_id = created["value"]["id"]
        .as_str()
        .ok_or_else(|| test_error("created ticket id missing"))?
        .to_string();

    let node = store.show_node(&ticket_id)?;

    // The Ticket contract is `works/<id>/ticket.md` bound by `brief_hash`:
    // write the markdown, then let `sync` parse it, bump the contract
    // revision, and derive docs/QA metadata.
    let brief_path = root.join(&node.content_dir).join("ticket.md");
    fs::create_dir_all(
        brief_path
            .parent()
            .ok_or_else(|| test_error("brief path has no parent"))?,
    )?;
    fs::write(&brief_path, ticket_markdown(&ticket_id))?;

    store.sync_ticket_with_context(&ticket_id, node.revision, ctx())?;

    // Markdown ambiguity gate: transition through Shaped to Ready.
    for status in [NodeStatus::Shaped, NodeStatus::Ready] {
        let revision = store.show_node(&ticket_id)?.revision;
        store.transition_node_with_context(&ticket_id, status, revision, None, ctx())?;
    }

    // Commit so the worktree is clean for the packet source check.
    let add = std::process::Command::new("git")
        .current_dir(root)
        .args(["add", "."])
        .output()?;
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
            "setup ready ticket for I6",
        ])
        .output()?;
    assert!(
        commit.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&commit.stderr)
    );

    Ok(ticket_id)
}

fn write_policy(root: &Path, grants: &[&str]) -> TestResult {
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
    fs::create_dir_all(
        policy_path
            .parent()
            .ok_or_else(|| test_error("policy path has no parent"))?,
    )?;
    fs::write(&policy_path, to_canonical_bytes(&policy)?)?;
    Ok(())
}
