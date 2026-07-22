use pulse::canonical_json::to_canonical_bytes;
use pulse::docs::{
    build_index, check_index, current_generation, index_status, query_lexical_index, read_current,
    validate_generation, DocsRegistry, DocumentAuthority, DocumentKind, DocumentLifecycle,
    DocumentRecord, DocumentRetrieval, DocumentScope, IndexOptions, RetrievalConfig, ReviewPolicy,
};
use std::fs;

fn doc(id: &str, path: &str, summary: &str) -> DocumentRecord {
    DocumentRecord {
        id: id.to_string(),
        revision: 1,
        path: path.to_string(),
        kind: DocumentKind::Domain,
        authority: DocumentAuthority::Approved,
        lifecycle: DocumentLifecycle::Current,
        owner: "team:docs".to_string(),
        summary: summary.to_string(),
        aliases: vec!["refresh-token".to_string()],
        scope: DocumentScope {
            paths: vec!["src/auth/**".to_string()],
            domains: vec!["authentication".to_string()],
            work_labels: vec!["auth".to_string()],
        },
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
    fs::write(repo.join("docs/domain/token.md"), b"# Token Lifecycle\n\n## Expired tokens\n\nTokenExpired means a refresh-token expired in v2.1.\n").unwrap();
    let registry = DocsRegistry {
        schema_version: 2,
        revision: 1,
        repository_id: manifest.repository_id,
        documents: vec![doc(
            "DOC-AUTH-DOMAIN",
            "docs/domain/token.md",
            "Token lifecycle and refresh-token expiry",
        )],
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
fn initial_build_publishes_complete_generation_and_queryable_tantivy_index() {
    let tmp = setup_repo();
    let repo = tmp.path();
    let report = build_index(repo, IndexOptions::default()).unwrap();
    assert_eq!(report.index.state, "current");
    assert_eq!(report.documents.eligible, 1);
    assert!(report.sections >= 2);
    let current = read_current(repo).unwrap();
    let gen = validate_generation(repo, &current).unwrap();
    assert_eq!(gen.state.fingerprint, report.index.fingerprint.unwrap());
    assert!(gen.sections_path.exists());
    assert!(gen.tantivy_path.exists());
    let hits = query_lexical_index(
        &gen.tantivy_path,
        &[
            "refresh-token".to_string(),
            "v2.1".to_string(),
            "docs/domain/token.md".to_string(),
        ],
        5,
    )
    .unwrap();
    assert!(!hits.is_empty());
    assert!(hits
        .iter()
        .any(|hit| hit.section_ref.starts_with("DOC-AUTH-DOMAIN#")));
}

#[test]
fn delete_cache_and_rebuild_preserves_fingerprint() {
    let tmp = setup_repo();
    let repo = tmp.path();
    let first = build_index(repo, IndexOptions::default()).unwrap();
    let first_fp = first.index.fingerprint.unwrap();
    fs::remove_dir_all(repo.join(".pulse/cache/docs-search")).unwrap();
    let second = build_index(repo, IndexOptions::default()).unwrap();
    assert_eq!(second.index.fingerprint.unwrap(), first_fp);
}

#[test]
fn changed_document_rebuilds_and_changes_fingerprint() {
    let tmp = setup_repo();
    let repo = tmp.path();
    let first = build_index(repo, IndexOptions::default()).unwrap();
    fs::write(
        repo.join("docs/domain/token.md"),
        b"# Token Lifecycle\n\n## Renewal\n\nRefresh-token renewal changed.\n",
    )
    .unwrap();
    let second = build_index(repo, IndexOptions::default()).unwrap();
    assert_ne!(second.index.fingerprint, first.index.fingerprint);
    assert_eq!(second.documents.changed, 1);
}

#[test]
fn retrieval_metadata_change_invalidates_but_review_policy_does_not() {
    let tmp = setup_repo();
    let repo = tmp.path();
    let first = build_index(repo, IndexOptions::default()).unwrap();
    let mut registry: DocsRegistry =
        pulse::storage::read_json(&repo.join(".pulse/docs/registry.json")).unwrap();
    registry.documents[0].review_policy = ReviewPolicy::Standard;
    registry.revision += 1;
    fs::write(
        repo.join(".pulse/docs/registry.json"),
        to_canonical_bytes(&registry).unwrap(),
    )
    .unwrap();
    let status = index_status(repo).unwrap();
    assert_eq!(
        status.index.state, "current",
        "review_policy is not retrieval-relevant"
    );
    assert_eq!(status.index.fingerprint, first.index.fingerprint);

    registry.documents[0].retrieval = Some(DocumentRetrieval {
        index: true,
        include_body: false,
        materialize_index: false,
    });
    registry.revision += 1;
    fs::write(
        repo.join(".pulse/docs/registry.json"),
        to_canonical_bytes(&registry).unwrap(),
    )
    .unwrap();
    let stale = index_status(repo).unwrap();
    assert_eq!(stale.index.state, "stale");
}

#[test]
fn corrupt_sections_is_detected_and_rebuild_repairs_cache_without_touching_docs() {
    let tmp = setup_repo();
    let repo = tmp.path();
    build_index(repo, IndexOptions::default()).unwrap();
    let current = read_current(repo).unwrap();
    let gen = validate_generation(repo, &current).unwrap();
    let before_doc = fs::read(repo.join("docs/domain/token.md")).unwrap();
    fs::write(&gen.sections_path, b"corrupt\n").unwrap();
    let status = index_status(repo).unwrap();
    assert_eq!(status.index.state, "corrupt");
    build_index(repo, IndexOptions::default()).unwrap();
    assert_eq!(
        fs::read(repo.join("docs/domain/token.md")).unwrap(),
        before_doc
    );
    assert_eq!(index_status(repo).unwrap().index.state, "current");
}

#[test]
fn explicit_index_ignores_auto_refresh_cost_guard() {
    let tmp = setup_repo();
    let repo = tmp.path();
    let mut registry: DocsRegistry =
        pulse::storage::read_json(&repo.join(".pulse/docs/registry.json")).unwrap();
    let mut cfg = registry.retrieval_config();
    cfg.auto_refresh_max_documents = 1;
    registry.retrieval = Some(cfg);
    fs::create_dir_all(repo.join("docs/extra")).unwrap();
    fs::write(repo.join("docs/extra/a.md"), b"# A\n").unwrap();
    let mut second = doc("DOC-EXTRA-DOMAIN", "docs/extra/a.md", "Extra");
    second.aliases = Vec::new();
    registry.documents.push(second);
    registry.normalize();
    fs::write(
        repo.join(".pulse/docs/registry.json"),
        to_canonical_bytes(&registry).unwrap(),
    )
    .unwrap();
    let report = build_index(repo, IndexOptions::default()).unwrap();
    assert_eq!(report.index.state, "current");
    assert_eq!(report.documents.eligible, 2);
}

#[test]
fn index_check_is_read_only_and_errors_when_cache_or_projections_missing() {
    let tmp = setup_repo();
    let repo = tmp.path();
    let err = check_index(repo).unwrap_err();
    assert_eq!(err.code(), "docs_index_missing");
    assert!(!repo.join(".pulse/cache/docs-search/CURRENT").exists());
    assert!(!repo.join("docs/_index.md").exists());

    build_index(repo, IndexOptions::default()).unwrap();
    fs::remove_file(repo.join("docs/_index.md")).unwrap();
    let err = check_index(repo).unwrap_err();
    assert_eq!(err.code(), "docs_index_projection_missing");
    assert!(!repo.join("docs/_index.md").exists());
}

#[test]
fn non_utf8_eligible_document_is_hard_error() {
    let tmp = setup_repo();
    let repo = tmp.path();
    fs::write(repo.join("docs/domain/token.md"), [0xff, 0xfe, b'\n']).unwrap();
    let err = build_index(repo, IndexOptions::default()).unwrap_err();
    assert_eq!(err.code(), "docs_document_not_utf8");
}

#[test]
fn rebuilding_same_fingerprint_preserves_current_generation_dir() {
    let tmp = setup_repo();
    let repo = tmp.path();
    build_index(repo, IndexOptions::default()).unwrap();
    let current = read_current(repo).unwrap();
    let sentinel = repo
        .join(".pulse/cache/docs-search/generations")
        .join(&current)
        .join("reader-sentinel");
    fs::write(&sentinel, b"reader-visible").unwrap();
    build_index(
        repo,
        IndexOptions {
            rebuild: true,
            ..IndexOptions::default()
        },
    )
    .unwrap();
    assert!(sentinel.exists());
    assert_eq!(read_current(repo).unwrap(), current);
}

#[test]
fn status_reports_missing_then_current() {
    let tmp = setup_repo();
    let repo = tmp.path();
    assert_eq!(index_status(repo).unwrap().index.state, "missing");
    build_index(repo, IndexOptions::default()).unwrap();
    assert_eq!(index_status(repo).unwrap().index.state, "current");
    assert!(current_generation(repo).unwrap().is_some());
}
