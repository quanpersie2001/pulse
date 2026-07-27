//! Evidence-side documentation receipt payload validation.
//!
//! This module owns the *structural* envelope proof for the documentation
//! validation receipt kind: payload version, required source/content bindings,
//! document identity shape and the shared document-entry content binding check.
//! It deliberately does not interpret the docs registry lifecycle or review
//! policy: that cross-domain policy interpretation lives in
//! [`crate::docs::receipt_validation`], which evidence's `verify_receipt`
//! assembles into the registry/policy/authorization dimensions.

use crate::evidence::model::{
    DocumentationValidationDocument, DocumentationValidationPayload, ReceiptEnvelope,
};
use crate::evidence::receipt::helpers::validate_document_entry_common;
use crate::{PulseError, Result};

pub(super) fn validate_docs_payload(
    receipt: &ReceiptEnvelope,
    p: &DocumentationValidationPayload,
) -> Result<()> {
    if p.payload_version != 1 {
        return Err(PulseError::validation(
            "receipt_version_unsupported",
            "payload",
        ));
    }
    if receipt.bindings.source.is_none() || receipt.bindings.content.is_empty() {
        return Err(PulseError::validation(
            "source_binding_missing",
            "documentation requires source/content binding",
        ));
    }
    for doc in &p.documents {
        validate_registry_document_identity(doc)?;
        validate_document_entry_common(receipt, &doc.path, &doc.content_hash)?;
    }
    for check in &p.checks {
        if check.kind.trim().is_empty() {
            return Err(PulseError::validation(
                "receipt_schema_invalid",
                "check kind is required",
            ));
        }
        if let Some(d) = &check.artifact {
            if !receipt.bindings.artifacts.iter().any(|a| &a.sha256 == d) {
                return Err(PulseError::validation("artifact_not_found", d.clone()));
            }
        }
    }
    Ok(())
}

fn validate_registry_document_identity(doc: &DocumentationValidationDocument) -> Result<()> {
    let document_id = doc.document_id.as_deref().ok_or_else(|| {
        PulseError::validation(
            "document_receipt_registry_mismatch",
            "document_id is required for documentation payload",
        )
    })?;
    validate_document_id(document_id)?;
    let Some(document_revision) = doc.document_revision else {
        return Err(PulseError::validation(
            "document_receipt_registry_mismatch",
            "document_revision is required for documentation payload",
        ));
    };
    if document_revision == 0 {
        return Err(PulseError::validation(
            "document_receipt_registry_mismatch",
            "document revision must be positive",
        ));
    }
    Ok(())
}

fn validate_document_id(id: &str) -> Result<()> {
    let Some(rest) = id.strip_prefix("DOC-") else {
        return Err(PulseError::validation(
            "document_receipt_registry_mismatch",
            "document id must start with DOC-",
        ));
    };
    if (3..=64).contains(&rest.len())
        && rest
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
    {
        Ok(())
    } else {
        Err(PulseError::validation(
            "document_receipt_registry_mismatch",
            "invalid document id",
        ))
    }
}
