use serde_json::{json, Value};
use std::fs;
use std::process::Command;
use tempfile::TempDir;

use crate::common_bin::bin;

fn run(repo: &TempDir, args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args(args)
        .output()
        .expect("run pulse")
}

fn run_ok(repo: &TempDir, args: &[&str]) -> Value {
    let output = run(repo, args);
    assert!(
        output.status.success(),
        "command failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("json stdout")
}

fn write_doc(repo: &TempDir, path: &str) {
    let full = repo.path().join(path);
    fs::create_dir_all(full.parent().expect("doc parent")).expect("create doc parent");
    fs::write(full, format!("# {path}\n")).expect("write doc");
}

fn write_document_record(repo: &TempDir, id: &str, path: &str) -> String {
    let file = repo.path().join(format!("{id}.json"));
    let record = json!({
        "id": id,
        "revision": 1,
        "path": path,
        "kind": "domain",
        "authority": "approved",
        "lifecycle": "current",
        "owner": "team:docs",
        "summary": format!("Summary for {id}"),
        "aliases": [],
        "scope": {"paths": ["src/auth/**"], "domains": ["authentication"], "work_labels": ["auth"]},
        "review_policy": "none",
        "verification_profile": "domain-doc",
        "generated": null,
        "superseded_by": null
    });
    fs::write(
        &file,
        serde_json::to_vec_pretty(&record).expect("record json"),
    )
    .expect("write record");
    file.to_string_lossy().to_string()
}

#[test]
fn two_processes_registering_same_registry_revision_have_one_winner() {
    let repo = tempfile::tempdir().expect("temp repo");
    write_doc(&repo, "docs/domain/one.md");
    write_doc(&repo, "docs/domain/two.md");
    let first_file = write_document_record(&repo, "DOC-AUTH-ONE", "docs/domain/one.md");
    let second_file = write_document_record(&repo, "DOC-AUTH-TWO", "docs/domain/two.md");

    let mut first = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "docs",
            "register",
            "--file",
            &first_file,
            "--expected-registry-revision",
            "1",
            "--actor",
            "human:test",
            "--json",
        ])
        .spawn()
        .expect("spawn first");
    let mut second = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "docs",
            "register",
            "--file",
            &second_file,
            "--expected-registry-revision",
            "1",
            "--actor",
            "human:test",
            "--json",
        ])
        .spawn()
        .expect("spawn second");

    let first_status = first.wait().expect("first status");
    let second_status = second.wait().expect("second status");
    assert_ne!(first_status.success(), second_status.success());

    let list = run_ok(&repo, &["docs", "list", "--json"]);
    assert_eq!(list["documents"].as_array().expect("documents").len(), 1);
    assert!(list["documents"][0]["id"]
        .as_str()
        .expect("document id")
        .starts_with("DOC-AUTH-"));
}

#[test]
fn two_processes_setting_same_ticket_documentation_revision_have_one_winner() {
    let repo = tempfile::tempdir().expect("temp repo");
    let created = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Docs",
            "--role",
            "implementation",
            "--risk",
            "low",
            "--materialization",
            "R0",
            "--json",
        ],
    );
    let id = created["value"]["id"]
        .as_str()
        .expect("ticket id")
        .to_string();

    let mut first = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "docs",
            "impact",
            &id,
            "--expected-revision",
            "1",
            "--posture",
            "required",
            "--rationale",
            "Auth behavior changes.",
            "--required-doc",
            "DOC-AUTH-DOMAIN",
            "--actor",
            "human:test",
            "--json",
        ])
        .spawn()
        .expect("spawn first");
    let mut second = Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args([
            "docs",
            "impact",
            &id,
            "--expected-revision",
            "1",
            "--posture",
            "none",
            "--rationale",
            "No docs change.",
            "--actor",
            "human:test",
            "--json",
        ])
        .spawn()
        .expect("spawn second");

    let first_status = first.wait().expect("first status");
    let second_status = second.wait().expect("second status");
    assert_ne!(first_status.success(), second_status.success());

    let shown = run_ok(&repo, &["work", "show", &id, "--json"]);
    assert_eq!(shown["node"]["revision"], 2);
    assert!(shown["node"]["documentation"].is_object());
}
