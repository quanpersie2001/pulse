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

fn run_err(repo: &TempDir, args: &[&str]) -> Value {
    let output = run(repo, args);
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stderr).expect("json stderr")
}

fn setup_repo() -> TempDir {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join("docs/domain")).unwrap();
    fs::write(repo.path().join("docs/domain/auth.md"), b"auth").unwrap();
    fs::write(repo.path().join("docs/domain/optional.md"), b"optional").unwrap();
    run_ok(&repo, &["evidence", "bootstrap", "--json"]);
    run_ok(&repo, &["graph", "bootstrap", "--json"]);
    fs::create_dir_all(repo.path().join(".pulse/docs")).unwrap();
    let manifest: Value = serde_json::from_slice(
        &fs::read(repo.path().join(".pulse/evidence/manifest.json")).unwrap(),
    )
    .unwrap();
    let registry = json!({
        "schema_version": 1,
        "revision": 1,
        "repository_id": manifest["repository_id"].as_str().unwrap(),
        "retrieval": {
            "schema_version": 1,
            "default_search_limit": 8,
            "default_get_max_lines": 120,
            "default_get_max_bytes": 32768
        },
        "documents": [
            {
                "id": "DOC-AUTH-DOMAIN",
                "revision": 1,
                "path": "docs/domain/auth.md",
                "kind": "domain",
                "status": "approved",
                "owner": "team:docs",
                "summary": "Auth domain",
                "scope": {"paths": []},
                "tags": [],
                "generated": null,
                "superseded_by": null
            },
            {
                "id": "DOC-OPTIONAL-DOMAIN",
                "revision": 1,
                "path": "docs/domain/optional.md",
                "kind": "domain",
                "status": "approved",
                "owner": "team:docs",
                "summary": "Optional auth",
                "scope": {"paths": ["src/auth/**"]},
                "tags": [],
                "generated": null,
                "superseded_by": null
            }
        ]
    });
    fs::write(
        repo.path().join(".pulse/docs/registry.json"),
        serde_json::to_vec_pretty(&registry).unwrap(),
    )
    .unwrap();
    run_ok(&repo, &["docs", "index", "--json"]);
    repo
}

#[test]
fn docs_list_show_validate_json_contracts_are_stable() {
    let repo = setup_repo();

    let list = run_ok(&repo, &["docs", "list", "--json"]);
    assert_eq!(list["schema_version"], 1);
    assert_eq!(list["code"], "ok");
    assert_eq!(list["documents"].as_array().unwrap().len(), 2);
    assert_eq!(list["documents"][0]["id"], "DOC-AUTH-DOMAIN");

    let filtered = run_ok(&repo, &["docs", "list", "--kind", "domain", "--json"]);
    assert_eq!(filtered["documents"].as_array().unwrap().len(), 2);

    let shown = run_ok(&repo, &["docs", "show", "DOC-AUTH-DOMAIN", "--json"]);
    assert_eq!(shown["schema_version"], 1);
    assert_eq!(shown["document"]["path"], "docs/domain/auth.md");

    let valid = run_ok(&repo, &["docs", "validate", "--json"]);
    assert_eq!(valid["schema_version"], 1);
    assert_eq!(valid["valid"], true);
}

#[test]
fn docs_applicable_uses_work_metadata_and_reports_unknown_gap() {
    let repo = setup_repo();
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
    let id = created["value"]["id"].as_str().unwrap();

    let unknown = run_ok(&repo, &["docs", "applicable", "--work", id, "--json"]);
    assert_eq!(unknown["schema_version"], 1);
    assert_eq!(unknown["gate"]["status"], "incomplete");
    assert!(unknown["gate"]["reason_codes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|code| code == "documentation_impact_unknown"));

    run_ok(
        &repo,
        &[
            "docs",
            "impact",
            id,
            "--expected-revision",
            "1",
            "--posture",
            "required",
            "--required-doc",
            "DOC-AUTH-DOMAIN",
            "--path",
            "src/auth/login.rs",
            "--actor",
            "human:test",
            "--json",
        ],
    );

    let applicable = run_ok(&repo, &["docs", "applicable", "--work", id, "--json"]);
    assert_eq!(applicable["gate"]["status"], "complete");
    assert_eq!(applicable["required"][0]["id"], "DOC-AUTH-DOMAIN");
    assert_eq!(applicable["optional"][0]["id"], "DOC-OPTIONAL-DOMAIN");
    assert_eq!(applicable["write_candidates"][0]["id"], "DOC-AUTH-DOMAIN");
}

#[test]
fn docs_show_missing_and_validate_invalid_emit_json_errors() {
    let repo = setup_repo();
    let missing = run_err(&repo, &["docs", "show", "DOC-NOT-FOUND", "--json"]);
    assert_eq!(missing["schema_version"], 1);
    assert_eq!(missing["code"], "not_found");

    let mut registry: Value =
        serde_json::from_slice(&fs::read(repo.path().join(".pulse/docs/registry.json")).unwrap())
            .unwrap();
    registry["documents"][0]["path"] = json!("../escape.md");
    fs::write(
        repo.path().join(".pulse/docs/registry.json"),
        serde_json::to_vec_pretty(&registry).unwrap(),
    )
    .unwrap();
    let output = run(&repo, &["docs", "validate", "--json"]);
    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["checks"][0]["result"], "failed");
    assert_eq!(report["checks"][1]["result"], "skipped");
    assert_eq!(report["checks"][2]["result"], "skipped");
    assert_eq!(report["checks"][3]["result"], "skipped");
    let invalid: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(invalid["code"], "invalid_docs_registry");
}

#[test]
fn docs_registry_mutation_cli_contracts_are_stable() {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join("docs/domain")).unwrap();
    fs::write(repo.path().join("docs/domain/auth.md"), b"auth").unwrap();
    fs::write(
        repo.path().join("docs/domain/auth-renamed.md"),
        b"auth renamed",
    )
    .unwrap();
    fs::write(
        repo.path().join("docs/domain/replacement.md"),
        b"replacement",
    )
    .unwrap();
    fs::write(repo.path().join("docs/domain/retired.md"), b"retired").unwrap();

    let record = |id: &str, path: &str| {
        json!({
            "id": id,
            "revision": 99,
            "path": path,
            "kind": "domain",
            "status": "approved",
            "owner": "team:docs",
            "summary": format!("Summary for {id}"),
            "scope": {"paths": ["src/auth/**"]},
            "tags": [],
            "generated": null,
            "superseded_by": null
        })
    };
    let first_file = repo.path().join("first-document.json");
    fs::write(
        &first_file,
        serde_json::to_vec_pretty(&record("DOC-AUTH-DOMAIN", "docs/domain/auth.md")).unwrap(),
    )
    .unwrap();
    let first_file = first_file.to_string_lossy().to_string();

    let registered = run_ok(
        &repo,
        &[
            "docs",
            "register",
            "--file",
            &first_file,
            "--expected-registry-revision",
            "1",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(registered["code"], "registered");
    assert_eq!(registered["registry_revision"], 2);
    assert_eq!(registered["value"]["revision"], 1);

    let patch_file = repo.path().join("patch.json");
    fs::write(
        &patch_file,
        serde_json::to_vec_pretty(&json!({"path": "docs/domain/auth-renamed.md"})).unwrap(),
    )
    .unwrap();
    let patch_file = patch_file.to_string_lossy().to_string();
    let edited = run_ok(
        &repo,
        &[
            "docs",
            "edit",
            "DOC-AUTH-DOMAIN",
            "--patch",
            &patch_file,
            "--expected-registry-revision",
            "2",
            "--expected-document-revision",
            "1",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(edited["code"], "updated");
    assert_eq!(edited["value"]["id"], "DOC-AUTH-DOMAIN");
    assert_eq!(edited["value"]["revision"], 2);
    assert_eq!(edited["value"]["path"], "docs/domain/auth-renamed.md");

    let replacement_file = repo.path().join("replacement-document.json");
    fs::write(
        &replacement_file,
        serde_json::to_vec_pretty(&record(
            "DOC-AUTH-REPLACEMENT",
            "docs/domain/replacement.md",
        ))
        .unwrap(),
    )
    .unwrap();
    let replacement_file = replacement_file.to_string_lossy().to_string();
    let replacement = run_ok(
        &repo,
        &[
            "docs",
            "register",
            "--file",
            &replacement_file,
            "--expected-registry-revision",
            "3",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(replacement["registry_revision"], 4);

    let retire_file = repo.path().join("retire-document.json");
    fs::write(
        &retire_file,
        serde_json::to_vec_pretty(&record("DOC-AUTH-RETIRED", "docs/domain/retired.md")).unwrap(),
    )
    .unwrap();
    let retire_file = retire_file.to_string_lossy().to_string();
    let retire_candidate = run_ok(
        &repo,
        &[
            "docs",
            "register",
            "--file",
            &retire_file,
            "--expected-registry-revision",
            "4",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(retire_candidate["registry_revision"], 5);

    let superseded = run_ok(
        &repo,
        &[
            "docs",
            "supersede",
            "DOC-AUTH-DOMAIN",
            "--by",
            "DOC-AUTH-REPLACEMENT",
            "--reason",
            "replacement approved",
            "--expected-registry-revision",
            "5",
            "--expected-document-revision",
            "2",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(superseded["code"], "superseded");
    assert_eq!(superseded["value"]["status"], "retired");
    assert_eq!(superseded["value"]["superseded_by"], "DOC-AUTH-REPLACEMENT");

    let retired = run_ok(
        &repo,
        &[
            "docs",
            "retire",
            "DOC-AUTH-RETIRED",
            "--reason",
            "obsolete",
            "--expected-registry-revision",
            "6",
            "--expected-document-revision",
            "1",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(retired["code"], "retired");
    assert_eq!(retired["value"]["status"], "retired");
}
