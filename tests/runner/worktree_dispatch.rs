//! Worktree dispatch integration tests (Decision 0015).
//!
//! The rule under test: a Pulse-owned worktree is a *workspace* of the main
//! repository, never a second Pulse repository. The worktree owns the source
//! plane; every mutation plane stays in the main repository under its single
//! lock.
//!
//! The fake worker here deliberately behaves like a real agent that knows
//! nothing but its own working directory: it resolves its input relative to
//! cwd and calls the CLI without `--repo-root`. Track B round 1 (TK-006)
//! failed exactly there — the run workspace only existed in the main
//! repository, so the agent wandered back into the shared checkout and
//! staled a concurrent Ticket's proof fence.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::cli_run::{node_status, setup_ready_ticket};
use crate::common_bin::bin;
use crate::common_fixture_repo::TestRepo;
use crate::common_git::commit_all;

fn worktree_path(repo: &TestRepo, ticket_id: &str) -> PathBuf {
    repo.path().join(".pulse/runtime/worktrees").join(ticket_id)
}

fn run_record(repo: &TestRepo, ticket_id: &str, role: &str) -> Value {
    let path = repo
        .path()
        .join(".pulse/runtime/run")
        .join(ticket_id)
        .join(format!("{role}-outcome.json"));
    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap()
}

/// Install a role script that resolves everything from its own cwd.
///
/// No `{input}`, no `{repo}`, no `--repo-root`: if the run workspace is not
/// mirrored into the worktree, or the CLI does not route state back to the
/// main repository, this script cannot succeed.
fn install_cwd_only_scripts(repo: &TestRepo) {
    let pulse = bin();
    let worker = format!(
        "#!/bin/sh\nset -e\n\
         TICKET=\"$1\"\n\
         RUN_DIR=\".pulse/runtime/run/$TICKET\"\n\
         mkdir -p \"$RUN_DIR/artifacts\"\n\
         exec 2>\"$RUN_DIR/artifacts/err.txt\"\n\
         # Input and env must be findable from cwd alone.\n\
         test -f \"$RUN_DIR/worker-input.json\"\n\
         . \"$RUN_DIR/worker-env\"\n\
         pwd > \"$RUN_DIR/artifacts/cwd.txt\"\n\
         echo \"// worker edit\" >> src/token.mjs\n\
         echo \"worker was here\" > worker-marker.txt\n\
         # No --repo-root: the CLI must route state to the main repository.\n\
         \"{pulse}\" work handoff --lease \"$LEASE_ID\" --session \"$SESSION_ID\" \\\n\
           --source-commit \"$SOURCE_COMMIT\" --summary \"did the work\" \\\n\
           --changed-path src/token.mjs \\\n\
           --check \"focused=node scripts/verify.mjs=0\" \\\n\
           --proof \"AC-1=focused=\" \\\n\
           --idempotency-key handoff-wt-1 --json > \"$RUN_DIR/artifacts/handoff.json\"\n\
         echo '{{\"status\": \"handed_off\", \"summary\": \"done\"}}'\n"
    );
    let reviewer = format!(
        "#!/bin/sh\nset -e\n\
         TICKET=\"$1\"\n\
         RUN_DIR=\".pulse/runtime/run/$TICKET\"\n\
         mkdir -p \"$RUN_DIR/artifacts\"\n\
         exec 2>\"$RUN_DIR/artifacts/reviewer-err.txt\"\n\
         test -f \"$RUN_DIR/reviewer-input.json\"\n\
         pwd > \"$RUN_DIR/artifacts/reviewer-cwd.txt\"\n\
         # The reviewer must see the worker's edit in its own tree.\n\
         grep -q 'worker edit' src/token.mjs\n\
         # The handoff to review comes from the mirrored reviewer input.\n\
         HANDOFF=$(grep '\"handoff_id\"' \"$RUN_DIR/reviewer-input.json\" | head -1 | cut -d'\"' -f4)\n\
         test -n \"$HANDOFF\"\n\
         \"{pulse}\" work verify \"$TICKET\" --handoff \"$HANDOFF\" \\\n\
           --actor agent:runner:reviewer \\\n\
           --source-commit \"$(git rev-parse HEAD)\" \\\n\
           --summary \"reviewed in the handed-off tree\" \\\n\
           --check \"focused=node scripts/verify.mjs=0\" \\\n\
           --proof \"AC-1=focused=\" \\\n\
           --idempotency-key verify-wt-1 --json\n\
         echo '{{\"disposition\": \"pass\", \"acceptance\": {{\"AC-1\": \"covered\"}}, \"findings\": []}}'\n"
    );
    for (name, body) in [("fake-worker.sh", worker), ("fake-reviewer.sh", reviewer)] {
        let path = repo.path().join("scripts").join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, body).unwrap();
    }
    let config = serde_json::json!({
        "worker": {"command": "sh scripts/fake-worker.sh {ticket}", "timeout_seconds": 60},
        "reviewer": {"command": "sh scripts/fake-reviewer.sh {ticket}", "timeout_seconds": 60},
        "qa": {"command": "echo '{\"cases\":[]}'", "timeout_seconds": 60},
    });
    fs::write(
        repo.path().join(".pulse/config/runners.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .unwrap();
    commit_all(repo.path());
}

#[test]
fn worker_finds_its_run_workspace_under_its_own_cwd() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    install_cwd_only_scripts(&repo);

    let main_before = pulse::source::worktree_dirty_identity(repo.path()).unwrap();

    let outcome = repo.pulse_ok(&[
        "run",
        "worker",
        "--ticket",
        &ticket_id,
        "--isolation",
        "worktree",
        "--json",
    ]);
    let tree = worktree_path(&repo, &ticket_id);
    let err = fs::read_to_string(
        tree.join(".pulse/runtime/run")
            .join(&ticket_id)
            .join("artifacts/err.txt"),
    )
    .unwrap_or_default();
    assert_eq!(
        outcome["status"], "handed_off",
        "outcome: {outcome}; worker stderr: {err}"
    );

    // (a) The agent worked inside the worktree, and the main checkout is
    // byte-for-byte as clean (or dirty) as it was before the run.
    let cwd = fs::read_to_string(
        tree.join(".pulse/runtime/run")
            .join(&ticket_id)
            .join("artifacts/cwd.txt"),
    )
    .unwrap();
    assert!(
        Path::new(cwd.trim()).ends_with(format!(".pulse/runtime/worktrees/{ticket_id}")),
        "worker ran in {cwd}, not in its worktree"
    );
    assert!(tree.join("worker-marker.txt").exists());
    assert!(
        !repo.path().join("worker-marker.txt").exists(),
        "worker leaked a file into the shared checkout"
    );
    let main_after = pulse::source::worktree_dirty_identity(repo.path()).unwrap();
    assert_eq!(
        main_before.identity, main_after.identity,
        "an isolated run must not move the main checkout's dirty identity"
    );
}

#[test]
fn isolated_run_writes_its_state_planes_into_the_main_repository() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    install_cwd_only_scripts(&repo);

    repo.pulse_ok(&[
        "run",
        "worker",
        "--ticket",
        &ticket_id,
        "--isolation",
        "worktree",
        "--json",
    ]);
    let tree = worktree_path(&repo, &ticket_id);

    // (b) The handoff proof landed in the main repository, not the worktree,
    // even though the agent never passed --repo-root.
    let handoffs = repo.path().join(".pulse/evidence/execution/handoffs");
    let recorded: Vec<PathBuf> = fs::read_dir(&handoffs)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(
        recorded.len(),
        1,
        "expected exactly one handoff in the main repository"
    );
    assert!(
        !tree.join(".pulse/evidence/execution/handoffs").exists(),
        "the worktree must never receive an evidence plane of its own"
    );
    // The lifecycle transition is visible from the main repository too.
    assert_eq!(node_status(&repo, &ticket_id), "verifying");

    // The run record stays in the main repository and names the workspace.
    let record = run_record(&repo, &ticket_id, "worker");
    assert_eq!(
        record["workspace_id"].as_str(),
        Some(format!(".pulse/runtime/worktrees/{ticket_id}").as_str())
    );
    assert_ne!(record["worktree_graph_stale"], Value::Bool(true));
}

#[test]
fn reviewer_runs_in_the_tree_the_worker_handed_off() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    install_cwd_only_scripts(&repo);

    repo.pulse_ok(&[
        "run",
        "worker",
        "--ticket",
        &ticket_id,
        "--isolation",
        "worktree",
        "--json",
    ]);
    let tree = worktree_path(&repo, &ticket_id);

    // (c) The reviewer inherits the worker's workspace. Its script greps the
    // worker's edit, so a reviewer pointed at the main checkout fails here.
    let outcome = repo.pulse_ok(&["run", "reviewer", "--ticket", &ticket_id, "--json"]);
    let err = fs::read_to_string(
        tree.join(".pulse/runtime/run")
            .join(&ticket_id)
            .join("artifacts/reviewer-err.txt"),
    )
    .unwrap_or_default();
    assert_eq!(
        outcome["status"], "passed",
        "outcome: {outcome}; reviewer stderr: {err}"
    );
    let cwd = fs::read_to_string(
        tree.join(".pulse/runtime/run")
            .join(&ticket_id)
            .join("artifacts/reviewer-cwd.txt"),
    )
    .unwrap();
    assert!(
        Path::new(cwd.trim()).ends_with(format!(".pulse/runtime/worktrees/{ticket_id}")),
        "reviewer ran in {cwd}, not in the handed-off worktree"
    );
    let record = run_record(&repo, &ticket_id, "reviewer");
    assert_eq!(
        record["workspace_id"].as_str(),
        Some(format!(".pulse/runtime/worktrees/{ticket_id}").as_str())
    );
}

// ---------------------------------------------------------------------------
// repo-root mapping
// ---------------------------------------------------------------------------

#[test]
fn pulse_owned_worktree_maps_its_state_root_to_the_main_repository() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    install_cwd_only_scripts(&repo);
    repo.pulse_ok(&[
        "run",
        "worker",
        "--ticket",
        &ticket_id,
        "--isolation",
        "worktree",
        "--json",
    ]);
    let tree = worktree_path(&repo, &ticket_id);

    let mapped = pulse::source::state_repo_root(&tree).unwrap();
    let expected = repo.path().canonicalize().unwrap();
    assert_eq!(mapped.canonicalize().unwrap(), expected);

    let marker = pulse::source::read_worktree_marker(&tree).unwrap().unwrap();
    assert_eq!(marker.ticket_id, ticket_id);
    assert_eq!(marker.schema_version, 1);
}

#[test]
fn a_plain_checkout_is_never_mapped() {
    let repo = TestRepo::from_fixture("minimal-service");
    let mapped = pulse::source::state_repo_root(repo.path()).unwrap();
    assert_eq!(mapped, repo.path().to_path_buf());
    assert!(pulse::source::read_worktree_marker(repo.path())
        .unwrap()
        .is_none());
}

#[test]
fn a_worktree_the_developer_made_is_left_alone() {
    let repo = TestRepo::from_fixture("minimal-service");
    let foreign = repo.path().join("developer-worktree");
    let output = std::process::Command::new("git")
        .current_dir(repo.path())
        .args(["worktree", "add", "--detach"])
        .arg(&foreign)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git worktree add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // A real linked worktree, but not one Pulse created: no marker, no map.
    let mapped = pulse::source::state_repo_root(&foreign).unwrap();
    assert_eq!(mapped, foreign);
}

#[test]
fn a_marker_outside_a_linked_worktree_is_ignored() {
    let repo = TestRepo::from_fixture("minimal-service");
    let elsewhere = tempfile::tempdir().unwrap();
    // A marker copied somewhere else must not redirect anyone's state: the
    // directory is not a linked worktree of the recorded repository.
    pulse::source::write_worktree_marker(elsewhere.path(), repo.path(), "TK-FAKE").unwrap();
    let mapped = pulse::source::state_repo_root(elsewhere.path()).unwrap();
    assert_eq!(mapped, elsewhere.path().to_path_buf());
}

#[test]
fn an_unreadable_marker_is_refused_rather_than_ignored() {
    let repo = TestRepo::from_fixture("minimal-service");
    fs::write(repo.path().join(".pulse-owned"), b"not json at all").unwrap();
    let error = pulse::source::state_repo_root(repo.path()).unwrap_err();
    assert_eq!(error.code(), "worktree_marker_invalid");
}
