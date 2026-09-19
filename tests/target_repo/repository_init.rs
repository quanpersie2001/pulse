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

/// Plan 0025 G2 through the real CLI: a conflict exits 0, names the file,
/// never hands the living prompt markers, and the JSON report carries the
/// per-file `refreshed` array.
#[test]
fn refresh_reports_a_conflict_with_exit_zero_and_per_file_lines() {
    let repo = TestRepo::from_fixture("minimal-service");
    repo.pulse_ok(&["init", "--json"]);
    let worker_path = repo.path().join(".pulse/prompts/worker.md");
    // Age base AND the file together (what an older template shipped),
    // then edit the aged heading locally: the template's side differs on
    // the same line — a real conflict.
    let base_path = repo.path().join(".pulse/base/prompts/worker.md");
    let shipped = std::fs::read_to_string(&base_path).unwrap();
    let aged = shipped.replace("# Pulse worker", "# Pulse worker (aged)");
    std::fs::write(&base_path, &aged).unwrap();
    std::fs::write(
        &worker_path,
        aged.replace("# Pulse worker (aged)", "# My own worker prompt"),
    )
    .unwrap();

    let output = repo.pulse(&["init", "--refresh"]);
    assert!(
        output.status.success(),
        "a handled conflict still exits 0: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("prompts/worker.md: CONFLICT"), "{stdout}");
    assert!(stdout.contains("1 conflict(s)"), "{stdout}");
    assert!(stdout.contains("--take-new prompts/worker.md"), "{stdout}");
    let text = std::fs::read_to_string(&worker_path).unwrap();
    assert!(text.contains("# My own worker prompt"));
    assert!(!text.contains("<<<<<<<"));
    assert!(repo
        .path()
        .join(".pulse/runtime/refresh/prompts-worker.md.conflict")
        .is_file());

    // The JSON report carries the same story: the local file now matches
    // its base again (the user "untouched" state), so the next refresh
    // simply moves it to the template.
    std::fs::write(&worker_path, &aged).unwrap();
    let report = repo.pulse_ok(&["init", "--refresh", "--json"]);
    let refreshed = report["refreshed"].as_array().unwrap();
    let worker = refreshed
        .iter()
        .find(|file| file["file"] == "prompts/worker.md")
        .unwrap();
    assert_eq!(worker["action"], "updated");
}

/// The `--take-new` / `--keep-mine` resolutions through the real CLI.
#[test]
fn refresh_resolutions_take_new_and_keep_mine() {
    let repo = TestRepo::from_fixture("minimal-service");
    repo.pulse_ok(&["init", "--json"]);
    let worker_path = repo.path().join(".pulse/prompts/worker.md");
    std::fs::write(&worker_path, "sentinel-do-not-keep\n").unwrap();
    std::fs::remove_file(repo.path().join(".pulse/base/prompts/worker.md")).unwrap();

    // A base-less file is kept, never overwritten.
    let output = repo.pulse(&["init", "--refresh"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("prompts/worker.md: kept"), "{stdout}");
    assert_eq!(
        std::fs::read_to_string(&worker_path).unwrap(),
        "sentinel-do-not-keep\n"
    );

    // --keep-mine keeps it and moves the base so the next refresh merges.
    repo.pulse_ok(&[
        "init",
        "--refresh",
        "--keep-mine",
        "prompts/worker.md",
        "--json",
    ]);
    assert_eq!(
        std::fs::read_to_string(&worker_path).unwrap(),
        "sentinel-do-not-keep\n"
    );
    let base = std::fs::read_to_string(repo.path().join(".pulse/base/prompts/worker.md")).unwrap();
    assert!(base.starts_with("# Pulse worker\n"), "{base}");

    // --take-new replaces the file and rebases the base on it.
    std::fs::write(&worker_path, "sentinel-replaced\n").unwrap();
    repo.pulse_ok(&[
        "init",
        "--refresh",
        "--take-new",
        "prompts/worker.md",
        "--json",
    ]);
    let text = std::fs::read_to_string(&worker_path).unwrap();
    assert!(text.starts_with("# Pulse worker\n"), "{text}");

    // An unmanaged name is refused with the dedicated code.
    let output = repo.pulse(&["init", "--refresh", "--take-new", "docs/README.md"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("init_refresh_unknown_file"), "{stderr}");
}
