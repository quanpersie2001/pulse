//! Phase 3 (Slice 5) generated `_index.md` projection tests:
//! - R4: `index=false` document is registered but excluded from projection.
//! - R5: informational current doc is included and labeled, never required.
//! - R5a: registered `AGENTS.md`/`PULSE.md` surface under a virtual Repository area.
//! - R5b: generated docs are opt-in; generated navigation `_index.md` is never indexed.
//! - R6: retired/superseded/stale/draft docs are excluded by default.
//! - R7: migration/work/runtime/cache paths are excluded.
//! - R18: root `_index.md` generation (marker/links/order/summary).
//! - R19: area threshold/config materialization.
//! - R20: user-authored `_index.md` conflict is reported, never overwritten.
//! - R21: projection determinism (delete/rebuild = identical bytes).

use std::fs;

use pulse::docs::model::{
    DocsRegistry, DocumentAuthority, DocumentKind, DocumentLifecycle, DocumentRecord,
    DocumentRetrieval, DocumentScope, RetrievalConfig, RetrievalScope, ReviewPolicy,
};
use pulse::docs::policy::{
    is_generated_navigation_path, is_protected_path, is_runtime_or_cache_path, is_work_content_path,
};
use pulse::docs::{
    check_projections, eligible_documents, is_pulse_generated, projection_state,
    projection_targets, render_area_index, render_root_index, ProjectionStatus, PROJECTION_MARKER,
    PROJECTION_SCHEMA_VERSION,
};

// --- builders ---------------------------------------------------------------

fn make_doc(
    id: &str,
    path: &str,
    kind: DocumentKind,
    authority: DocumentAuthority,
) -> DocumentRecord {
    DocumentRecord {
        id: id.to_string(),
        revision: 1,
        path: path.to_string(),
        kind,
        authority,
        lifecycle: DocumentLifecycle::Current,
        owner: "team:docs".to_string(),
        summary: format!("summary {id}"),
        aliases: Vec::new(),
        scope: DocumentScope::default(),
        review_policy: ReviewPolicy::None,
        verification_profile: "domain-doc".to_string(),
        generated: None,
        superseded_by: None,
        retrieval: None,
    }
}

fn registry(documents: Vec<DocumentRecord>) -> DocsRegistry {
    registry_with(documents, RetrievalConfig::defaults())
}

fn registry_with(documents: Vec<DocumentRecord>, retrieval: RetrievalConfig) -> DocsRegistry {
    let mut documents = documents;
    documents.sort_by(|a, b| a.id.cmp(&b.id));
    DocsRegistry {
        schema_version: 2,
        revision: 7,
        repository_id: "repo_test".to_string(),
        documents,
        retrieval: Some(retrieval),
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

// --- R4: index=false --------------------------------------------------------

#[test]
fn r4_index_false_document_is_registered_but_excluded_from_projection() {
    let mut disabled = make_doc(
        "DOC-IDX-OFF",
        "docs/domain/disabled.md",
        DocumentKind::Domain,
        DocumentAuthority::Approved,
    );
    disabled.retrieval = Some(DocumentRetrieval {
        index: false,
        include_body: true,
        materialize_index: false,
    });
    let enabled = make_doc(
        "DOC-IDX-ON",
        "docs/domain/enabled.md",
        DocumentKind::Domain,
        DocumentAuthority::Approved,
    );
    let reg = registry(vec![disabled, enabled]);

    // Still present in registry (registry/applicability unaffected).
    assert_eq!(reg.documents.len(), 2);

    // Excluded from the projection/searchable set.
    assert_eq!(eligible_ids(&reg), vec!["DOC-IDX-ON".to_string()]);

    let out = render_root_index(&reg).unwrap();
    assert!(out.contains("enabled.md"));
    assert!(!out.contains("disabled.md"));
    assert!(!out.contains("DOC-IDX-OFF"));
}

// --- R5: informational doc --------------------------------------------------

#[test]
fn r5_informational_doc_is_included_and_labeled() {
    let info = make_doc(
        "DOC-INFO",
        "docs/domain/guide.md",
        DocumentKind::Informational,
        DocumentAuthority::Informational,
    );
    let reg = registry(vec![info]);

    // Searchable / projected.
    assert!(eligible_ids(&reg).contains(&"DOC-INFO".to_string()));

    let out = render_root_index(&reg).unwrap();
    assert!(out.contains("[Guide](domain/guide.md)"));
    assert!(out.contains("Authority: informational"));
}

// --- R5a: repository map + policy ------------------------------------------

#[test]
fn r5a_repository_map_and_policy_surface_under_repository_area() {
    let agents = make_doc(
        "DOC-AGENTS",
        "AGENTS.md",
        DocumentKind::RepositoryMap,
        DocumentAuthority::Approved,
    );
    let policy = make_doc(
        "DOC-PULSE",
        "PULSE.md",
        DocumentKind::Policy,
        DocumentAuthority::Approved,
    );
    let domain = make_doc(
        "DOC-DOMAIN",
        "docs/domain/token-lifecycle.md",
        DocumentKind::Domain,
        DocumentAuthority::Approved,
    );
    let reg = registry(vec![agents, policy, domain]);

    let out = render_root_index(&reg).unwrap();
    assert!(out.contains("## Repository"));
    assert!(out.contains("[AGENTS](../AGENTS.md)"));
    assert!(out.contains("[PULSE](../PULSE.md)"));
    // Domain doc still under its own area, not the Repository area.
    assert!(out.contains("[Token Lifecycle](domain/token-lifecycle.md)"));
}

#[test]
fn r5a_repository_docs_hidden_when_include_flags_false() {
    let agents = make_doc(
        "DOC-AGENTS",
        "AGENTS.md",
        DocumentKind::RepositoryMap,
        DocumentAuthority::Approved,
    );
    let mut retrieval = RetrievalConfig::defaults();
    retrieval.include_repository_map = false;
    retrieval.include_repository_policy = false;
    let reg = registry_with(vec![agents], retrieval);

    let out = render_root_index(&reg).unwrap();
    assert!(!out.contains("## Repository"));
    assert!(!out.contains("AGENTS.md"));
}

// --- R5b: generated opt-in + _index.md never indexed ------------------------

#[test]
fn r5b_generated_doc_requires_explicit_opt_in_and_index_md_is_never_indexed() {
    let generated_default = make_doc(
        "DOC-GEN-DEFAULT",
        "docs/operations/runbook.md",
        DocumentKind::Generated,
        DocumentAuthority::Generated,
    );
    // No retrieval override -> generated docs are opt-in by default -> excluded.
    let mut generated_opt_in = make_doc(
        "DOC-GEN-OPTIN",
        "docs/operations/generated-optin.md",
        DocumentKind::Generated,
        DocumentAuthority::Generated,
    );
    generated_opt_in.retrieval = Some(DocumentRetrieval {
        index: true,
        include_body: true,
        materialize_index: false,
    });
    // A generated navigation _index.md must never be indexed/projected.
    let mut index_nav = make_doc(
        "DOC-INDEX-NAV",
        "docs/domain/_index.md",
        DocumentKind::Domain,
        DocumentAuthority::Approved,
    );
    index_nav.scope.domains = vec!["auth".to_string()];
    let reg = registry(vec![generated_default, generated_opt_in, index_nav]);

    let eligible = eligible_ids(&reg);
    assert!(eligible.contains(&"DOC-GEN-OPTIN".to_string()));
    assert!(!eligible.contains(&"DOC-GEN-DEFAULT".to_string()));
    assert!(!eligible.contains(&"DOC-INDEX-NAV".to_string()));

    let out = render_root_index(&reg).unwrap();
    assert!(out.contains("generated-optin.md"));
    assert!(!out.contains("runbook.md"));
    assert!(!out.contains("_index.md"));
}

// --- R6: retired/superseded/stale/draft excluded ----------------------------

#[test]
fn r6_retired_superseded_stale_draft_are_excluded_by_default() {
    let current = make_doc(
        "DOC-CURRENT",
        "docs/domain/current.md",
        DocumentKind::Domain,
        DocumentAuthority::Approved,
    );
    let mut retired = make_doc(
        "DOC-RETIRED",
        "docs/domain/retired.md",
        DocumentKind::Domain,
        DocumentAuthority::Approved,
    );
    retired.lifecycle = DocumentLifecycle::Retired;
    let mut superseded = make_doc(
        "DOC-SUPERSEDED",
        "docs/domain/superseded.md",
        DocumentKind::Domain,
        DocumentAuthority::Approved,
    );
    superseded.lifecycle = DocumentLifecycle::Superseded;
    let mut stale = make_doc(
        "DOC-STALE",
        "docs/domain/stale.md",
        DocumentKind::Domain,
        DocumentAuthority::Approved,
    );
    stale.lifecycle = DocumentLifecycle::Stale;
    let mut suspected = make_doc(
        "DOC-SUSPECTED",
        "docs/domain/suspected.md",
        DocumentKind::Domain,
        DocumentAuthority::Approved,
    );
    suspected.lifecycle = DocumentLifecycle::SuspectedStale;
    let draft = make_doc(
        "DOC-DRAFT",
        "docs/domain/draft.md",
        DocumentKind::Domain,
        DocumentAuthority::Draft,
    );

    let reg = registry(vec![current, retired, superseded, stale, suspected, draft]);

    assert_eq!(eligible_ids(&reg), vec!["DOC-CURRENT".to_string()]);
    let out = render_root_index(&reg).unwrap();
    assert!(out.contains("current.md"));
    for excluded in [
        "retired.md",
        "superseded.md",
        "stale.md",
        "suspected.md",
        "draft.md",
    ] {
        assert!(!out.contains(excluded), "{excluded} should be excluded");
    }
}

// --- R7: protected/runtime/work/cache paths --------------------------------

#[test]
fn r7_protected_runtime_work_cache_paths_are_excluded() {
    // Predicate-level policy.
    assert!(is_protected_path(".pulse/migrations/docs-backups"));
    assert!(is_protected_path(".pulse/migrations/docs-backups/old.json"));
    assert!(is_runtime_or_cache_path(
        ".pulse/cache/docs-search/state.json"
    ));
    assert!(is_runtime_or_cache_path(".pulse/runtime/state.json"));
    assert!(is_runtime_or_cache_path(".pulse/evidence/x.json"));
    assert!(is_work_content_path("works/EP-001/story.md"));
    assert!(is_generated_navigation_path("docs/_index.md"));
    assert!(is_generated_navigation_path("docs/domain/_index.md"));

    // A document registered on a protected/work path is excluded from projection.
    let mut work_doc = make_doc(
        "DOC-WORK",
        "works/EP-001/story.md",
        DocumentKind::Domain,
        DocumentAuthority::Approved,
    );
    work_doc.scope.domains = vec!["auth".to_string()];
    let good = make_doc(
        "DOC-GOOD",
        "docs/domain/token.md",
        DocumentKind::Domain,
        DocumentAuthority::Approved,
    );
    let reg = registry(vec![work_doc, good]);

    assert_eq!(eligible_ids(&reg), vec!["DOC-GOOD".to_string()]);
    let out = render_root_index(&reg).unwrap();
    assert!(!out.contains("works/"));
    assert!(out.contains("token.md"));
}

// --- R18: root index determinism / marker / links / ordering ----------------

#[test]
fn r18_root_index_generation_is_deterministic_with_marker_links_and_order() {
    let domain = make_doc(
        "DOC-AUTH-DOMAIN",
        "docs/domain/token-lifecycle.md",
        DocumentKind::Domain,
        DocumentAuthority::Approved,
    );
    let arch = make_doc(
        "DOC-AUTH-ARCH",
        "docs/architecture/authentication.md",
        DocumentKind::Architecture,
        DocumentAuthority::Approved,
    );
    let reg = registry(vec![domain, arch]);
    let out = render_root_index(&reg).unwrap();

    assert!(out.starts_with("# Documentation Index\n"));
    assert!(out.contains("> Generated by `pulse docs index`. Do not edit manually."));
    assert!(out.contains("> Registry fingerprint: `sha256:"));
    assert!(out.contains(PROJECTION_MARKER));
    assert!(out.contains(&format!(
        "<!-- pulse-docs-projection:schema-version={PROJECTION_SCHEMA_VERSION} -->"
    )));
    assert!(is_pulse_generated(out.as_bytes()));

    // Portable repository-relative links.
    assert!(out.contains("[Token Lifecycle](domain/token-lifecycle.md)"));
    assert!(out.contains("[Authentication](architecture/authentication.md)"));
    // Registry summary (no LLM).
    assert!(out.contains("summary DOC-AUTH-DOMAIN"));
    assert!(out.contains("Authority: approved"));

    // Deterministic area ordering: Architecture before Domain.
    let arch_at = out.find("## Architecture").unwrap();
    let domain_at = out.find("## Domain").unwrap();
    assert!(arch_at < domain_at);

    // No timestamp markers leaking.
    assert!(!out.contains("UTC"));
    assert!(!out.contains("timestamp"));
}

// --- R19: area threshold / scope / override ---------------------------------

#[test]
fn r19_only_threshold_areas_materialize() {
    let mut arch_docs: Vec<DocumentRecord> = (1..=3)
        .map(|n| {
            make_doc(
                &format!("DOC-ARCH-{n:02}"),
                &format!("docs/architecture/a{n:02}.md"),
                DocumentKind::Architecture,
                DocumentAuthority::Approved,
            )
        })
        .collect();
    let domain_docs: Vec<DocumentRecord> = (1..=6)
        .map(|n| {
            make_doc(
                &format!("DOC-DOM-{n:02}"),
                &format!("docs/domain/d{n:02}.md"),
                DocumentKind::Domain,
                DocumentAuthority::Approved,
            )
        })
        .collect();
    arch_docs.extend(domain_docs);

    let mut retrieval = RetrievalConfig::defaults();
    retrieval.area_index_threshold = 5;
    let reg = registry_with(arch_docs, retrieval);

    let targets = projection_targets(&reg);
    let paths: Vec<String> = targets.iter().map(|t| t.path.clone()).collect();
    // Root + the 6-doc domain area; the 3-doc architecture area is below threshold.
    assert_eq!(
        paths,
        vec![
            "docs/_index.md".to_string(),
            "docs/domain/_index.md".to_string()
        ]
    );
}

#[test]
fn r19_scope_forces_area_materialization() {
    let arch_docs: Vec<DocumentRecord> = (1..=3)
        .map(|n| {
            make_doc(
                &format!("DOC-ARCH-{n:02}"),
                &format!("docs/architecture/a{n:02}.md"),
                DocumentKind::Architecture,
                DocumentAuthority::Approved,
            )
        })
        .collect();
    let mut retrieval = RetrievalConfig::defaults();
    retrieval.area_index_threshold = 5;
    retrieval.scopes = vec![RetrievalScope {
        path: "docs/architecture".to_string(),
        summary: "System boundaries.".to_string(),
        materialize_index: Some(true),
    }];
    let reg = registry_with(arch_docs, retrieval);

    let paths: Vec<String> = projection_targets(&reg)
        .iter()
        .map(|t| t.path.clone())
        .collect();
    assert_eq!(
        paths,
        vec![
            "docs/_index.md".to_string(),
            "docs/architecture/_index.md".to_string()
        ]
    );

    // The area index embeds the scope summary and portable per-area links.
    let area = render_area_index(&reg, "docs/architecture").unwrap();
    assert!(area.starts_with("# Architecture Index\n"));
    assert!(area.contains("System boundaries."));
    assert!(area.contains("[A01](a01.md)"));
}

#[test]
fn r19_per_document_override_forces_area_materialization() {
    let mut forced = make_doc(
        "DOC-OPS-01",
        "docs/operations/runbook.md",
        DocumentKind::Operations,
        DocumentAuthority::Approved,
    );
    forced.retrieval = Some(DocumentRetrieval {
        index: true,
        include_body: true,
        materialize_index: true,
    });
    let mut retrieval = RetrievalConfig::defaults();
    retrieval.area_index_threshold = 100; // above the single-doc area
    let reg = registry_with(vec![forced], retrieval);

    let paths: Vec<String> = projection_targets(&reg)
        .iter()
        .map(|t| t.path.clone())
        .collect();
    assert_eq!(
        paths,
        vec![
            "docs/_index.md".to_string(),
            "docs/operations/_index.md".to_string()
        ]
    );
}

// --- R20: user-authored _index.md conflict ----------------------------------

#[test]
fn r20_user_authored_index_md_is_reported_as_conflict_and_preserved() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("docs")).unwrap();
    let reg = registry(vec![make_doc(
        "DOC-AUTH-DOMAIN",
        "docs/domain/token-lifecycle.md",
        DocumentKind::Domain,
        DocumentAuthority::Approved,
    )]);

    // Write a user-authored _index.md WITHOUT the Pulse marker.
    let user_content = "# My hand-written index\n\nDo not touch.\n";
    fs::write(tmp.path().join("docs/_index.md"), user_content).unwrap();

    let snapshot = projection_state(tmp.path(), &reg).unwrap();
    assert_eq!(snapshot.state, ProjectionStatus::Conflict);
    let root_state = snapshot.targets.iter().find(|t| t.area.is_none()).unwrap();
    assert_eq!(root_state.state, ProjectionStatus::Conflict);

    let report = check_projections(tmp.path(), &reg).unwrap();
    assert!(!report.ok);
    assert_eq!(report.state, ProjectionStatus::Conflict);
    assert!(report.conflict.contains(&"docs/_index.md".to_string()));

    // The projection module is read-only: the user file is preserved verbatim.
    let after = fs::read_to_string(tmp.path().join("docs/_index.md")).unwrap();
    assert_eq!(after, user_content);
    assert!(!is_pulse_generated(after.as_bytes()));
}

// --- R21: determinism + projection_state current/stale/missing --------------

#[test]
fn r21_render_is_deterministic() {
    let reg = registry(vec![
        make_doc(
            "DOC-AUTH-DOMAIN",
            "docs/domain/token-lifecycle.md",
            DocumentKind::Domain,
            DocumentAuthority::Approved,
        ),
        make_doc(
            "DOC-AUTH-ARCH",
            "docs/architecture/authentication.md",
            DocumentKind::Architecture,
            DocumentAuthority::Approved,
        ),
    ]);
    let first = render_root_index(&reg).unwrap();
    let second = render_root_index(&reg).unwrap();
    assert_eq!(first, second, "render must be byte-stable");
}

#[test]
fn r21_projection_state_current_stale_missing() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("docs/domain")).unwrap();
    let reg = registry(vec![make_doc(
        "DOC-AUTH-DOMAIN",
        "docs/domain/token-lifecycle.md",
        DocumentKind::Domain,
        DocumentAuthority::Approved,
    )]);

    // Missing: no root index on disk.
    let missing = projection_state(tmp.path(), &reg).unwrap();
    assert_eq!(missing.state, ProjectionStatus::Missing);
    let report = check_projections(tmp.path(), &reg).unwrap();
    assert!(!report.ok);
    assert!(report.missing.contains(&"docs/_index.md".to_string()));

    // Current: write the exact expected projection.
    let expected = render_root_index(&reg).unwrap();
    fs::write(tmp.path().join("docs/_index.md"), &expected).unwrap();
    let current = projection_state(tmp.path(), &reg).unwrap();
    assert_eq!(current.state, ProjectionStatus::Current);
    assert!(check_projections(tmp.path(), &reg).unwrap().ok);

    // Stale: mutate the generated bytes (keep the marker so it stays recognized).
    let mut stale_bytes = expected.clone();
    stale_bytes.push_str("- extra trailing line\n");
    fs::write(tmp.path().join("docs/_index.md"), &stale_bytes).unwrap();
    assert!(is_pulse_generated(stale_bytes.as_bytes()));
    let stale = projection_state(tmp.path(), &reg).unwrap();
    assert_eq!(stale.state, ProjectionStatus::Stale);
    let report = check_projections(tmp.path(), &reg).unwrap();
    assert!(!report.ok);
    assert!(report.stale.contains(&"docs/_index.md".to_string()));

    // Delete + recompute: same expected bytes (rebuild yields identical projection).
    fs::remove_file(tmp.path().join("docs/_index.md")).unwrap();
    let recomputed = render_root_index(&reg).unwrap();
    assert_eq!(recomputed, expected);
}

// --- marker detection edge cases -------------------------------------------

#[test]
fn is_pulse_generated_requires_both_marker_and_supported_schema() {
    assert!(!is_pulse_generated(b"# just a normal index\n"));
    assert!(!is_pulse_generated(b"<!-- pulse-docs-projection -->\n"));
    let valid = format!(
        "{PROJECTION_MARKER}\n<!-- pulse-docs-projection:schema-version={PROJECTION_SCHEMA_VERSION} -->\n"
    );
    assert!(is_pulse_generated(valid.as_bytes()));
    // Unsupported schema version is not recognized (preserved, not rewritten).
    let future =
        format!("{PROJECTION_MARKER}\n<!-- pulse-docs-projection:schema-version=999 -->\n");
    assert!(!is_pulse_generated(future.as_bytes()));
}
