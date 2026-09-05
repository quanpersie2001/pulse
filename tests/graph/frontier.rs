//! Execution-frontier projection tests.
//!
//! Covers the execution frontier over a coherent snapshot: ready-only
//! membership, shaped-but-not-transitioned and stale-ready exclusion,
//! hard-blocker and soft-preference handling, the `--for` destination filter,
//! the explicit `claim_state=not_evaluated` boundary (no persisted claim),
//! deterministic priority-agnostic ID ordering and cache-independence. Also
//! covers the CLI contract: stable JSON, profile validation and
//! empty-frontier success.
//!
//! These tests exercise the harness against temporary target repositories only.
//! They never point Pulse at this development repository.

use chrono::Utc;
use pulse::graph::model::contract::{
    Materialization, PublicCreateClassification, Risk, TicketRole,
};
use pulse::graph::model::edge::EdgeType;
use pulse::graph::model::node::NodeStatus;
use pulse::graph::read::frontier::{ExecutionFrontierItem, ExecutionFrontierReport};
use pulse::graph::store::OperationContext;
use pulse::id::WorkKind;
use pulse::JsonGraphStore;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

use super::assignment_fixture::{setup_ready_ticket, write_policy};
use crate::common_bin::bin;
use crate::common_canon::write_json;

fn ctx() -> OperationContext {
    OperationContext {
        actor: "human:tester".to_string(),
        now: Utc::now(),
    }
}

/// Build a fully-ready implementation Ticket through the markdown-gated
/// fixture flow (`works/<id>/ticket.md` -> `sync_ticket` -> Shaped -> Ready).
fn ready_ticket(repo: &std::path::Path, store: &JsonGraphStore) -> String {
    write_policy(repo, &[]);
    setup_ready_ticket(repo, store)
}

/// Build an implementation Ticket whose contract satisfies every readiness
/// gate but whose lifecycle stops at `shaped` (no ready transition).
fn shaped_implementation_ticket(repo: &std::path::Path, store: &JsonGraphStore) -> String {
    write_policy(repo, &[]);
    let node = store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Shaped".to_string(),
            PublicCreateClassification {
                role: Some(TicketRole::Implementation),
                risk: Some(Risk::Low),
                materialization: Some(Materialization::R1),
            },
            ctx(),
        )
        .unwrap()
        .value;
    // Node creation writes the default ticket.md template; sync binds it and
    // derives the none-posture docs/QA metadata the readiness gates require.
    let node = store
        .sync_ticket_with_context(&node.id, node.revision, ctx())
        .unwrap()
        .value;
    store
        .transition_node_with_context(&node.id, NodeStatus::Shaped, node.revision, None, ctx())
        .unwrap();
    node.id
}

fn create_story(store: &JsonGraphStore, title: &str) -> pulse::graph::model::node::Node {
    store
        .create_node_public_with_context(
            WorkKind::Story,
            title.to_string(),
            PublicCreateClassification::default(),
            OperationContext::default(),
        )
        .unwrap()
        .value
}

fn execution_report(store: &JsonGraphStore, for_owner: Option<&str>) -> ExecutionFrontierReport {
    store.frontier(for_owner, None, true).unwrap()
}

fn exec_item_ids(items: &[ExecutionFrontierItem]) -> Vec<String> {
    items.iter().map(|i| i.id.clone()).collect()
}

fn excluded_by_id(report: &ExecutionFrontierReport) -> BTreeMap<String, Vec<String>> {
    report
        .excluded
        .iter()
        .map(|e| (e.id.clone(), e.reason_codes.clone()))
        .collect()
}

#[test]
fn execution_frontier_includes_only_current_ready_implementation_tickets() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let ready = ready_ticket(repo, &store);

    let report = execution_report(&store, None);
    assert_eq!(report.kind, "execution");
    assert_eq!(report.claim_state, "not_evaluated");
    assert!(!report.dispatch_authorized);
    assert_eq!(report.readiness_profile, "contract_readiness");
    assert_eq!(exec_item_ids(&report.items), vec![ready.clone()]);
    assert!(report.items[0].frontier_eligible);
    assert!(report.items[0].readiness_fingerprint.starts_with("sha256:"));
    assert_eq!(report.items[0].reason_codes, vec!["contract_ready"]);
}

#[test]
fn shaped_but_not_transitioned_ticket_is_not_in_execution_frontier() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let shaped = shaped_implementation_ticket(repo, &store);

    let report = execution_report(&store, None);
    assert!(report.items.is_empty(), "shaped ticket must not appear");
    // The shaped ticket is excluded with an explicit lifecycle reason.
    let excluded = excluded_by_id(&report);
    assert_eq!(
        excluded.get(&shaped).map(|codes| codes.first().cloned()),
        Some(Some("execution_not_transitioned".to_string()))
    );
}

#[test]
fn stale_ready_ticket_excluded_from_execution_frontier() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let ready = ready_ticket(repo, &store);

    // Mutate a readiness input (documentation metadata back to missing, i.e.
    // posture `unknown`) after the ready transition. Lifecycle stays `ready`
    // but the current readiness evaluation fails.
    let node = store.show_node(&ready).unwrap();
    let mut edited = node.clone();
    edited.documentation = None; // missing docs -> posture unknown -> readiness fails
    edited.revision = node.revision + 1;
    edited.updated_at = Utc::now();
    write_json(
        &repo
            .join(".pulse/workgraph/nodes")
            .join(format!("{}.json", node.id)),
        &edited,
    );

    let report = execution_report(&store, None);
    assert!(report.items.is_empty(), "stale-ready must not appear");
    let excluded = excluded_by_id(&report);
    let codes = excluded.get(&ready).expect("stale ticket excluded");
    assert!(
        codes.contains(&"ready_state_stale".to_string()),
        "expected ready_state_stale reason, got {codes:?}"
    );
}

#[test]
fn hard_blocker_excludes_from_execution_frontier() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let ready = ready_ticket(repo, &store);
    let blocker = store
        .create_node_public_with_context(
            WorkKind::Ticket,
            "Blocker".to_string(),
            PublicCreateClassification {
                role: Some(TicketRole::Implementation),
                risk: Some(Risk::Low),
                materialization: Some(Materialization::R0),
            },
            ctx(),
        )
        .unwrap()
        .value;
    // A `ready` ticket blocked by an open (draft) ticket must leave the
    // frontier: the current readiness evaluation catches the open hard
    // blocker and reports the ready node as stale.
    store
        .add_edge_with_context(
            EdgeType::BlockedBy,
            ready.clone(),
            blocker.id.clone(),
            ctx(),
        )
        .unwrap();

    let report = execution_report(&store, None);
    assert!(
        report.items.is_empty(),
        "hard-blocked ticket must not appear"
    );
    let excluded = excluded_by_id(&report);
    let codes = excluded.get(&ready).expect("blocked ticket excluded");
    assert!(
        codes.contains(&"ready_state_stale".to_string()),
        "{codes:?}"
    );
}

#[test]
fn soft_preference_keeps_execution_work_eligible() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let foundation = ready_ticket(repo, &store);
    let later = ready_ticket(repo, &store);
    // `later` preferred_after `foundation`: advisory only, not a blocker.
    store
        .add_edge_with_context(
            EdgeType::PreferredAfter,
            later.clone(),
            foundation.clone(),
            ctx(),
        )
        .unwrap();

    let report = execution_report(&store, None);
    let ids = exec_item_ids(&report.items);
    assert!(ids.contains(&foundation));
    assert!(ids.contains(&later));
}

#[test]
fn for_filter_excludes_other_destination_owners() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let owner_a = create_story(&store, "Owner A");
    let owner_b = create_story(&store, "Owner B");
    let ticket_b = ready_ticket(repo, &store);
    store
        .add_edge_with_context(
            EdgeType::Parent,
            ticket_b.clone(),
            owner_b.id.clone(),
            ctx(),
        )
        .unwrap();

    let for_a = execution_report(&store, Some(&owner_a.id));
    let for_b = execution_report(&store, Some(&owner_b.id));
    assert_eq!(for_a.for_.as_deref(), Some(owner_a.id.as_str()));
    assert!(
        for_a.items.is_empty(),
        "out-of-scope ticket must not appear"
    );
    assert_eq!(exec_item_ids(&for_b.items), vec![ticket_b.clone()]);
    let excluded = excluded_by_id(&for_a);
    let codes = excluded.get(&ticket_b).expect("out-of-scope excluded");
    assert!(
        codes.contains(&"execution_wrong_destination".to_string()),
        "{codes:?}"
    );
}

#[test]
fn claim_state_not_evaluated_and_not_persisted() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let _ready = ready_ticket(repo, &store);
    let fingerprint_before = store.export().unwrap().graph_fingerprint;

    let execution = execution_report(&store, None);
    assert_eq!(execution.claim_state, "not_evaluated");

    // A read-only projection must not change the graph and must not persist any
    // frontier/claim state.
    let fingerprint_after = store.export().unwrap().graph_fingerprint;
    assert_eq!(fingerprint_before, fingerprint_after);
    let events_dir = repo.join(".pulse/events");
    let mut pred = |v: &Value| {
        v.get("event_type")
            .and_then(|t| t.as_str())
            .map(|t| t.contains("frontier") || t.contains("claim"))
            .unwrap_or(false)
    };
    let has_frontier_event = walk_json(&events_dir, &mut pred);
    assert!(!has_frontier_event, "frontier must not emit events");
    assert!(
        !repo.join(".pulse/frontiers").exists(),
        "frontier must not persist a frontier store"
    );
}

fn walk_json(dir: &std::path::Path, pred: &mut dyn FnMut(&Value) -> bool) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if walk_json(&path, pred) {
                return true;
            }
        } else if let Ok(bytes) = fs::read(&path) {
            if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                if pred(&v) {
                    return true;
                }
            }
        }
    }
    false
}

#[test]
fn deterministic_id_ordering_is_priority_agnostic() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    // Allocate several ready tickets; allocation is monotonic so later
    // creations get lexicographically larger IDs. Membership order must follow
    // ID, independent of creation intent.
    let mut ids = Vec::new();
    for _ in 0..3 {
        ids.push(ready_ticket(repo, &store));
    }
    let mut sorted = ids.clone();
    sorted.sort();

    let report = execution_report(&store, None);
    assert_eq!(exec_item_ids(&report.items), sorted);

    // Re-running yields identical, stable output.
    let again = execution_report(&store, None);
    assert_eq!(again.items, report.items);
}

#[test]
fn cache_corruption_or_deletion_rebuilds_equivalent_semantics() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let store = JsonGraphStore::new(repo);
    let _ready = ready_ticket(repo, &store);

    let baseline_execution = execution_report(&store, None);

    // Corrupt then delete the disposable projection cache; semantics must not
    // depend on it.
    let cache = repo.join(".pulse/cache/workgraph.snapshot.json");
    if cache.exists() {
        fs::write(&cache, b"not json").unwrap();
    }
    let after_corrupt = execution_report(&store, None);
    assert_eq!(after_corrupt, baseline_execution);
    let _ = fs::remove_file(&cache);
    let after_delete_execution = execution_report(&store, None);
    assert_eq!(after_delete_execution, baseline_execution);
}

#[test]
fn empty_frontier_is_success() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(tmp.path());
    let report = store.frontier(None, None, false).unwrap();
    assert_eq!(report.kind, "execution");
    assert_eq!(report.claim_state, "not_evaluated");
    assert!(report.items.is_empty());
    assert!(report.excluded.is_empty());
}

#[test]
fn execution_frontier_rejects_unsupported_profile() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(tmp.path());
    let err = store
        .frontier(None, Some("unsupported_profile"), true)
        .unwrap_err();
    assert_eq!(err.code(), "readiness_profile_unsupported");
}

#[test]
fn frontier_rejects_invalid_destination_owner() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(tmp.path());
    // A Ticket id is not a valid destination owner.
    let err = store.frontier(Some("TK-999"), None, true).unwrap_err();
    assert_eq!(err.code(), "frontier_destination_invalid");
    // A well-formed but missing owner id is NotFound.
    let err = store.frontier(Some("ST-999"), None, true).unwrap_err();
    assert_eq!(err.code(), "not_found");
}

// ---------------------------------------------------------------------------
// CLI contract
// ---------------------------------------------------------------------------

fn run(repo: &TempDir, args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args(args)
        .output()
        .expect("run pulse")
}

#[test]
fn cli_execution_frontier_emits_stable_json() {
    let repo = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(repo.path());
    let ready = ready_ticket(repo.path(), &store);

    let output = run(&repo, &["work", "frontier", "--json"]);
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["kind"], "execution");
    assert_eq!(report["code"], "execution_frontier");
    assert_eq!(report["claim_state"], "not_evaluated");
    assert_eq!(report["dispatch_authorized"], false);
    assert_eq!(report["readiness_profile"], "contract_readiness");
    assert_eq!(report["items"][0]["id"], ready);
    assert_eq!(report["items"][0]["frontier_eligible"], true);
}

#[test]
fn cli_empty_frontier_succeeds() {
    let repo = tempfile::tempdir().unwrap();
    let output = run(&repo, &["work", "frontier", "--json"]);
    assert!(output.status.success(), "empty frontier should succeed");
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["items"].as_array().unwrap().is_empty());
}

#[test]
fn cli_include_excluded_populates_excluded() {
    let repo = tempfile::tempdir().unwrap();
    let store = JsonGraphStore::new(repo.path());
    let owner_a = create_story(&store, "A");
    let owner_b = create_story(&store, "B");
    let ticket_b = ready_ticket(repo.path(), &store);
    store
        .add_edge_with_context(
            EdgeType::Parent,
            ticket_b.clone(),
            owner_b.id.clone(),
            ctx(),
        )
        .unwrap();

    // Without --include-excluded: excluded is empty.
    let out = run(&repo, &["work", "frontier", "--for", &owner_a.id, "--json"]);
    assert!(out.status.success());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(report["excluded"].as_array().unwrap().is_empty());

    // With --include-excluded: the other-destination ticket appears.
    let out = run(
        &repo,
        &[
            "work",
            "frontier",
            "--for",
            &owner_a.id,
            "--include-excluded",
            "--json",
        ],
    );
    assert!(out.status.success());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    let excluded = report["excluded"].as_array().unwrap();
    assert!(!excluded.is_empty());
    assert!(excluded.iter().any(|e| e["reason_codes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c == "execution_wrong_destination")));
}

#[test]
fn cli_rejects_unsupported_profile() {
    let repo = tempfile::tempdir().unwrap();
    let out = run(
        &repo,
        &[
            "work",
            "frontier",
            "--profile",
            "unsupported_profile",
            "--json",
        ],
    );
    assert!(!out.status.success());
    let err: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(err["code"], "readiness_profile_unsupported");
}
