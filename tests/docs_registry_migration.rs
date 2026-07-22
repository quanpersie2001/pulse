//! Phase 1 (Slice 5) foundation tests:
//! - R1: exact v1 -> v2 registry/schema migration (ids/revisions preserved,
//!   registry revision bumped once, immutable event, idempotent).
//! - R2: unknown/modified predecessor schema is rejected and files preserved.
//! - R3: retrieval config defaults are deterministic/normalized.
//! - R29a: retrieval-only document edit bumps registry revision but NOT the
//!   receipt-bound document revision.
//! - R29b: a valid pre-migration v1 documentation receipt remains integrity- and
//!   binding-valid before and after registry v2 migration. Payload v1 is treated
//!   as historical/legacy-unresolved while retrieval-only metadata is ignored by
//!   verification.

use chrono::Utc;
use pulse::canonical_json::{hash_bytes, to_canonical_bytes};
use pulse::docs::manifest::{migrate_registry, predecessor_schema, schema_version, SchemaVersion};
use pulse::docs::model::{
    DocsRegistry, DocumentAuthority, DocumentKind, DocumentLifecycle, DocumentPatch,
    DocumentRecord, DocumentRetrieval, DocumentScope, RetrievalConfig, ReviewPolicy,
};
use pulse::docs::{
    bootstrap as docs_bootstrap, edit as docs_edit, is_retrieval_only_change,
    register as docs_register, validate_registry,
};
use pulse::evidence::model::*;
use pulse::evidence::{manifest as evidence_manifest, record_receipt, verify_receipt};
use serde_json::json;
use std::fs;
use std::process::Command;

fn git(repo: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn commit_all(repo: &std::path::Path) -> String {
    if !repo.join(".git").exists() {
        git(repo, &["init"]);
        git(repo, &["config", "user.email", "test@example.com"]);
        git(repo, &["config", "user.name", "Test"]);
    }
    git(repo, &["add", "."]);
    git(repo, &["commit", "--allow-empty", "-m", "snapshot"]);
    git(repo, &["rev-parse", "HEAD"])
}

fn write_v1_repo(
    repo: &std::path::Path,
    repository_id: &str,
    documents: Vec<DocumentRecord>,
) -> (String, DocsRegistry) {
    let registry = DocsRegistry {
        schema_version: 1,
        revision: 1,
        repository_id: repository_id.to_string(),
        documents,
        retrieval: None,
    };
    fs::create_dir_all(repo.join(".pulse/docs/schemas")).unwrap();
    let v1_schema_value: serde_json::Value = serde_json::from_str(predecessor_schema()).unwrap();
    fs::write(
        repo.join(".pulse/docs/schemas/document.schema.json"),
        to_canonical_bytes(&v1_schema_value).unwrap(),
    )
    .unwrap();
    fs::write(
        repo.join(".pulse/docs/registry.json"),
        to_canonical_bytes(&registry).unwrap(),
    )
    .unwrap();
    let schema_hash = hash_bytes(&to_canonical_bytes(&v1_schema_value).unwrap());
    (schema_hash, registry)
}

fn domain_doc(id: &str, path: &str) -> DocumentRecord {
    DocumentRecord {
        id: id.to_string(),
        revision: 1,
        path: path.to_string(),
        kind: DocumentKind::Domain,
        authority: DocumentAuthority::Approved,
        lifecycle: DocumentLifecycle::Current,
        owner: "team:docs".to_string(),
        summary: format!("Summary {id}"),
        aliases: Vec::new(),
        scope: DocumentScope::default(),
        review_policy: ReviewPolicy::None,
        verification_profile: "domain-doc".to_string(),
        generated: None,
        superseded_by: None,
        retrieval: None,
    }
}

#[test]
fn r1_exact_predecessor_migrates_to_v2_preserving_identity_once_and_idempotently() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let repository_id = evidence_manifest::bootstrap(repo)
        .unwrap()
        .manifest
        .repository_id;
    fs::create_dir_all(repo.join("docs/domain")).unwrap();
    fs::write(repo.join("docs/domain/token.md"), b"# Token\n").unwrap();
    let doc = domain_doc("DOC-AUTH-DOMAIN", "docs/domain/token.md");
    let (v1_schema_hash, v1_registry) = write_v1_repo(repo, &repository_id, vec![doc.clone()]);
    let v1_registry_hash = hash_bytes(&to_canonical_bytes(&v1_registry).unwrap());

    // Before migration the schema classifies as the known predecessor.
    assert_eq!(
        schema_version(repo).unwrap(),
        SchemaVersion::KnownPredecessor
    );

    let outcome = migrate_registry(repo, &repository_id).unwrap();
    assert_eq!(outcome.code, "schema_migrated");
    assert_eq!(outcome.status, pulse::docs::MigrationStatus::Migrated);
    assert_eq!(outcome.registry_revision_before, 1);
    assert_eq!(outcome.registry_revision_after, 2);
    assert_eq!(outcome.schema_hash_before, v1_schema_hash);
    assert_ne!(outcome.schema_hash_before, outcome.schema_hash_after);
    assert_eq!(outcome.registry.schema_version, 2);
    assert_eq!(
        outcome.registry.retrieval,
        Some(RetrievalConfig::defaults())
    );
    // Document identity + revision preserved.
    assert_eq!(outcome.registry.documents.len(), 1);
    assert_eq!(outcome.registry.documents[0].id, "DOC-AUTH-DOMAIN");
    assert_eq!(outcome.registry.documents[0].revision, 1);

    // Schema is now current.
    assert_eq!(schema_version(repo).unwrap(), SchemaVersion::Current);

    // The migration event exists and records before/after hashes.
    let events_dir = repo.join(".pulse/events");
    let found = fs::read_dir(&events_dir).unwrap().any(|day| {
        let day = day.unwrap().path();
        fs::read_dir(&day).unwrap().any(|entry| {
            let value: serde_json::Value =
                serde_json::from_slice(&fs::read(entry.unwrap().path()).unwrap()).unwrap();
            value.get("event_type").and_then(|v| v.as_str())
                == Some("docs.registry.schema_migrated")
                && value["payload"]["schema_hash_before"].as_str() == Some(v1_schema_hash.as_str())
                && value["payload"]["registry_hash_before"].as_str()
                    == Some(v1_registry_hash.as_str())
                && value["payload"]["document_revisions_preserved"].as_bool() == Some(true)
        })
    });
    assert!(found, "docs.registry.schema_migrated event must be emitted");

    // Idempotent retry reports already_current with no further revision bump.
    let again = migrate_registry(repo, &repository_id).unwrap();
    assert_eq!(again.code, "already_current");
    assert_eq!(again.status, pulse::docs::MigrationStatus::AlreadyCurrent);
    assert_eq!(again.registry_revision_before, 2);
    assert_eq!(again.registry_revision_after, 2);
}

#[test]
fn r2_unknown_predecessor_is_rejected_and_files_preserved() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let repository_id = evidence_manifest::bootstrap(repo)
        .unwrap()
        .manifest
        .repository_id;
    fs::create_dir_all(repo.join("docs/domain")).unwrap();
    fs::write(repo.join("docs/domain/token.md"), b"# Token\n").unwrap();
    let doc = domain_doc("DOC-AUTH-DOMAIN", "docs/domain/token.md");
    let (_, _) = write_v1_repo(repo, &repository_id, vec![doc.clone()]);
    // Corrupt the schema into an unknown shape (neither predecessor nor current).
    let unknown_schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "schema_version": { "const": 99 } }
    });
    fs::write(
        repo.join(".pulse/docs/schemas/document.schema.json"),
        to_canonical_bytes(&unknown_schema).unwrap(),
    )
    .unwrap();
    let before_registry = fs::read(repo.join(".pulse/docs/registry.json")).unwrap();

    let err = migrate_registry(repo, &repository_id).unwrap_err();
    assert_eq!(err.code(), "docs_registry_schema_invalid");

    // Canonical files preserved untouched.
    let after_registry = fs::read(repo.join(".pulse/docs/registry.json")).unwrap();
    assert_eq!(before_registry, after_registry);
    assert_eq!(
        schema_version(repo).unwrap_err().code(),
        "docs_registry_schema_invalid"
    );
}

#[test]
fn r3_retrieval_config_defaults_are_deterministic_and_normalized() {
    let a = RetrievalConfig::defaults();
    let b = RetrievalConfig::defaults();
    assert_eq!(a, b);
    let ha = hash_bytes(&to_canonical_bytes(&a).unwrap());
    let hb = hash_bytes(&to_canonical_bytes(&b).unwrap());
    assert_eq!(ha, hb, "identical retrieval config must hash identically");

    // Defaults are within contract ranges and therefore valid.
    let tmp = tempfile::tempdir().unwrap();
    let repo_root = tmp.path();
    fs::create_dir_all(repo_root.join("docs")).unwrap();
    fs::write(repo_root.join("docs/x.md"), b"# X\n").unwrap();
    let registry = DocsRegistry {
        schema_version: 2,
        revision: 1,
        repository_id: "repo_test".to_string(),
        documents: vec![domain_doc("DOC-EXAMPLE", "docs/x.md")],
        retrieval: Some(a),
    };
    let report = validate_registry(repo_root, "repo_test", &registry).unwrap();
    assert!(report.valid, "defaults must validate: {:?}", report.errors);

    // Out-of-range config is rejected.
    let mut bad = RetrievalConfig::defaults();
    bad.default_search_limit = 0;
    let bad_registry = DocsRegistry {
        schema_version: 2,
        revision: 1,
        repository_id: "repo_test".to_string(),
        documents: vec![],
        retrieval: Some(bad),
    };
    let report = validate_registry(repo_root, "repo_test", &bad_registry).unwrap();
    assert!(!report.valid);
    assert!(report
        .errors
        .iter()
        .any(|f| f.code == "docs_registry_retrieval_config_invalid"));
}

#[test]
fn r29a_retrieval_only_edit_bumps_registry_not_document_revision() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    evidence_manifest::bootstrap(repo).unwrap();
    docs_bootstrap(repo).unwrap();
    fs::create_dir_all(repo.join("docs/domain")).unwrap();
    fs::write(repo.join("docs/domain/auth.md"), b"# Auth\n").unwrap();
    let reg = docs_register(
        repo,
        1,
        domain_doc("DOC-AUTH-DOMAIN", "docs/domain/auth.md"),
        "human:test",
    )
    .unwrap();
    assert_eq!(reg.registry_revision, 2);
    let doc_revision_before = reg.value.revision;

    // Retrieval-only patch: only `retrieval` changes.
    let patch = DocumentPatch {
        retrieval: Some(Some(DocumentRetrieval {
            index: false,
            include_body: true,
            materialize_index: false,
        })),
        ..DocumentPatch::default()
    };
    let out = docs_edit(
        repo,
        "DOC-AUTH-DOMAIN",
        2,
        doc_revision_before,
        patch,
        "human:test",
    )
    .unwrap();
    assert!(out.retrieval_only);
    assert_eq!(out.code, "retrieval_updated");
    assert_eq!(out.registry_revision, 3); // registry revision bumped
    assert_eq!(
        out.value.revision, doc_revision_before,
        "retrieval-only edit must NOT bump the receipt-bound document revision"
    );
    assert_eq!(
        out.value.retrieval,
        Some(DocumentRetrieval {
            index: false,
            include_body: true,
            materialize_index: false,
        })
    );

    // is_retrieval_only_change helper.
    assert!(is_retrieval_only_change(&["retrieval".to_string()]));
    assert!(!is_retrieval_only_change(&[
        "retrieval".to_string(),
        "summary".to_string()
    ]));
    assert!(!is_retrieval_only_change(&[]));
}

#[test]
fn r29b_pre_migration_v1_receipt_verifies_identically_after_migration() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let repository_id = evidence_manifest::bootstrap(repo)
        .unwrap()
        .manifest
        .repository_id;
    fs::create_dir_all(repo.join("docs/domain")).unwrap();
    fs::write(repo.join("docs/domain/token.md"), b"# Token lifecycle\n").unwrap();
    let path = "docs/domain/token.md".to_string();
    let hash = hash_bytes(&fs::read(repo.join(&path)).unwrap());
    let doc = domain_doc("DOC-AUTH-DOMAIN", &path);
    write_v1_repo(repo, &repository_id, vec![doc]);
    let source_commit = commit_all(repo);

    // Record a v1 documentation receipt against the v1 registry.
    let receipt = ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 1,
        id: "rcpt_01J00000000000000000000200".to_string(),
        kind: ReceiptKind::DocumentationValidation,
        result: ReceiptResult::Passed,
        actor: ActorRef {
            kind: ActorKind::Human,
            id: "tester".to_string(),
        },
        recorded_at: Utc::now(),
        subject: SubjectRef {
            kind: "documentation".to_string(),
            id: "DOC-AUTH-DOMAIN".to_string(),
        },
        bindings: ReceiptBindings {
            work: vec![],
            source: Some(SourceBinding {
                kind: "git_commit".to_string(),
                commit: source_commit.clone(),
                repository_id: repository_id.clone(),
            }),
            content: vec![ContentBinding {
                path: path.clone(),
                sha256: hash.clone(),
            }],
            artifacts: vec![],
            graph_fingerprint_observed: None,
        },
        payload: ReceiptPayload::DocumentationValidation(DocumentationValidationPayload {
            payload_version: 1,
            documents: vec![DocumentationValidationDocument {
                proposed_document_id: Some("DOC-AUTH-DOMAIN".to_string()),
                document_id: None,
                document_revision: None,
                path: path.clone(),
                content_hash: hash.clone(),
                result: ReceiptResult::Passed,
            }],
            checks: vec![],
        }),
    };
    let file = repo.join("receipt.json");
    fs::write(&file, to_canonical_bytes(&receipt).unwrap()).unwrap();
    record_receipt(repo, None, &file).unwrap();

    // Verify before migration. Payload v1 remains historical-valid but not
    // registry/gate current after Slice 4 canonical document IDs landed.
    let before = verify_receipt(repo, &receipt.id, true, None).unwrap();
    assert_eq!(before.integrity.status, "valid");
    assert_eq!(before.bindings.status, "current");
    assert_eq!(before.registry.status, "legacy_unresolved");
    assert_eq!(
        before.registry.reason_codes,
        vec!["legacy_unresolved".to_string()]
    );
    assert_eq!(before.policy.status, "structurally_satisfied");
    assert!(!before.gate_eligible);

    // Verification lazily migrates v1 -> v2 (adds retrieval defaults; does NOT
    // bump document revision). Explicit retry is idempotent/current.
    let migration = migrate_registry(repo, &repository_id).unwrap();
    assert_eq!(
        migration.status,
        pulse::docs::MigrationStatus::AlreadyCurrent
    );
    assert_eq!(migration.registry.documents[0].revision, 1);

    // Verify after migration: identical outcome. Retrieval-only metadata ignored.
    let after = verify_receipt(repo, &receipt.id, true, None).unwrap();
    assert_eq!(after.registry.status, before.registry.status);
    assert_eq!(after.registry.reason_codes, before.registry.reason_codes);
    assert_eq!(after.policy.status, before.policy.status);
    assert_eq!(after.gate_eligible, before.gate_eligible);
    assert_eq!(after.integrity.status, "valid");
    assert_eq!(after.bindings.status, "current");
}
