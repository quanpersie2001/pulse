//! S7-I4 CLI contract smoke test for `pulse work ready`. The library-depth
//! readiness/gate logic lives in `readiness.rs`; this file covers the CLI
//! wiring: stable JSON output, profile validation and the documented non-zero
//! gate exit for not-ready work.

use chrono::Utc;
use pulse::canonical_json::to_canonical_bytes;
use pulse::graph::model::contract::{
    Materialization, PublicCreateClassification, Risk, TicketRole,
};
use pulse::graph::model::node::NodeStatus;
use pulse::graph::store::OperationContext;
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
            kind: pulse::identity::actor::ActorKind::Human,
            id: "tester".to_string(),
            grants: sorted,
        }],
    };
    let path = repo.path().join(".pulse/policy/authority.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, to_canonical_bytes(&policy).unwrap()).unwrap();
}

/// Build a fully-ready ticket through the library so the CLI test focuses on the
/// `work ready` command surface. The ticket.md contract is bound via
/// `sync_ticket`, then the markdown-gated draft -> shaped -> ready transitions
/// run; no evidence receipts are involved.
fn ready_ticket(repo: &TempDir, store: &JsonGraphStore) -> String {
    write_policy(repo, &["work.transition.shaped", "work.transition.ready"]);
    let node = store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Ready ticket".to_string(),
            PublicCreateClassification {
                role: Some(TicketRole::Implementation),
                risk: Some(Risk::Low),
                materialization: Some(Materialization::R1),
            },
            OperationContext::default(),
        )
        .unwrap()
        .value;
    let brief_path = repo.path().join(format!("{}/ticket.md", node.content_dir));
    fs::create_dir_all(brief_path.parent().unwrap()).unwrap();
    fs::write(
        &brief_path,
        format!(
            "# {} Ready ticket\n\n\
             ## Objective\nShip the change.\n\n\
             ## Current behavior\nBehavior is missing.\n\n\
             ## Target behavior\nBehavior is present.\n\n\
             ## Code anchors\n- src/auth.rs\n\n\
             ## Required changes\n- Implement the behavior.\n\n\
             ## Invariants\n- Do not leak secrets.\n\n\
             ## Implementation freedom\nguided: agent chooses internal structure within this contract.\n\n\
             ## Acceptance\n- AC-1: Behavior is observable.\n\n\
             ## Verify\n- cargo test\n\n\
             ## Documentation impact\n- Posture: none\n- Rationale: No durable docs impact.\n- Documents:\n\n\
             ## QA impact\n- Owner:\n- Posture: none\n- Cases:\n- Reason: No product QA impact.\n",
            node.id
        ),
    )
    .unwrap();
    let node = store
        .sync_ticket_with_context(&node.id, node.revision, ctx())
        .unwrap()
        .value;
    let node = store
        .transition_node_with_context(&node.id, NodeStatus::Shaped, node.revision, None, ctx())
        .unwrap()
        .value;
    let ready = store
        .transition_node_with_context(&node.id, NodeStatus::Ready, node.revision, None, ctx())
        .unwrap()
        .value;
    ready.id
}

#[test]
fn work_ready_emits_stable_json_for_ready_ticket() {
    let repo = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(repo.path());
    let id = ready_ticket(&repo, &store);

    let output = run(&repo, &["work", "ready", &id, "--json"]);
    assert!(
        output.status.success(),
        "ready query failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["profile"], "contract_readiness");
    assert_eq!(report["status"], "ready");
    assert_eq!(report["transition_eligible"], true);
    assert_eq!(report["dispatch_authorized"], false);
    assert!(report["readiness_fingerprint"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert!(report["gate_families"].is_array());
    assert!(report["future_gate_families"].is_array());
}

#[test]
fn work_ready_returns_nonzero_for_not_ready_work() {
    let repo = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(repo.path());
    // A fresh draft ticket is not ready (no bound contract, QA unknown).
    let node = store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Draft".to_string(),
            PublicCreateClassification {
                role: Some(TicketRole::Implementation),
                risk: Some(Risk::Low),
                materialization: Some(Materialization::R1),
            },
            OperationContext::default(),
        )
        .unwrap()
        .value;

    let output = run(&repo, &["work", "ready", &node.id, "--json"]);
    assert!(
        !output.status.success(),
        "not-ready query should exit non-zero"
    );
    // The report is still rendered on stdout for CI/automation.
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_ne!(report["status"], "ready");
    // The error envelope is on stderr with a stable code.
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["code"], "readiness_not_ready");
}

#[test]
fn work_ready_rejects_unsupported_profile() {
    let repo = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(repo.path());
    let id = ready_ticket(&repo, &store);
    let output = run(
        &repo,
        &[
            "work",
            "ready",
            &id,
            "--profile",
            "unsupported_profile",
            "--json",
        ],
    );
    assert!(!output.status.success());
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(err["code"], "readiness_profile_unsupported");
}
