use pulse::canonical_json::to_canonical_bytes;
use pulse::docs::{
    build_index, check_index, current_generation, index_status, query_lexical_index, read_current,
    validate_generation, DocsRegistry, DocumentKind, DocumentRecord, DocumentScope, DocumentStatus,
    IndexOptions,
};
use std::fs;

fn doc(id: &str, path: &str, summary: &str) -> DocumentRecord {
    DocumentRecord {
        id: id.to_string(),
        revision: 1,
        path: path.to_string(),
        kind: DocumentKind::Domain,
        status: DocumentStatus::Approved,
        owner: "team:docs".to_string(),
        summary: summary.to_string(),
        scope: DocumentScope {
            paths: vec!["src/auth/**".to_string()],
        },
        tags: vec![],
        generated: None,
        superseded_by: None,
    }
}

fn setup_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    fs::create_dir_all(repo.join(".pulse/docs")).unwrap();
    let manifest = pulse::evidence::manifest::bootstrap(repo).unwrap().manifest;
    fs::create_dir_all(repo.join("docs/domain")).unwrap();
    fs::write(repo.join("docs/domain/token.md"), b"# Token Lifecycle\n\n## Expired tokens\n\nTokenExpired means a refresh-token expired in v2.1.\n").unwrap();
    let registry = DocsRegistry {
        schema_version: 1,
        revision: 1,
        repository_id: manifest.repository_id,
        documents: vec![doc(
            "DOC-AUTH-DOMAIN",
            "docs/domain/token.md",
            "Token lifecycle and refresh-token expiry",
        )],
        retrieval: None,
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
fn status_metadata_change_invalidates_index() {
    let tmp = setup_repo();
    let repo = tmp.path();
    let _first = build_index(repo, IndexOptions::default()).unwrap();
    let mut registry: DocsRegistry =
        pulse::storage::read_json(&repo.join(".pulse/docs/registry.json")).unwrap();
    registry.documents[0].status = DocumentStatus::Stale;
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
fn explicit_index_indexes_all_eligible_documents() {
    let tmp = setup_repo();
    let repo = tmp.path();
    let mut registry: DocsRegistry =
        pulse::storage::read_json(&repo.join(".pulse/docs/registry.json")).unwrap();
    fs::create_dir_all(repo.join("docs/extra")).unwrap();
    fs::write(repo.join("docs/extra/a.md"), b"# A\n").unwrap();
    let second = doc("DOC-EXTRA-DOMAIN", "docs/extra/a.md", "Extra");
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

// ===================================================================
// P2S1-I4: Cache-only search build does not write tracked projections
// ===================================================================

#[test]
fn build_search_cache_never_writes_tracked_projections() {
    // Per P2S1-D9: cache-only build must not write docs/**/_index.md
    // because that would dirty a clean worktree.
    let tmp = setup_repo();
    let repo = tmp.path();

    // Check that no _index.md exists before.
    let root_index = repo.join("docs/_index.md");
    let existed_before = root_index.exists();

    let report = pulse::docs::build_search_cache(repo, IndexOptions::default()).unwrap();
    assert_eq!(report.code, "indexed");
    assert_eq!(report.index.state, "current");
    assert_eq!(
        report.projections.state, "cache_only",
        "cache-only build must report cache_only projection state"
    );
    assert!(
        report.projections.files.is_empty(),
        "cache-only build must not list any projection files"
    );

    // Verify no tracked _index.md was created by the cache-only build.
    let exists_after = root_index.exists();
    assert_eq!(
        existed_before, exists_after,
        "cache-only build must not create or remove docs/_index.md"
    );

    // Verify the cache generation is queryable.
    let gen = pulse::docs::current_generation(repo).unwrap();
    assert!(gen.is_some());
    assert!(gen.unwrap().tantivy_path.exists());

    // Verify the full build_index still writes projections.
    let _full = pulse::docs::build_index(repo, IndexOptions::default()).unwrap();
    assert!(
        root_index.exists(),
        "full build_index must create docs/_index.md"
    );
}
