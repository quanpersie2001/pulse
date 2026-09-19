//! The pre-edit gate end-to-end (plan 0025 G1): the real `pulse` binary
//! answering a host hook against a temporary enrolled repo — never this
//! development repository (AGENTS.md).
//!
//! The exit codes are the contract the host depends on: allow = 0 and
//! silent, deny = 2 with the reason on stderr, internal failure = 1. A
//! deny that surfaced as 1 would silently block edits; an internal failure
//! surfaced as 2 would look like a decision. Both directions are pinned
//! here.

use std::fs;
use std::path::Path;
use std::process::Output;

use serde_json::{json, Value};

#[allow(dead_code)]
#[path = "common/bin.rs"]
mod common_bin;
#[allow(dead_code)]
#[path = "common/git.rs"]
mod common_git;

use common_bin::bin;

const PULSE_MD: &str = "profiles:\n  cli-low: {lanes: [review-correctness]}\n";

/// An enrolled temporary repo with a committed baseline so the fence has a
/// clean tree to look at.
fn setup(records: &[Value]) -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    common_git::git(repo.path(), &["init", "-q"]);
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "fn main() {}\n").unwrap();
    fs::write(repo.path().join("PULSE.md"), PULSE_MD).unwrap();
    common_git::commit_all(repo.path());
    let output = std::process::Command::new(bin())
        .args(["init", "--json"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "pulse init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    common_git::commit_all(repo.path());
    write_records(repo.path(), records);
    repo
}

fn write_records(repo: &Path, records: &[Value]) {
    use std::io::Write;
    let path = repo.join(".pulse/issues.jsonl");
    let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
    for record in records {
        writeln!(file, "{record}").unwrap();
    }
}

fn active_ticket(id: &str, touches: &[&str], worker: &str) -> Value {
    let mut ticket = json!({
        "schema": 3, "id": id, "kind": "ticket", "title": "t",
        "status": "active", "revision": 1,
        "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
        "role": "implementation",
    });
    if !touches.is_empty() {
        ticket["touches"] = json!(touches);
    }
    ticket["lease"] = json!({
        "role": "worker", "actor": format!("agent:{worker}"),
        "run_id": "run_1",
        "expires_at": "2099-01-01T00:00:00Z",
    });
    ticket
}

fn verifying_ticket(id: &str, touches: &[&str]) -> Value {
    let mut ticket = active_ticket(id, touches, "worker-1");
    ticket["status"] = json!("verifying");
    ticket["lease"] = json!(null);
    ticket
}

/// Run `pulse hook pre-edit` the way a host would, with `cwd` at the repo.
fn pre_edit(repo: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = std::process::Command::new(bin());
    command
        .args(["hook", "pre-edit"])
        .args(args)
        .current_dir(repo);
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("pulse hook pre-edit should run")
}

fn deny_message(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_string()
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn an_unenrolled_repo_allows_with_exit_zero_and_no_output() {
    let repo = tempfile::tempdir().unwrap();
    let output = pre_edit(repo.path(), &["--path", "src/lib.rs"], &[]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout_of(&output).is_empty());
    assert!(deny_message(&output).is_empty());
}

#[test]
fn a_path_inside_the_holders_touches_allows() {
    let repo = setup(&[active_ticket("TK-aaaa", &["src/**"], "worker-1")]);
    let output = pre_edit(
        repo.path(),
        &["--path", "src/lib.rs", "--actor", "agent:worker-1"],
        &[],
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout_of(&output).is_empty());
}

#[test]
fn a_path_outside_every_touches_denies_with_pulse_reserve_on_stderr() {
    let repo = setup(&[active_ticket("TK-aaaa", &["src/**"], "worker-1")]);
    let output = pre_edit(repo.path(), &["--path", "docs/notes.md"], &[]);
    assert_eq!(output.status.code(), Some(2));
    let message = deny_message(&output);
    assert!(message.contains("pulse reserve"), "{message}");
    assert!(
        stdout_of(&output).is_empty(),
        "a deny prints nothing on stdout"
    );
}

#[test]
fn a_path_of_a_verifying_ticket_denies() {
    let repo = setup(&[
        active_ticket("TK-aaaa", &["web/**"], "worker-2"),
        verifying_ticket("TK-bbbb", &["src/**"]),
    ]);
    let output = pre_edit(repo.path(), &["--path", "src/lib.rs"], &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(deny_message(&output).contains("under review in TK-bbbb"));
}

#[test]
fn a_lane_actor_is_denied_on_source_but_allowed_on_evidence() {
    let repo = setup(&[]);
    let output = pre_edit(
        repo.path(),
        &["--path", "src/lib.rs"],
        &[("PULSE_ACTOR", "agent:review-correctness")],
    );
    assert_eq!(output.status.code(), Some(2));
    let message = deny_message(&output);
    assert!(message.contains("review lane"), "{message}");

    let output = pre_edit(
        repo.path(),
        &["--path", ".pulse/evidence/TK-aaaa/review-correctness.json"],
        &[("PULSE_ACTOR", "agent:review-correctness")],
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(deny_message(&output).is_empty());
}

#[test]
fn a_torn_store_exits_one_not_two() {
    // Exit 2 means "a deliberate denial"; a torn store must never lock
    // every edit of the repository, so an internal failure is exit 1.
    let repo = setup(&[]);
    fs::write(repo.path().join(".pulse/issues.jsonl"), b"{not json\n").unwrap();
    let output = pre_edit(repo.path(), &["--path", "src/lib.rs"], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(!deny_message(&output).is_empty());
}

#[test]
fn a_stdin_json_claude_payload_decides_by_its_file_path() {
    let repo = setup(&[active_ticket("TK-aaaa", &["src/**"], "worker-1")]);
    let payload = json!({
        "session_id": "s", "tool_name": "Edit",
        "tool_input": {"file_path": "src/lib.rs", "old_string": "a", "new_string": "b"},
        "cwd": repo.path(),
    });
    use std::process::{Command, Stdio};
    let mut child = Command::new(bin())
        .args(["hook", "pre-edit", "--stdin-json"])
        .current_dir(repo.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pulse hook pre-edit");
    child
        .stdin
        .as_mut()
        .expect("stdin pipe")
        .write_all(payload.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0), "holder edit inside touches");

    let payload = json!({
        "session_id": "s", "tool_name": "Write",
        "tool_input": {"file_path": "docs/notes.md", "content": "x"},
        "cwd": repo.path(),
    });
    let mut child = Command::new(bin())
        .args(["hook", "pre-edit", "--stdin-json"])
        .current_dir(repo.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pulse hook pre-edit");
    child
        .stdin
        .as_mut()
        .expect("stdin pipe")
        .write_all(payload.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(deny_message(&output).contains("pulse reserve"));
}

#[test]
fn malformed_stdin_json_exits_one_not_two() {
    let repo = setup(&[]);
    use std::process::{Command, Stdio};
    let mut child = Command::new(bin())
        .args(["hook", "pre-edit", "--stdin-json"])
        .current_dir(repo.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pulse hook pre-edit");
    child
        .stdin
        .as_mut()
        .expect("stdin pipe")
        .write_all(b"{not json")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn snippet_claude_prints_valid_json_with_the_gate_command() {
    let repo = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(bin())
        .args(["hook", "snippet", "claude"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let parsed: Value =
        serde_json::from_str(&stdout_of(&output)).expect("snippet claude prints valid JSON");
    let entry = &parsed["hooks"]["PreToolUse"][0];
    assert_eq!(entry["matcher"], "Edit|Write|MultiEdit|NotebookEdit");
    assert_eq!(
        entry["hooks"][0]["command"],
        "pulse hook pre-edit --stdin-json"
    );
}

#[test]
fn snippet_for_an_unverified_host_is_hook_host_unknown() {
    let repo = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(bin())
        .args(["hook", "snippet", "nope"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = deny_message(&output);
    assert!(stderr.contains("hook_host_unknown"), "{stderr}");
    assert!(stderr.contains("claude"), "{stderr}");
}

#[test]
fn init_hints_at_the_snippet_once() {
    let repo = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(bin())
        .args(["init"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    let stdout = stdout_of(&output);
    assert!(stdout.contains("pulse hook snippet"), "{stdout}");
}

use std::io::Write as _;
