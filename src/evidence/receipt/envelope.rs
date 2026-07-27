//! Generic receipt envelope integrity, manifest-kind validation, binding
//! normalization and kind dispatch.
//!
//! This module owns the immutable envelope proof shared by every receipt kind:
//! schema/receipt version checks, id shape, manifest kind/version registration,
//! structural binding validation (duplicates, artifact existence and
//! record-time currentness), normalization, and dispatch to the kind-specific
//! payload validators in [`super::supersession`], [`super::shaping`],
//! [`super::decision`] and [`super::documentation`]. Documentation registry
//! lifecycle/review-policy interpretation is intentionally not handled here; it
//! lives in [`crate::docs::receipt_validation`] and is assembled by
//! `verify_receipt` in [`super::store`].

use super::bindings::{binding_codes_for, code_to_static};
use super::decision::validate_decision_acceptance_payload;
use super::documentation::validate_docs_payload;
use super::shaping::validate_shaping_payload;
use super::supersession::validate_supersession_payload;
use crate::evidence::artifact::artifact_exists;
use crate::evidence::manifest::{self, EvidenceManifest};
use crate::evidence::model::{ReceiptBindings, ReceiptEnvelope, ReceiptPayload};
use crate::{PulseError, Result};
use std::collections::BTreeSet;
use std::path::Path;

pub(super) fn validate_envelope(
    repo_root: &Path,
    receipt: &ReceiptEnvelope,
    record_time: bool,
) -> Result<()> {
    if receipt.schema_version != 1 || receipt.receipt_version != 1 {
        return Err(PulseError::validation(
            "receipt_version_unsupported",
            "unsupported receipt version",
        ));
    }
    validate_receipt_id(&receipt.id)?;
    validate_manifest_kind(&manifest::load(repo_root)?, receipt)?;
    if receipt.actor.id.trim().is_empty() || receipt.subject.id.trim().is_empty() {
        return Err(PulseError::validation(
            "receipt_schema_invalid",
            "actor and subject are required",
        ));
    }
    if receipt.kind.as_str() != payload_kind(&receipt.payload) {
        return Err(PulseError::validation(
            "receipt_kind_unsupported",
            "kind/payload mismatch",
        ));
    }
    validate_bindings(repo_root, &receipt.bindings, record_time)?;
    match &receipt.payload {
        ReceiptPayload::SupersessionReconciliation(p) => validate_supersession_payload(receipt, p),
        ReceiptPayload::ShapingValidation(p) => validate_shaping_payload(receipt, p),
        ReceiptPayload::DecisionAcceptance(p) => validate_decision_acceptance_payload(receipt, p),
        ReceiptPayload::DocumentationValidation(p) => validate_docs_payload(receipt, p),
    }
}

pub(super) fn validate_receipt_id(id: &str) -> Result<()> {
    let suffix = id.strip_prefix("rcpt_");
    if suffix.is_some_and(|suffix| {
        suffix.len() == 26
            && suffix
                .chars()
                .all(|c| matches!(c, '0'..='9' | 'A'..='H' | 'J'..='K' | 'M'..='N' | 'P'..='T' | 'V'..='Z'))
    }) {
        Ok(())
    } else {
        Err(PulseError::validation(
            "receipt_schema_invalid",
            "invalid receipt id",
        ))
    }
}

pub(super) fn normalize_bindings(receipt: &mut ReceiptEnvelope) {
    receipt.bindings.work.sort_by(|a, b| a.id.cmp(&b.id));
    receipt.bindings.content.sort_by(|a, b| a.path.cmp(&b.path));
    receipt
        .bindings
        .artifacts
        .sort_by(|a, b| a.sha256.cmp(&b.sha256));
}

pub(super) fn validate_manifest_kind(
    manifest: &EvidenceManifest,
    receipt: &ReceiptEnvelope,
) -> Result<()> {
    if !manifest
        .receipt_schemas
        .contains_key(&receipt.receipt_version.to_string())
    {
        return Err(PulseError::validation(
            "receipt_version_unsupported",
            "unknown receipt envelope version",
        ));
    }
    let versions = manifest
        .receipt_kinds
        .get(receipt.kind.as_str())
        .ok_or_else(|| {
            PulseError::validation("receipt_kind_unsupported", "unknown receipt kind")
        })?;
    let payload_version = match &receipt.payload {
        ReceiptPayload::SupersessionReconciliation(payload) => payload.payload_version,
        ReceiptPayload::ShapingValidation(payload) => payload.payload_version,
        ReceiptPayload::DecisionAcceptance(payload) => payload.payload_version,
        ReceiptPayload::DocumentationValidation(payload) => payload.payload_version,
    };
    if !versions.contains_key(&payload_version.to_string()) {
        return Err(PulseError::validation(
            "receipt_version_unsupported",
            "unknown receipt payload version",
        ));
    }
    Ok(())
}

fn validate_bindings(
    repo_root: &Path,
    bindings: &ReceiptBindings,
    record_time: bool,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    for work in &bindings.work {
        if !seen.insert(&work.id) {
            return Err(PulseError::validation(
                "work_binding_missing",
                "duplicate work binding",
            ));
        }
    }
    for artifact in &bindings.artifacts {
        if !artifact_exists(repo_root, &artifact.sha256) {
            return Err(PulseError::validation(
                "artifact_not_found",
                artifact.sha256.clone(),
            ));
        }
    }
    if record_time {
        let codes = binding_codes_for(repo_root, bindings, None)?;
        if let Some(code) = codes.into_iter().next() {
            return Err(PulseError::validation(
                code_to_static(&code),
                "receipt bindings are not current",
            ));
        }
    }
    Ok(())
}

fn payload_kind(payload: &ReceiptPayload) -> &'static str {
    match payload {
        ReceiptPayload::SupersessionReconciliation(_) => "supersession_reconciliation",
        ReceiptPayload::ShapingValidation(_) => "shaping_validation",
        ReceiptPayload::DecisionAcceptance(_) => "decision_acceptance",
        ReceiptPayload::DocumentationValidation(_) => "documentation_validation",
    }
}
