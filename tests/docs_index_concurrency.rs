use pulse::canonical_json::to_canonical_bytes;
use pulse::docs::{
    build_index, open_reader_generation, read_current, DocsRegistry, DocsSearchWriteLock,
    DocumentAuthority, DocumentKind, DocumentLifecycle, DocumentRecord, DocumentScope,
    IndexOptions, RetrievalConfig, ReviewPolicy,
};
use std::fs;
use std::time::Duration;

fn doc(id: &str, path: &str) -> DocumentRecord {
    DocumentRecord {
        id: id.to_string(),
        revision: 1,
        path: path.to_string(),
        kind: DocumentKind::Domain,
        authority: DocumentAuthority::Approved,
        lifecycle: DocumentLifecycle::Current,
        owner: "team:docs".to_string(),
        summary: "Concurrency docs".to_string(),
        aliases: Vec::new(),
        scope: DocumentScope::default(),
        review_policy: ReviewPolicy::None,
        verification_profile: "domain-doc".to_string(),
        generated: None,
        superseded_by: None,
        retrieval: None,
    }
}

fn setup_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let manifest = pulse::evidence::manifest::bootstrap(repo).unwrap().manifest;
    fs::create_dir_all(repo.join(".pulse/docs/schemas")).unwrap();
    let schema: serde_json::Value = serde_json::from_str(pulse::docs::DOCUMENT_SCHEMA).unwrap();
    fs::write(
        repo.join(".pulse/docs/schemas/document.schema.json"),
        to_canonical_bytes(&schema).unwrap(),
    )
    .unwrap();
    fs::create_dir_all(repo.join("docs/domain")).unwrap();
    fs::write(repo.join("docs/domain/a.md"), b"# A\n\n## One\n\nalpha\n").unwrap();
    let registry = DocsRegistry {
        schema_version: 1,
        revision: 1,
        repository_id: manifest.repository_id,
        documents: vec![doc("DOC-A-DOMAIN", "docs/domain/a.md")],
        retrieval: Some(RetrievalConfig::defaults()),
    };
    fs::write(
        repo.join(".pulse/docs/registry.json"),
        to_canonical_bytes(&registry).unwrap(),
    )
    .unwrap();
    tmp
}

#[test]
fn stale_orphan_build_dir_does_not_affect_current_reader() {
    let tmp = setup_repo();
    let repo = tmp.path();
    build_index(repo, IndexOptions::default()).unwrap();
    let first = read_current(repo).unwrap();
    fs::create_dir_all(repo.join(".pulse/cache/docs-search/builds/build_orphan/tantivy")).unwrap();
    assert_eq!(read_current(repo).unwrap(), first);
    assert!(open_reader_generation(repo).unwrap().is_some());
}

#[test]
fn published_generation_opens_as_complete() {
    let tmp = setup_repo();
    let repo = tmp.path();
    build_index(repo, IndexOptions::default()).unwrap();
    let before = read_current(repo).unwrap();
    fs::write(repo.join("docs/domain/a.md"), b"# A\n\n## Two\n\nbeta\n").unwrap();
    build_index(repo, IndexOptions::default()).unwrap();
    let after = read_current(repo).unwrap();
    assert_ne!(before, after);
    let generation = open_reader_generation(repo).unwrap().unwrap();
    assert_eq!(generation.state.generation_id, after);
    assert!(generation.sections_path.exists());
    assert!(generation.tantivy_path.exists());
}

#[test]
fn docs_search_lock_excludes_second_writer() {
    let tmp = setup_repo();
    let repo = tmp.path();
    let _held = DocsSearchWriteLock::acquire(repo).unwrap();
    let err =
        DocsSearchWriteLock::acquire_with_timeout(repo, Duration::from_millis(50)).unwrap_err();
    assert_eq!(err.code(), "lock_timeout");
}

#[test]
fn repeated_index_writers_publish_one_valid_current_generation() {
    let tmp = setup_repo();
    let repo = tmp.path();
    let first = build_index(repo, IndexOptions::default()).unwrap();
    let second = build_index(repo, IndexOptions::default()).unwrap();
    assert_eq!(first.index.fingerprint, second.index.fingerprint);
    assert!(open_reader_generation(repo).unwrap().is_some());
}
