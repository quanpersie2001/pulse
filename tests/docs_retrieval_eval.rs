use pulse::canonical_json::to_canonical_bytes;
use pulse::docs::{
    run_retrieval_evals, DocsRegistry, DocumentAuthority, DocumentKind, DocumentLifecycle,
    DocumentRecord, DocumentRetrieval, DocumentScope, GeneratedContract, RetrievalConfig,
    ReviewPolicy,
};
use std::fs;

fn doc(
    id: &str,
    path: &str,
    kind: DocumentKind,
    summary: &str,
    aliases: Vec<&str>,
    domains: Vec<&str>,
) -> DocumentRecord {
    let mut record = DocumentRecord {
        id: id.to_string(),
        revision: 1,
        path: path.to_string(),
        kind,
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
    };
    record.normalize();
    record
}

fn with_scope(mut doc: DocumentRecord, paths: Vec<&str>, labels: Vec<&str>) -> DocumentRecord {
    doc.scope.paths = paths.into_iter().map(str::to_string).collect();
    doc.scope.work_labels = labels.into_iter().map(str::to_string).collect();
    doc.normalize();
    doc
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
    fs::create_dir_all(repo.join("docs/architecture")).unwrap();
    fs::create_dir_all(repo.join("docs/generated")).unwrap();
    fs::write(
        repo.join("docs/domain/token.md"),
        b"# Token Lifecycle\n\n## Expired Tokens\n\nTokenExpired means the refresh-token expired in v2.1.\n\n## Vietnamese Expiry\n\nKhi refresh token het han khong dau, va khi refresh token h\xE1\xBA\xBFt h\xE1\xBA\xA1n th\xC3\xAC h\xE1\xBB\x87 th\xE1\xBB\x91ng tr\xE1\xBA\xA3 v\xE1\xBB\x81 TokenExpired.\n",
    )
    .unwrap();
    fs::write(
        repo.join("docs/architecture/auth-routing.md"),
        b"# Authentication Architecture\n\n## Routing Needle\n\nRoutingNeedle aligns authentication work with token handling.\n",
    )
    .unwrap();
    fs::write(
        repo.join("docs/domain/generic-routing.md"),
        b"# Generic Routing\n\n## Routing Needle\n\nRoutingNeedle is an unrelated operations note.\n",
    )
    .unwrap();
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
    fs::write(
        repo.join("docs/_index.md"),
        b"<!-- pulse-docs-projection -->\n<!-- pulse-docs-projection:schema-version=1 -->\n# Documentation Index\n\n## Generated Navigation\n\nNavNeedle should never be indexed from generated navigation.\n",
    )
    .unwrap();
    fs::write(
        repo.join("docs/generated/report.md"),
        b"# Generated Report\n\n## Generated Output\n\nGeneratedNeedle should require explicit opt-in before indexing.\n",
    )
    .unwrap();
    let registry = DocsRegistry {
        schema_version: 2,
        revision: 1,
        repository_id: manifest.repository_id,
        documents: vec![
            with_scope(
                doc(
                    "DOC-AUTH-ARCH",
                    "docs/architecture/auth-routing.md",
                    DocumentKind::Architecture,
                    "Authentication routing for scoped work",
                    vec!["RoutingNeedle"],
                    vec!["authentication"],
                ),
                vec!["src/auth/**"],
                vec!["auth"],
            ),
            with_scope(
                doc(
                    "DOC-AUTH-DOMAIN",
                    "docs/domain/token.md",
                    DocumentKind::Domain,
                    "Token lifecycle with refresh-token and v2.1 expiry behavior",
                    vec!["TokenExpired", "refresh-token", "v2.1"],
                    vec!["authentication"],
                ),
                vec!["src/auth/**"],
                vec!["auth"],
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
                    DocumentKind::Domain,
                    "Draft token lifecycle",
                    vec!["refresh-token"],
                    vec!["authentication"],
                )
            },
            DocumentRecord {
                kind: DocumentKind::Generated,
                authority: DocumentAuthority::Generated,
                generated: Some(GeneratedContract {
                    sources: vec!["docs/domain/token.md".to_string()],
                    command: "pulse docs index".to_string(),
                    outputs: vec!["docs/generated/report.md".to_string()],
                    editable: false,
                    freshness_check: "pulse docs index --check".to_string(),
                }),
                ..doc(
                    "DOC-GENERATED-REPORT",
                    "docs/generated/report.md",
                    DocumentKind::Generated,
                    "Generated report",
                    vec!["GeneratedNeedle"],
                    vec!["generated"],
                )
            },
            doc(
                "DOC-GENERIC-ROUTING",
                "docs/domain/generic-routing.md",
                DocumentKind::Domain,
                "Generic routing note",
                vec!["RoutingNeedle"],
                vec!["operations"],
            ),
            doc(
                "DOC-RECOVERY-DOMAIN",
                "docs/domain/recovery.md",
                DocumentKind::Operations,
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
        "{\"id\":\"exact-identifier-hyphen-dotted\",\"query\":\"TokenExpired refresh-token v2.1\",\"filters\":{\"domain\":\"authentication\",\"limit\":8},\"expected\":{\"top_k\":[\"DOC-AUTH-DOMAIN#expired-tokens\"],\"must_exclude\":[\"DOC-DRAFT-DOMAIN#token-preview\"],\"max_first_relevant_rank\":3,\"max_context_bytes_before_first_relevant\":3000}}\n",
        "{\"id\":\"command-heading-phrase\",\"query\":\"pulse docs index rollback\",\"filters\":{\"limit\":8},\"expected\":{\"top_k\":[\"DOC-RECOVERY-DOMAIN#rollback\"],\"must_exclude\":[],\"max_first_relevant_rank\":3,\"max_context_bytes_before_first_relevant\":3000}}\n",
        "{\"id\":\"vietnamese-tokenization\",\"query\":\"refresh token hết hạn\",\"filters\":{\"domain\":\"authentication\",\"limit\":8},\"expected\":{\"top_k\":[\"DOC-AUTH-DOMAIN#vietnamese-expiry\"],\"must_exclude\":[],\"max_first_relevant_rank\":3,\"max_context_bytes_before_first_relevant\":3000}}\n",
        "{\"id\":\"work-context-adjustment\",\"query\":\"RoutingNeedle\",\"filters\":{\"limit\":8,\"explain\":true},\"work_context\":{\"required_documents\":[\"DOC-AUTH-ARCH\"],\"paths\":[\"src/auth/token.rs\"],\"domains\":[\"authentication\"],\"labels\":[\"auth\"]},\"expected\":{\"top_k\":[\"DOC-AUTH-ARCH#routing-needle\"],\"must_exclude\":[],\"max_first_relevant_rank\":1,\"max_context_bytes_before_first_relevant\":0}}\n",
        "{\"id\":\"generated-navigation-and-generated-output-exclusion\",\"query\":\"NavNeedle GeneratedNeedle TokenExpired\",\"filters\":{\"limit\":8},\"expected\":{\"top_k\":[\"DOC-AUTH-DOMAIN#expired-tokens\"],\"must_exclude\":[\"DOC-GENERATED-REPORT#generated-output\",\"DOC-DRAFT-DOMAIN#token-preview\"],\"max_first_relevant_rank\":5,\"max_context_bytes_before_first_relevant\":3000}}\n",
        "{\"id\":\"generated-navigation-no-result\",\"query\":\"NavNeedle\",\"filters\":{\"limit\":8},\"expected\":{\"top_k\":[],\"must_exclude\":[],\"max_first_relevant_rank\":null,\"max_context_bytes_before_first_relevant\":null}}\n",
        "{\"id\":\"no-result\",\"query\":\"zzzz-no-match\",\"filters\":{\"limit\":8},\"expected\":{\"top_k\":[],\"must_exclude\":[],\"max_first_relevant_rank\":null,\"max_context_bytes_before_first_relevant\":null}}\n"
    )).unwrap();
    let report = run_retrieval_evals(tmp.path(), &fixture).unwrap();
    assert_eq!(report.schema_version, 1);
    assert_eq!(report.fixture_count, 7);
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
