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

fn run_err(repo: &TempDir, args: &[&str]) -> Value {
    let output = run(repo, args);
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stderr).expect("json stderr")
}

#[test]
fn l31_transition_json_output_and_error_contracts_are_stable() {
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

    let ok = run_ok(
        &repo,
        &[
            "work",
            "transition",
            &id,
            "--to",
            "cancelled",
            "--expected-revision",
            "1",
            "--actor",
            "test:actor",
            "--reason-code",
            "obsolete",
            "--reason",
            "No longer needed",
            "--reference",
            "DEC-001",
            "--json",
        ],
    );
    assert_eq!(ok["schema_version"], 1);
    assert_eq!(ok["code"], "transitioned");
    assert_eq!(ok["value"]["status"], "cancelled");
    assert_eq!(ok["value"]["status_reason"]["code"], "obsolete");

    let stale = run_err(
        &repo,
        &[
            "work",
            "transition",
            &id,
            "--to",
            "blocked",
            "--expected-revision",
            "1",
            "--actor",
            "test:actor",
            "--reason-code",
            "blocked",
            "--reason",
            "Blocked",
            "--json",
        ],
    );
    assert_eq!(stale["schema_version"], 1);
    assert_eq!(stale["code"], "cas_conflict");
    assert_eq!(stale["subject"], id);
    assert_eq!(stale["expected_revision"], 1);
    assert_eq!(stale["current_revision"], 2);
}

#[test]
fn cli_missing_reason_fails_before_commit() {
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
    let id = created["value"]["id"].as_str().unwrap();
    let err = run_err(
        &repo,
        &[
            "work",
            "transition",
            id,
            "--to",
            "cancelled",
            "--expected-revision",
            "1",
            "--actor",
            "test:actor",
            "--json",
        ],
    );
    assert_eq!(err["code"], "missing_status_reason");

    let shown = run_ok(&repo, &["work", "show", id, "--json"]);
    assert_eq!(shown["node"]["status"], "draft");
    assert_eq!(shown["node"]["revision"], 1);
}

#[test]
fn cli_supersede_requires_receipt_and_rejects_inline_assertion() {
    let repo = tempfile::tempdir().unwrap();
    let old = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Old",
            "--role",
            "implementation",
            "--risk",
            "low",
            "--materialization",
            "R0",
            "--json",
        ],
    );
    let old_id = old["value"]["id"].as_str().unwrap().to_string();
    let replacement = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "story",
            "--title",
            "Replacement",
            "--json",
        ],
    );
    let replacement_id = replacement["value"]["id"].as_str().unwrap().to_string();

    let missing = run_err(
        &repo,
        &[
            "work",
            "supersede",
            &old_id,
            "--by",
            &replacement_id,
            "--expected-revision",
            "1",
            "--reason",
            "absorbed",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(missing["code"], "supersession_receipt_required");

    let assertion = repo.path().join("assertion.json");
    std::fs::write(
        &assertion,
        r#"{
  "assertion_version": 1,
  "asserted_by": "human:test",
  "source_revisions": ["TK-001@1"],
  "claim": "absorbed",
  "references": ["ST-001"]
}"#,
    )
    .unwrap();
    let inline = run_err(
        &repo,
        &[
            "work",
            "supersede",
            &old_id,
            "--by",
            &replacement_id,
            "--expected-revision",
            "1",
            "--reason",
            "absorbed",
            "--assertion",
            assertion.to_str().unwrap(),
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(inline["code"], "inline_supersession_assertion_unsupported");

    let shown = run_ok(&repo, &["work", "show", &old_id, "--json"]);
    assert_eq!(shown["node"]["status"], "draft");
    assert_eq!(shown["node"]["revision"], 1);
}

#[test]
fn public_cli_ticket_create_requires_explicit_assessed_classification() {
    let repo = tempfile::tempdir().unwrap();

    let missing = run_err(
        &repo,
        &[
            "work", "create", "--kind", "ticket", "--title", "Missing", "--json",
        ],
    );
    assert_eq!(missing["code"], "work_classification_missing");

    let unassessed = run_err(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Unassessed",
            "--role",
            "implementation",
            "--risk",
            "unassessed",
            "--materialization",
            "R0",
            "--json",
        ],
    );
    assert_eq!(unassessed["code"], "risk_materialization_unassessed");

    let created = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Assessed",
            "--role",
            "decision_work",
            "--risk",
            "medium",
            "--materialization",
            "R2",
            "--json",
        ],
    );
    assert_eq!(created["code"], "created");
    assert_eq!(created["value"]["role"], "decision_work");
    assert_eq!(created["value"]["risk"], "medium");
    assert_eq!(created["value"]["materialization"], "R2");
    assert!(created["value"].get("decision_work").is_none());
}

#[test]
fn cli_ready_to_active_public_transition_requires_reservation_activation() {
    let repo = tempfile::tempdir().unwrap();
    let created = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "ReadyTicket",
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

    // Manually set the node to Ready (bypass readiness gate for test setup).
    let node_path = repo
        .path()
        .join(format!(".pulse/workgraph/nodes/{id}.json"));
    let mut node: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&node_path).unwrap()).unwrap();
    node["status"] = serde_json::json!("ready");
    node["revision"] = serde_json::json!(2u64);
    std::fs::write(&node_path, serde_json::to_vec_pretty(&node).unwrap()).unwrap();

    // Now call the CLI with --expected-revision 2 (matching the manual bump).
    let err = run_err(
        &repo,
        &[
            "work",
            "transition",
            &id,
            "--to",
            "active",
            "--expected-revision",
            "2",
            "--actor",
            "test:actor",
            "--json",
        ],
    );
    assert_eq!(
        err["code"], "reservation_activation_required",
        "CLI transition Ready->Active must report reservation_activation_required not {:?}",
        err["code"]
    );

    // The node must remain unchanged.
    let shown = run_ok(&repo, &["work", "show", &id, "--json"]);
    assert_eq!(shown["node"]["status"], "ready");
    assert_eq!(shown["node"]["revision"], 2);
}

#[test]
fn public_cli_rejects_classification_flags_for_non_tickets() {
    let repo = tempfile::tempdir().unwrap();
    let err = run_err(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "story",
            "--title",
            "Story",
            "--role",
            "implementation",
            "--risk",
            "low",
            "--materialization",
            "R0",
            "--json",
        ],
    );
    assert_eq!(err["code"], "work_classification_not_allowed");
}

/// `pulse work list` is the query surface that exists so an agent never greps
/// the graph files (PRODUCT §5.1). Every filter it documents must work, and
/// they must compose: the new-session recovery path asks "what is active
/// here?", and an unknown flag would send the agent back to grepping.
#[test]
fn work_list_filters_by_status_role_and_tag() {
    let repo = tempfile::tempdir().unwrap();
    run_ok(&repo, &["init", "--actor", "human:tester", "--json"]);

    let ticket = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Implementation ticket",
            "--role",
            "implementation",
            "--risk",
            "low",
            "--tag",
            "security",
            "--json",
        ],
    )["value"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let spike = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Decision work ticket",
            "--role",
            "decision_work",
            "--risk",
            "low",
            "--json",
        ],
    )["value"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let story = run_ok(
        &repo,
        &[
            "work", "create", "--kind", "story", "--title", "A story", "--json",
        ],
    )["value"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let ids = |value: &Value| -> Vec<String> {
        value["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_str().unwrap().to_string())
            .collect()
    };

    // No filter: everything.
    let all = ids(&run_ok(&repo, &["work", "list", "--json"]));
    assert_eq!(all.len(), 3, "unfiltered list returns every node: {all:?}");

    // Status: all three are draft, none is active. An empty result is a real
    // answer here, not an error.
    let draft = ids(&run_ok(
        &repo,
        &["work", "list", "--status", "draft", "--json"],
    ));
    assert_eq!(draft.len(), 3);
    let active = ids(&run_ok(
        &repo,
        &["work", "list", "--status", "active", "--json"],
    ));
    assert!(active.is_empty(), "nothing is active yet: {active:?}");

    // Role: a set role excludes nodes that carry none, the Story included.
    let implementation = ids(&run_ok(
        &repo,
        &["work", "list", "--role", "implementation", "--json"],
    ));
    assert_eq!(implementation, vec![ticket.clone()]);
    let decision_work = ids(&run_ok(
        &repo,
        &["work", "list", "--role", "decision_work", "--json"],
    ));
    assert_eq!(decision_work, vec![spike.clone()]);
    assert!(!implementation.contains(&story) && !decision_work.contains(&story));

    // Tag.
    let tagged = ids(&run_ok(
        &repo,
        &["work", "list", "--tag", "security", "--json"],
    ));
    assert_eq!(tagged, vec![ticket.clone()]);

    // Filters compose with AND, and a contradiction yields an empty list.
    let composed = ids(&run_ok(
        &repo,
        &[
            "work",
            "list",
            "--kind",
            "ticket",
            "--status",
            "draft",
            "--role",
            "implementation",
            "--tag",
            "security",
            "--json",
        ],
    ));
    assert_eq!(composed, vec![ticket]);
    let contradiction = ids(&run_ok(
        &repo,
        &[
            "work",
            "list",
            "--role",
            "decision_work",
            "--tag",
            "security",
            "--json",
        ],
    ));
    assert!(contradiction.is_empty(), "{contradiction:?}");

    // An unknown tag is an empty answer, not a failure.
    let unknown = ids(&run_ok(
        &repo,
        &["work", "list", "--tag", "no-such-tag", "--json"],
    ));
    assert!(unknown.is_empty());
}

/// A pre-graph prose draft under `works/_drafts/<slug>/` is invisible to the
/// graph (Decision 0021).
///
/// `grill` and `spec` write there before any node exists, so `graph validate`
/// and `work list` must keep treating the directory as ordinary prose: the
/// `works/<id>` content_dir rule constrains a node's own directory, not what
/// else may live beside it.
#[test]
fn a_pre_graph_prose_draft_is_not_a_node_and_does_not_fail_validation() {
    let repo = tempfile::tempdir().unwrap();
    let created = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Adopted later",
            "--role",
            "implementation",
            "--risk",
            "low",
            "--materialization",
            "R0",
            "--json",
        ],
    );
    let ticket = created["value"]["id"].as_str().unwrap().to_string();

    let draft = repo
        .path()
        .join("works")
        .join("_drafts")
        .join("schedule-crud");
    std::fs::create_dir_all(draft.join("research")).unwrap();
    for (name, body) in [
        ("story.md", "# Story\n\nAdmins manage export schedules.\n"),
        ("approach.md", "# Approach\n\nReuse the SecretStore seam.\n"),
        ("qa.md", "# QA\n\n### QA-011 Admin creates a schedule\n"),
    ] {
        std::fs::write(draft.join(name), body).unwrap();
    }
    std::fs::write(
        draft.join("research").join("size-ceiling.md"),
        "# Finding\n",
    )
    .unwrap();

    let report = run_ok(&repo, &["graph", "validate", "--json"]);
    assert_eq!(
        report["valid"], true,
        "a prose draft beside works/<id> must not invalidate the graph: {report}"
    );

    let listed = run_ok(&repo, &["work", "list", "--json"]);
    let listed: Vec<String> = listed["items"]
        .as_array()
        .expect("items array")
        .iter()
        .map(|item| item["id"].as_str().expect("id").to_string())
        .collect();
    assert_eq!(
        listed,
        vec![ticket],
        "the draft directory must not appear as a work item"
    );

    // The draft survives graph reads untouched: adoption is the only thing that
    // moves it, and it must stay readable until `work sync` has bound the prose.
    assert!(draft.join("story.md").exists());
}

/// `qa baseline` resolves only after the draft has been adopted (Decision 0021).
///
/// This is what forces adoption to be copy-then-sync-then-baseline rather than
/// any other order: the baseline loader reads `works/<story-id>/qa.md`, so a
/// `qa.md` still sitting in the draft is not a baseline yet.
#[test]
fn qa_baseline_resolves_only_after_the_draft_qa_is_adopted() {
    let repo = tempfile::tempdir().unwrap();
    let created = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "story",
            "--title",
            "Admins manage schedules",
            "--json",
        ],
    );
    let story = created["value"]["id"].as_str().unwrap().to_string();

    let baseline = format!(
        r#"# {story} Schedule management QA

## Scope
Schedule management stays observable to an admin.

## Posture
automated

## Risks
- RISK-LIMIT: a fourth active schedule slips past the limit.

## Exit criteria
- The required case passes on the candidate source.

## Cases

### QA-011 Admin creates a schedule
- Intent: A created schedule appears in the list.
- Surface: api
- Priority: critical
- Risks: RISK-LIMIT
- Steps:
  1. create a schedule as an admin
- Expected:
  - the schedule appears in the list
"#
    );

    // Still in the draft: the Story owns no baseline yet.
    let draft = repo
        .path()
        .join("works")
        .join("_drafts")
        .join("schedule-crud");
    std::fs::create_dir_all(&draft).unwrap();
    std::fs::write(draft.join("qa.md"), &baseline).unwrap();
    let error = run_err(&repo, &["qa", "baseline", &story, "--json"]);
    assert!(
        error["code"].is_string(),
        "a draft-only baseline must fail with a coded error: {error}"
    );

    // Adopted to the node path: the same bytes now resolve.
    let adopted = repo.path().join("works").join(&story);
    std::fs::create_dir_all(&adopted).unwrap();
    std::fs::write(adopted.join("qa.md"), &baseline).unwrap();
    let report = run_ok(&repo, &["qa", "baseline", &story, "--json"]);
    assert_eq!(report["owner_id"], story);
    assert_eq!(
        report["cases"].as_array().expect("cases").len(),
        1,
        "adopted baseline must expose its case: {report}"
    );
}
