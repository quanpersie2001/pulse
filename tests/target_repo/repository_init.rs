//! `pulse init` against a fixture target repo.
//!
//! Covers what `kernel::init` writes: `.pulse/` with an empty
//! `issues.jsonl`, the `PULSE.md` profile seed, the worker/review prompts,
//! the AGENTS block, the docs seeds and the runtime/cache `.gitignore`
//! entries — idempotently. There is no dispatch table to seed: Pulse spawns
//! no agents, so a host-specific runner config and detector never existed
//! in a v3 target repo.

use crate::common::fixture_repo::TestRepo;

#[test]
fn first_run_initializes_and_second_run_reports_unchanged() {
    let repo = TestRepo::from_fixture("minimal-service");

    let first = repo.pulse_ok(&["init", "--json"]);
    assert_eq!(first["status"], "initialized");
    assert!(!first["created"].as_array().unwrap().is_empty());

    assert!(repo.path().join(".pulse/issues.jsonl").is_file());
    assert!(repo.path().join(".pulse/prompts/worker.md").is_file());
    assert!(!repo.path().join(".pulse/runners.json").exists());
    assert!(repo.path().join("PULSE.md").is_file());
    let gitignore = std::fs::read_to_string(repo.path().join(".gitignore")).unwrap();
    assert!(gitignore.contains("**/.pulse/runtime/"));
    assert!(gitignore.contains("**/.pulse/cache/"));

    let second = repo.pulse_ok(&["init", "--json"]);
    assert_eq!(second["status"], "unchanged");
    assert!(second["created"].as_array().unwrap().is_empty());
}

#[test]
fn fresh_init_passes_docs_check() {
    let repo = TestRepo::from_fixture("minimal-service");

    repo.pulse_ok(&["init", "--json"]);
    assert!(repo.path().join("docs/operations/run.md").is_file());

    let output = repo.pulse(&["docs", "check", "--json"]);
    assert!(
        output.status.success(),
        "pulse docs check should exit 0 on a freshly initialized repo: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("docs check JSON stdout");
    assert_eq!(report["verdict"], "pass");
    assert!(report["findings"].as_array().unwrap().is_empty());
}

#[test]
fn init_preserves_an_existing_hand_edited_gitignore() {
    let repo = TestRepo::from_fixture("minimal-service");
    std::fs::write(repo.path().join(".gitignore"), "node_modules/\n").unwrap();

    repo.pulse_ok(&["init", "--json"]);

    let gitignore = std::fs::read_to_string(repo.path().join(".gitignore")).unwrap();
    assert!(gitignore.contains("node_modules/"));
    assert!(gitignore.contains("**/.pulse/runtime/"));
}
