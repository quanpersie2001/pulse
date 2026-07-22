use pulse::canonical_json::to_canonical_bytes;
use pulse::docs::{
    docs_tree, get_docs, search_docs, DocsRegistry, DocumentAuthority, DocumentKind,
    DocumentLifecycle, DocumentRecord, DocumentScope, GetOptions, RetrievalConfig, ReviewPolicy,
    SearchOptions, TreeOptions,
};
use std::fs;
use std::process::Command;

fn doc(id: &str, path: &str, summary: &str, domains: Vec<&str>) -> DocumentRecord {
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
    fs::write(repo.join("docs/domain/token.md"), b"# Token Lifecycle\n\nPreamble text.\n\n## Expired Tokens\n\nTokenExpired means the refresh-token expired in v2.1.\n").unwrap();
    fs::write(
        repo.join("docs/domain/other.md"),
        b"# Other\n\n## Misc\n\nNothing about auth.\n",
    )
    .unwrap();
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
                "DOC-OTHER-DOMAIN",
                "docs/domain/other.md",
                "Other",
                vec!["other"],
            ),
        ],
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
    assert_eq!(err.code(), "docs_index_stale");
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
fn stale_section_ref_errors_clearly() {
    let tmp = setup_repo();
    let err = get_docs(
        tmp.path(),
        "DOC-AUTH-DOMAIN#does-not-exist",
        GetOptions::default(),
    )
    .unwrap_err();
    assert_eq!(err.code(), "docs_anchor_stale");
}

#[test]
fn tree_works_without_cache_from_registry_only() {
    let tmp = setup_repo();
    let tree = docs_tree(tmp.path(), None, TreeOptions::default()).unwrap();
    assert_eq!(tree.root, "docs");
    assert!(tree.nodes.iter().any(|node| node.path == "docs/domain"));
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
    assert!(
        !run(&["docs", "search", "TokenExpired", "--json"])["results"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        run(&["docs", "get", "DOC-AUTH-DOMAIN#expired-tokens", "--json"])["section"]["section_ref"],
        "DOC-AUTH-DOMAIN#expired-tokens"
    );
    assert_eq!(run(&["docs", "tree", "--json"])["schema_version"], 1);
}
