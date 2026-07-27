//! Shared generic validation primitives reused across receipt payload
//! validators (shaping, decision, documentation).
//!
//! These helpers are intentionally envelope/payload-agnostic: they validate
//! primitive invariants (non-empty strings, identifiers, content hashes,
//! repo-relative paths and the shared document-entry content binding check) that
//! the kind-specific payload validators compose. Keeping them here avoids
//! duplicating these small structural rules across the payload modules and keeps
//! each payload module focused on its own kind semantics.

use crate::evidence::model::{ActorRef, ReceiptEnvelope};
use crate::{PulseError, Result};

pub(super) fn validate_non_empty(
    value: &str,
    code: &'static str,
    message: &'static str,
) -> Result<()> {
    if value.trim().is_empty() {
        Err(PulseError::validation(code, message))
    } else {
        Ok(())
    }
}

pub(super) fn validate_unique_by<'a>(
    code: &'static str,
    values: impl Iterator<Item = &'a str>,
) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    for value in values {
        validate_non_empty(value, code, "identifier required")?;
        if !seen.insert(value) {
            return Err(PulseError::validation(code, "duplicate identifier"));
        }
    }
    Ok(())
}

pub(super) fn validate_id_prefix(id: &str, prefix: &str, code: &'static str) -> Result<()> {
    validate_non_empty(id, code, "identifier required")?;
    if !id.starts_with(prefix)
        || !id
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(PulseError::validation(code, "invalid identifier"));
    }
    Ok(())
}

pub(super) fn validate_work_id_kind(id: &str, prefix: &str, code: &'static str) -> Result<()> {
    if !prefix.is_empty() && !id.starts_with(prefix) {
        return Err(PulseError::validation(code, "unexpected work id kind"));
    }
    crate::id::validate_work_id(id).map_err(|_| PulseError::validation(code, "invalid work id"))
}

pub(super) fn validate_content_hash(hash: &str, code: &'static str) -> Result<()> {
    if hash.starts_with("sha256:")
        && hash.len() == 71
        && hash[7..].chars().all(|c| c.is_ascii_hexdigit())
    {
        Ok(())
    } else {
        Err(PulseError::validation(
            code,
            "content hash must be sha256:<hex>",
        ))
    }
}

pub(super) fn validate_actor(actor: &ActorRef) -> Result<()> {
    validate_non_empty(&actor.id, "receipt_schema_invalid", "actor id required")
}

pub(super) fn validate_document_entry_common(
    receipt: &ReceiptEnvelope,
    path: &str,
    content_hash: &str,
) -> Result<()> {
    crate::storage::safe_repo_relative(path)?;
    if !content_hash.starts_with("sha256:") || content_hash.len() != 71 {
        return Err(PulseError::validation(
            "receipt_schema_invalid",
            "content hash must be sha256:<hex>",
        ));
    }
    if !receipt
        .bindings
        .content
        .iter()
        .any(|c| c.path == path && c.sha256 == content_hash)
    {
        return Err(PulseError::validation(
            "content_binding_missing",
            path.to_string(),
        ));
    }
    Ok(())
}
