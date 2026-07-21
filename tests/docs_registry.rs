use std::fs;

use pulse::docs::{
    bootstrap as docs_bootstrap, edit as docs_edit, list as docs_list, register as docs_register,
    retire as docs_retire, show as docs_show, supersede as docs_supersede, DocumentAuthority,
    DocumentKind, DocumentLifecycle, DocumentPatch, DocumentRecord, DocumentScope, ReviewPolicy,
};
use pulse::error::PulseError;
fn write_doc(repo: &std::path::Path, path: &str) {
    let full = repo.join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, format!("# {path}\n")).unwrap();
}

fn record(id: &str, path: &str) -> DocumentRecord {
    DocumentRecord {
        id: id.to_string(),
        revision: 1,
        path: path.to_string(),
        kind: DocumentKind::Domain,
        authority: DocumentAuthority::Approved,
        lifecycle: DocumentLifecycle::Current,
        owner: "team:docs".to_string(),
        summary: format!("Summary for {id}"),
        aliases: vec!["alpha".to_string(), "beta".to_string()],
        scope: DocumentScope {
            paths: vec!["src/auth/**".to_string()],
            domains: vec!["authentication".to_string()],
            work_labels: vec!["auth".to_string()],
        },
        review_policy: ReviewPolicy::Standard,
        verification_profile: "domain-doc".to_string(),
        generated: None,
        superseded_by: None,
    }
}

#[test]
fn bootstrap_reuses_evidence_repository_id_and_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();

    let evidence = pulse::evidence::manifest::bootstrap(repo).unwrap().manifest;
    let first = docs_bootstrap(repo).unwrap();
    assert_eq!(first.registry.schema_version, 1);
    assert_eq!(first.registry.revision, 1);
    assert_eq!(first.registry.repository_id, evidence.repository_id);
    assert!(repo.join(".pulse/docs/registry.json").exists());
    assert!(repo
        .join(".pulse/docs/schemas/document.schema.json")
        .exists());

    let second = docs_bootstrap(repo).unwrap();
    assert_eq!(second.registry, first.registry);
    assert!(second.created.is_empty());
    assert!(second
        .preserved
        .iter()
        .any(|path| path.ends_with("registry.json")));
}

#[test]
fn bootstrap_rejects_existing_registry_schema_drift() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let _ = pulse::evidence::manifest::bootstrap(repo).unwrap();
    fs::create_dir_all(repo.join(".pulse/docs/schemas")).unwrap();
    fs::write(
        repo.join(".pulse/docs/schemas/document.schema.json"),
        b"{}\n",
    )
    .unwrap();

    let error = docs_bootstrap(repo).unwrap_err();
    assert_eq!(error.code(), "docs_registry_schema_invalid");
}

#[test]
fn valid_registration_list_and_show() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    docs_bootstrap(repo).unwrap();
    write_doc(repo, "docs/domain/auth.md");

    let out = docs_register(
        repo,
        1,
        record("DOC-AUTH-DOMAIN", "docs/domain/auth.md"),
        "human:test",
    )
    .unwrap();
    assert_eq!(out.registry_revision, 2);
    assert_eq!(out.value.revision, 1);

    let list = docs_list(repo).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "DOC-AUTH-DOMAIN");

    let shown = docs_show(repo, "DOC-AUTH-DOMAIN").unwrap();
    assert_eq!(shown.path, "docs/domain/auth.md");
    assert_eq!(shown.owner, "team:docs");
}

#[test]
fn duplicate_id_or_path_rejects_before_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    docs_bootstrap(repo).unwrap();
    write_doc(repo, "docs/domain/auth.md");
    write_doc(repo, "docs/domain/auth-copy.md");
    docs_register(
        repo,
        1,
        record("DOC-AUTH-DOMAIN", "docs/domain/auth.md"),
        "human:test",
    )
    .unwrap();

    let duplicate_id = docs_register(
        repo,
        2,
        record("DOC-AUTH-DOMAIN", "docs/domain/auth-copy.md"),
        "human:test",
    )
    .unwrap_err();
    assert_eq!(duplicate_id.code(), "already_exists");

    let duplicate_path = docs_register(
        repo,
        2,
        record("DOC-AUTH-OTHER", "docs/domain/auth.md"),
        "human:test",
    )
    .unwrap_err();
    assert_eq!(duplicate_path.code(), "invalid_docs_registry");
    assert_eq!(docs_list(repo).unwrap().len(), 1);
}

#[test]
fn unsafe_protected_and_work_paths_reject() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    docs_bootstrap(repo).unwrap();
    write_doc(repo, "docs/domain/auth.md");
    write_doc(repo, "works/TK-001/ticket.md");
    write_doc(repo, ".pulse/migrations/docs-backups/old.md");

    let unsafe_error = docs_register(
        repo,
        1,
        record("DOC-AUTH-UNSAFE", "../escape.md"),
        "human:test",
    )
    .unwrap_err();
    assert_eq!(unsafe_error.code(), "invalid_docs_registry");

    let work_error = docs_register(
        repo,
        1,
        record("DOC-AUTH-WORK", "works/TK-001/ticket.md"),
        "human:test",
    )
    .unwrap_err();
    assert_eq!(work_error.code(), "invalid_docs_registry");

    let backup_error = docs_register(
        repo,
        1,
        record("DOC-AUTH-BACKUP", ".pulse/migrations/docs-backups/old.md"),
        "human:test",
    )
    .unwrap_err();
    assert_eq!(backup_error.code(), "invalid_docs_registry");

    write_doc(repo, "docs/domain/_index.md");
    let nav_error = docs_register(
        repo,
        1,
        record("DOC-AUTH-NAV", "docs/domain/_index.md"),
        "human:test",
    )
    .unwrap_err();
    assert_eq!(nav_error.code(), "invalid_docs_registry");
}

#[test]
fn registry_cas_conflict_rejects_stale_writer() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    docs_bootstrap(repo).unwrap();
    write_doc(repo, "docs/domain/auth.md");
    write_doc(repo, "docs/domain/product.md");
    docs_register(
        repo,
        1,
        record("DOC-AUTH-DOMAIN", "docs/domain/auth.md"),
        "human:test",
    )
    .unwrap();

    let error = docs_register(
        repo,
        1,
        record("DOC-AUTH-PRODUCT", "docs/domain/product.md"),
        "human:test",
    )
    .unwrap_err();
    assert!(matches!(error, PulseError::CasConflict { .. }));
}

#[test]
fn path_edit_preserves_id_and_bumps_document_revision() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    docs_bootstrap(repo).unwrap();
    write_doc(repo, "docs/domain/auth.md");
    write_doc(repo, "docs/domain/token-lifecycle.md");
    docs_register(
        repo,
        1,
        record("DOC-AUTH-DOMAIN", "docs/domain/auth.md"),
        "human:test",
    )
    .unwrap();

    let out = docs_edit(
        repo,
        "DOC-AUTH-DOMAIN",
        2,
        1,
        DocumentPatch {
            path: Some("docs/domain/token-lifecycle.md".to_string()),
            ..DocumentPatch::default()
        },
        "human:test",
    )
    .unwrap();

    assert_eq!(out.value.id, "DOC-AUTH-DOMAIN");
    assert_eq!(out.value.revision, 2);
    assert_eq!(out.registry_revision, 3);
    assert_eq!(
        docs_show(repo, "DOC-AUTH-DOMAIN").unwrap().path,
        "docs/domain/token-lifecycle.md"
    );
}

#[test]
fn retire_supersede_and_cycle_behavior() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    docs_bootstrap(repo).unwrap();
    write_doc(repo, "docs/domain/old.md");
    write_doc(repo, "docs/domain/new.md");
    write_doc(repo, "docs/domain/third.md");
    docs_register(
        repo,
        1,
        record("DOC-AUTH-OLD", "docs/domain/old.md"),
        "human:test",
    )
    .unwrap();
    docs_register(
        repo,
        2,
        record("DOC-AUTH-NEW", "docs/domain/new.md"),
        "human:test",
    )
    .unwrap();
    docs_register(
        repo,
        3,
        record("DOC-AUTH-THIRD", "docs/domain/third.md"),
        "human:test",
    )
    .unwrap();

    let retired = docs_retire(repo, "DOC-AUTH-THIRD", 4, 1, "obsolete", "human:test").unwrap();
    assert_eq!(retired.value.lifecycle, DocumentLifecycle::Retired);
    assert_eq!(retired.value.revision, 2);

    let self_error = docs_supersede(
        repo,
        "DOC-AUTH-OLD",
        "DOC-AUTH-OLD",
        5,
        1,
        "replace",
        "human:test",
    )
    .unwrap_err();
    assert_eq!(self_error.code(), "document_supersession_cycle");

    let superseded = docs_supersede(
        repo,
        "DOC-AUTH-OLD",
        "DOC-AUTH-NEW",
        5,
        1,
        "replace",
        "human:test",
    )
    .unwrap();
    assert_eq!(superseded.value.lifecycle, DocumentLifecycle::Superseded);
    assert_eq!(
        superseded.value.superseded_by.as_deref(),
        Some("DOC-AUTH-NEW")
    );

    let cycle_patch_error = docs_edit(
        repo,
        "DOC-AUTH-NEW",
        6,
        1,
        DocumentPatch {
            lifecycle: Some(DocumentLifecycle::Superseded),
            superseded_by: Some(Some("DOC-AUTH-OLD".to_string())),
            ..DocumentPatch::default()
        },
        "human:test",
    )
    .unwrap_err();
    assert_eq!(cycle_patch_error.code(), "invalid_docs_registry");
}
