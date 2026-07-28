use std::process::Command;

use crate::common::fixture_repo::TestRepo;
use pulse::workspace::{
    bind_in_place, cleanup_worktree, create_isolated_worktree, generate_workspace_id_with_suffix,
};

fn git(repo: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn enroll(repo: &TestRepo) -> String {
    let manifest = pulse::evidence::bootstrap(repo.path()).unwrap().manifest;
    pulse::docs::manifest::bootstrap(repo.path()).unwrap();
    std::fs::write(repo.path().join(".gitignore"), ".pulse/runtime/\n").unwrap();
    git(repo.path(), &["add", ".pulse", ".gitignore"]);
    git(repo.path(), &["commit", "-m", "enroll pulse"]);
    manifest.repository_id
}

#[test]
fn target_repo_in_place_binds_repository_identity_and_rejects_untracked() {
    let repo = TestRepo::from_fixture("minimal-service");
    let repository_id = enroll(&repo);
    let head = repo.git_head();

    let binding = bind_in_place(repo.path(), &head, &repository_id).unwrap();
    assert_eq!(binding.repository_id, repository_id);
    assert_eq!(binding.head_commit, head);

    std::fs::write(repo.path().join("untracked.txt"), b"new").unwrap();
    let err = bind_in_place(repo.path(), &binding.head_commit, &binding.repository_id).unwrap_err();
    assert_eq!(err.code(), "work_packet_dirty_source_unsupported");
}

#[test]
fn target_repo_isolated_worktree_is_detached_and_cleanup_refuses_dirty_state() {
    let repo = TestRepo::from_fixture("minimal-service");
    let repository_id = enroll(&repo);
    let head = repo.git_head();
    let runtime_root = repo.path().join(".pulse/runtime/workspaces");
    let workspace_id = generate_workspace_id_with_suffix("TK-TR", "detached");

    let binding = create_isolated_worktree(
        repo.path(),
        &runtime_root,
        &workspace_id,
        &head,
        &repository_id,
    )
    .unwrap();
    assert!(binding.was_newly_created);
    assert_eq!(binding.worktree_root_kind, "linked_worktree");
    assert_eq!(
        git(&binding.path, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "HEAD"
    );
    assert_eq!(git(&binding.path, &["rev-parse", "HEAD"]), head);

    std::fs::write(binding.path.join("operator-note.txt"), b"do not delete").unwrap();
    let err = cleanup_worktree(repo.path(), &binding.path).unwrap_err();
    assert_eq!(err.code(), "assignment_workspace_not_safe_to_remove");

    git(
        repo.path(),
        &[
            "worktree",
            "remove",
            "--force",
            binding.path.to_str().unwrap(),
        ],
    );
}
