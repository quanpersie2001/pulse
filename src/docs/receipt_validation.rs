//! Documentation receipt validation dimensions: registry lifecycle and review
//! policy interpretation.
//!
//! This module owns the cross-domain docs-policy interpretation for
//! `verify_receipt`: it loads the docs registry, matches each documentation
//! receipt document against the current registry record (identity, revision,
//! path, lifecycle), classifies the registry dimension, evaluates the configured
//! review policy against the receipt's checks, and keeps authorization
//! deliberately mechanical/unresolved. Evidence assembles the resulting
//! registry/policy/authorization dimensions and the gate-eligible flag into the
//! final [`ValidationReport`]; this module never promotes an actor or review
//! policy into an authorized gate pass.
//!
//! Structural documentation envelope validation (payload version, required
//! source/content bindings, document identity shape) stays in evidence at
//! [`crate::evidence::receipt::documentation`]; only registry lifecycle and
//! review-policy interpretation lives here.

use crate::canonical_json::hash_bytes;
use crate::docs::{load_registry_or_empty, DocumentLifecycle, ReviewPolicy};
use crate::evidence::model::{
    DocumentationValidationDocument, DocumentationValidationPayload, ReceiptEnvelope,
    ReceiptPayload, ReceiptResult, ValidationDimension,
};
use crate::{PulseError, Result};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// Compute the registry, policy and authorization dimensions (plus the
/// gate-eligible flag) for a documentation receipt.
///
/// For non-documentation receipts the dimensions are `not_applicable` /
/// `not_evaluated`. `current` controls whether registry codes are populated;
/// `integrity_valid` and `bindings_current` feed the gate-eligible computation.
pub(crate) fn documentation_validation_dimensions(
    repo_root: &Path,
    receipt: &ReceiptEnvelope,
    current: bool,
    integrity_valid: bool,
    bindings_current: bool,
) -> Result<(
    ValidationDimension,
    ValidationDimension,
    ValidationDimension,
    bool,
)> {
    let ReceiptPayload::DocumentationValidation(payload) = &receipt.payload else {
        return Ok((
            ValidationDimension {
                status: "not_applicable".to_string(),
                reason_codes: Vec::new(),
            },
            ValidationDimension {
                status: "not_applicable".to_string(),
                reason_codes: Vec::new(),
            },
            ValidationDimension {
                status: "not_evaluated".to_string(),
                reason_codes: vec!["authority_resolver_unavailable".to_string()],
            },
            false,
        ));
    };

    if payload.payload_version != 1 {
        return Ok((
            ValidationDimension {
                status: "invalid".to_string(),
                reason_codes: vec!["receipt_version_unsupported".to_string()],
            },
            ValidationDimension {
                status: "not_evaluated".to_string(),
                reason_codes: Vec::new(),
            },
            ValidationDimension {
                status: "not_evaluated".to_string(),
                reason_codes: vec!["authority_resolver_unavailable".to_string()],
            },
            false,
        ));
    }

    let registry = load_docs_registry(repo_root)?;
    let mut registry_codes = Vec::new();
    let mut policies = BTreeSet::new();
    for doc in &payload.documents {
        validate_doc_against_registry(
            repo_root,
            &registry,
            doc,
            &mut registry_codes,
            &mut policies,
        )?;
    }
    registry_codes.sort();
    registry_codes.dedup();

    let mut policy_codes = Vec::new();
    // Authorization is deliberately mechanical/unresolved in the evidence
    // foundation. Registry/policy checks below can prove only structure; they
    // must never promote an actor or review policy into an authorized gate pass.
    let mut authorization_codes = vec!["authority_resolver_unavailable".to_string()];
    let mut authorization_status = "not_evaluated";
    for policy in &policies {
        match policy.as_str() {
            "none" => {}
            "light" => require_passed_checks(payload, &["content_review"], &mut policy_codes),
            "standard" => require_passed_checks(
                payload,
                &["link_check", "semantic_review"],
                &mut policy_codes,
            ),
            "independent" => {
                require_passed_checks(
                    payload,
                    &["link_check", "semantic_review"],
                    &mut policy_codes,
                );
                authorization_status = "unresolved";
                authorization_codes.push("independent_authorization_unresolved".to_string());
            }
            "human" => {
                policy_codes.push("human_approval_unresolved".to_string());
                authorization_status = "unresolved";
                authorization_codes.push("human_approval_unresolved".to_string());
            }
            _ => policy_codes.push("document_receipt_policy_incomplete".to_string()),
        }
    }
    policy_codes.sort();
    policy_codes.dedup();
    authorization_codes.sort();
    authorization_codes.dedup();

    let registry_dimension = ValidationDimension {
        status: if !current {
            "not_checked"
        } else if registry_codes.is_empty() {
            "current"
        } else if registry_codes.iter().any(|code| {
            matches!(
                code.as_str(),
                "document_retired" | "document_superseded" | "document_stale"
            )
        }) {
            "not_current"
        } else {
            "mismatch"
        }
        .to_string(),
        reason_codes: if current { registry_codes } else { Vec::new() },
    };
    let policy_dimension = ValidationDimension {
        status: if policy_codes.is_empty() {
            "structurally_satisfied"
        } else {
            "incomplete"
        }
        .to_string(),
        reason_codes: policy_codes,
    };
    let authorization_dimension = ValidationDimension {
        status: authorization_status.to_string(),
        reason_codes: authorization_codes,
    };
    let gate_eligible = integrity_valid
        && bindings_current
        && registry_dimension.status == "current"
        && policy_dimension.status == "structurally_satisfied"
        && authorization_dimension.status == "not_evaluated"
        && policies.iter().all(|policy| policy == "none");

    Ok((
        registry_dimension,
        policy_dimension,
        authorization_dimension,
        gate_eligible,
    ))
}

fn validate_doc_against_registry(
    repo_root: &Path,
    registry: &DocsRegistrySnapshot,
    doc: &DocumentationValidationDocument,
    registry_codes: &mut Vec<String>,
    policies: &mut BTreeSet<String>,
) -> Result<()> {
    let Some(document_id) = doc.document_id.as_deref() else {
        registry_codes.push("document_receipt_registry_mismatch".to_string());
        return Ok(());
    };
    let Some(record) = registry
        .documents
        .iter()
        .find(|candidate| candidate.id == document_id)
    else {
        registry_codes.push("document_receipt_registry_mismatch".to_string());
        if registry
            .documents
            .iter()
            .any(|candidate| candidate.path == doc.path)
        {
            registry_codes.push("document_receipt_wrong_id_for_path".to_string());
        }
        return Ok(());
    };
    policies.insert(record.review_policy.clone());
    if record.path != doc.path {
        registry_codes.push("document_receipt_registry_mismatch".to_string());
    }
    if doc.document_revision != Some(record.revision) {
        registry_codes.push("document_receipt_revision_stale".to_string());
    }
    validate_registry_record_state(repo_root, record, doc, registry_codes)
}

fn validate_registry_record_state(
    repo_root: &Path,
    record: &DocsRegistryDocument,
    doc: &DocumentationValidationDocument,
    registry_codes: &mut Vec<String>,
) -> Result<()> {
    match current_content_hash(repo_root, &record.path)? {
        Some(current_hash) if current_hash == doc.content_hash => {}
        Some(_) => registry_codes.push("document_receipt_registry_mismatch".to_string()),
        None => registry_codes.push("document_receipt_registry_mismatch".to_string()),
    }
    match record.lifecycle.as_str() {
        "current" => {}
        "retired" => registry_codes.push("document_retired".to_string()),
        "superseded" => registry_codes.push("document_superseded".to_string()),
        "stale" | "suspected_stale" => registry_codes.push("document_stale".to_string()),
        _ => registry_codes.push("document_receipt_registry_mismatch".to_string()),
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct DocsRegistrySnapshot {
    documents: Vec<DocsRegistryDocument>,
}

#[derive(Debug, Clone)]
struct DocsRegistryDocument {
    id: String,
    revision: u64,
    path: String,
    lifecycle: String,
    review_policy: String,
}

fn load_docs_registry(repo_root: &Path) -> Result<DocsRegistrySnapshot> {
    let registry = load_registry_or_empty(repo_root)?;
    Ok(DocsRegistrySnapshot {
        documents: registry
            .documents
            .into_iter()
            .map(|document| DocsRegistryDocument {
                id: document.id,
                revision: document.revision,
                path: document.path,
                lifecycle: lifecycle_name(document.lifecycle).to_string(),
                review_policy: review_policy_name(document.review_policy).to_string(),
            })
            .collect(),
    })
}

fn lifecycle_name(lifecycle: DocumentLifecycle) -> &'static str {
    match lifecycle {
        DocumentLifecycle::Current => "current",
        DocumentLifecycle::SuspectedStale => "suspected_stale",
        DocumentLifecycle::Stale => "stale",
        DocumentLifecycle::Retired => "retired",
        DocumentLifecycle::Superseded => "superseded",
    }
}

fn review_policy_name(policy: ReviewPolicy) -> &'static str {
    match policy {
        ReviewPolicy::None => "none",
        ReviewPolicy::Light => "light",
        ReviewPolicy::Standard => "standard",
        ReviewPolicy::Independent => "independent",
        ReviewPolicy::Human => "human",
    }
}

fn current_content_hash(repo_root: &Path, path: &str) -> Result<Option<String>> {
    let rel = crate::storage::safe_repo_relative(path)?;
    let path = repo_root.join(rel);
    match fs::read(&path) {
        Ok(bytes) => Ok(Some(hash_bytes(&bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(PulseError::io(path, error)),
    }
}

fn require_passed_checks(
    payload: &DocumentationValidationPayload,
    required: &[&str],
    policy_codes: &mut Vec<String>,
) {
    for required_kind in required {
        if !payload
            .checks
            .iter()
            .any(|check| check.kind == *required_kind && check.result == ReceiptResult::Passed)
        {
            policy_codes.push("document_receipt_policy_incomplete".to_string());
        }
    }
}
