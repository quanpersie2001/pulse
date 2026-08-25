use crate::common::fixture_repo::{fixture_path, snapshot_tree, TestRepo};
use pulse::canonical_json::to_canonical_bytes;
use pulse::policy::{AuthorityPolicy, AuthorityPrincipal};
use pulse::storage::NODE_SCHEMA_JSON;
use serde_json::Value;
use std::fs;

#[test]
fn public_init_enrolls_fixture_preserves_user_files_and_is_idempotent() {
    let fixture = fixture_path("minimal-service");
    let fixture_before = snapshot_tree(&fixture).unwrap();
    let repo = TestRepo::from_fixture("minimal-service");
    let ignore_before = fs::read(repo.path().join(".gitignore")).unwrap();
    let readme_before = fs::read(repo.path().join("README.md")).unwrap();
    let docs_before = fs::read(repo.path().join("docs/product/authentication.md")).unwrap();

    let first = repo.pulse_ok(&["init", "--json"]);
    assert_eq!(first["schema_version"], 1);
    assert_eq!(first["code"], "repository_initialized");
    assert_eq!(first["status"], "initialized");
    assert_eq!(first["authority_policy_revision"], 1);
    assert_eq!(
        first["proposed_ignore_entries"],
        serde_json::json!([".pulse/runtime/", ".pulse/cache/"])
    );
    let repository_id = first["repository_id"].as_str().unwrap();
    assert!(repository_id.starts_with("repo_"));

    for path in [
        "docs",
        "works",
        "knowledge/learnings",
        ".pulse/events",
        ".pulse/workgraph/manifest.json",
        ".pulse/evidence/manifest.json",
        ".pulse/docs/registry.json",
        ".pulse/knowledge/manifest.json",
        ".pulse/policy/authority.json",
        ".pulse/runtime/transactions",
        ".pulse/cache",
    ] {
        assert!(repo.path().join(path).exists(), "init should create {path}");
    }

    let evidence = pulse::evidence::manifest::load_existing(repo.path())
        .unwrap()
        .unwrap();
    let docs = pulse::docs::manifest::load_existing(repo.path())
        .unwrap()
        .unwrap();
    let knowledge = pulse::knowledge::manifest::load_existing(repo.path())
        .unwrap()
        .unwrap();
    assert_eq!(evidence.repository_id, repository_id);
    assert_eq!(docs.repository_id, repository_id);
    assert_eq!(knowledge.repository_id, repository_id);

    let authority = pulse::policy::load_authority_policy(repo.path()).unwrap();
    assert!(authority.available);
    assert!(authority.valid);
    assert!(authority.principals.is_empty());
    assert_eq!(authority.policy_revision, Some(1));

    assert_eq!(
        fs::read(repo.path().join(".gitignore")).unwrap(),
        ignore_before
    );
    assert_eq!(
        fs::read(repo.path().join("README.md")).unwrap(),
        readme_before
    );
    assert_eq!(
        fs::read(repo.path().join("docs/product/authentication.md")).unwrap(),
        docs_before
    );

    let second = repo.pulse_ok(&["init", "--json"]);
    assert_eq!(second["status"], "unchanged");
    assert_eq!(second["repository_id"], repository_id);
    assert_eq!(second["created"], serde_json::json!([]));

    let fixture_after = snapshot_tree(&fixture).unwrap();
    assert_eq!(fixture_before, fixture_after);
    assert!(!fixture.join(".pulse").exists());
}

#[test]
fn public_init_completes_safe_partial_workgraph_and_preserves_authority() {
    let repo = TestRepo::from_fixture("minimal-service");
    let schema_path = repo
        .path()
        .join(".pulse/workgraph/schemas/node.schema.json");
    fs::create_dir_all(schema_path.parent().unwrap()).unwrap();
    fs::write(&schema_path, NODE_SCHEMA_JSON).unwrap();

    write_canonical_schema(
        &repo
            .path()
            .join(".pulse/evidence/schemas/receipt-envelope.schema.json"),
        pulse::evidence::manifest::RECEIPT_ENVELOPE_SCHEMA,
    );
    fs::create_dir_all(repo.path().join(".pulse/evidence/artifacts/sha256")).unwrap();
    fs::create_dir_all(repo.path().join(".pulse/evidence/receipts")).unwrap();
    write_canonical_schema(
        &repo.path().join(".pulse/docs/schemas/document.schema.json"),
        pulse::docs::manifest::DOCUMENT_SCHEMA,
    );
    write_canonical_schema(
        &repo
            .path()
            .join(".pulse/knowledge/schemas/learning.schema.json"),
        pulse::knowledge::manifest::LEARNING_SCHEMA,
    );
    fs::create_dir_all(repo.path().join(".pulse/knowledge/entries")).unwrap();
    fs::create_dir_all(repo.path().join(".pulse/knowledge/relations")).unwrap();

    let policy_path = repo.path().join(".pulse/policy/authority.json");
    fs::create_dir_all(policy_path.parent().unwrap()).unwrap();
    let policy = AuthorityPolicy {
        schema_version: 1,
        revision: 7,
        principals: vec![AuthorityPrincipal {
            kind: pulse::identity::actor::ActorKind::Human,
            id: "maintainer".to_string(),
            grants: vec!["shape.apply".to_string()],
        }],
    };
    let policy_bytes = to_canonical_bytes(&policy).unwrap();
    fs::write(&policy_path, &policy_bytes).unwrap();

    let report = repo.pulse_ok(&["init", "--json"]);
    assert_eq!(report["status"], "initialized");
    assert_eq!(report["authority_policy_revision"], 7);
    assert!(repo.path().join(".pulse/workgraph/manifest.json").is_file());
    assert_eq!(fs::read(&policy_path).unwrap(), policy_bytes);
}

#[test]
fn public_init_refuses_late_domain_drift_before_canonical_writes() {
    let repo = TestRepo::from_fixture("minimal-service");
    let drift_path = repo.path().join(".pulse/docs/schemas/document.schema.json");
    fs::create_dir_all(drift_path.parent().unwrap()).unwrap();
    fs::write(&drift_path, b"{\"title\":\"user-owned drift\"}\n").unwrap();
    let drift_before = fs::read(&drift_path).unwrap();
    let ignore_before = fs::read(repo.path().join(".gitignore")).unwrap();

    let output = repo.pulse(&["init", "--json"]);
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "docs_registry_schema_invalid");
    assert_eq!(fs::read(&drift_path).unwrap(), drift_before);
    assert_eq!(
        fs::read(repo.path().join(".gitignore")).unwrap(),
        ignore_before
    );

    for path in [
        ".pulse/workgraph",
        ".pulse/evidence",
        ".pulse/knowledge",
        ".pulse/policy",
        ".pulse/events",
        "works",
        "knowledge",
    ] {
        assert!(
            !repo.path().join(path).exists(),
            "preflight failure must not create canonical path {path}"
        );
    }
    assert!(repo.path().join(".pulse/runtime/locks").is_dir());
}

#[test]
fn public_init_refuses_managed_path_collision_without_overwrite() {
    let repo = TestRepo::from_fixture("minimal-service");
    let works = repo.path().join("works");
    fs::write(&works, b"user-owned file\n").unwrap();

    let output = repo.pulse(&["init", "--json"]);
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "repository_init_path_conflict");
    assert_eq!(fs::read(&works).unwrap(), b"user-owned file\n");
    assert!(!repo.path().join(".pulse/workgraph").exists());
}

#[cfg(unix)]
#[test]
fn public_init_refuses_managed_symlink_before_runtime_lock_creation() {
    use std::os::unix::fs::symlink;

    let repo = TestRepo::from_fixture("minimal-service");
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), repo.path().join("works")).unwrap();

    let output = repo.pulse(&["init", "--json"]);
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "repository_init_path_conflict");
    assert!(!repo.path().join(".pulse").exists());
    assert!(fs::read_dir(outside.path()).unwrap().next().is_none());
}

#[cfg(unix)]
#[test]
fn public_init_refuses_nested_managed_symlink_without_external_writes() {
    use std::os::unix::fs::symlink;

    let repo = TestRepo::from_fixture("minimal-service");
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".pulse/evidence")).unwrap();
    symlink(outside.path(), repo.path().join(".pulse/evidence/schemas")).unwrap();

    let output = repo.pulse(&["init", "--json"]);
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "repository_init_path_conflict");
    assert!(!repo.path().join(".pulse/runtime").exists());
    assert!(fs::read_dir(outside.path()).unwrap().next().is_none());
}

fn write_canonical_schema(path: &std::path::Path, schema: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let value: Value = serde_json::from_str(schema).unwrap();
    fs::write(path, to_canonical_bytes(&value).unwrap()).unwrap();
}
