use std::fs;

use pulse::canonical_json::hash_bytes;
use pulse::docs::applicability::{applicable_docs, ApplicabilityOptions, FsContentResolver};
use pulse::docs::model::{
    DocsRegistry, DocumentKind, DocumentRecord, DocumentScope, DocumentStatus,
    DocumentationPosture, WorkDocumentationContext,
};

fn doc(id: &str, path: &str) -> DocumentRecord {
    DocumentRecord {
        id: id.to_string(),
        revision: 1,
        path: path.to_string(),
        kind: DocumentKind::Domain,
        status: DocumentStatus::Approved,
        owner: "team:docs".to_string(),
        summary: format!("summary {id}"),
        scope: DocumentScope::default(),
        tags: Vec::new(),
        generated: None,
        superseded_by: None,
    }
}

fn registry(mut documents: Vec<DocumentRecord>) -> DocsRegistry {
    documents.sort_by(|a, b| a.id.cmp(&b.id));
    DocsRegistry {
        schema_version: 1,
        revision: 7,
        repository_id: "repo_test".to_string(),
        documents,
        retrieval: None,
    }
}

fn resolver<'a>(root: &'a std::path::Path) -> FsContentResolver<'a> {
    FsContentResolver::new(root)
}

#[test]
fn explicit_required_doc_is_required_with_hash_and_write_candidate() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("docs/domain")).unwrap();
    let path = tmp.path().join("docs/domain/auth.md");
    fs::write(&path, b"auth v1\n").unwrap();

    let reg = registry(vec![doc("DOC-AUTH-DOMAIN", "docs/domain/auth.md")]);
    let work = WorkDocumentationContext {
        work_id: "TK-001".to_string(),
        revision: 4,
        posture: DocumentationPosture::Required,
        required_documents: vec!["DOC-AUTH-DOMAIN".to_string()],
        paths: Vec::new(),
        tags: Vec::new(),
        domains: Vec::new(),
        labels: Vec::new(),
    };

    let out = applicable_docs(
        &work,
        &reg,
        &resolver(tmp.path()),
        ApplicabilityOptions::default(),
    )
    .unwrap();

    assert_eq!(out.schema_version, 1);
    assert_eq!(out.gate.status, "complete");
    assert_eq!(out.required.len(), 1);
    assert_eq!(out.required[0].id, "DOC-AUTH-DOMAIN");
    assert_eq!(out.required[0].content_hash, hash_bytes(b"auth v1\n"));
    assert_eq!(out.required[0].reasons, vec!["explicit_required_document"]);
    assert_eq!(out.write_candidates.len(), 1);
}

#[test]
fn path_and_tag_scopes_match_optional_docs() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("docs/domain")).unwrap();
    fs::write(tmp.path().join("docs/domain/auth.md"), b"auth").unwrap();
    fs::write(tmp.path().join("docs/domain/billing.md"), b"billing").unwrap();
    let mut scoped = doc("DOC-AUTH-DOMAIN", "docs/domain/auth.md");
    scoped.scope.paths = vec!["src/auth/**".to_string()];
    scoped.tags = vec!["authentication".to_string()];
    let mut unrelated = doc("DOC-BILLING-DOMAIN", "docs/domain/billing.md");
    unrelated.tags = vec!["billing".to_string()];

    let out = applicable_docs(
        &WorkDocumentationContext {
            work_id: "TK-001".to_string(),
            revision: 1,
            posture: DocumentationPosture::None,
            required_documents: Vec::new(),
            paths: vec!["src/auth/login.rs".to_string()],
            tags: vec!["authentication".to_string()],
            domains: Vec::new(),
            labels: Vec::new(),
        },
        &registry(vec![unrelated, scoped]),
        &resolver(tmp.path()),
        ApplicabilityOptions::default(),
    )
    .unwrap();

    assert!(out.required.is_empty());
    assert_eq!(out.optional.len(), 1);
    assert_eq!(out.optional[0].id, "DOC-AUTH-DOMAIN");
    assert_eq!(
        out.optional[0].reasons,
        vec!["path_scope_match", "tag_scope_match"]
    );
    assert!(out
        .excluded
        .iter()
        .all(|document| document.id != "DOC-BILLING-DOMAIN"));
}

#[test]
fn missing_retired_and_superseded_required_docs_make_gate_incomplete() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("docs/domain")).unwrap();
    fs::write(tmp.path().join("docs/domain/replacement.md"), b"new").unwrap();
    fs::write(tmp.path().join("docs/domain/retired.md"), b"old").unwrap();
    fs::write(tmp.path().join("docs/domain/superseded.md"), b"old").unwrap();
    let mut retired = doc("DOC-OLD-RETIRED", "docs/domain/retired.md");
    retired.status = DocumentStatus::Retired;
    let mut superseded = doc("DOC-OLD-SUPERSEDED", "docs/domain/superseded.md");
    superseded.superseded_by = Some("DOC-NEW-CURRENT".to_string());
    let replacement = doc("DOC-NEW-CURRENT", "docs/domain/replacement.md");

    let out = applicable_docs(
        &WorkDocumentationContext {
            work_id: "TK-001".to_string(),
            revision: 1,
            posture: DocumentationPosture::Required,
            required_documents: vec![
                "DOC-MISSING-REQ".to_string(),
                "DOC-OLD-RETIRED".to_string(),
                "DOC-OLD-SUPERSEDED".to_string(),
            ],
            paths: Vec::new(),
            tags: Vec::new(),
            domains: Vec::new(),
            labels: Vec::new(),
        },
        &registry(vec![retired, superseded, replacement]),
        &resolver(tmp.path()),
        ApplicabilityOptions::default(),
    )
    .unwrap();

    assert_eq!(out.gate.status, "incomplete");
    assert!(out
        .gate
        .reason_codes
        .contains(&"required_document_missing".to_string()));
    assert!(out
        .gate
        .reason_codes
        .contains(&"required_document_retired".to_string()));
    assert!(out
        .gate
        .reason_codes
        .contains(&"required_document_superseded".to_string()));
    let old = out
        .excluded
        .iter()
        .find(|document| document.id == "DOC-OLD-SUPERSEDED")
        .unwrap();
    assert_eq!(old.replacement.as_deref(), Some("DOC-NEW-CURRENT"));
    assert!(out
        .optional
        .iter()
        .any(|document| document.id == "DOC-NEW-CURRENT"));
}

#[test]
fn draft_stale_and_generated_index_exclusion_follow_include_flags() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("docs/domain")).unwrap();
    fs::write(tmp.path().join("docs/domain/draft.md"), b"draft").unwrap();
    fs::write(tmp.path().join("docs/domain/stale.md"), b"stale").unwrap();
    fs::write(tmp.path().join("docs/_index.md"), b"index").unwrap();
    let mut draft = doc("DOC-DRAFT-CURRENT", "docs/domain/draft.md");
    draft.status = DocumentStatus::Draft;
    draft.tags = vec!["authentication".to_string()];
    let mut stale = doc("DOC-STALE-CURRENT", "docs/domain/stale.md");
    stale.status = DocumentStatus::Stale;
    stale.tags = vec!["authentication".to_string()];
    let mut index = doc("DOC-GENERATED-INDEX", "docs/_index.md");
    index.tags = vec!["authentication".to_string()];

    let work = WorkDocumentationContext {
        work_id: "TK-001".to_string(),
        revision: 1,
        posture: DocumentationPosture::None,
        required_documents: Vec::new(),
        paths: Vec::new(),
        tags: vec!["authentication".to_string()],
        domains: Vec::new(),
        labels: Vec::new(),
    };
    let reg = registry(vec![draft, stale, index]);
    let strict = applicable_docs(
        &work,
        &reg,
        &resolver(tmp.path()),
        ApplicabilityOptions::default(),
    )
    .unwrap();
    assert!(strict
        .excluded
        .iter()
        .any(|document| document.id == "DOC-DRAFT-CURRENT"));
    assert!(strict
        .excluded
        .iter()
        .any(|document| document.id == "DOC-STALE-CURRENT"));
    assert!(strict
        .excluded
        .iter()
        .any(|document| document.id == "DOC-GENERATED-INDEX"));

    let included = applicable_docs(
        &work,
        &reg,
        &resolver(tmp.path()),
        ApplicabilityOptions {
            include_draft: true,
            include_stale: true,
        },
    )
    .unwrap();
    assert!(included
        .optional
        .iter()
        .any(|document| document.id == "DOC-DRAFT-CURRENT"));
    assert!(included
        .optional
        .iter()
        .any(|document| document.id == "DOC-STALE-CURRENT"));
    assert!(!included
        .optional
        .iter()
        .any(|document| document.id == "DOC-GENERATED-INDEX"));
}

#[test]
fn buckets_are_sorted_and_content_hash_changes_with_file_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("docs/domain")).unwrap();
    fs::write(tmp.path().join("docs/domain/a.md"), b"a1").unwrap();
    fs::write(tmp.path().join("docs/domain/b.md"), b"b1").unwrap();
    let mut a = doc("DOC-A-SORTED", "docs/domain/a.md");
    a.tags = vec!["x".to_string()];
    let mut b = doc("DOC-B-SORTED", "docs/domain/b.md");
    b.tags = vec!["x".to_string()];
    let reg = registry(vec![b, a]);
    let work = WorkDocumentationContext {
        work_id: "TK-001".to_string(),
        revision: 1,
        posture: DocumentationPosture::None,
        required_documents: Vec::new(),
        paths: Vec::new(),
        tags: vec!["x".to_string()],
        domains: Vec::new(),
        labels: Vec::new(),
    };
    let first = applicable_docs(
        &work,
        &reg,
        &resolver(tmp.path()),
        ApplicabilityOptions::default(),
    )
    .unwrap();
    assert_eq!(
        first
            .optional
            .iter()
            .map(|document| document.id.as_str())
            .collect::<Vec<_>>(),
        vec!["DOC-A-SORTED", "DOC-B-SORTED"]
    );
    let old_hash = first.optional[0].content_hash.clone();
    fs::write(tmp.path().join("docs/domain/a.md"), b"a2").unwrap();
    let second = applicable_docs(
        &work,
        &reg,
        &resolver(tmp.path()),
        ApplicabilityOptions::default(),
    )
    .unwrap();
    assert_ne!(second.optional[0].content_hash, old_hash);
    assert_eq!(second.optional[0].content_hash, hash_bytes(b"a2"));
}
