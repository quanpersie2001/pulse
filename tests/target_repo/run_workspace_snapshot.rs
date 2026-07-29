use pulse::source::{workspace_snapshot_feasibility, WorkspaceSnapshotOptions};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[path = "../common/git.rs"]
mod git;

#[test]
fn snapshot_ignores_pulse_runtime_but_not_canonical_pulse_state() {
    let tmp = tempfile::tempdir().unwrap();
    git::commit_all(tmp.path());
    let base = git::git(tmp.path(), &["rev-parse", "HEAD"]);
    fs::create_dir_all(tmp.path().join(".pulse/runtime/run/logs")).unwrap();
    fs::write(tmp.path().join(".pulse/runtime/run/logs/a.log"), b"runtime").unwrap();
    let options = WorkspaceSnapshotOptions::feasibility_defaults("repo", "wt", "in_place", &base);
    let snapshot = workspace_snapshot_feasibility(tmp.path(), &options).unwrap();
    assert_eq!(snapshot.snapshot_status, "complete");
    assert_eq!(snapshot.cleanliness, "clean");

    fs::create_dir_all(tmp.path().join(".pulse/workgraph/nodes")).unwrap();
    fs::write(tmp.path().join(".pulse/workgraph/nodes/TK-1.json"), b"{}").unwrap();
    let snapshot = workspace_snapshot_feasibility(tmp.path(), &options).unwrap();
    assert_eq!(snapshot.cleanliness, "dirty");
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

    let options =
        WorkspaceSnapshotOptions::feasibility_defaults("repo", "wt", "isolated_worktree", &base);
    let snapshot = workspace_snapshot_feasibility(tmp.path(), &options).unwrap();
    assert_eq!(snapshot.snapshot_status, "complete");
    assert_eq!(snapshot.cleanliness, "dirty");
    assert!(snapshot.untracked_manifest_identity.starts_with("sha256:"));

    fs::write(tmp.path().join("huge.bin"), vec![7_u8; 32]).unwrap();
    let mut capped = options.clone();
    capped.max_untracked_file_bytes = 8;
    capped.max_untracked_total_bytes = 16;
    let snapshot = workspace_snapshot_feasibility(tmp.path(), &capped).unwrap();
    assert_eq!(snapshot.snapshot_status, "bounded_out");
    assert!(snapshot
        .reason_codes
        .contains(&"untracked_manifest_bounded_out".to_string()));
}

#[test]
fn snapshot_hashes_lfs_pointer_as_worktree_file_and_rejects_submodule_gitlink() {
    let tmp = tempfile::tempdir().unwrap();
    git::commit_all(tmp.path());
    let base = git::git(tmp.path(), &["rev-parse", "HEAD"]);
    fs::write(
        tmp.path().join("asset.bin"),
        b"version https://git-lfs.github.com/spec/v1\noid sha256:abc\nsize 1\n",
    )
    .unwrap();
    let options = WorkspaceSnapshotOptions::feasibility_defaults("repo", "wt", "in_place", &base);
    let snapshot = workspace_snapshot_feasibility(tmp.path(), &options).unwrap();
    assert_eq!(snapshot.snapshot_status, "complete");

    let sub = tempfile::tempdir().unwrap();
    git::commit_all(sub.path());
    let status = Command::new("git")
        .current_dir(tmp.path())
        .args([
            "submodule",
            "add",
            sub.path().to_str().unwrap(),
            "vendor/sub",
        ])
        .output()
        .unwrap();
    if status.status.success() {
        let snapshot = workspace_snapshot_feasibility(tmp.path(), &options).unwrap();
        assert_ne!(snapshot.snapshot_status, "complete");
    }
}
