use pulse::canonical_json::to_canonical_bytes;
use pulse::docs::{
    run_retrieval_evals, DocsRegistry, DocumentAuthority, DocumentKind, DocumentLifecycle,
    DocumentRecord, DocumentRetrieval, DocumentScope, RetrievalConfig, ReviewPolicy,
};
use std::fs;

fn doc(
    id: &str,
    path: &str,
    summary: &str,
    aliases: Vec<&str>,
    domains: Vec<&str>,
) -> DocumentRecord {
    DocumentRecord {
        id: id.to_string(),
        revision: 1,
        path: path.to_string(),
        kind: DocumentKind::Domain,
        authority: DocumentAuthority::Approved,
        lifecycle: DocumentLifecycle::Current,
        owner: "team:docs".to_string(),
        summary: summary.to_string(),
        aliases: aliases.into_iter().map(str::to_string).collect(),
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
    fs::write(repo.join("docs/domain/token.md"), b"# Token Lifecycle\n\n## Expired Tokens\n\nTokenExpired means the refresh-token expired.\n").unwrap();
    fs::write(
        repo.join("docs/domain/recovery.md"),
        b"# Recovery\n\n## Rollback\n\nUse pulse docs index to rebuild cache.\n",
    )
    .unwrap();
    fs::write(
        repo.join("docs/domain/draft-token.md"),
        b"# Draft Token\n\n## Token Preview\n\nTokenExpired draft preview.\n",
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
                vec!["refresh-token"],
                vec!["authentication"],
            ),
            DocumentRecord {
                authority: DocumentAuthority::Draft,
                retrieval: Some(DocumentRetrieval {
                    index: true,
                    include_body: true,
                    materialize_index: false,
                }),
                ..doc(
                    "DOC-DRAFT-DOMAIN",
                    "docs/domain/draft-token.md",
                    "Draft token lifecycle",
                    vec!["refresh-token"],
                    vec!["authentication"],
                )
            },
            doc(
                "DOC-RECOVERY-DOMAIN",
                "docs/domain/recovery.md",
                "Recovery operations",
                vec!["rollback"],
                vec!["operations"],
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
fn retrieval_eval_fixture_reports_recall_mrr_exclusions_and_context_budget() {
    let tmp = setup_repo();
    let fixture = tmp.path().join("eval.jsonl");
    fs::write(&fixture, concat!(
        "{\"id\":\"exact-token-expiry\",\"query\":\"TokenExpired refresh-token\",\"filters\":{\"domain\":\"authentication\",\"limit\":8},\"expected\":{\"top_k\":[\"DOC-AUTH-DOMAIN#expired-tokens\"],\"must_exclude\":[\"DOC-RECOVERY-DOMAIN#rollback\"],\"max_first_relevant_rank\":3,\"max_context_bytes_before_first_relevant\":3000}}\n",
        "{\"id\":\"no-result\",\"query\":\"zzzz-no-match\",\"filters\":{\"limit\":8},\"expected\":{\"top_k\":[],\"must_exclude\":[],\"max_first_relevant_rank\":null,\"max_context_bytes_before_first_relevant\":null}}\n"
    )).unwrap();
    let report = run_retrieval_evals(tmp.path(), &fixture).unwrap();
    assert_eq!(report.schema_version, 1);
    assert_eq!(report.fixture_count, 2);
    assert!(report.passed, "{report:?}");
    assert_eq!(report.must_exclude_violations, 0);
    assert!(report.recall_at_k >= 1.0);
    assert!(report.mean_reciprocal_rank > 0.0);
}

#[test]
fn retrieval_eval_filters_by_kind_and_authority_and_accepts_expected_reason_codes() {
    let tmp = setup_repo();
    let fixture = tmp.path().join("filtered-eval.jsonl");
    fs::write(&fixture, concat!(
        "{\"id\":\"approved-domain-filter\",\"query\":\"TokenExpired refresh-token\",\"filters\":{\"domain\":\"authentication\",\"kind\":\"domain\",\"authority\":\"approved\",\"limit\":8},\"expected\":{\"top_k\":[\"DOC-AUTH-DOMAIN#expired-tokens\"],\"must_exclude\":[\"DOC-DRAFT-DOMAIN#token-preview\"],\"reason_codes\":[],\"max_first_relevant_rank\":3,\"max_context_bytes_before_first_relevant\":3000}}\n",
        "{\"id\":\"expected-miss\",\"query\":\"zzzz-no-match\",\"filters\":{\"kind\":\"domain\",\"authority\":\"approved\",\"limit\":8},\"expected\":{\"top_k\":[\"DOC-AUTH-DOMAIN#expired-tokens\"],\"reason_codes\":[\"docs_search_miss\"],\"max_first_relevant_rank\":1}}\n"
    )).unwrap();

    let report = run_retrieval_evals(tmp.path(), &fixture).unwrap();
    assert!(report.passed, "{report:?}");
    assert_eq!(report.fixture_count, 2);
    assert_eq!(report.results[0].reason_codes, Vec::<String>::new());
    assert_eq!(report.results[1].reason_codes, vec!["docs_search_miss"]);
}

#[test]
fn retrieval_eval_invalid_jsonl_is_typed_error() {
    let tmp = setup_repo();
    let fixture = tmp.path().join("bad.jsonl");
    fs::write(&fixture, "{bad json}\n").unwrap();
    let err = run_retrieval_evals(tmp.path(), &fixture).unwrap_err();
    assert_eq!(err.code(), "docs_retrieval_eval_invalid");
}
