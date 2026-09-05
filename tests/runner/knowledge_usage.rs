//! Learning usage feedback: `work handoff --learning-used` records how the
//! worker used packet-injected learnings, Pulse derives `injected` from the
//! committed packet, and `knowledge show` aggregates the outcome counts.

use std::fs;

use serde_json::json;

use crate::cli_run::{
    install_worker_script, run_outcome, set_worker_command, setup_ready_ticket, ACTOR,
};
use crate::common_fixture_repo::TestRepo;
use crate::common_git::commit_all;

#[test]
fn handoff_records_usage_and_knowledge_show_aggregates_it() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);

    // LRN-001 is a validated repository learning whose path matches the
    // ticket anchor, so the committed packet really injects it.
    let shown = repo.pulse_ok(&["work", "show", &ticket_id, "--json"]);
    let revision = shown["node"]["revision"].as_u64().unwrap();
    let draft = json!({
        "title": "Repository lesson",
        "kind": "failure_pattern",
        "severity": "medium",
        "summary": "Repository-scope lesson summary.",
        "guidance": {
            "do": ["Follow the lesson."],
            "avoid": [],
            "required_checks": []
        },
        "applicability": {"paths": ["src/**"]},
        "provenance_targets": [{
            "relation": "derived_from",
            "kind": "work",
            "id": ticket_id,
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
    let draft_file = repo.path().join("learning.json");
    fs::write(&draft_file, serde_json::to_string_pretty(&draft).unwrap()).unwrap();
    repo.pulse_ok(&[
        "knowledge",
        "create",
        "--file",
        &draft_file.display().to_string(),
        "--actor",
        ACTOR,
        "--json",
    ]);
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
            "baseline_revision": 1,
            "baseline_content_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "cases": [{"case_id": "QA-001", "case_revision": 1, "outcome": "passed"}],
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
    // LRN-002 is a harness learning: never injected into packets by design.
    let mut harness_draft = draft.clone();
    harness_draft["title"] = json!("Harness lesson");
    harness_draft["scope"] = json!("harness");
    let harness_file = repo.path().join("learning-harness.json");
    fs::write(
        &harness_file,
        serde_json::to_string_pretty(&harness_draft).unwrap(),
    )
    .unwrap();
    repo.pulse_ok(&[
        "knowledge",
        "create",
        "--file",
        &harness_file.display().to_string(),
        "--actor",
        ACTOR,
        "--json",
    ]);
    for learning_id in ["LRN-001", "LRN-002"] {
        repo.pulse_ok(&[
            "knowledge",
            "validate-learning",
            learning_id,
            "--evidence",
            "rcpt_01J00000000000000000000001",
            "--actor",
            ACTOR,
            "--json",
        ]);
    }
    commit_all(repo.path());

    // The worker reports: LRN-001 was helpful, LRN-002 was never needed.
    install_worker_script(
        &repo,
        r#"
RUN_DIR="$(dirname "$1")"
. "$RUN_DIR/worker-env"
"$PULSE" work handoff --lease "$LEASE_ID" --session "$SESSION_ID" \
  --source-commit "$SOURCE_COMMIT" --summary "did the work" \
  --learning-used LRN-001=helpful \
  --learning-used LRN-002=not_needed \
  --idempotency-key handoff-1 --json
echo '{"status": "handed_off", "summary": "done"}'
"#,
    );
    set_worker_command(&repo, "sh scripts/fake-worker.sh {input}");
    let outcome = run_outcome(&repo, &ticket_id);
    assert_eq!(outcome["status"], "handed_off");

    let handoff_files: Vec<_> =
        fs::read_dir(repo.path().join(".pulse/evidence/execution/handoffs"))
            .unwrap()
            .collect();
    assert_eq!(handoff_files.len(), 1);
    let handoff: serde_json::Value =
        serde_json::from_slice(&fs::read(handoff_files[0].as_ref().unwrap().path()).unwrap())
            .unwrap();
    let usage = handoff["knowledge_usage"].as_array().unwrap();
    assert_eq!(usage.len(), 2);
    assert_eq!(usage[0]["learning_id"], "LRN-001");
    assert_eq!(usage[0]["injected"], true);
    assert_eq!(usage[0]["applied"], true);
    assert_eq!(usage[0]["outcome"], "helpful");
    // LRN-999 does not exist, let alone appear in the packet: Pulse records
    // the claim but marks injected false.
    assert_eq!(usage[1]["learning_id"], "LRN-002");
    assert_eq!(usage[1]["injected"], false);
    assert_eq!(usage[1]["applied"], false);
    assert_eq!(usage[1]["outcome"], "not_needed");

    let show = repo.pulse_ok(&["knowledge", "show", "LRN-001", "--json"]);
    assert_eq!(show["usage"]["helpful"], 1);
    assert_eq!(show["usage"]["not_needed"], 0);
    assert_eq!(show["usage"]["misleading"], 0);
    let show_harness = repo.pulse_ok(&["knowledge", "show", "LRN-002", "--json"]);
    assert_eq!(show_harness["usage"]["not_needed"], 1);
}
