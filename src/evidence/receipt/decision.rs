//! Decision acceptance receipt payload validation.
//!
//! Validates the immutable envelope proof for decision acceptance receipts:
//! payload version, subject identity (the accepted Decision work item),
//! decision work/content binding, accepted outcome, approver actor and the
//! source posture requirement for clean-git-commit decisions.

use crate::evidence::model::{
    DecisionAcceptancePayload, ReceiptEnvelope, SourcePosture, WorkRevisionRef,
};
use crate::evidence::receipt::bindings::require_work_binding;
use crate::evidence::receipt::helpers::{
    validate_actor, validate_content_hash, validate_document_entry_common, validate_non_empty,
    validate_work_id_kind,
};
use crate::PulseError;
use crate::Result;

pub(super) fn validate_decision_acceptance_payload(
    receipt: &ReceiptEnvelope,
    p: &DecisionAcceptancePayload,
) -> Result<()> {
    if p.payload_version != 1 {
        return Err(PulseError::validation(
            "receipt_version_unsupported",
            "payload",
        ));
    }
    if receipt.subject.id != p.decision.id || receipt.subject.kind != "work" {
        return Err(PulseError::validation(
            "decision_acceptance_stale",
            "decision acceptance subject must be the accepted Decision work item",
        ));
    }
    validate_work_id_kind(&p.decision.id, "DEC-", "decision_acceptance_stale")?;
    require_work_binding(
        &receipt.bindings,
        &WorkRevisionRef {
            id: p.decision.id.clone(),
            revision: p.decision.revision_observed,
        },
    )?;
    validate_content_hash(&p.decision.content.content_hash, "receipt_schema_invalid")?;
    validate_document_entry_common(
        receipt,
        &p.decision.content.path,
        &p.decision.content.content_hash,
    )?;
    validate_non_empty(
        &p.accepted_outcome,
        "receipt_schema_invalid",
        "accepted outcome required",
    )?;
    validate_actor(&p.approver)?;
    if p.source_posture == SourcePosture::CleanGitCommit && receipt.bindings.source.is_none() {
        return Err(PulseError::validation(
            "source_binding_missing",
            "decision acceptance requires source binding for clean_git_commit",
        ));
    }
    Ok(())
}
