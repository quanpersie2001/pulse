use std::fs;

use pulse::canonical_json::{hash_bytes, to_canonical_bytes};
use pulse::docs::{bootstrap as docs_bootstrap, DocsRegistry, RetrievalConfig, DOCUMENT_SCHEMA};

fn current_schema_hash() -> String {
    let schema: serde_json::Value = serde_json::from_str(DOCUMENT_SCHEMA).unwrap();
    hash_bytes(&to_canonical_bytes(&schema).unwrap())
}

#[test]
fn bootstrap_writes_current_registry_schema_version_one() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let evidence = pulse::evidence::manifest::bootstrap(repo).unwrap().manifest;

    let outcome = docs_bootstrap(repo).unwrap();

    assert_eq!(outcome.schema_version, 1);
    assert_eq!(outcome.registry.schema_version, 1);
    assert_eq!(outcome.registry.revision, 1);
    assert_eq!(outcome.registry.repository_id, evidence.repository_id);
    assert_eq!(
        outcome.registry.retrieval,
        Some(RetrievalConfig::defaults())
    );

    let schema_bytes = fs::read(repo.join(".pulse/docs/schemas/document.schema.json")).unwrap();
    assert_eq!(hash_bytes(&schema_bytes), current_schema_hash());

    let registry: DocsRegistry =
        pulse::storage::read_json(&repo.join(".pulse/docs/registry.json")).unwrap();
    assert_eq!(registry, outcome.registry);
}

#[test]
fn bootstrap_refuses_current_schema_drift_without_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    pulse::evidence::manifest::bootstrap(repo).unwrap();
    docs_bootstrap(repo).unwrap();

    let schema_path = repo.join(".pulse/docs/schemas/document.schema.json");
    let drifted_schema = serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "schema_version": { "const": 99 } }
    });
    let drifted_bytes = to_canonical_bytes(&drifted_schema).unwrap();
    fs::write(&schema_path, &drifted_bytes).unwrap();

    let err = docs_bootstrap(repo).unwrap_err();

    assert_eq!(err.code(), "docs_registry_schema_invalid");
    assert_eq!(fs::read(&schema_path).unwrap(), drifted_bytes);
}

#[test]
fn bootstrap_refuses_registry_envelope_schema_drift() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    pulse::evidence::manifest::bootstrap(repo).unwrap();
    docs_bootstrap(repo).unwrap();

    let registry_path = repo.join(".pulse/docs/registry.json");
    let mut registry: DocsRegistry = pulse::storage::read_json(&registry_path).unwrap();
    registry.schema_version = 99;
    let registry_bytes = to_canonical_bytes(&registry).unwrap();
    fs::write(&registry_path, &registry_bytes).unwrap();

    let err = docs_bootstrap(repo).unwrap_err();

    assert_eq!(err.code(), "docs_registry_schema_invalid");
    assert_eq!(fs::read(&registry_path).unwrap(), registry_bytes);
}
