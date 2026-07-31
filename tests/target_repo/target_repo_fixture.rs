use crate::common::fixture_repo::{
    assert_safe_target, development_repo_root, fixture_path, snapshot_tree, TestRepo,
};
use std::fs;

#[test]
fn fixture_copy_is_a_clean_git_repository_with_real_verification() {
    let fixture = fixture_path("minimal-service");
    let before = snapshot_tree(&fixture).expect("snapshot tracked fixture");
    let repo = TestRepo::from_fixture("minimal-service");

    assert_ne!(
        repo.path().canonicalize().unwrap(),
        development_repo_root(),
        "fixture working copy must not be the Pulse development repository"
    );
    assert!(repo.path().join(".git").is_dir());
    assert!(!fixture.join(".git").exists());
    assert!(!fixture.join(".pulse").exists());
    assert!(repo.git_is_clean());
    assert_eq!(repo.git_head().len(), 40);

    let verification = repo.run_verify();
    assert!(
        verification.status.success(),
        "fixture verification failed: stdout={} stderr={}",
        String::from_utf8_lossy(&verification.stdout),
        String::from_utf8_lossy(&verification.stderr)
    );

    let after = snapshot_tree(&fixture).expect("snapshot tracked fixture after verification");
    assert_eq!(
        before, after,
        "verification must not mutate tracked fixture"
    );
}

#[test]
fn pulse_mutates_only_the_temporary_fixture_copy() {
    let fixture = fixture_path("minimal-service");
    let before = snapshot_tree(&fixture).expect("snapshot tracked fixture");
    let repo = TestRepo::from_fixture("minimal-service");

    let bootstrap = repo.pulse_ok(&["graph", "bootstrap", "--json"]);
    assert_eq!(bootstrap["code"], "bootstrapped");

    let created = repo.pulse_ok(&[
        "work",
        "create",
        "--kind",
        "ticket",
        "--title",
        "Classify refresh-token failures",
        "--role",
        "implementation",
        "--risk",
        "low",
        "--materialization",
        "R0",
        "--json",
    ]);
    let work_id = created["value"]["id"].as_str().expect("created work ID");

    assert!(repo.path().join(".pulse/workgraph/manifest.json").is_file());
    assert!(repo
        .path()
        .join(".pulse/workgraph/nodes")
        .join(format!("{work_id}.json"))
        .is_file());
    assert!(!fixture.join(".pulse").exists());

    let after = snapshot_tree(&fixture).expect("snapshot tracked fixture after Pulse mutation");
    assert_eq!(
        before, after,
        "Pulse must not mutate tracked fixture source"
    );
}

#[test]
fn core_query_stays_offline_when_daemon_is_stopped() {
    let repo = TestRepo::from_fixture("minimal-service");
    let daemon_home = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(crate::common_bin::resolve_pulse_bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args(["work", "list", "--json"])
        .env("PULSE_DAEMON_HOME", daemon_home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "offline Core query failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(std::fs::read_dir(daemon_home.path())
        .unwrap()
        .next()
        .is_none());
}

#[test]
fn fixture_working_copies_are_isolated() {
    let first = TestRepo::from_fixture("minimal-service");
    let second = TestRepo::from_fixture("minimal-service");

    fs::write(
        first.path().join("src/token.mjs"),
        "export const changedOnlyInFirstCopy = true;\n",
    )
    .unwrap();

    assert!(!first.git_is_clean());
    assert!(second.git_is_clean());
    assert_ne!(
        fs::read(first.path().join("src/token.mjs")).unwrap(),
        fs::read(second.path().join("src/token.mjs")).unwrap()
    );
}

#[test]
fn safety_guard_rejects_development_repo_and_tracked_fixture() {
    let development_root = development_repo_root();
    let fixture = fixture_path("minimal-service");
    let temp = tempfile::tempdir().unwrap();

    assert!(assert_safe_target(&development_root).is_err());
    assert!(assert_safe_target(&fixture).is_err());
    assert!(assert_safe_target(temp.path()).is_ok());
}
