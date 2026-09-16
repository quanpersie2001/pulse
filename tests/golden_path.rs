//! Golden path v3 (plan 0022 §15, F2 of P1.12): every step drives the real
//! `pulse` binary as a subprocess against a fresh temporary git repository,
//! with fake shell "agents" standing in for `worker`/`review-correctness`/
//! `qa-cli`. Nothing here calls a `pulse::kernel`/`pulse::store` function
//! directly — that is the point: this proves the CLI surface, not the
//! library, carries a Ticket from `init` through `close-story`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{json, Value};

#[path = "common/bin.rs"]
mod common_bin;
#[path = "common/git.rs"]
mod common_git;

const REVIEW_CORRECTNESS_SH: &str = "#!/bin/sh\n\
set -e\n\
REPO=\"$1\"\n\
ARTIFACT_DIR=\"$2\"\n\
COMMIT=$(git -C \"$REPO\" rev-parse HEAD)\n\
mkdir -p \"$ARTIFACT_DIR\"\n\
cat > \"$ARTIFACT_DIR/review-correctness.json\" <<EOF\n\
{\"verdict\":\"pass\",\"acceptance\":[{\"id\":\"AC-1\",\"status\":\"pass\",\"how\":\"looks correct\"}],\"cases\":[],\"findings\":[],\"commands_run\":[],\"environment\":{\"commit\":\"$COMMIT\"}}\n\
EOF\n\
printf '%s\\n' '{\"status\":\"done\"}'\n";

const QA_CLI_SH: &str = "#!/bin/sh\n\
set -e\n\
REPO=\"$1\"\n\
ARTIFACT_DIR=\"$2\"\n\
COMMIT=$(git -C \"$REPO\" rev-parse HEAD)\n\
mkdir -p \"$ARTIFACT_DIR\"\n\
cat > \"$ARTIFACT_DIR/qa-cli.json\" <<EOF\n\
{\"verdict\":\"pass\",\"acceptance\":[],\"cases\":[{\"id\":\"QA-001\",\"status\":\"pass\",\"observation\":\"cli path returns the expected output\",\"artifacts\":[]}],\"findings\":[],\"commands_run\":[],\"environment\":{\"commit\":\"$COMMIT\"}}\n\
EOF\n\
printf '%s\\n' '{\"status\":\"done\"}'\n";

const PULSE_MD: &str = "\
fence_ignore: []
profiles:
  cli-low: {lanes: [review-correctness, qa-cli]}
";

fn write_file(path: &Path, body: &str) -> PathBuf {
    fs::write(path, body).unwrap();
    path.to_path_buf()
}

fn pulse_output(repo: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(common_bin::bin());
    command
        .arg("--repo-root")
        .arg(repo)
        .args(args)
        .env_remove("PULSE_ACTOR");
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("spawn pulse")
}

fn pulse_ok(repo: &Path, args: &[&str]) -> Value {
    pulse_ok_env(repo, args, &[])
}

fn pulse_ok_env(repo: &Path, args: &[&str], env: &[(&str, &str)]) -> Value {
    let output = pulse_output(repo, args, env);
    assert!(
        output.status.success(),
        "pulse {args:?} failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "pulse {args:?} did not print JSON: {error}\nstdout={}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn pulse_err_env(repo: &Path, args: &[&str], env: &[(&str, &str)]) -> Value {
    let output = pulse_output(repo, args, env);
    assert!(
        !output.status.success(),
        "pulse {args:?} unexpectedly succeeded:\nstdout={}",
        String::from_utf8_lossy(&output.stdout),
    );
    serde_json::from_slice(&output.stderr).unwrap_or_else(|error| {
        panic!(
            "pulse {args:?} did not print a JSON error: {error}\nstderr={}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn receipts(repo: &Path) -> Vec<Value> {
    let dir = repo.join(".pulse/receipts");
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            out.push(serde_json::from_slice(&fs::read(&path).unwrap()).unwrap());
        }
    }
    out
}

fn has_receipt(repo: &Path, kind: &str, subject_id: &str) -> bool {
    receipts(repo)
        .iter()
        .any(|r| r["kind"] == kind && r["subject"]["id"] == subject_id)
}

fn assert_issues_list_parses(repo: &Path) {
    let list = pulse_ok(repo, &["work", "list", "--json"]);
    assert!(
        list.is_array(),
        "pulse work list --json must print an array: {list}"
    );
}

#[test]
fn golden_path_new_to_close_story_with_fake_agents() {
    let repo = tempfile::tempdir().unwrap();
    let harness = tempfile::tempdir().unwrap();

    // --- Bootstrap a real git repo with one tracked file and one commit. ---
    fs::write(repo.path().join("README.md"), "# Golden Path\n").unwrap();
    common_git::commit_all(repo.path());

    // --- Step 1: init. ---
    pulse_ok(repo.path(), &["init", "--json"]);
    assert!(repo.path().join("PULSE.md").is_file());
    assert!(repo.path().join(".pulse/runners.json").is_file());
    assert!(repo.path().join("docs/README.md").is_file());
    let gitignore = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
    assert!(gitignore.contains("**/.pulse/runtime/"));

    // A custom profile: cli-low needs both review-correctness and qa-cli.
    fs::write(repo.path().join("PULSE.md"), PULSE_MD).unwrap();

    // --- Fake agents live outside the repo so they never dirty the tree. ---
    let pulse_bin = common_bin::bin();
    let cp_json = write_file(
        &harness.path().join("cp.json"),
        &serde_json::to_string(&json!({
            "run_id": "run_test_1",
            "done_ac": ["AC-1"],
            "in_progress": "",
            "next": [],
            "files": ["README.md"],
            "decisions": [],
            "gotchas": [],
            "commands_run": [{"argv": ["true"], "exit": 0}],
        }))
        .unwrap(),
    );
    let handoff_json = write_file(
        &harness.path().join("handoff.json"),
        &serde_json::to_string(&json!({
            "summary": "Implemented the CLI task path.",
            "changed_files": [],
            "acceptance": [{"id": "AC-1", "status": "done", "how": "ran the CLI path"}],
            "verify_results": [{"name": "unit", "exit": 0}],
            "docs_updated": [],
            "learnings_used": [],
            "friction": [],
            "open_risks": [],
        }))
        .unwrap(),
    );
    let worker_sh = format!(
        "#!/bin/sh\nset -e\n\"{pulse_bin}\" --repo-root \"$1\" checkpoint \"$2\" --from \"{}\" --json >/dev/null\n\"{pulse_bin}\" --repo-root \"$1\" handoff \"$2\" --from \"{}\" --json >/dev/null\nprintf '%s\\n' '{{\"status\":\"handed_off\"}}'\n",
        cp_json.display(),
        handoff_json.display(),
    );
    let worker_script = write_file(&harness.path().join("worker.sh"), &worker_sh);
    let review_script = write_file(
        &harness.path().join("review-correctness.sh"),
        REVIEW_CORRECTNESS_SH,
    );
    let qa_script = write_file(&harness.path().join("qa-cli.sh"), QA_CLI_SH);

    let runners = json!({
        "worker": {
            "command": format!("sh {} {{repo}} {{ticket}}", worker_script.display()),
            "timeout_seconds": 30,
        },
        "review-correctness": {
            "command": format!("sh {} {{repo}} {{artifact_dir}}", review_script.display()),
            "timeout_seconds": 30,
        },
        "qa-cli": {
            "command": format!("sh {} {{repo}} {{artifact_dir}}", qa_script.display()),
            "timeout_seconds": 30,
        },
    });
    fs::write(
        repo.path().join(".pulse/runners.json"),
        serde_json::to_vec_pretty(&runners).unwrap(),
    )
    .unwrap();

    // Commit the scaffold (PULSE.md, docs/README.md, AGENTS.md, .gitignore,
    // .pulse/runners.json, the empty .pulse/issues.jsonl) as a clean baseline.
    common_git::commit_all(repo.path());

    // --- Step 2: Story. ---
    let story_json = write_file(
        &harness.path().join("story.json"),
        &serde_json::to_string(&json!({
            "outcome": "A user can create and complete tasks from the CLI.",
            "qa_cases": [
                {"id": "QA-001", "intent": "create then list a task via the CLI",
                 "surface": "cli", "priority": "high"},
            ],
        }))
        .unwrap(),
    );
    let story = pulse_ok(repo.path(), &["work", "new", "story", "S", "--json"]);
    let story_id = story["id"].as_str().unwrap().to_string();
    pulse_ok(
        repo.path(),
        &[
            "work",
            "update",
            &story_id,
            "--from",
            story_json.to_str().unwrap(),
            "--json",
        ],
    );
    let story = pulse_ok(repo.path(), &["work", "ready", &story_id, "--json"]);
    assert_eq!(story["status"], "ready");
    assert_issues_list_parses(repo.path());

    // --- Step 3: Ticket. ---
    let ticket_json = write_file(
        &harness.path().join("t.json"),
        &serde_json::to_string(&json!({
            "objective": "Add a `tasks add`/`tasks list` CLI path.",
            "context": {"anchors": ["README.md"]},
            "acceptance": [
                {"id": "AC-1", "when": "a task is added and listed", "then": "it appears in the list"},
            ],
            "verify": [{"name": "unit", "argv": ["true"]}],
            "qa_cases": ["QA-001"],
        }))
        .unwrap(),
    );
    let ticket = pulse_ok(
        repo.path(),
        &[
            "work",
            "new",
            "ticket",
            "T",
            "--story",
            &story_id,
            "--risk",
            "low",
            "--surface",
            "cli",
            "--json",
        ],
    );
    let ticket_id = ticket["id"].as_str().unwrap().to_string();
    pulse_ok(
        repo.path(),
        &[
            "work",
            "update",
            &ticket_id,
            "--from",
            ticket_json.to_str().unwrap(),
            "--json",
        ],
    );
    let ticket = pulse_ok(repo.path(), &["work", "ready", &ticket_id, "--json"]);
    assert_eq!(ticket["status"], "ready");
    assert_issues_list_parses(repo.path());

    // --- Step 5: worker (checkpoint + handoff, through the CLI). ---
    let ticket = pulse_ok(
        repo.path(),
        &[
            "run",
            "worker",
            &ticket_id,
            "--actor",
            "agent:worker",
            "--json",
        ],
    );
    assert_eq!(ticket["status"], "verifying");
    assert!(
        has_receipt(repo.path(), "checkpoint", &ticket_id),
        "expected a checkpoint receipt: {:?}",
        receipts(repo.path())
    );
    assert!(
        has_receipt(repo.path(), "handoff", &ticket_id),
        "expected a handoff receipt: {:?}",
        receipts(repo.path())
    );
    assert_issues_list_parses(repo.path());

    // --- Step 6: review (both lanes in the cli-low profile). ---
    let ticket = pulse_ok(repo.path(), &["run", "review", &ticket_id, "--json"]);
    assert_eq!(ticket["verdicts"]["review-correctness"]["verdict"], "pass");
    assert_eq!(ticket["verdicts"]["qa-cli"]["verdict"], "pass");
    assert_issues_list_parses(repo.path());

    // --- Step 7: a tracked edit outside .pulse/ staled the handoff. ---
    let original_readme = fs::read_to_string(repo.path().join("README.md")).unwrap();
    fs::write(
        repo.path().join("README.md"),
        format!("{original_readme}\nedited after handoff\n"),
    )
    .unwrap();
    let error = pulse_err_env(
        repo.path(),
        &["close", &ticket_id, "--json"],
        &[("PULSE_ACTOR", "human:test")],
    );
    assert_eq!(error["code"], "gate_failed");
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("close_source_stale"), "{message}");
    assert!(message.contains("README.md"), "{message}");
    fs::write(repo.path().join("README.md"), &original_readme).unwrap();

    // --- Step 8: close. ---
    let ticket = pulse_ok_env(
        repo.path(),
        &["close", &ticket_id, "--json"],
        &[("PULSE_ACTOR", "human:test")],
    );
    assert_eq!(ticket["status"], "done");
    assert!(has_receipt(repo.path(), "close", &ticket_id));
    assert_issues_list_parses(repo.path());

    // --- Step 9: story-scope qa-cli, then close-story. ---
    pulse_ok(
        repo.path(),
        &["run", "qa-cli", &story_id, "--force", "--json"],
    );
    assert!(
        has_receipt(repo.path(), "lane", &story_id),
        "expected a story-scope lane receipt: {:?}",
        receipts(repo.path())
    );
    let story = pulse_ok(repo.path(), &["close-story", &story_id, "--json"]);
    assert_eq!(story["status"], "done");
    assert_issues_list_parses(repo.path());
}
