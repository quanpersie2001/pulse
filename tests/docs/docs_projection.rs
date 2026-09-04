//! Generated documentation navigation projection tests.

use std::fs;

use pulse::docs::model::{
    DocsRegistry, DocumentKind, DocumentRecord, DocumentScope, DocumentStatus,
};
use pulse::docs::policy::{
    is_generated_navigation_path, is_protected_path, is_runtime_or_cache_path, is_work_content_path,
};
use pulse::docs::{
    check_projections, eligible_documents, is_pulse_generated, projection_state,
    projection_targets, render_area_index, render_root_index, ProjectionStatus, PROJECTION_MARKER,
    PROJECTION_SCHEMA_VERSION,
};

fn make_doc(id: &str, path: &str, kind: DocumentKind) -> DocumentRecord {
    DocumentRecord {
        id: id.to_string(),
        revision: 1,
        path: path.to_string(),
        kind,
        status: DocumentStatus::Approved,
        owner: "team:docs".to_string(),
        summary: format!("summary {id}"),
        scope: DocumentScope::default(),
        tags: Vec::new(),
        generated: None,
        superseded_by: None,
    }
}

fn registry(documents: Vec<DocumentRecord>) -> DocsRegistry {
    DocsRegistry {
        schema_version: 1,
        revision: 7,
        repository_id: "repo_test".to_string(),
        documents,
        retrieval: None,
    }
}

fn eligible_ids(reg: &DocsRegistry) -> Vec<String> {
    let mut ids: Vec<String> = eligible_documents(reg, Default::default())
        .into_iter()
        .map(|(doc, _)| doc.id.clone())
        .collect();
    ids.sort();
    ids
}

#[test]
fn approved_documents_are_eligible_and_rendered() {
    let reg = registry(vec![make_doc(
        "DOC-AUTH-DOMAIN",
        "docs/domain/token-lifecycle.md",
        DocumentKind::Domain,
    )]);
    assert_eq!(eligible_ids(&reg), vec!["DOC-AUTH-DOMAIN"]);
    let out = render_root_index(&reg).unwrap();
    assert!(out.contains("[Token Lifecycle](domain/token-lifecycle.md)"));
    assert!(out.contains("Authority: approved"));
}

#[test]
fn repository_policy_is_projected_under_repository_area() {
    let agents = make_doc("DOC-AGENTS", "AGENTS.md", DocumentKind::Policy);
    let policy = make_doc("DOC-PULSE", "PULSE.md", DocumentKind::Policy);
    let domain = make_doc(
        "DOC-DOMAIN",
        "docs/domain/token-lifecycle.md",
        DocumentKind::Domain,
    );
    let out = render_root_index(&registry(vec![agents, policy, domain])).unwrap();
    assert!(out.contains("## Repository"));
    assert!(out.contains("[AGENTS](../AGENTS.md)"));
    assert!(out.contains("[PULSE](../PULSE.md)"));
    assert!(out.contains("[Token Lifecycle](domain/token-lifecycle.md)"));
}

#[test]
fn generated_navigation_and_protected_paths_are_excluded() {
    let mut nav = make_doc("DOC-NAV", "docs/domain/_index.md", DocumentKind::Domain);
    nav.tags = vec!["auth".to_string()];
    let work = make_doc("DOC-WORK", "works/EP-001/story.md", DocumentKind::Domain);
    let cache = make_doc(
        "DOC-CACHE",
        ".pulse/cache/docs/state.json",
        DocumentKind::Domain,
    );
    let good = make_doc("DOC-GOOD", "docs/domain/token.md", DocumentKind::Domain);
    let reg = registry(vec![nav, work, cache, good]);
    assert_eq!(eligible_ids(&reg), vec!["DOC-GOOD"]);
    assert!(is_generated_navigation_path("docs/_index.md"));
    assert!(is_protected_path(".pulse/migrations/docs-backups/old.md"));
    assert!(is_runtime_or_cache_path(
        ".pulse/cache/docs-search/state.json"
    ));
    assert!(is_work_content_path("works/EP-001/story.md"));
}

#[test]
fn status_controls_eligibility() {
    let current = make_doc(
        "DOC-CURRENT",
        "docs/domain/current.md",
        DocumentKind::Domain,
    );
    let mut draft = make_doc("DOC-DRAFT", "docs/domain/draft.md", DocumentKind::Domain);
    draft.status = DocumentStatus::Draft;
    let mut stale = make_doc("DOC-STALE", "docs/domain/stale.md", DocumentKind::Domain);
    stale.status = DocumentStatus::Stale;
    let mut retired = make_doc(
        "DOC-RETIRED",
        "docs/domain/retired.md",
        DocumentKind::Domain,
    );
    retired.status = DocumentStatus::Retired;
    let mut superseded = make_doc(
        "DOC-SUPERSEDED",
        "docs/domain/superseded.md",
        DocumentKind::Domain,
    );
    superseded.superseded_by = Some("DOC-CURRENT".to_string());
    assert_eq!(
        eligible_ids(&registry(vec![current, draft, stale, retired, superseded])),
        vec!["DOC-CURRENT"]
    );
}

#[test]
fn area_targets_materialize_at_fixed_threshold() {
    let documents: Vec<_> = (1..=3)
        .map(|n| {
            make_doc(
                &format!("DOC-ARCH-{n:02}"),
                &format!("docs/architecture/a{n:02}.md"),
                DocumentKind::Architecture,
            )
        })
        .chain((1..=5).map(|n| {
            make_doc(
                &format!("DOC-DOM-{n:02}"),
                &format!("docs/domain/d{n:02}.md"),
                DocumentKind::Domain,
            )
        }))
        .collect();
    let paths: Vec<_> = projection_targets(&registry(documents))
        .into_iter()
        .map(|target| target.path)
        .collect();
    assert_eq!(paths, vec!["docs/_index.md", "docs/domain/_index.md"]);
}

#[test]
fn projection_state_reports_missing_current_stale_and_conflict() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("docs/domain")).unwrap();
    let reg = registry(vec![make_doc(
        "DOC-AUTH-DOMAIN",
        "docs/domain/token.md",
        DocumentKind::Domain,
    )]);
    let missing = projection_state(tmp.path(), &reg).unwrap();
    assert_eq!(missing.state, ProjectionStatus::Missing);
    let expected = render_root_index(&reg).unwrap();
    fs::write(tmp.path().join("docs/_index.md"), &expected).unwrap();
    assert_eq!(
        projection_state(tmp.path(), &reg).unwrap().state,
        ProjectionStatus::Current
    );
    let mut stale = expected.clone();
    stale.push_str("extra\n");
    fs::write(tmp.path().join("docs/_index.md"), stale).unwrap();
    assert_eq!(
        projection_state(tmp.path(), &reg).unwrap().state,
        ProjectionStatus::Stale
    );
    fs::write(tmp.path().join("docs/_index.md"), "# User index\n").unwrap();
    let report = check_projections(tmp.path(), &reg).unwrap();
    assert_eq!(report.state, ProjectionStatus::Conflict);
    assert!(!report.ok);
}

#[test]
fn projection_rendering_is_deterministic_and_area_links_are_portable() {
    let reg = registry(vec![
        make_doc("DOC-DOMAIN", "docs/domain/token.md", DocumentKind::Domain),
        make_doc(
            "DOC-ARCH",
            "docs/architecture/auth.md",
            DocumentKind::Architecture,
        ),
    ]);
    let first = render_root_index(&reg).unwrap();
    assert_eq!(first, render_root_index(&reg).unwrap());
    assert!(first.starts_with("# Documentation Index\n"));
    assert!(first.contains(PROJECTION_MARKER));
    assert!(first.contains(&format!("schema-version={PROJECTION_SCHEMA_VERSION}")));
    assert!(render_area_index(&reg, "docs/domain")
        .unwrap()
        .contains("[Token](token.md)"));
}

#[test]
fn generated_marker_requires_supported_schema() {
    assert!(!is_pulse_generated(b"# normal index\n"));
    let valid = format!("{PROJECTION_MARKER}\n<!-- pulse-docs-projection:schema-version={PROJECTION_SCHEMA_VERSION} -->\n");
    assert!(is_pulse_generated(valid.as_bytes()));
    assert!(!is_pulse_generated(
        b"<!-- pulse-docs-projection -->\n<!-- pulse-docs-projection:schema-version=999 -->\n"
    ));
}
