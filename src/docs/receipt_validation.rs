//! Registry checks for documentation validation receipts.

use crate::canonical_json::hash_bytes;
use crate::docs::DocsRegistry;
use crate::evidence::model::{
    DocumentationValidationDocument, ReceiptEnvelope, ReceiptPayload, ValidationDimension,
};
use crate::{PulseError, Result};
use std::fs;
use std::path::Path;

pub(crate) fn documentation_validation_dimensions(
    repo_root: &Path,
    receipt: &ReceiptEnvelope,
    current: bool,
    integrity_valid: bool,
    bindings_current: bool,
    registry: Option<&DocsRegistry>,
) -> Result<(
    ValidationDimension,
    ValidationDimension,
    ValidationDimension,
    bool,
)> {
    let ReceiptPayload::DocumentationValidation(payload) = &receipt.payload else {
        return Ok((
            dimension("not_applicable", Vec::new()),
            dimension("not_applicable", Vec::new()),
            dimension(
                "not_evaluated",
                vec!["authority_resolver_unavailable".to_string()],
            ),
            false,
        ));
    };
    if payload.payload_version != 1 {
        return Ok((
            dimension("invalid", vec!["receipt_version_unsupported".to_string()]),
            dimension("not_evaluated", Vec::new()),
            dimension(
                "not_evaluated",
                vec!["authority_resolver_unavailable".to_string()],
            ),
            false,
        ));
    }
    let registry = registry.map_or_else(
        || load_docs_registry(repo_root),
        |value| Ok(snapshot_registry(value)),
    )?;
    let mut codes = Vec::new();
    for document in &payload.documents {
        validate_doc_against_registry(repo_root, &registry, document, &mut codes)?;
    }
    codes.sort();
    codes.dedup();
    let registry_dimension = dimension(
        if !current {
            "not_checked"
        } else if codes.is_empty() {
            "current"
        } else if codes.iter().any(|code| {
            matches!(
                code.as_str(),
                "document_retired" | "document_superseded" | "document_stale"
            )
        }) {
            "not_current"
        } else {
            "mismatch"
        },
        if current { codes } else { Vec::new() },
    );
    let policy_dimension = dimension("structurally_satisfied", Vec::new());
    let authorization_dimension = dimension(
        "not_evaluated",
        vec!["authority_resolver_unavailable".to_string()],
    );
    let gate_eligible =
        integrity_valid && bindings_current && registry_dimension.status == "current";
    Ok((
        registry_dimension,
        policy_dimension,
        authorization_dimension,
        gate_eligible,
    ))
}

fn dimension(status: &str, reason_codes: Vec<String>) -> ValidationDimension {
    ValidationDimension {
        status: status.to_string(),
        reason_codes,
    }
}

fn validate_doc_against_registry(
    repo_root: &Path,
    registry: &DocsRegistrySnapshot,
    doc: &DocumentationValidationDocument,
    codes: &mut Vec<String>,
) -> Result<()> {
    let Some(id) = doc.document_id.as_deref() else {
        codes.push("document_receipt_registry_mismatch".to_string());
        return Ok(());
    };
    let Some(record) = registry
        .documents
        .iter()
        .find(|candidate| candidate.id == id)
    else {
        codes.push("document_receipt_registry_mismatch".to_string());
        return Ok(());
    };
    if record.path != doc.path {
        codes.push("document_receipt_registry_mismatch".to_string());
    }
    if doc.document_revision != Some(record.revision) {
        codes.push("document_receipt_revision_stale".to_string());
    }
    validate_registry_record_state(repo_root, record, doc, codes)
}

fn validate_registry_record_state(
    repo_root: &Path,
    record: &DocsRegistryDocument,
    doc: &DocumentationValidationDocument,
    codes: &mut Vec<String>,
) -> Result<()> {
    match current_content_hash(repo_root, &record.path)? {
        Some(hash) if hash == doc.content_hash => {}
        _ => codes.push("document_receipt_registry_mismatch".to_string()),
    }
    match record.status.as_str() {
        "approved" => {}
        "retired" => codes.push("document_retired".to_string()),
        "stale" => codes.push("document_stale".to_string()),
        "draft" => codes.push("document_not_authoritative".to_string()),
        _ => codes.push("document_receipt_registry_mismatch".to_string()),
    }
    if record.superseded_by.is_some() {
        codes.push("document_superseded".to_string());
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
    status: String,
    superseded_by: Option<String>,
}

fn load_docs_registry(repo_root: &Path) -> Result<DocsRegistrySnapshot> {
    Ok(snapshot_registry(&crate::docs::load_registry_or_empty(
        repo_root,
    )?))
}
fn snapshot_registry(registry: &DocsRegistry) -> DocsRegistrySnapshot {
    DocsRegistrySnapshot {
        documents: registry
            .documents
            .iter()
            .map(|document| DocsRegistryDocument {
                id: document.id.clone(),
                revision: document.revision,
                path: document.path.clone(),
                status: serde_json::to_string(&document.status)
                    .unwrap_or_default()
                    .trim_matches('"')
                    .to_string(),
                superseded_by: document.superseded_by.clone(),
            })
            .collect(),
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

#[allow(dead_code)]
fn require_passed_checks(
    _: &crate::evidence::model::DocumentationValidationPayload,
    _: &[&str],
    _: &mut Vec<String>,
) {
}
