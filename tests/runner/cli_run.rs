//! CLI integration tests for `pulse run <role> --ticket <id>` and the
//! `work handoff` / `work verify` proof surface.
//!
//! Every run executes a repo-local fake worker script instead of a real
//! agent; the script uses the public CLI to record proofs exactly like a
//! real agent would. Assertions cover lifecycle gating, lease handling,
//! outcome classification (handed_off / blocked / inconclusive families),
//! runner-actor provisioning and the runners.json bootstrap.

use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::common_bin::bin;
use crate::common_fixture_repo::TestRepo;
use crate::common_git::commit_all;

pub(crate) const ACTOR: &str = "human:tester";

fn ticket_markdown(ticket_id: &str) -> String {
    format!(
        "# {ticket_id} Classify token failures\n\n\
         ## Objective\nSplit expired and invalid token outcomes.\n\n\
         ## Current behavior\nBoth outcomes map to InvalidToken.\n\n\
         ## Target behavior\nExpired maps to TokenExpired; invalid stays InvalidToken.\n\n\
         ## Code anchors\n- src/token.mjs\n\n\
         ## Required changes\n- Add the expired branch.\n\n\
         ## Invariants\n- Public envelope shape is stable.\n\n\
         ## Implementation freedom\nguided: agent chooses internal structure.\n\n\
         ## Acceptance\n- AC-1: Expired tokens return TokenExpired.\n\n\
         ## Verify\n- node scripts/verify.mjs\n\n\
         ## Documentation impact\n- Posture: none\n- Rationale: No durable docs impact.\n- Documents:\n\n\
         ## QA impact\n- Owner:\n- Posture: none\n- Cases:\n- Reason: No product QA impact.\n"
    )
}

/// Bring a fixture repo to a `ready` implementation Ticket through the
/// markdown contract, returning the Ticket ID.
pub(crate) fn setup_ready_ticket(repo: &TestRepo) -> String {
    repo.pulse_ok(&["init", "--actor", ACTOR, "--json"]);
    let created = repo.pulse_ok(&[
        "work",
        "create",
        "--kind",
        "ticket",
        "--title",
        "Classify token failures",
        "--risk",
        "low",
        "--materialization",
        "R1",
        "--json",
    ]);
    let ticket_id = created["value"]["id"].as_str().unwrap().to_string();
    let revision = created["value"]["revision"].as_u64().unwrap();
    fs::write(
        repo.path().join("works").join(&ticket_id).join("ticket.md"),
        ticket_markdown(&ticket_id),
    )
    .unwrap();
    repo.pulse_ok(&[
        "work",
        "sync",
        &ticket_id,
        "--expected-revision",
        &revision.to_string(),
        "--actor",
        ACTOR,
        "--json",
    ]);
    let shown = repo.pulse_ok(&["work", "show", &ticket_id, "--json"]);
    let revision = shown["node"]["revision"].as_u64().unwrap();
    repo.pulse_ok(&[
        "work",
        "transition",
        &ticket_id,
        "--to",
        "shaped",
        "--expected-revision",
        &revision.to_string(),
        "--actor",
        ACTOR,
        "--json",
    ]);
    let shown = repo.pulse_ok(&["work", "show", &ticket_id, "--json"]);
    let revision = shown["node"]["revision"].as_u64().unwrap();
    repo.pulse_ok(&[
        "work",
        "transition",
        &ticket_id,
        "--to",
        "ready",
        "--expected-revision",
        &revision.to_string(),
        "--actor",
        ACTOR,
        "--json",
    ]);
    commit_all(repo.path());
    ticket_id
}

pub(crate) fn install_worker_script(repo: &TestRepo, body: &str) {
    let script = format!(
        "#!/bin/sh\nset -e\nREPO_ROOT=\"$(pwd)\"\nPULSE=\"{}\"\n{body}\n",
        bin()
    );
    let path = repo.path().join("scripts/fake-worker.sh");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, script).unwrap();
    commit_all(repo.path());
}

pub(crate) fn set_worker_command(repo: &TestRepo, command: &str) {
    let config = serde_json::json!({
        "worker": {"command": command, "timeout_seconds": 30},
        "reviewer": {"command": "echo '{\"ok\":true}'", "timeout_seconds": 60},
        "qa": {"command": "echo '{\"cases\":[]}'", "timeout_seconds": 60},
    });
    let pretty = serde_json::to_string_pretty(&config).unwrap();
    fs::write(repo.path().join(".pulse/config/runners.json"), pretty).unwrap();
    commit_all(repo.path());
}

/// Re-point exactly one role command, leaving the other roles untouched.
pub(crate) fn set_command(repo: &TestRepo, role: &str, command: &str) {
    let path = repo.path().join(".pulse/config/runners.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config[role]["command"] = Value::String(command.to_string());
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
    commit_all(repo.path());
}

fn error_code(output: &std::process::Output) -> String {
    assert!(
        !output.status.success(),
        "expected failure: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    err["code"].as_str().unwrap().to_string()
}

pub(crate) fn run_outcome(repo: &TestRepo, ticket_id: &str) -> Value {
    repo.pulse_ok(&["run", "worker", "--ticket", ticket_id, "--json"])
}

pub(crate) fn node_status(repo: &TestRepo, ticket_id: &str) -> String {
    let shown = repo.pulse_ok(&["work", "show", ticket_id, "--json"]);
    shown["node"]["status"].as_str().unwrap().to_string()
}

// ---------------------------------------------------------------------------
// init bootstrap
// ---------------------------------------------------------------------------

#[test]
fn init_bootstraps_runners_json_and_preserves_edits() {
    let repo = TestRepo::from_fixture("minimal-service");
    let first = repo.pulse_ok(&["init", "--actor", ACTOR, "--json"]);
    assert!(first["created"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry == ".pulse/config/runners.json"));
    let config: Value =
        serde_json::from_slice(&fs::read(repo.path().join(".pulse/config/runners.json")).unwrap())
            .unwrap();
    assert!(config["worker"]["command"].is_string());
    assert!(config["reviewer"]["timeout_seconds"].is_u64());
    assert!(config["qa"]["max_output_bytes"].is_null());

    // Edits survive re-init.
    set_worker_command(&repo, "echo worker");
    repo.pulse_ok(&["init", "--actor", ACTOR, "--json"]);
    let config: Value =
        serde_json::from_slice(&fs::read(repo.path().join(".pulse/config/runners.json")).unwrap())
            .unwrap();
    assert_eq!(config["worker"]["command"], "echo worker");
}

// ---------------------------------------------------------------------------
// worker happy path
// ---------------------------------------------------------------------------

#[test]
fn worker_run_hands_off_and_moves_ticket_to_verifying() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    install_worker_script(
        &repo,
        r#"
RUN_DIR="$(dirname "$1")"
. "$RUN_DIR/worker-env"
"$PULSE" work handoff --lease "$LEASE_ID" --session "$SESSION_ID" \
  --source-commit "$SOURCE_COMMIT" --summary "did the work" \
  --idempotency-key handoff-1 --json
echo '{"status": "handed_off", "summary": "done"}'
"#,
    );
    set_worker_command(&repo, "sh scripts/fake-worker.sh {input}");

    let outcome = run_outcome(&repo, &ticket_id);
    assert_eq!(outcome["status"], "handed_off");
    assert_eq!(outcome["code"], "run_handed_off");
    let lease_id = outcome["lease_id"].as_str().unwrap();
    assert!(lease_id.starts_with("lease_"));
    assert_eq!(node_status(&repo, &ticket_id), "verifying");

    // Handoff proof is bound to the lease.
    let handoffs = fs::read_dir(repo.path().join(".pulse/evidence/execution/handoffs"))
        .unwrap()
        .count();
    assert_eq!(handoffs, 1);

    // Event recorded with the outcome.
    let events = walk_events(&repo.path().join(".pulse/events"));
    assert!(events
        .iter()
        .any(|event| event["event_type"] == "run.completed"
            && event["payload"]["status"] == "handed_off"));

    // Runner actor provisioned with narrow grants.
    let policy: Value = serde_json::from_slice(
        &fs::read(repo.path().join(".pulse/policy/authority.json")).unwrap(),
    )
    .unwrap();
    let runner = policy["principals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|principal| principal["id"] == "runner:worker")
        .expect("runner:worker principal provisioned");
    let grants: Vec<&str> = runner["grants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|grant| grant.as_str().unwrap())
        .collect();
    assert!(grants.contains(&"work.assignment.handoff"));
    assert!(grants.contains(&"work.assignment.prepare"));
    assert!(!grants.contains(&"work.close"));
    assert!(!grants.contains(&"work.assignment.verify"));
}

fn walk_events(dir: &Path) -> Vec<Value> {
    // `dir` is always `<repo>/.pulse/events`; the shared reader owns the layout.
    let repo_root = dir.parent().and_then(Path::parent).expect("events dir");
    crate::common_events::read_events(repo_root)
        .iter()
        .map(|event| serde_json::to_value(event).expect("event to value"))
        .collect()
}

// ---------------------------------------------------------------------------
// inconclusive families
// ---------------------------------------------------------------------------

#[test]
fn worker_run_with_malformed_output_is_inconclusive_and_keeps_lease() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    install_worker_script(&repo, "echo 'not json'");
    set_worker_command(&repo, "sh scripts/fake-worker.sh {input}");

    let outcome = run_outcome(&repo, &ticket_id);
    assert_eq!(outcome["status"], "inconclusive");
    assert_eq!(outcome["inconclusive_reason"], "malformed_output");
    assert_eq!(node_status(&repo, &ticket_id), "active");
}

#[test]
fn worker_run_with_nonzero_exit_is_inconclusive() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    install_worker_script(&repo, "echo boom >&2\nexit 3");
    set_worker_command(&repo, "sh scripts/fake-worker.sh {input}");

    let outcome = run_outcome(&repo, &ticket_id);
    assert_eq!(outcome["status"], "inconclusive");
    assert_eq!(outcome["inconclusive_reason"], "exit_nonzero");
    assert_eq!(node_status(&repo, &ticket_id), "active");
}

#[test]
fn worker_run_claiming_handoff_without_proof_is_inconclusive() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    install_worker_script(
        &repo,
        "echo '{\"status\": \"handed_off\", \"summary\": \"lied\"}'",
    );
    set_worker_command(&repo, "sh scripts/fake-worker.sh {input}");

    let outcome = run_outcome(&repo, &ticket_id);
    assert_eq!(outcome["status"], "inconclusive");
    assert_eq!(outcome["inconclusive_reason"], "unproven_claim");
    assert_eq!(node_status(&repo, &ticket_id), "active");
}

#[test]
fn worker_run_timeout_is_inconclusive() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    install_worker_script(&repo, "sleep 30");
    let config = serde_json::json!({
        "worker": {"command": "sh scripts/fake-worker.sh {input}", "timeout_seconds": 1},
    });
    fs::write(
        repo.path().join(".pulse/config/runners.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .unwrap();

    let outcome = run_outcome(&repo, &ticket_id);
    assert_eq!(outcome["status"], "inconclusive");
    assert_eq!(outcome["inconclusive_reason"], "timeout");
    assert_eq!(node_status(&repo, &ticket_id), "active");
}

#[test]
fn worker_blocked_run_keeps_ticket_active() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    install_worker_script(
        &repo,
        "echo '{\"status\": \"blocked\", \"reason\": \"missing decision DEC-1\"}'",
    );
    set_worker_command(&repo, "sh scripts/fake-worker.sh {input}");

    let outcome = run_outcome(&repo, &ticket_id);
    assert_eq!(outcome["status"], "blocked");
    assert_eq!(outcome["summary"], "missing decision DEC-1");
    assert_eq!(node_status(&repo, &ticket_id), "active");
}

// ---------------------------------------------------------------------------
// lifecycle gating and config errors
// ---------------------------------------------------------------------------

#[test]
fn run_rejects_worker_on_non_ready_ticket_and_missing_config() {
    let repo = TestRepo::from_fixture("minimal-service");
    repo.pulse_ok(&["init", "--actor", ACTOR, "--json"]);
    let created = repo.pulse_ok(&[
        "work",
        "create",
        "--kind",
        "ticket",
        "--title",
        "Draft ticket",
        "--risk",
        "low",
        "--json",
    ]);
    let ticket_id = created["value"]["id"].as_str().unwrap().to_string();

    let output = repo.pulse(&["run", "worker", "--ticket", &ticket_id, "--json"]);
    assert_eq!(error_code(&output), "run_ticket_not_ready");

    fs::remove_file(repo.path().join(".pulse/config/runners.json")).unwrap();
    let output = repo.pulse(&["run", "worker", "--ticket", &ticket_id, "--json"]);
    assert_eq!(error_code(&output), "run_config_missing");

    let output = repo.pulse(&["run", "audit", "--ticket", &ticket_id, "--json"]);
    assert_eq!(error_code(&output), "run_role_lifecycle_unsupported");
}

#[test]
fn reviewer_and_qa_roles_require_verifying_tickets() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);

    let output = repo.pulse(&["run", "reviewer", "--ticket", &ticket_id, "--json"]);
    assert_eq!(error_code(&output), "run_ticket_not_verifying");
    let output = repo.pulse(&["run", "qa", "--ticket", &ticket_id, "--json"]);
    assert_eq!(error_code(&output), "run_ticket_not_verifying");
}

#[test]
fn qa_role_runs_after_worker_handoff_with_baseline_input() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    install_worker_script(
        &repo,
        r#"
RUN_DIR="$(dirname "$1")"
. "$RUN_DIR/worker-env"
"$PULSE" work handoff --lease "$LEASE_ID" --session "$SESSION_ID" \
  --source-commit "$SOURCE_COMMIT" --summary "did the work" \
  --idempotency-key handoff-1 --json
echo '{"status": "handed_off", "summary": "done"}'
"#,
    );
    set_worker_command(&repo, "sh scripts/fake-worker.sh {input}");
    let outcome = run_outcome(&repo, &ticket_id);
    assert_eq!(outcome["status"], "handed_off");

    // The default qa command just echoes JSON; the run must classify it as
    // completed and leave the verifying status untouched.
    let out = repo.pulse_ok(&["run", "qa", "--ticket", &ticket_id, "--json"]);
    assert_eq!(out["status"], "completed");
    assert_eq!(node_status(&repo, &ticket_id), "verifying");

    let qa_input: Value = serde_json::from_slice(
        &fs::read(
            repo.path()
                .join(".pulse/runtime/run")
                .join(&ticket_id)
                .join("qa-input.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(qa_input["schema_version"], 1);
    assert_eq!(qa_input["ticket_id"], ticket_id.as_str());
    assert!(qa_input["source_commit"].is_string());
}

// ---------------------------------------------------------------------------
// bootstrap prompt and reviewer input contract
// ---------------------------------------------------------------------------

#[test]
fn worker_run_writes_exact_handoff_syntax_into_bootstrap_prompt() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    install_worker_script(
        &repo,
        r#"echo '{"status": "blocked", "reason": "not started"}'"#,
    );
    set_worker_command(&repo, "sh scripts/fake-worker.sh {input}");

    let outcome = run_outcome(&repo, &ticket_id);
    assert_eq!(outcome["status"], "blocked");
    let lease_id = outcome["lease_id"].as_str().unwrap().to_string();

    let run_dir = repo.path().join(".pulse/runtime/run").join(&ticket_id);
    let prompt = fs::read_to_string(run_dir.join("worker-prompt.md")).unwrap();
    // The exact, runnable handoff command with this run's facts filled in.
    assert!(prompt.contains(&format!(
        "pulse --idempotency-key handoff:{ticket_id}:{lease_id}-a1 work handoff"
    )));
    assert!(prompt.contains(&format!("--lease {lease_id}")));
    assert!(prompt.contains("--session "));
    assert!(prompt.contains("--source-commit "));
    assert!(prompt.contains("agent:runner:worker"));
    // The old broken forms must not come back.
    assert!(!prompt.contains(&format!("work handoff {ticket_id}")));
    assert!(!prompt.contains("--actor runner:worker"));
    // Decision 0013 §4: the context-exhausted exit is spelled out.
    assert!(prompt.contains("context_exhausted"));
    assert!(prompt.contains(
        "--work {ticket_id}"
            .replace("{ticket_id}", &ticket_id)
            .as_str()
    ));
}

#[test]
fn reviewer_run_input_carries_the_verifying_handoff_and_prompt_syntax() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    install_worker_script(
        &repo,
        r#"
RUN_DIR="$(dirname "$1")"
. "$RUN_DIR/worker-env"
"$PULSE" work handoff --lease "$LEASE_ID" --session "$SESSION_ID" \
  --source-commit "$SOURCE_COMMIT" --summary "did the work" \
  --idempotency-key handoff-1 --json
echo '{"status": "handed_off", "summary": "done"}'
"#,
    );
    set_worker_command(&repo, "sh scripts/fake-worker.sh {input}");
    let outcome = run_outcome(&repo, &ticket_id);
    assert_eq!(outcome["status"], "handed_off");

    let handoffs = fs::read_dir(repo.path().join(".pulse/evidence/execution/handoffs"))
        .unwrap()
        .count();
    assert_eq!(handoffs, 1);

    install_worker_script(
        &repo,
        r#"echo '{"disposition": "pass", "acceptance": {"AC-1": "re-ran the verify command"}, "findings": []}'"#,
    );
    set_command(&repo, "reviewer", "sh scripts/fake-worker.sh {input}");
    let out = repo.pulse_ok(&["run", "reviewer", "--ticket", &ticket_id, "--json"]);
    // A well-formed claim without a recorded verification receipt stays
    // inconclusive: the runner never takes the JSON at face value.
    assert_eq!(out["status"], "inconclusive");
    assert_eq!(out["inconclusive_reason"], "unproven_claim");

    let run_dir = repo.path().join(".pulse/runtime/run").join(&ticket_id);
    let input: Value =
        serde_json::from_slice(&fs::read(run_dir.join("reviewer-input.json")).unwrap()).unwrap();
    assert_eq!(input["ticket_id"], ticket_id.as_str());
    assert!(input["contract_revision"].is_u64());
    assert_eq!(input["reviewers_required"], 1);
    let handoffs = input["handoffs"].as_array().unwrap();
    assert_eq!(handoffs.len(), 1);
    assert!(handoffs[0]["handoff_id"]
        .as_str()
        .unwrap()
        .starts_with("handoff_"));
    assert_eq!(handoffs[0]["recorded_by"], "agent:runner:worker");
    assert_eq!(handoffs[0]["changed_paths"].as_array().unwrap().len(), 0);
    // Claims travel even when this worker recorded none; prose never does.
    assert!(handoffs[0]["checks"].is_array());
    assert!(handoffs[0]["acceptance_proofs"].is_array());
    assert!(handoffs[0].get("summary").is_none());
    assert!(!input["acceptance"].as_array().unwrap().is_empty());

    let prompt = fs::read_to_string(run_dir.join("reviewer-prompt.md")).unwrap();
    assert!(prompt.contains(&format!("verify:{ticket_id}:<handoff_id>-a1 work verify")));
    assert!(prompt.contains("--actor agent:runner:reviewer"));
    assert!(prompt.contains("--disposition passed"));
    // There is no worker summary left to distrust (Decision 0012 §2).
    assert!(!prompt.contains("Do not trust"));
}

/// Decision 0012: `work handoff --check/--proof` records the worker's claim
/// as machine-readable fields. A proof referencing an acceptance id the
/// contract does not define is still recorded — coverage is the reviewer's
/// close-gate duty, not the worker's.
#[test]
fn handoff_records_claims_without_enforcing_acceptance_coverage() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    install_worker_script(
        &repo,
        r#"
RUN_DIR="$(dirname "$1")"
. "$RUN_DIR/worker-env"
"$PULSE" work handoff --lease "$LEASE_ID" --session "$SESSION_ID" \
  --source-commit "$SOURCE_COMMIT" --summary "did the work" \
  --check "focused=node scripts/verify.mjs=0" \
  --proof "AC-DOES-NOT-EXIST=focused=" \
  --idempotency-key handoff-1 --json
echo '{"status": "handed_off", "summary": "done"}'
"#,
    );
    set_worker_command(&repo, "sh scripts/fake-worker.sh {input}");

    let outcome = run_outcome(&repo, &ticket_id);
    assert_eq!(outcome["status"], "handed_off");

    let mut entries = fs::read_dir(repo.path().join(".pulse/evidence/execution/handoffs")).unwrap();
    let receipt: Value =
        serde_json::from_slice(&fs::read(entries.next().unwrap().unwrap().path()).unwrap())
            .unwrap();
    assert_eq!(receipt["checks"][0]["name"], "focused");
    assert_eq!(receipt["checks"][0]["exit_code"], 0);
    assert_eq!(
        receipt["acceptance_proofs"][0]["acceptance_id"],
        "AC-DOES-NOT-EXIST"
    );
}

/// The one-line summary cap pushes AC mapping and check results into the
/// structured claim fields instead of prose.
#[test]
fn handoff_rejects_a_summary_longer_than_300_chars() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    install_worker_script(
        &repo,
        r#"
RUN_DIR="$(dirname "$1")"
. "$RUN_DIR/worker-env"
LONG=$(awk 'BEGIN{s="";for(i=0;i<301;i++)s=s "x";print s}')
if "$PULSE" work handoff --lease "$LEASE_ID" --session "$SESSION_ID" \
  --source-commit "$SOURCE_COMMIT" --summary "$LONG" \
  --idempotency-key handoff-long --json 2>/dev/null; then
  echo '{"status": "handed_off", "summary": "lied"}'
  exit 0
fi
echo '{"status": "blocked", "reason": "summary rejected"}'
"#,
    );
    set_worker_command(&repo, "sh scripts/fake-worker.sh {input}");

    let outcome = run_outcome(&repo, &ticket_id);
    assert_eq!(outcome["status"], "blocked");
    assert_eq!(outcome["summary"], "summary rejected");
    assert!(
        !repo
            .path()
            .join(".pulse/evidence/execution/handoffs")
            .exists(),
        "the over-long summary must not produce a handoff receipt"
    );
}

/// Decision 0012 §5: `reviewers_required` in the reviewer input comes from
/// the strictest profile declared in PULSE.md, not a constant.
#[test]
fn reviewer_input_carries_the_profile_reviewers_requirement() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    std::fs::write(
        repo.path().join("PULSE.md"),
        "# Verification Profiles\n\n- `service-change`: `node scripts/verify.mjs`, reviewers: 2\n- `docs-only`: inspect changed Markdown links\n",
    )
    .unwrap();
    commit_all(repo.path());
    install_worker_script(
        &repo,
        r#"
RUN_DIR="$(dirname "$1")"
. "$RUN_DIR/worker-env"
"$PULSE" work handoff --lease "$LEASE_ID" --session "$SESSION_ID" \
  --source-commit "$SOURCE_COMMIT" --summary "did the work" \
  --idempotency-key handoff-1 --json
echo '{"status": "handed_off", "summary": "done"}'
"#,
    );
    set_worker_command(&repo, "sh scripts/fake-worker.sh {input}");
    let outcome = run_outcome(&repo, &ticket_id);
    assert_eq!(outcome["status"], "handed_off");

    install_worker_script(
        &repo,
        r#"echo '{"disposition": "pass", "acceptance": {"AC-1": "re-ran"}, "findings": []}'"#,
    );
    set_command(&repo, "reviewer", "sh scripts/fake-worker.sh {input}");
    repo.pulse_ok(&["run", "reviewer", "--ticket", &ticket_id, "--json"]);

    let run_dir = repo.path().join(".pulse/runtime/run").join(&ticket_id);
    let input: Value =
        serde_json::from_slice(&fs::read(run_dir.join("reviewer-input.json")).unwrap()).unwrap();
    assert_eq!(input["reviewers_required"], 2);
}

/// Decision 0013 §4: a worker that flushes and ends with
/// `context_exhausted` classifies like any blocked run — the lease stays
/// and the next run resumes with the same packet and the handoff note.
#[test]
fn worker_context_exhausted_classifies_blocked_and_keeps_the_lease() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    install_worker_script(
        &repo,
        r#"
RUN_DIR="$(dirname "$1")"
. "$RUN_DIR/worker-env"
"$PULSE" note --work "$TICKET_ID" \
  --message "handoff: acceptance mapping verified; next: implement the expired branch" \
  --from agent:runner:worker --json >/dev/null
echo '{"status": "blocked", "reason": "context_exhausted"}'
"#,
    );
    set_worker_command(&repo, "sh scripts/fake-worker.sh {input}");

    let outcome = run_outcome(&repo, &ticket_id);
    assert_eq!(outcome["status"], "blocked");
    assert_eq!(outcome["summary"], "context_exhausted");
    // The lease survives for the resume run.
    assert!(outcome["lease_id"].as_str().unwrap().starts_with("lease_"));
    assert_eq!(node_status(&repo, &ticket_id), "active");

    // The handoff note the worker flushed is on the event log for the
    // resume session to find.
    let tail = repo.pulse_ok(&["events", "tail", "--ticket", &ticket_id, "--json"]);
    assert!(tail.as_array().unwrap().iter().any(|event| {
        event["event_type"] == "note.recorded"
            && event["payload"]["message"]
                .as_str()
                .unwrap()
                .contains("next: implement the expired branch")
    }));
}
