//! `pulse init` (plan 0022 P1.3 interim minimal version).
//!
//! The full asset set (AGENTS block, PULSE.md profiles, docs/README.md seed,
//! host detector files, prompts) lands in plan 0022 P1.10. This only covers
//! what `kernel::init` does today: create `.pulse/`, an empty
//! `issues.jsonl`, an empty `runners.json`, a placeholder `PULSE.md`, and
//! the runtime/cache `.gitignore` entries — idempotently.

use crate::common::fixture_repo::TestRepo;

#[test]
fn first_run_initializes_and_second_run_reports_unchanged() {
    let repo = TestRepo::from_fixture("minimal-service");

    let first = repo.pulse_ok(&["init", "--json"]);
    assert_eq!(first["status"], "initialized");
    assert!(!first["created"].as_array().unwrap().is_empty());

    assert!(repo.path().join(".pulse/issues.jsonl").is_file());
    assert!(repo.path().join(".pulse/runners.json").is_file());
    assert!(repo.path().join("PULSE.md").is_file());
    let gitignore = std::fs::read_to_string(repo.path().join(".gitignore")).unwrap();
    assert!(gitignore.contains("**/.pulse/runtime/"));
    assert!(gitignore.contains("**/.pulse/cache/"));

    let second = repo.pulse_ok(&["init", "--json"]);
    assert_eq!(second["status"], "unchanged");
    assert!(second["created"].as_array().unwrap().is_empty());
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
