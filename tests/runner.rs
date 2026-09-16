//! Runner execution-contract integration tests.
//!
//! Covers the `pulse::runner` mechanics: command-spec parsing and validation,
//! shell-free argv splitting, placeholder substitution, process execution with
//! timeout and cancellation, bounded output capture and the final-JSON output
//! contract. Timing-sensitive assertions use generous margins so the suite
//! stays deterministic under parallel load.

//! The CLI-level `pulse run` submodule suites (artifacts, cli_run,
//! harness_learning, isolation, knowledge_usage, qa_receipt, recovery,
//! reviewer, story_close, worktree_dispatch) tested the v2 `kernel::run`,
//! deleted with the rest of the v2 graph/docs/knowledge stack (plan 0022
//! P1.3/P1.4). Only this crate's own inline tests below survive: they
//! exercise `pulse::runner` (spawn/timeout/bounded output) directly, which
//! plan §14's file-fate table keeps as-is. The v3 runner + lane suite is
//! rebuilt in P1.9 alongside `kernel::run`/`kernel::lane`.

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::time::Duration;

use pulse::runner;

fn sh(script: &str) -> Vec<String> {
    vec!["sh".to_string(), "-c".to_string(), script.to_string()]
}

// ---------------------------------------------------------------------------
// CommandSpec
// ---------------------------------------------------------------------------

#[test]
fn command_spec_parses_valid_role_entry() {
    let value = serde_json::json!({
        "command": "claude -p --output-format json",
        "timeout_seconds": 3600,
        "max_output_bytes": 1_048_576
    });
    let spec = runner::CommandSpec::from_value(&value).unwrap();
    assert_eq!(spec.timeout_seconds, 3600);
    assert_eq!(spec.max_output_bytes, 1_048_576);
}

#[test]
fn command_spec_rejects_unknown_fields_and_bad_bounds() {
    let unknown_field = serde_json::json!({
        "command": "x",
        "timeout_seconds": 10,
        "shell": true
    });
    let error = runner::CommandSpec::from_value(&unknown_field).unwrap_err();
    assert_eq!(error.code(), "runner_spec_invalid");

    let zero_timeout = serde_json::json!({ "command": "x", "timeout_seconds": 0 });
    let error = runner::CommandSpec::from_value(&zero_timeout).unwrap_err();
    assert_eq!(error.code(), "runner_spec_invalid");

    let huge_timeout = serde_json::json!({ "command": "x", "timeout_seconds": 200_000 });
    let error = runner::CommandSpec::from_value(&huge_timeout).unwrap_err();
    assert_eq!(error.code(), "runner_spec_invalid");

    let tiny_output = serde_json::json!({
        "command": "x",
        "timeout_seconds": 10,
        "max_output_bytes": 8
    });
    let error = runner::CommandSpec::from_value(&tiny_output).unwrap_err();
    assert_eq!(error.code(), "runner_spec_invalid");

    let blank = serde_json::json!({ "command": "   ", "timeout_seconds": 10 });
    let error = runner::CommandSpec::from_value(&blank).unwrap_err();
    assert_eq!(error.code(), "runner_spec_invalid");
}

// ---------------------------------------------------------------------------
// argv splitting (never a shell)
// ---------------------------------------------------------------------------

#[test]
fn split_argv_respects_quotes_and_escapes() {
    let argv =
        runner::split_argv("claude -p --model 'main model' \"a \\\"b\\\" c\" plain").unwrap();
    assert_eq!(
        argv,
        vec![
            "claude".to_string(),
            "-p".to_string(),
            "--model".to_string(),
            "main model".to_string(),
            "a \"b\" c".to_string(),
            "plain".to_string()
        ]
    );
}

#[test]
fn split_argv_rejects_unterminated_quotes_and_empty_commands() {
    assert_eq!(
        runner::split_argv("x 'unterminated").unwrap_err().code(),
        "runner_argv_invalid"
    );
    assert_eq!(
        runner::split_argv("x trailing\\").unwrap_err().code(),
        "runner_argv_invalid"
    );
    assert_eq!(
        runner::split_argv("   ").unwrap_err().code(),
        "runner_argv_empty"
    );
}

#[test]
fn split_argv_rejects_oversized_argv() {
    let long = runner::split_argv(&"a ".repeat(80)).unwrap_err();
    assert_eq!(long.code(), "runner_argv_invalid");
    let big_arg = runner::split_argv(&format!("x {}", "a".repeat(5_000))).unwrap_err();
    assert_eq!(big_arg.code(), "runner_argv_invalid");
}

// ---------------------------------------------------------------------------
// placeholder substitution
// ---------------------------------------------------------------------------

#[test]
fn materialize_argv_substitutes_known_placeholders() {
    let argv = runner::split_argv("node run.mjs --input {input} --ticket {ticket} {repo}").unwrap();
    let values = BTreeMap::from([
        ("input".to_string(), "/run/TK-1/input.json".to_string()),
        ("ticket".to_string(), "TK-1".to_string()),
        ("repo".to_string(), "/tmp/repo".to_string()),
    ]);
    let materialized = runner::materialize_argv(&argv, &values).unwrap();
    assert_eq!(
        materialized,
        vec![
            "node".to_string(),
            "run.mjs".to_string(),
            "--input".to_string(),
            "/run/TK-1/input.json".to_string(),
            "--ticket".to_string(),
            "TK-1".to_string(),
            "/tmp/repo".to_string(),
        ]
    );
}

#[test]
fn materialize_argv_rejects_unknown_placeholders() {
    let argv = runner::split_argv("node run.mjs {inputs}").unwrap();
    let error = runner::materialize_argv(&argv, &BTreeMap::new()).unwrap_err();
    assert_eq!(error.code(), "runner_placeholder_unknown");
    // Non-placeholder-shaped braces pass through untouched.
    let argv = runner::split_argv("node run.mjs '{weird name}'").unwrap();
    let materialized = runner::materialize_argv(&argv, &BTreeMap::new()).unwrap();
    assert_eq!(materialized[2], "{weird name}");
}

// ---------------------------------------------------------------------------
// execution mechanics
// ---------------------------------------------------------------------------

#[test]
fn execute_captures_exit_code_and_streams() {
    let tmp = tempfile::tempdir().unwrap();
    let outcome = runner::execute(
        tmp.path(),
        &sh("echo stdout-line; echo stderr-line >&2; exit 3"),
        &[],
        Duration::from_secs(30),
        runner::DEFAULT_MAX_OUTPUT_BYTES,
        None,
    )
    .unwrap();
    assert_eq!(outcome.exit_code, Some(3));
    assert!(!outcome.exited_cleanly());
    assert!(String::from_utf8_lossy(&outcome.stdout).contains("stdout-line"));
    assert!(String::from_utf8_lossy(&outcome.stderr).contains("stderr-line"));
    assert!(!outcome.stdout_truncated && !outcome.stderr_truncated);
    assert!(!outcome.timed_out && !outcome.cancelled);
}

#[test]
fn execute_kills_on_timeout_within_the_bound() {
    let tmp = tempfile::tempdir().unwrap();
    let outcome = runner::execute(
        tmp.path(),
        &sh("sleep 60"),
        &[],
        Duration::from_secs(2),
        runner::DEFAULT_MAX_OUTPUT_BYTES,
        None,
    )
    .unwrap();
    assert!(outcome.timed_out);
    assert!(outcome.duration < Duration::from_secs(30));
    // The process group kill must have actually terminated the child.
    assert!(outcome.exit_code.is_none() || !outcome.exited_cleanly());
}

#[test]
fn execute_kills_process_group_not_just_direct_child() {
    let tmp = tempfile::tempdir().unwrap();
    // The shell backgrounds a grandchild that outlives the shell itself.
    let outcome = runner::execute(
        tmp.path(),
        &sh("sleep 300 & sleep 300 & wait"),
        &[],
        Duration::from_secs(2),
        runner::DEFAULT_MAX_OUTPUT_BYTES,
        None,
    )
    .unwrap();
    assert!(outcome.timed_out);
    assert!(outcome.duration < Duration::from_secs(30));
}

#[test]
fn execute_honours_cancellation() {
    let tmp = tempfile::tempdir().unwrap();
    let cancel = runner::cancel_flag();
    let handle = {
        let cancel = cancel.clone();
        std::thread::spawn(move || {
            // Give the runner a moment to spawn, then cancel.
            std::thread::sleep(Duration::from_millis(300));
            cancel.store(true, Ordering::SeqCst);
        })
    };
    let outcome = runner::execute(
        tmp.path(),
        &sh("sleep 60"),
        &[],
        Duration::from_secs(60),
        runner::DEFAULT_MAX_OUTPUT_BYTES,
        Some(&cancel),
    )
    .unwrap();
    handle.join().unwrap();
    assert!(outcome.cancelled);
    assert!(!outcome.exited_cleanly());
    assert!(outcome.duration < Duration::from_secs(30));
}

#[test]
fn execute_bounds_output_and_reports_truncation() {
    let tmp = tempfile::tempdir().unwrap();
    let outcome = runner::execute(
        tmp.path(),
        &sh("yes x | head -c 100000"),
        &[],
        Duration::from_secs(30),
        4_096,
        None,
    )
    .unwrap();
    assert_eq!(outcome.stdout.len(), 4_096);
    assert!(outcome.stdout_truncated);
}

#[test]
fn execute_reports_spawn_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let error = runner::execute(
        tmp.path(),
        &["definitely-not-a-real-binary-4f9d".to_string()],
        &[],
        Duration::from_secs(10),
        runner::DEFAULT_MAX_OUTPUT_BYTES,
        None,
    )
    .unwrap_err();
    assert_eq!(error.code(), "runner_spawn_failed");
}

// ---------------------------------------------------------------------------
// final-output JSON contract
// ---------------------------------------------------------------------------

#[test]
fn parse_output_json_reads_the_last_nonempty_line() {
    let outcome = runner::Outcome {
        exit_code: Some(0),
        timed_out: false,
        cancelled: false,
        stdout: b"noise line\n{\"status\":\"handed_off\"}\n".to_vec(),
        stderr: vec![],
        stdout_truncated: false,
        stderr_truncated: false,
        duration: Duration::from_millis(10),
    };
    let value = runner::parse_output_json(&outcome).unwrap();
    assert_eq!(value["status"], "handed_off");
}

#[test]
fn parse_output_json_rejects_malformed_output() {
    let malformed = |stdout: &[u8]| {
        let outcome = runner::Outcome {
            exit_code: Some(0),
            timed_out: false,
            cancelled: false,
            stdout: stdout.to_vec(),
            stderr: vec![],
            stdout_truncated: false,
            stderr_truncated: false,
            duration: Duration::from_millis(10),
        };
        runner::parse_output_json(&outcome).unwrap_err()
    };
    assert_eq!(malformed(b"").code(), "runner_output_malformed");
    assert_eq!(malformed(b"not json\n").code(), "runner_output_malformed");
    assert_eq!(
        malformed(b"{\"a\":1}\ntwo lines of json\n").code(),
        "runner_output_malformed"
    );
}

// ---------------------------------------------------------------------------
// shell-boundary guard: the command must never run through sh -c
// ---------------------------------------------------------------------------

#[test]
fn argv_splitted_commands_do_not_invoke_a_shell() {
    // If Pulse ran commands through `sh -c`, the semicolon would execute a
    // second command. Through direct argv execution the semicolon is a plain
    // argument and the target binary does not exist.
    let tmp = tempfile::tempdir().unwrap();
    let argv = vec![
        "definitely-not-a-real-binary-4f9d".to_string(),
        "echo pwned; echo pwned > marker".to_string(),
    ];
    let error = runner::execute(
        tmp.path(),
        &argv,
        &[],
        Duration::from_secs(10),
        runner::DEFAULT_MAX_OUTPUT_BYTES,
        None,
    )
    .unwrap_err();
    assert_eq!(error.code(), "runner_spawn_failed");
    assert!(!tmp.path().join("marker").exists());
    let _ = Command::new("true").output();
}
