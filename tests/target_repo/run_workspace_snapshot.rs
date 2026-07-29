use pulse::run::{WorkspaceCleanlinessV1, WorkspaceOperationStateV1, WorkspaceSnapshotStatusV1};
use pulse::source::{workspace_snapshot, workspace_snapshot_feasibility, WorkspaceSnapshotOptions};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[path = "../common/git.rs"]
mod git;

fn options(base: &str) -> WorkspaceSnapshotOptions {
    WorkspaceSnapshotOptions::feasibility_defaults("repo", "wt", "in_place", base)
}

#[test]
fn snapshot_is_deterministic_and_captured_at_excluded_from_identity() {
    let tmp = tempfile::tempdir().unwrap();
    git::commit_all(tmp.path());
    let base = git::git(tmp.path(), &["rev-parse", "HEAD"]);
    fs::write(tmp.path().join("README.md"), b"changed").unwrap();

    let mut first = options(&base);
    first.captured_at = "2026-01-01T00:00:00Z".to_string();
    let mut second = first.clone();
    second.captured_at = "2026-01-01T00:00:01Z".to_string();

    let one = workspace_snapshot(tmp.path(), &first).unwrap();
    let two = workspace_snapshot(tmp.path(), &second).unwrap();
    assert_eq!(one.snapshot_status, WorkspaceSnapshotStatusV1::Complete);
    assert_eq!(one.cleanliness, WorkspaceCleanlinessV1::Dirty);
    assert_eq!(one.snapshot_identity, two.snapshot_identity);
    assert_eq!(one.tracked_diff_identity, two.tracked_diff_identity);
}

#[test]
fn snapshot_ignores_pulse_runtime_but_not_canonical_pulse_state() {
    let tmp = tempfile::tempdir().unwrap();
    git::commit_all(tmp.path());
    let base = git::git(tmp.path(), &["rev-parse", "HEAD"]);
    fs::create_dir_all(tmp.path().join(".pulse/runtime/run/logs")).unwrap();
    fs::write(tmp.path().join(".pulse/runtime/run/logs/a.log"), b"runtime").unwrap();
    fs::create_dir_all(tmp.path().join(".pulse/cache/docs")).unwrap();
    fs::write(tmp.path().join(".pulse/cache/docs/index"), b"cache").unwrap();
    let snapshot = workspace_snapshot(tmp.path(), &options(&base)).unwrap();
    assert_eq!(
        snapshot.snapshot_status,
        WorkspaceSnapshotStatusV1::Complete
    );
    assert_eq!(snapshot.cleanliness, WorkspaceCleanlinessV1::Clean);

    fs::create_dir_all(tmp.path().join(".pulse/workgraph/nodes")).unwrap();
    fs::write(tmp.path().join(".pulse/workgraph/nodes/TK-1.json"), b"{}").unwrap();
    let snapshot = workspace_snapshot(tmp.path(), &options(&base)).unwrap();
    assert_eq!(snapshot.cleanliness, WorkspaceCleanlinessV1::Dirty);
}

#[test]
fn snapshot_detects_untracked_file_mode_symlink_and_huge_caps() {
    let tmp = tempfile::tempdir().unwrap();
    git::commit_all(tmp.path());
    let base = git::git(tmp.path(), &["rev-parse", "HEAD"]);
    fs::write(tmp.path().join("tool.sh"), b"#!/bin/sh\n").unwrap();
    let mut permissions = fs::metadata(tmp.path().join("tool.sh"))
        .unwrap()
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(tmp.path().join("tool.sh"), permissions).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("tool.sh", tmp.path().join("tool-link")).unwrap();

    let snapshot = workspace_snapshot(tmp.path(), &options(&base)).unwrap();
    assert_eq!(
        snapshot.snapshot_status,
        WorkspaceSnapshotStatusV1::Complete
    );
    assert_eq!(snapshot.cleanliness, WorkspaceCleanlinessV1::Dirty);
    assert!(snapshot.untracked_manifest_identity.starts_with("sha256:"));

    fs::write(tmp.path().join("huge.bin"), vec![7_u8; 32]).unwrap();
    let mut capped = options(&base);
    capped.max_untracked_file_bytes = 8;
    capped.max_untracked_total_bytes = 16;
    let snapshot = workspace_snapshot(tmp.path(), &capped).unwrap();
    assert_eq!(
        snapshot.snapshot_status,
        WorkspaceSnapshotStatusV1::BoundedOut
    );
}

#[test]
fn snapshot_hashes_tracked_binary_and_mode_changes() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("bin.dat"), [0, 159, 146, 150]).unwrap();
    git::commit_all(tmp.path());
    let base = git::git(tmp.path(), &["rev-parse", "HEAD"]);
    fs::write(tmp.path().join("bin.dat"), [0, 1, 2, 3, 255]).unwrap();
    let changed_bytes = workspace_snapshot(tmp.path(), &options(&base)).unwrap();

    git::git(tmp.path(), &["checkout", "--", "bin.dat"]);
    let mut permissions = fs::metadata(tmp.path().join("bin.dat"))
        .unwrap()
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(tmp.path().join("bin.dat"), permissions).unwrap();
    let mode_change = workspace_snapshot(tmp.path(), &options(&base)).unwrap();

    assert_eq!(changed_bytes.cleanliness, WorkspaceCleanlinessV1::Dirty);
    assert_eq!(mode_change.cleanliness, WorkspaceCleanlinessV1::Dirty);
    assert_ne!(
        changed_bytes.tracked_diff_identity,
        mode_change.tracked_diff_identity
    );
}

#[test]
fn tracked_diff_ignores_textconv_and_external_diff_transforms() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("constant-filter.sh"),
        "#!/bin/sh\nprintf constant\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(tmp.path().join("constant-filter.sh"))
        .unwrap()
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(tmp.path().join("constant-filter.sh"), permissions).unwrap();
    git::commit_all(tmp.path());

    fs::write(tmp.path().join("blob.bin"), b"first raw bytes").unwrap();
    fs::write(tmp.path().join(".gitattributes"), b"*.bin diff=constant\n").unwrap();
    git::git(
        tmp.path(),
        &["config", "diff.constant.textconv", "./constant-filter.sh"],
    );
    git::commit_all(tmp.path());
    let base = git::git(tmp.path(), &["rev-parse", "HEAD"]);

    fs::write(tmp.path().join("blob.bin"), b"second distinct raw bytes").unwrap();
    let snapshot = workspace_snapshot(tmp.path(), &options(&base)).unwrap();
    assert_eq!(
        snapshot.snapshot_status,
        WorkspaceSnapshotStatusV1::Complete
    );
    assert_eq!(snapshot.cleanliness, WorkspaceCleanlinessV1::Dirty);

    git::git(
        tmp.path(),
        &["config", "diff.external", "./constant-filter.sh"],
    );
    let with_external_config = workspace_snapshot(tmp.path(), &options(&base)).unwrap();
    assert_eq!(
        with_external_config.snapshot_status,
        WorkspaceSnapshotStatusV1::Complete
    );
    assert_eq!(
        with_external_config.cleanliness,
        WorkspaceCleanlinessV1::Dirty
    );
    assert_eq!(
        snapshot.tracked_diff_identity,
        with_external_config.tracked_diff_identity
    );
}

#[test]
fn snapshot_hashes_lfs_pointer_as_worktree_file_and_rejects_nested_repo() {
    let tmp = tempfile::tempdir().unwrap();
    git::commit_all(tmp.path());
    let base = git::git(tmp.path(), &["rev-parse", "HEAD"]);
    fs::write(
        tmp.path().join("asset.bin"),
        b"version https://git-lfs.github.com/spec/v1\noid sha256:abc\nsize 1\n",
    )
    .unwrap();
    let snapshot = workspace_snapshot(tmp.path(), &options(&base)).unwrap();
    assert_eq!(
        snapshot.snapshot_status,
        WorkspaceSnapshotStatusV1::Complete
    );

    fs::create_dir_all(tmp.path().join("nested")).unwrap();
    git::git(&tmp.path().join("nested"), &["init"]);
    let snapshot = workspace_snapshot(tmp.path(), &options(&base)).unwrap();
    assert_eq!(
        snapshot.snapshot_status,
        WorkspaceSnapshotStatusV1::Unsupported
    );
}

#[test]
fn snapshot_reports_git_operation_and_diff_base_mismatch() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("README.md"), b"base\n").unwrap();
    git::commit_all(tmp.path());
    let base = git::git(tmp.path(), &["rev-parse", "HEAD"]);
    fs::write(tmp.path().join(".git/MERGE_HEAD"), &base).unwrap();
    let snapshot = workspace_snapshot(tmp.path(), &options(&base)).unwrap();
    assert_eq!(
        snapshot.snapshot_status,
        WorkspaceSnapshotStatusV1::Unsupported
    );
    assert_eq!(snapshot.operation_state, WorkspaceOperationStateV1::Merge);

    let mut bad = options(&base);
    bad.diff_base_commit = "0000000000000000000000000000000000000000".to_string();
    let snapshot = workspace_snapshot(tmp.path(), &bad).unwrap();
    assert_eq!(
        snapshot.snapshot_status,
        WorkspaceSnapshotStatusV1::Unsupported
    );
}

#[test]
fn snapshot_bounds_tracked_diff_and_status_output() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("large.bin"), vec![1_u8; 4096]).unwrap();
    git::commit_all(tmp.path());
    let base = git::git(tmp.path(), &["rev-parse", "HEAD"]);
    fs::write(tmp.path().join("large.bin"), vec![2_u8; 4096]).unwrap();
    let mut capped = options(&base);
    capped.max_tracked_diff_bytes = 128;
    let snapshot = workspace_snapshot(tmp.path(), &capped).unwrap();
    assert_eq!(
        snapshot.snapshot_status,
        WorkspaceSnapshotStatusV1::BoundedOut
    );

    let mut status_capped = options(&base);
    status_capped.max_status_bytes = 4;
    let snapshot = workspace_snapshot(tmp.path(), &status_capped).unwrap();
    assert_eq!(
        snapshot.snapshot_status,
        WorkspaceSnapshotStatusV1::BoundedOut
    );
}

#[test]
fn ignored_file_in_scope_is_not_silently_hidden() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join(".gitignore"), b"*.secret\n").unwrap();
    git::commit_all(tmp.path());
    let base = git::git(tmp.path(), &["rev-parse", "HEAD"]);
    fs::write(tmp.path().join("notes.secret"), b"worker ignored change").unwrap();
    let snapshot = workspace_snapshot(tmp.path(), &options(&base)).unwrap();
    assert_eq!(
        snapshot.snapshot_status,
        WorkspaceSnapshotStatusV1::Unsupported
    );
}

#[test]
fn feasibility_wrapper_preserves_i0_surface() {
    let tmp = tempfile::tempdir().unwrap();
    git::commit_all(tmp.path());
    let base = git::git(tmp.path(), &["rev-parse", "HEAD"]);
    let snapshot = workspace_snapshot_feasibility(tmp.path(), &options(&base)).unwrap();
    assert_eq!(snapshot.schema_version, 1);
    assert_eq!(snapshot.snapshot_status, "complete");
}

#[cfg(unix)]
#[test]
fn snapshot_rejects_special_files() {
    let tmp = tempfile::tempdir().unwrap();
    git::commit_all(tmp.path());
    let base = git::git(tmp.path(), &["rev-parse", "HEAD"]);
    let status = Command::new("mkfifo")
        .arg(tmp.path().join("pipe"))
        .status()
        .unwrap();
    assert!(status.success());
    let snapshot = workspace_snapshot(tmp.path(), &options(&base)).unwrap();
    assert_eq!(
        snapshot.snapshot_status,
        WorkspaceSnapshotStatusV1::Unsupported
    );
}

#[cfg(unix)]
#[test]
fn scoped_snapshot_special_file_and_nested_repo_scans_honor_scope_and_runtime_exclusions() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("src-other/src2")).unwrap();
    fs::write(tmp.path().join("src-other/file.txt"), b"tracked").unwrap();
    git::commit_all(tmp.path());
    let base = git::git(tmp.path(), &["rev-parse", "HEAD"]);

    let status = Command::new("mkfifo")
        .arg(tmp.path().join("outside.pipe"))
        .status()
        .unwrap();
    assert!(status.success());
    git::git(&tmp.path().join("src-other/src2"), &["init"]);
    fs::create_dir_all(tmp.path().join(".pulse/runtime/run/tmp")).unwrap();
    let status = Command::new("mkfifo")
        .arg(tmp.path().join(".pulse/runtime/run/tmp/runtime.pipe"))
        .status()
        .unwrap();
    assert!(status.success());

    let mut scoped = options(&base);
    scoped.included_paths = vec!["src-other/file.txt".to_string()];
    let snapshot = workspace_snapshot(tmp.path(), &scoped).unwrap();
    assert_eq!(
        snapshot.snapshot_status,
        WorkspaceSnapshotStatusV1::Complete
    );
    assert_eq!(snapshot.cleanliness, WorkspaceCleanlinessV1::Clean);

    let mut dir_scoped = options(&base);
    dir_scoped.included_paths = vec!["src-other".to_string()];
    let snapshot = workspace_snapshot(tmp.path(), &dir_scoped).unwrap();
    assert_eq!(
        snapshot.snapshot_status,
        WorkspaceSnapshotStatusV1::Unsupported
    );

    let mut prefix_confusion = options(&base);
    prefix_confusion.included_paths = vec!["src".to_string()];
    let snapshot = workspace_snapshot(tmp.path(), &prefix_confusion).unwrap();
    assert_eq!(
        snapshot.snapshot_status,
        WorkspaceSnapshotStatusV1::Complete
    );
    assert_eq!(snapshot.cleanliness, WorkspaceCleanlinessV1::Clean);
}

#[test]
fn snapshot_validates_base_commit_as_head_ancestor() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("README.md"), b"base\n").unwrap();
    git::commit_all(tmp.path());
    let base = git::git(tmp.path(), &["rev-parse", "HEAD"]);
    fs::write(tmp.path().join("README.md"), b"descendant\n").unwrap();
    git::commit_all(tmp.path());
    let descendant = git::git(tmp.path(), &["rev-parse", "HEAD"]);
    git::git(tmp.path(), &["checkout", "--detach", &descendant]);
    let snapshot = workspace_snapshot(tmp.path(), &options(&base)).unwrap();
    assert_eq!(
        snapshot.snapshot_status,
        WorkspaceSnapshotStatusV1::Complete
    );
    assert_eq!(snapshot.cleanliness, WorkspaceCleanlinessV1::Dirty);

    let mut missing = options("ffffffffffffffffffffffffffffffffffffffff");
    missing.diff_base_commit = missing.base_commit.clone();
    let snapshot = workspace_snapshot(tmp.path(), &missing).unwrap();
    assert_eq!(
        snapshot.snapshot_status,
        WorkspaceSnapshotStatusV1::Unsupported
    );

    let unrelated = tempfile::tempdir().unwrap();
    fs::write(unrelated.path().join("OTHER.md"), b"other\n").unwrap();
    git::commit_all(unrelated.path());
    let unrelated_base = git::git(unrelated.path(), &["rev-parse", "HEAD"]);
    git::git(
        tmp.path(),
        &["fetch", unrelated.path().to_str().unwrap(), &unrelated_base],
    );
    let snapshot = workspace_snapshot(tmp.path(), &options(&unrelated_base)).unwrap();
    assert_eq!(
        snapshot.snapshot_status,
        WorkspaceSnapshotStatusV1::Unsupported
    );

    git::git(tmp.path(), &["checkout", &base]);
    fs::write(tmp.path().join("README.md"), b"rebased line\n").unwrap();
    git::git(tmp.path(), &["commit", "-am", "rewrite from base"]);
    let rewritten = git::git(tmp.path(), &["rev-parse", "HEAD"]);
    let mut non_ancestor = options(&descendant);
    non_ancestor.diff_base_commit = descendant;
    let snapshot = workspace_snapshot(tmp.path(), &non_ancestor).unwrap();
    assert_eq!(rewritten, snapshot.head_commit);
    assert_eq!(
        snapshot.snapshot_status,
        WorkspaceSnapshotStatusV1::Unsupported
    );
}
