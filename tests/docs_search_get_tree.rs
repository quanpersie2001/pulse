use chrono::{TimeZone, Utc};
use pulse::canonical_json::{hash_bytes, to_canonical_bytes};
use pulse::docs::{
    build_index, docs_tree, get_docs, read_current, search_docs, validate_generation, DocsRegistry,
    DocumentAuthority, DocumentKind, DocumentLifecycle, DocumentRecord, DocumentScope, GetOptions,
    RetrievalConfig, RetrievalScope, ReviewPolicy, SearchOptions, TreeOptions,
    WorkDocumentationContext,
};
use pulse::graph::node::DocumentationImpactPosture;
use pulse::graph::store::{DocumentationImpactUpdate, OperationContext};
use pulse::id::WorkKind;
use pulse::JsonGraphStore;
use std::fs;
use std::process::Command;

fn doc(id: &str, path: &str, summary: &str, domains: Vec<&str>) -> DocumentRecord {
    let authority = if id.contains("DRAFT") {
        DocumentAuthority::Draft
    } else {
        DocumentAuthority::Approved
    };
    let lifecycle = if id.contains("STALE") {
        DocumentLifecycle::Stale
    } else {
        DocumentLifecycle::Current
    };
    DocumentRecord {
        id: id.to_string(),
        revision: 1,
        path: path.to_string(),
        kind: DocumentKind::Domain,
        authority,
        lifecycle,
        owner: "team:docs".to_string(),
        summary: summary.to_string(),
        aliases: vec!["refresh-token".to_string()],
        scope: DocumentScope {
            paths: vec![],
            domains: domains.into_iter().map(str::to_string).collect(),
            work_labels: vec![],
        },
        review_policy: ReviewPolicy::None,
        verification_profile: "domain-doc".to_string(),
        generated: None,
        superseded_by: None,
        retrieval: None,
    }
}

fn ctx(actor: &str, sec: i64) -> OperationContext {
    OperationContext {
        actor: actor.to_string(),
        now: Utc.timestamp_opt(sec, 0).unwrap(),
    }
}

fn setup_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let manifest = pulse::evidence::manifest::bootstrap(repo).unwrap().manifest;
    pulse::storage::bootstrap(repo).unwrap();
    fs::create_dir_all(repo.join(".pulse/docs/schemas")).unwrap();
    let schema: serde_json::Value = serde_json::from_str(pulse::docs::DOCUMENT_SCHEMA).unwrap();
    fs::write(
        repo.join(".pulse/docs/schemas/document.schema.json"),
        to_canonical_bytes(&schema).unwrap(),
    )
    .unwrap();
    fs::create_dir_all(repo.join("docs/domain")).unwrap();
    fs::create_dir_all(repo.join("docs/authentication")).unwrap();
    fs::write(repo.join("docs/domain/token.md"), b"# Token Lifecycle\n\nPreamble text.\n\n## Expired Tokens\n\nTokenExpired means the refresh-token expired in v2.1.\n").unwrap();
    fs::write(
        repo.join("docs/domain/other.md"),
        b"# Other\n\n## Misc\n\nNothing about auth. OutScopeNeedle OutScopeNeedle OutScopeNeedle.\n",
    )
    .unwrap();
    fs::write(
        repo.join("docs/authentication/guide.md"),
        b"# Auth Guide\n\nAuthentication scope guide.\n",
    )
    .unwrap();
    fs::write(
        repo.join("docs/domain/draft.md"),
        b"# Draft Docs\n\n## Draft\n\nDraftFlag lexical hit.\n",
    )
    .unwrap();
    fs::write(
        repo.join("docs/domain/stale.md"),
        b"# Stale Docs\n\n## Stale\n\nStaleFlag lexical hit.\n",
    )
    .unwrap();
    fs::write(repo.join("AGENTS.md"), b"# Repo Map\n").unwrap();
    fs::write(repo.join("PULSE.md"), b"# Repo Policy\n").unwrap();
    let registry = DocsRegistry {
        schema_version: 2,
        revision: 1,
        repository_id: manifest.repository_id,
        documents: vec![
            doc(
                "DOC-AUTH-DOMAIN",
                "docs/domain/token.md",
                "Token lifecycle",
                vec!["authentication"],
            ),
            doc(
                "DOC-AUTH-GUIDE",
                "docs/authentication/guide.md",
                "Authentication guide",
                vec!["authentication"],
            ),
            doc(
                "DOC-DRAFT-DOMAIN",
                "docs/domain/draft.md",
                "DraftFlag documentation",
                vec!["drafts"],
            ),
            doc(
                "DOC-OTHER-DOMAIN",
                "docs/domain/other.md",
                "Other",
                vec!["other"],
            ),
            DocumentRecord {
                kind: DocumentKind::RepositoryMap,
                path: "AGENTS.md".to_string(),
                summary: "Repository map".to_string(),
                ..doc(
                    "DOC-REPO-MAP",
                    "AGENTS.md",
                    "Repository map",
                    vec!["repository"],
                )
            },
            DocumentRecord {
                kind: DocumentKind::Policy,
                path: "PULSE.md".to_string(),
                summary: "Repository policy".to_string(),
                ..doc(
                    "DOC-REPO-POLICY",
                    "PULSE.md",
                    "Repository policy",
                    vec!["repository"],
                )
            },
            doc(
                "DOC-STALE-DOMAIN",
                "docs/domain/stale.md",
                "StaleFlag documentation",
                vec!["stale"],
            ),
        ],
        retrieval: Some(RetrievalConfig {
            scopes: vec![RetrievalScope {
                path: "docs/domain".to_string(),
                summary: "Domain documentation area".to_string(),
                materialize_index: None,
            }],
            ..RetrievalConfig::defaults()
        }),
    };
    fs::write(
        repo.join(".pulse/docs/registry.json"),
        to_canonical_bytes(&registry).unwrap(),
    )
    .unwrap();
    tmp
}

#[test]
fn search_auto_refreshes_and_returns_bounded_section_snippet() {
    let tmp = setup_repo();
    let report = search_docs(
        tmp.path(),
        "TokenExpired refresh-token",
        SearchOptions {
            limit: Some(4),
            ..SearchOptions::default()
        },
    )
    .unwrap();
    assert_eq!(report.schema_version, 1);
    assert_eq!(report.index.state, "current");
    assert!(!report.results.is_empty());
    let top = &report.results[0];
    assert_eq!(top.document_id, "DOC-AUTH-DOMAIN");
    assert!(top.snippet.contains("TokenExpired") || top.snippet.contains("Expired Tokens"));
    assert!(report.budget.returned_snippet_bytes <= report.budget.result_limit * 500);
}

#[test]
fn search_filters_by_domain_and_no_refresh_errors_when_missing() {
    let tmp = setup_repo();
    let err = search_docs(
        tmp.path(),
        "TokenExpired",
        SearchOptions {
            no_refresh: true,
            ..SearchOptions::default()
        },
    )
    .unwrap_err();
    assert_eq!(err.code(), "docs_index_missing");
    let report = search_docs(
        tmp.path(),
        "TokenExpired",
        SearchOptions {
            domain: Some("authentication".to_string()),
            ..SearchOptions::default()
        },
    )
    .unwrap();
    assert!(report
        .results
        .iter()
        .all(|hit| hit.domains_contains("authentication")));
}

trait DomainsContains {
    fn domains_contains(&self, domain: &str) -> bool;
}
impl DomainsContains for pulse::docs::SearchResult {
    fn domains_contains(&self, domain: &str) -> bool {
        self.applicability_reasons.iter().any(|r| r == domain)
            || self.document_id == "DOC-AUTH-DOMAIN"
    }
}

#[test]
fn search_auto_refresh_respects_cost_guard_but_explicit_index_does_not() {
    let tmp = setup_repo();
    let repo = tmp.path();
    let mut registry: DocsRegistry =
        pulse::storage::read_json(&repo.join(".pulse/docs/registry.json")).unwrap();
    let mut config = registry.retrieval_config();
    config.auto_refresh_max_documents = 1;
    registry.retrieval = Some(config);
    fs::write(
        repo.join(".pulse/docs/registry.json"),
        to_canonical_bytes(&registry).unwrap(),
    )
    .unwrap();

    let err = search_docs(repo, "TokenExpired", SearchOptions::default()).unwrap_err();
    assert_eq!(err.code(), "docs_index_refresh_required");

    let indexed = build_index(repo, pulse::docs::IndexOptions::default()).unwrap();
    assert_eq!(indexed.index.state, "current");
}

#[test]
fn search_refreshes_corrupt_current_and_no_refresh_reports_typed_error() {
    let tmp = setup_repo();
    let repo = tmp.path();
    build_index(repo, pulse::docs::IndexOptions::default()).unwrap();
    fs::write(
        repo.join(".pulse/cache/docs-search/CURRENT"),
        b"not-a-generation\n",
    )
    .unwrap();

    let err = search_docs(
        repo,
        "TokenExpired",
        SearchOptions {
            no_refresh: true,
            ..SearchOptions::default()
        },
    )
    .unwrap_err();
    assert_eq!(err.code(), "docs_index_corrupt");

    let report = search_docs(repo, "TokenExpired", SearchOptions::default()).unwrap();
    assert_eq!(report.index.state, "current");
}

#[test]
fn search_refreshes_corrupt_generation_and_no_refresh_reports_typed_error() {
    let tmp = setup_repo();
    let repo = tmp.path();
    build_index(repo, pulse::docs::IndexOptions::default()).unwrap();
    let current = read_current(repo).unwrap();
    let generation = validate_generation(repo, &current).unwrap();
    fs::write(&generation.sections_path, b"corrupt sections\n").unwrap();

    let err = search_docs(
        repo,
        "TokenExpired",
        SearchOptions {
            no_refresh: true,
            ..SearchOptions::default()
        },
    )
    .unwrap_err();
    assert_eq!(err.code(), "docs_index_corrupt");

    let report = search_docs(repo, "TokenExpired", SearchOptions::default()).unwrap();
    assert_eq!(report.index.state, "current");
    assert!(!report.results.is_empty());
}

#[test]
fn search_no_refresh_reports_incompatible_generation() {
    let tmp = setup_repo();
    let repo = tmp.path();
    build_index(repo, pulse::docs::IndexOptions::default()).unwrap();
    let current = read_current(repo).unwrap();
    let generation = validate_generation(repo, &current).unwrap();
    let mut state: serde_json::Value =
        pulse::storage::read_json(&generation.generation_path.join("state.json")).unwrap();
    state["schema_version"] = serde_json::json!(999);
    fs::write(
        generation.generation_path.join("state.json"),
        to_canonical_bytes(&state).unwrap(),
    )
    .unwrap();

    let err = search_docs(
        repo,
        "TokenExpired",
        SearchOptions {
            no_refresh: true,
            ..SearchOptions::default()
        },
    )
    .unwrap_err();
    assert_eq!(err.code(), "docs_index_incompatible");
}

#[test]
fn search_preserves_identifiers_explain_and_plain_text_query() {
    let tmp = setup_repo();
    let report = search_docs(
        tmp.path(),
        "document_id:DOC-OTHER-DOMAIN OR TokenExpired refresh-token v2.1 pulse docs index",
        SearchOptions {
            explain: true,
            ..SearchOptions::default()
        },
    )
    .unwrap();
    assert!(report
        .results
        .iter()
        .any(|hit| hit.document_id == "DOC-AUTH-DOMAIN"));
    assert!(report
        .results
        .iter()
        .any(|hit| !hit.matched_fields.is_empty()));
}

#[test]
fn search_include_flags_and_work_reasons_populate_without_fake_required_hits() {
    let tmp = setup_repo();
    assert!(
        search_docs(tmp.path(), "DraftFlag", SearchOptions::default())
            .unwrap()
            .results
            .is_empty()
    );
    assert!(search_docs(
        tmp.path(),
        "DraftFlag",
        SearchOptions {
            include_draft: true,
            ..SearchOptions::default()
        },
    )
    .unwrap()
    .results
    .iter()
    .any(|hit| hit.document_id == "DOC-DRAFT-DOMAIN"));
    assert!(search_docs(
        tmp.path(),
        "StaleFlag",
        SearchOptions {
            include_stale: true,
            ..SearchOptions::default()
        },
    )
    .unwrap()
    .results
    .iter()
    .any(|hit| hit.document_id == "DOC-STALE-DOMAIN"));

    let work = WorkDocumentationContext {
        work_id: "TK-SEARCH".to_string(),
        revision: 7,
        posture: pulse::docs::DocumentationPosture::Required,
        required_documents: vec!["DOC-AUTH-DOMAIN".to_string()],
        paths: vec![],
        domains: vec!["authentication".to_string()],
        labels: vec![],
    };
    let report = search_docs(
        tmp.path(),
        "TokenExpired",
        SearchOptions {
            explain: true,
            work: Some(work),
            ..SearchOptions::default()
        },
    )
    .unwrap();
    assert_eq!(report.work.as_ref().unwrap().id, "TK-SEARCH");
    let hit = report
        .results
        .iter()
        .find(|hit| hit.document_id == "DOC-AUTH-DOMAIN")
        .unwrap();
    assert!(hit
        .applicability_reasons
        .iter()
        .any(|reason| reason == "explicit_required_document"));

    assert!(hit.score > hit.lexical_score);

    let missing_required = WorkDocumentationContext {
        work_id: "TK-MISSING".to_string(),
        revision: 1,
        posture: pulse::docs::DocumentationPosture::Required,
        required_documents: vec!["DOC-OTHER-DOMAIN".to_string()],
        paths: vec![],
        domains: vec![],
        labels: vec![],
    };
    let no_fake = search_docs(
        tmp.path(),
        "TokenExpired",
        SearchOptions {
            work: Some(missing_required),
            ..SearchOptions::default()
        },
    )
    .unwrap();
    assert!(no_fake
        .results
        .iter()
        .any(|hit| hit.document_id == "DOC-AUTH-DOMAIN"));
    assert!(no_fake
        .results
        .iter()
        .all(|hit| hit.document_id != "DOC-OTHER-DOMAIN"));
}

#[test]
fn search_work_boosts_without_hard_filtering_lexical_hits() {
    let tmp = setup_repo();
    let work = WorkDocumentationContext {
        work_id: "TK-WORK-SCOPE".to_string(),
        revision: 3,
        posture: pulse::docs::DocumentationPosture::Required,
        required_documents: vec!["DOC-AUTH-GUIDE".to_string()],
        paths: vec![],
        domains: vec!["authentication".to_string()],
        labels: vec![],
    };
    let report = search_docs(
        tmp.path(),
        "auth OutScopeNeedle",
        SearchOptions {
            work: Some(work),
            explain: true,
            limit: Some(10),
            ..SearchOptions::default()
        },
    )
    .unwrap();

    let out_of_scope = report
        .results
        .iter()
        .find(|hit| hit.document_id == "DOC-OTHER-DOMAIN")
        .expect("out-of-scope lexical hit remains visible under --work");
    assert_eq!(out_of_scope.score, out_of_scope.lexical_score);
    assert!(out_of_scope.applicability_reasons.is_empty());

    let scoped = report
        .results
        .iter()
        .find(|hit| hit.document_id == "DOC-AUTH-GUIDE")
        .expect("required/scoped lexical hit is retained");
    assert!(scoped.score > scoped.lexical_score);
    assert!(scoped
        .applicability_reasons
        .iter()
        .any(|reason| reason == "explicit_required_document"));
    assert!(scoped
        .applicability_reasons
        .iter()
        .any(|reason| reason == "domain_scope_match"));

    let ranks = report
        .results
        .iter()
        .map(|hit| hit.rank)
        .collect::<Vec<_>>();
    assert_eq!(ranks, (1..=report.results.len() as u32).collect::<Vec<_>>());
}

#[test]
fn search_snippets_are_utf8_safe_and_fallback_when_doc_modified() {
    let tmp = setup_repo();
    search_docs(tmp.path(), "TokenExpired", SearchOptions::default()).unwrap();
    let unicode = format!(
        "# Token Lifecycle\n\n## Expired Tokens\n\n{} TokenExpired\n",
        "é".repeat(600)
    );
    fs::write(tmp.path().join("docs/domain/token.md"), unicode).unwrap();
    let report = search_docs(tmp.path(), "TokenExpired", SearchOptions::default()).unwrap();
    let snippet = &report.results[0].snippet;
    assert!(snippet.is_char_boundary(snippet.len()));
    assert!(!snippet.is_empty());
}

#[test]
fn get_document_outline_and_section_body_are_bounded_and_current() {
    let tmp = setup_repo();
    let doc = get_docs(tmp.path(), "DOC-AUTH-DOMAIN", GetOptions::default()).unwrap();
    assert!(doc.section.is_none());
    assert!(doc
        .outline
        .iter()
        .any(|item| item.section_ref == "DOC-AUTH-DOMAIN#expired-tokens"));
    let section = get_docs(
        tmp.path(),
        "DOC-AUTH-DOMAIN#expired-tokens",
        GetOptions {
            max_lines: Some(20),
            ..GetOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        section.section.unwrap().section_ref,
        "DOC-AUTH-DOMAIN#expired-tokens"
    );
    assert!(section.body.unwrap().contains("TokenExpired"));
}

#[test]
fn get_supports_chunk_refs_path_ranges_exact_hashes_and_safe_truncation() {
    let tmp = setup_repo();
    let bytes = fs::read(tmp.path().join("docs/domain/token.md")).unwrap();
    let range = get_docs(
        tmp.path(),
        "docs/domain/token.md:5-7",
        GetOptions::default(),
    )
    .unwrap();
    let section = range.section.unwrap();
    assert_eq!(section.range.start_line, 5);
    assert_eq!(section.range.end_line, 7);
    let expected = hash_bytes(&bytes["# Token Lifecycle\n\nPreamble text.\n\n".len()..]);
    assert_eq!(section.section_content_hash, expected);
    assert!(range.body.unwrap().contains("TokenExpired"));

    let invalid = get_docs(
        tmp.path(),
        "docs/domain/token.md:7-5",
        GetOptions::default(),
    )
    .unwrap_err();
    assert_eq!(invalid.code(), "docs_get_range_invalid");
    let unregistered = get_docs(
        tmp.path(),
        "docs/domain/missing.md:1-1",
        GetOptions::default(),
    )
    .unwrap_err();
    assert_eq!(unregistered.code(), "docs_anchor_stale");

    fs::write(
        tmp.path().join("docs/domain/token.md"),
        b"# Token Lifecycle\n\n## Expired Tokens\n\nabc\xE2\x82\xACdef\n",
    )
    .unwrap();
    let truncated = get_docs(
        tmp.path(),
        "DOC-AUTH-DOMAIN#expired-tokens",
        GetOptions {
            max_bytes: Some(17),
            ..GetOptions::default()
        },
    )
    .unwrap();
    assert!(truncated.truncated);
    assert!(truncated.body.unwrap().is_char_boundary(17));

    let mut big = String::from("# Token Lifecycle\n");
    for line in 0..170 {
        big.push_str(&format!("line {line}\n"));
    }
    fs::write(tmp.path().join("docs/domain/token.md"), big).unwrap();
    let chunk = get_docs(
        tmp.path(),
        "DOC-AUTH-DOMAIN#token-lifecycle@2",
        GetOptions::default(),
    )
    .unwrap();
    assert_eq!(
        chunk.section.unwrap().section_ref,
        "DOC-AUTH-DOMAIN#token-lifecycle@2"
    );
}

#[test]
fn get_base_ref_for_oversized_section_resolves_to_chunks() {
    let tmp = setup_repo();
    let repo = tmp.path();
    let mut big = String::from("# Token Lifecycle\n");
    for line in 0..170 {
        big.push_str(&format!("oversized line {line}\n"));
    }
    fs::write(repo.join("docs/domain/token.md"), big).unwrap();

    let base = get_docs(
        repo,
        "DOC-AUTH-DOMAIN#token-lifecycle",
        GetOptions {
            max_lines: Some(200),
            max_bytes: Some(32_768),
            ..GetOptions::default()
        },
    )
    .unwrap();
    let section = base.section.as_ref().unwrap();
    assert_eq!(section.section_ref, "DOC-AUTH-DOMAIN#token-lifecycle");
    assert_eq!(section.range.start_line, 1);
    assert_eq!(section.range.end_line, 171);
    assert!(base.truncated, "base ref defaults to first chunk only");
    let body = base.body.unwrap();
    assert!(body.contains("oversized line 0"));
    assert!(body.contains("oversized line 158"));
    assert!(!body.contains("oversized line 169"));
    assert!(base
        .outline
        .iter()
        .any(|item| item.section_ref == "DOC-AUTH-DOMAIN#token-lifecycle@2"));
}

#[test]
fn get_full_section_for_oversized_base_ref_spans_later_chunks() {
    let tmp = setup_repo();
    let repo = tmp.path();
    let mut big = String::from("# Token Lifecycle\n");
    for line in 0..170 {
        big.push_str(&format!("full section line {line}\n"));
    }
    fs::write(repo.join("docs/domain/token.md"), big).unwrap();

    let full = get_docs(
        repo,
        "DOC-AUTH-DOMAIN#token-lifecycle",
        GetOptions {
            max_bytes: Some(32_768),
            full_section: true,
            ..GetOptions::default()
        },
    )
    .unwrap();
    assert!(!full.truncated);
    assert_eq!(
        full.section.unwrap().section_ref,
        "DOC-AUTH-DOMAIN#token-lifecycle"
    );
    let body = full.body.unwrap();
    assert!(body.contains("full section line 0"));
    assert!(body.contains("full section line 169"));
}

#[test]
fn search_child_only_term_does_not_match_parent_from_duplicated_body() {
    let tmp = setup_repo();
    let repo = tmp.path();
    fs::write(
        repo.join("docs/domain/token.md"),
        b"# Token Lifecycle\n\nParent intro without the child-only term.\n\n## Child Details\n\nNestedChildNeedle appears only inside the child section.\n",
    )
    .unwrap();

    let report = search_docs(
        repo,
        "NestedChildNeedle",
        SearchOptions {
            limit: Some(10),
            ..SearchOptions::default()
        },
    )
    .unwrap();
    assert!(report
        .results
        .iter()
        .any(|hit| hit.section_ref == "DOC-AUTH-DOMAIN#child-details"));
    assert!(
        report
            .results
            .iter()
            .all(|hit| hit.section_ref != "DOC-AUTH-DOMAIN#token-lifecycle"),
        "parent section must not match solely because child body text was duplicated into its index body"
    );
}

#[test]
fn stale_section_ref_errors_clearly() {
    let tmp = setup_repo();
    let err = get_docs(
        tmp.path(),
        "DOC-AUTH-DOMAIN#does-not-exist",
        GetOptions::default(),
    )
    .unwrap_err();
    assert_eq!(err.code(), "docs_anchor_stale");
    let message = err.to_string();
    assert!(message.contains("current_document"));
    assert!(message.contains("candidate_section_refs"));
}

#[test]
fn get_uses_registry_defaults_when_cli_limits_are_absent_and_explicit_limits_override() {
    let tmp = setup_repo();
    let repo = tmp.path();
    let mut body = String::from("# Token Lifecycle\n\n## Expired Tokens\n\n");
    for line in 0..6 {
        body.push_str(&format!("default limited line {line}\n"));
    }
    fs::write(repo.join("docs/domain/token.md"), body).unwrap();

    let mut registry: DocsRegistry =
        pulse::storage::read_json(&repo.join(".pulse/docs/registry.json")).unwrap();
    let mut config = registry.retrieval_config();
    config.default_get_max_lines = 2;
    config.default_get_max_bytes = 1024;
    registry.retrieval = Some(config);
    fs::write(
        repo.join(".pulse/docs/registry.json"),
        to_canonical_bytes(&registry).unwrap(),
    )
    .unwrap();

    let default_limited = get_docs(
        repo,
        "DOC-AUTH-DOMAIN#expired-tokens",
        GetOptions::default(),
    )
    .unwrap();
    assert!(default_limited.truncated);
    assert_eq!(default_limited.body.unwrap().lines().count(), 2);

    let explicit_limit = get_docs(
        repo,
        "DOC-AUTH-DOMAIN#expired-tokens",
        GetOptions {
            max_lines: Some(7),
            ..GetOptions::default()
        },
    )
    .unwrap();
    assert!(explicit_limit
        .body
        .unwrap()
        .contains("default limited line 4"));
}

#[test]
fn tree_works_without_cache_from_registry_only() {
    let tmp = setup_repo();
    let tree = docs_tree(tmp.path(), None, TreeOptions::default()).unwrap();
    assert_eq!(tree.root, "docs");
    let domain = tree
        .nodes
        .iter()
        .find(|node| node.path == "docs/domain")
        .unwrap();
    assert_eq!(domain.summary.as_deref(), Some("Domain documentation area"));
    let repository = tree
        .nodes
        .iter()
        .find(|node| node.path == "Repository")
        .unwrap();
    assert_eq!(repository.kind, "Repository");
    let repository_doc_ids = repository
        .children
        .iter()
        .filter_map(|node| node.document_id.as_deref())
        .collect::<Vec<_>>();
    assert!(repository_doc_ids.contains(&"DOC-REPO-MAP"));
    assert!(repository_doc_ids.contains(&"DOC-REPO-POLICY"));
}

#[test]
fn tree_path_safety_uses_component_boundaries_and_excludes_virtual_docs_for_subtrees() {
    let tmp = setup_repo();
    let auth = docs_tree(tmp.path(), Some("docs/auth"), TreeOptions::default()).unwrap();
    assert!(
        auth.nodes.is_empty(),
        "docs/auth must not match docs/authentication"
    );
    let subtree = docs_tree(tmp.path(), Some("docs/domain"), TreeOptions::default()).unwrap();
    assert!(subtree.nodes.iter().all(|node| node.path != "Repository"));
    assert!(subtree
        .nodes
        .iter()
        .flat_map(|node| node.children.iter())
        .all(|node| node.path.starts_with("docs/domain/")));
    let err = docs_tree(tmp.path(), Some("../docs"), TreeOptions::default()).unwrap_err();
    assert_eq!(err.code(), "docs_tree_path_invalid");
}

#[test]
fn cli_index_status_search_get_tree_json_contracts() {
    let tmp = setup_repo();
    let bin =
        std::env::var("CARGO_BIN_EXE_pulse").unwrap_or_else(|_| "target/debug/pulse".to_string());
    let run = |args: &[&str]| -> serde_json::Value {
        let out = Command::new(&bin)
            .arg("--repo-root")
            .arg(tmp.path())
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "stderr={} stdout={}",
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    };
    assert_eq!(
        run(&["docs", "index", "--json"])["index"]["state"],
        "current"
    );
    assert_eq!(
        run(&["docs", "status", "--json"])["index"]["state"],
        "current"
    );
    assert!(!run(&[
        "docs",
        "search",
        "TokenExpired",
        "--include-draft",
        "--include-stale",
        "--explain",
        "--json"
    ])["results"]
        .as_array()
        .unwrap()
        .is_empty());

    let store = JsonGraphStore::new(tmp.path());
    let ticket = store
        .create_node_with_context(WorkKind::Ticket, "Search docs".into(), ctx("test", 1))
        .unwrap()
        .value;
    store
        .update_documentation_impact_with_context(
            &ticket.id,
            ticket.revision,
            DocumentationImpactUpdate {
                posture: DocumentationImpactPosture::Required,
                rationale: None,
                required_documents: vec!["DOC-AUTH-DOMAIN".to_string()],
                deferred_to: vec![],
                paths: vec![],
                domains: vec!["authentication".to_string()],
                labels: vec![],
            },
            ctx("test", 2),
        )
        .unwrap();
    let worked = run(&[
        "docs",
        "search",
        "TokenExpired",
        "--work",
        &ticket.id,
        "--explain",
        "--json",
    ]);
    assert_eq!(worked["work"]["id"], ticket.id);
    assert!(worked["results"][0]["applicability_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason == "explicit_required_document"));
    assert_eq!(
        run(&["docs", "get", "DOC-AUTH-DOMAIN#expired-tokens", "--json"])["section"]["section_ref"],
        "DOC-AUTH-DOMAIN#expired-tokens"
    );
    assert_eq!(run(&["docs", "tree", "--json"])["schema_version"], 1);
}
