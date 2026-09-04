use std::fs;

use pulse::canonical_json::to_canonical_bytes;
use pulse::docs::{bootstrap as docs_bootstrap, DocsRegistry, RetrievalConfig};

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

    let registry: DocsRegistry =
        pulse::storage::read_json(&repo.join(".pulse/docs/registry.json")).unwrap();
    assert_eq!(registry, outcome.registry);
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
