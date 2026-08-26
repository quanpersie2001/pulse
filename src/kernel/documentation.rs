//! Documentation validation and immutable receipt composition.
//!
//! Docs owns mechanical validation and profile policy; Evidence owns receipt
//! integrity and persistence. This kernel module snapshots document identity
//! before effectful checks, composes both domains, and relies on Evidence's
//! record-time source/content revalidation to reject concurrent drift.

use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::docs::{
    validate_registry, validate_repository, DocsCheckKind, DocsCheckResult, DocsRegistry,
    DocsRepositoryValidationReport,
};
use crate::evidence::model::{
    DocumentCheck, DocumentationValidationDocument, DocumentationValidationPayload,
    ReceiptBindings, ReceiptEnvelope, ReceiptKind, ReceiptPayload, ReceiptResult, SourceBinding,
    SubjectRef, ValidationReport,
};
use crate::evidence::{new_receipt_id, ReceiptOutcome};
use crate::storage::transaction::TransactionFailpoint;
use crate::{PulseError, PulseResult};

mod snapshot;

use snapshot::{snapshot_validation_inputs, ValidationSnapshot};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocumentationValidationRun {
    pub schema_version: u32,
    pub code: String,
    pub validation: DocsRepositoryValidationReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<ReceiptOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<ValidationReport>,
}

/// Run repository documentation validation and optionally record an immutable
/// documentation validation receipt.
///
/// When `actor` is present, current document bytes are hashed before any
/// declared freshness command runs. Evidence revalidates those hashes and the
/// source binding at record time, so concurrent edits or a mutating check fail
/// closed instead of receiving proof for unchecked bytes.
///
/// # Errors
///
/// Returns typed errors for unsafe/missing receipt content, registry drift,
/// invalid actor identity, source drift, or Evidence persistence failure.
/// Mechanical validation failures are returned in `validation` without a
/// receipt, allowing the CLI to render the complete finding report.
pub fn run_documentation_validation(
    repo_root: &Path,
    failpoint: Option<TransactionFailpoint>,
    actor: Option<&str>,
) -> PulseResult<DocumentationValidationRun> {
    let registry = crate::docs::registry::load_registry_unvalidated(repo_root)?;
    let structural = validate_registry(repo_root, &registry.repository_id, &registry)?;
    if !structural.valid {
        return validation_only(validate_repository(repo_root, &registry)?);
    }

    let snapshot = actor
        .map(|_| snapshot_validation_inputs(repo_root, &registry))
        .transpose()?;
    let validation = validate_repository(repo_root, &registry)?;
    if !validation.valid || actor.is_none() {
        return validation_only(validation);
    }

    let actor = actor.expect("recording branch requires actor");
    if actor.trim().is_empty() {
        return Err(PulseError::validation(
            "docs_validation_actor_required",
            "--record requires a non-empty --actor",
        ));
    }
    let current_registry = crate::docs::registry::load_registry_unvalidated(repo_root)?;
    if current_registry != registry {
        return Err(PulseError::validation(
            "docs_validation_snapshot_changed",
            "docs registry changed while validation was running",
        ));
    }
    let snapshot = snapshot.expect("recording branch captures document snapshot");
    let current_snapshot = snapshot_validation_inputs(repo_root, &registry)?;
    if current_snapshot != snapshot {
        return Err(PulseError::validation(
            "docs_validation_snapshot_changed",
            "documentation inputs or generated outputs changed while validation was running",
        ));
    }
    if snapshot.documents.is_empty() {
        return Err(PulseError::validation(
            "docs_validation_receipt_empty",
            "documentation validation receipt requires at least one current document",
        ));
    }

    let receipt = build_receipt(repo_root, &registry, &validation, snapshot, actor)?;
    let receipt = crate::evidence::record_receipt_envelope(repo_root, failpoint, receipt)?;
    let verification = crate::evidence::verify_receipt(repo_root, &receipt.receipt.id, true, None)?;
    Ok(DocumentationValidationRun {
        schema_version: 1,
        code: "documentation_validation_recorded".to_string(),
        validation,
        receipt: Some(receipt),
        verification: Some(verification),
    })
}

fn validation_only(
    validation: DocsRepositoryValidationReport,
) -> PulseResult<DocumentationValidationRun> {
    Ok(DocumentationValidationRun {
        schema_version: 1,
        code: validation.code.clone(),
        validation,
        receipt: None,
        verification: None,
    })
}

fn build_receipt(
    repo_root: &Path,
    registry: &DocsRegistry,
    validation: &DocsRepositoryValidationReport,
    snapshot: ValidationSnapshot,
    actor: &str,
) -> PulseResult<ReceiptEnvelope> {
    let documents = snapshot
        .documents
        .into_iter()
        .map(|document| DocumentationValidationDocument {
            document_id: Some(document.document_id),
            document_revision: Some(document.document_revision),
            verification_profile: Some(document.verification_profile),
            path: document.path,
            content_hash: document.content_hash,
            result: ReceiptResult::Passed,
        })
        .collect();
    let checks = validation
        .checks
        .iter()
        .map(|check| DocumentCheck {
            kind: receipt_check_kind(check.kind).to_string(),
            result: match check.result {
                DocsCheckResult::Passed => ReceiptResult::Passed,
                DocsCheckResult::Failed => ReceiptResult::Failed,
                DocsCheckResult::Skipped => ReceiptResult::Inconclusive,
            },
            artifact: None,
        })
        .collect();
    Ok(ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 1,
        id: new_receipt_id(),
        kind: ReceiptKind::DocumentationValidation,
        result: ReceiptResult::Passed,
        actor: crate::policy::parse_actor(actor),
        recorded_at: Utc::now(),
        subject: SubjectRef {
            kind: "documentation_registry".to_string(),
            id: registry.repository_id.clone(),
        },
        bindings: ReceiptBindings {
            work: Vec::new(),
            source: Some(SourceBinding {
                kind: "git_commit".to_string(),
                commit: crate::source::head_commit(repo_root)?,
                repository_id: registry.repository_id.clone(),
            }),
            content: snapshot.content,
            artifacts: Vec::new(),
            graph_fingerprint_observed: None,
        },
        payload: ReceiptPayload::DocumentationValidation(DocumentationValidationPayload {
            payload_version: 1,
            documents,
            checks,
        }),
    })
}

const fn receipt_check_kind(kind: DocsCheckKind) -> &'static str {
    match kind {
        DocsCheckKind::Registry => "registry_check",
        DocsCheckKind::InternalLinks => "link_check",
        DocsCheckKind::GeneratedFreshness => "generated_freshness_check",
        DocsCheckKind::NavigationProjections => "navigation_projection_check",
    }
}
