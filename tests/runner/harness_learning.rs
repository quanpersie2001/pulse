//! Scope-aware knowledge injection.
//!
//! Repository learnings inject into packets by path/tag; harness learnings
//! (about operating Pulse itself) never inject into packets — their
//! injection point is the runner bootstrap prompt's `## Harness learnings`
//! section.

use std::fs;

use serde_json::{json, Value};

use crate::cli_run::{
    install_worker_script, run_outcome, set_worker_command, setup_ready_ticket, ACTOR,
};
use crate::common_fixture_repo::TestRepo;
use crate::common_git::commit_all;

fn write_json(repo: &TestRepo, name: &str, value: &Value) -> String {
    let path = repo.path().join(name);
    fs::write(&path, serde_json::to_string_pretty(value).unwrap()).unwrap();
    path.display().to_string()
}

fn draft(work_id: &str, revision: u64, title: &str, summary: &str, scope: Option<&str>) -> Value {
    let mut value = json!({
        "title": title,
        "kind": "failure_pattern",
        "severity": "medium",
        "summary": summary,
        "guidance": {
            "do": ["Follow the lesson."],
            "avoid": [],
            "required_checks": ["Check the harness lesson was applied."]
        },
        "applicability": {"paths": ["src/**"]},
        "provenance_targets": [{
            "relation": "derived_from",
            "kind": "work",
            "id": work_id,
            "revision": revision,
            "content_hash": null
        }],
        "source_commits": [],
        "routing": null,
        "promotion": null,
        "freshness": null,
        "trust": null,
        "content": null
    });
    if let Some(scope) = scope {
        value["scope"] = json!(scope);
    }
    value
}

#[test]
fn harness_learnings_inject_into_the_worker_prompt_not_the_packet() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);

    let shown = repo.pulse_ok(&["work", "show", &ticket_id, "--json"]);
    let revision = shown["node"]["revision"].as_u64().unwrap();
    let repo_draft = write_json(
        &repo,
        "learning-repository.json",
        &draft(
            &ticket_id,
            revision,
            "Repository lesson",
            "Repository-scope lesson summary marker.",
            None,
        ),
    );
    let harness_draft = write_json(
        &repo,
        "learning-harness.json",
        &draft(
            &ticket_id,
            revision,
            "Harness lesson",
            "Harness-scope lesson summary marker.",
            Some("harness"),
        ),
    );
    repo.pulse_ok(&[
        "knowledge",
        "create",
        "--file",
        &repo_draft,
        "--actor",
        ACTOR,
        "--json",
    ]);
    repo.pulse_ok(&[
        "knowledge",
        "create",
        "--file",
        &harness_draft,
        "--actor",
        ACTOR,
        "--json",
    ]);

    // Both learnings validate against a resolvable evidence receipt.
    let receipt = json!({
        "schema_version": 1,
        "receipt_version": 2,
        "id": "rcpt_01J00000000000000000000001",
        "kind": "qa_checkpoint",
        "result": "passed",
        "actor": {"kind": "human", "id": "tester"},
        "recorded_at": "2026-09-06T00:00:00Z",
        "subject": {"kind": "work", "id": ticket_id},
        "bindings": {},
        "payload": {
            "payload_version": 1,
            "qa_scope": "ticket_checkpoint",
            "story_id": "ST-000",
            "ticket_id": ticket_id,
            "baseline_content_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "cases": [{"case_id": "QA-001", "case_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000", "outcome": "passed"}],
            "executor": {"name": "t", "version": "1"},
            "observations": ["observed"]
        }
    });
    fs::create_dir_all(repo.path().join(".pulse/evidence/receipts")).unwrap();
    fs::write(
        repo.path()
            .join(".pulse/evidence/receipts/rcpt_01J00000000000000000000001.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
    for learning_id in ["LRN-001", "LRN-002"] {
        repo.pulse_ok(&[
            "knowledge",
            "validate",
            learning_id,
            "--evidence",
            "rcpt_01J00000000000000000000001",
            "--actor",
            ACTOR,
            "--json",
        ]);
    }
    commit_all(repo.path());

    // Run the (fake) worker: the packet and bootstrap prompt get committed.
    install_worker_script(&repo, r#"echo '{"status": "blocked", "reason": "probe"}'"#);
    set_worker_command(&repo, "sh scripts/fake-worker.sh {input}");
    let outcome = run_outcome(&repo, &ticket_id);
    assert_eq!(outcome["status"], "blocked");

    let run_dir = repo.path().join(".pulse/runtime/run").join(&ticket_id);
    let packet: serde_json::Value =
        serde_json::from_slice(&fs::read(run_dir.join("worker-input.json")).unwrap()).unwrap();
    let injected = packet["knowledge"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["detail_ref"].as_str().unwrap_or(""))
        .collect::<Vec<_>>();
    assert!(
        injected.contains(&"LRN-001"),
        "repository learning must inject into the packet: {injected:?}"
    );
    assert!(
        !injected.contains(&"LRN-002"),
        "harness learning must never inject into the packet: {injected:?}"
    );

    let prompt = fs::read_to_string(run_dir.join("worker-prompt.md")).unwrap();
    assert!(prompt.contains("## Harness learnings"), "prompt: {prompt}");
    assert!(prompt.contains("Harness-scope lesson summary marker."));
    assert!(
        !prompt.contains("Repository-scope lesson summary marker."),
        "repository learnings stay in the packet, not the prompt"
    );
}
