//! Supersession reconciliation receipt validation.
//!
//! [`validate_for_supersession`] is the public entrypoint used by graph
//! lifecycle mutation to prove a supersession is backed by a current, valid,
//! passed reconciliation receipt bound to the exact old/target work revisions.
//! [`validate_supersession_payload`] is the kind-specific payload validator
//! invoked by the generic envelope dispatcher during record-time validation.

use super::bindings::require_work_binding;
use super::envelope::validate_envelope;
use super::store::{load_receipt, verify_receipt};
use crate::evidence::model::{
    ReceiptEnvelope, ReceiptKind, ReceiptPayload, ReceiptReference, ReceiptResult,
    SupersessionReceiptClaim, SupersessionReceiptTarget, SupersessionReconciliationPayload,
    WorkRevisionRef,
};
use crate::{PulseError, Result};
use std::path::Path;

pub fn validate_for_supersession(
    repo_root: &Path,
    id: &str,
    old_id: &str,
    old_rev: u64,
    target_id: &str,
    target_rev: u64,
) -> Result<ReceiptReference> {
    let (receipt, hash) = load_receipt(repo_root, id)?;
    validate_envelope(repo_root, &receipt, false)?;
    if receipt.kind != ReceiptKind::SupersessionReconciliation
        || receipt.result != ReceiptResult::Passed
    {
        return Err(PulseError::validation(
            "receipt_result_ineligible",
            "supersession requires passed reconciliation receipt",
        ));
    }
    let ReceiptPayload::SupersessionReconciliation(payload) = &receipt.payload else {
        return Err(PulseError::validation(
            "receipt_kind_unsupported",
            "payload mismatch",
        ));
    };
    if payload.old.id != old_id || payload.old.revision != old_rev {
        return Err(PulseError::validation(
            "supersession_receipt_mismatch",
            "old work mismatch",
        ));
    }
    let (rid, rrev) = match &payload.target {
        SupersessionReceiptTarget::Replacement { id, revision }
        | SupersessionReceiptTarget::DecisionExplanation { id, revision } => (id, *revision),
    };
    if rid != target_id || rrev != target_rev {
        return Err(PulseError::validation(
            "supersession_receipt_mismatch",
            "target mismatch",
        ));
    }
    let report = verify_receipt(repo_root, id, true, None)?;
    if report.integrity.status != "valid" || report.bindings.status != "current" {
        return Err(PulseError::validation(
            "supersession_receipt_mismatch",
            "receipt is not current and valid",
        ));
    }
    Ok(ReceiptReference {
        id: id.to_string(),
        hash,
    })
}

pub(super) fn validate_supersession_payload(
    receipt: &ReceiptEnvelope,
    p: &SupersessionReconciliationPayload,
) -> Result<()> {
    if p.payload_version != 1 || receipt.result != ReceiptResult::Passed {
        return Err(PulseError::validation(
            "receipt_result_ineligible",
            "supersession receipt must be passed v1",
        ));
    }
    if receipt.bindings.source.is_none() {
        return Err(PulseError::validation(
            "source_binding_missing",
            "source binding required",
        ));
    }
    if receipt.bindings.content.is_empty() {
        return Err(PulseError::validation(
            "content_binding_missing",
            "content binding required",
        ));
    }
    require_work_binding(&receipt.bindings, &p.old)?;
    let target_ref = match &p.target {
        SupersessionReceiptTarget::Replacement { id, revision }
        | SupersessionReceiptTarget::DecisionExplanation { id, revision } => WorkRevisionRef {
            id: id.clone(),
            revision: *revision,
        },
    };
    require_work_binding(&receipt.bindings, &target_ref)?;
    if p.claim == SupersessionReceiptClaim::FollowUpRequired && p.follow_up_work.is_empty() {
        return Err(PulseError::validation(
            "work_binding_missing",
            "follow_up_required needs follow up work",
        ));
    }
    for work in &p.follow_up_work {
        require_work_binding(&receipt.bindings, work)?;
    }
    Ok(())
}
