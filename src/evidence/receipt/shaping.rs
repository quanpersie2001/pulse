//! Shaping validation receipt payload validation.
//!
//! Validates the immutable envelope proof for shaping validation receipts:
//! payload version, subject/owning-work binding, affected-work bindings, source
//! posture, destination/map snapshot (for persisted-map mode), branch/fog/
//! out-of-scope/resolution/reconciliation structures and remaining uncertainty.
//! These rules encode shaping semantics only; they reuse the generic binding and
//! shared validation primitives from [`super::bindings`] and [`super::helpers`].

use super::envelope::validate_receipt_id;
use crate::evidence::model::{
    BranchDisposition, ReceiptEnvelope, ShapeMode, ShapingBranch, ShapingDestination, ShapingFog,
    ShapingMapSnapshot, ShapingOutOfScope, ShapingReconciliation, ShapingResolutionPointer,
    ShapingValidationPayload, SourcePosture,
};
use crate::evidence::receipt::bindings::require_work_binding;
use crate::evidence::receipt::helpers::{
    validate_actor, validate_content_hash, validate_document_entry_common, validate_id_prefix,
    validate_non_empty, validate_unique_by, validate_work_id_kind,
};
use crate::{PulseError, Result};

pub(super) fn validate_shaping_payload(
    receipt: &ReceiptEnvelope,
    p: &ShapingValidationPayload,
) -> Result<()> {
    if p.payload_version != 1 {
        return Err(PulseError::validation(
            "receipt_version_unsupported",
            "payload",
        ));
    }
    if receipt.subject.id != p.owning_work.id {
        return Err(PulseError::validation(
            "shaping_receipt_subject_mismatch",
            "shaping receipt subject must match owning_work",
        ));
    }
    require_work_binding(&receipt.bindings, &p.owning_work.revision_ref())?;
    for work in &p.affected_work {
        require_work_binding(&receipt.bindings, &work.revision_ref())?;
    }
    if p.source_posture == SourcePosture::CleanGitCommit && receipt.bindings.source.is_none() {
        return Err(PulseError::validation(
            "source_binding_missing",
            "clean_git_commit shaping requires a source binding",
        ));
    }
    if receipt.bindings.content.is_empty() {
        return Err(PulseError::validation(
            "content_binding_missing",
            "shaping validation requires at least one content binding",
        ));
    }
    validate_materialization(&p.materialization)?;
    validate_non_empty(
        &p.approval.reference,
        "receipt_schema_invalid",
        "approval reference required",
    )?;
    validate_actor(&p.approval.approved_by)?;
    if p.shape_mode == ShapeMode::PersistedMap {
        let Some(destination) = &p.destination else {
            return Err(PulseError::validation(
                "shaping_destination_missing",
                "persisted_map shaping requires destination",
            ));
        };
        validate_destination(destination)?;
        let Some(map) = &p.map else {
            return Err(PulseError::validation(
                "shaping_map_required",
                "persisted_map shaping requires map binding",
            ));
        };
        validate_map(receipt, map)?;
    } else {
        if let Some(destination) = &p.destination {
            validate_destination(destination)?;
        }
        if let Some(map) = &p.map {
            validate_map(receipt, map)?;
        }
    }
    validate_unique_by(
        "shaping_branch_duplicate",
        p.branches.iter().map(|b| b.id.as_str()),
    )?;
    validate_unique_by("shaping_fog_duplicate", p.fog.iter().map(|f| f.id.as_str()))?;
    validate_unique_by(
        "shaping_out_of_scope_duplicate",
        p.out_of_scope.iter().map(|o| o.id.as_str()),
    )?;
    for branch in &p.branches {
        validate_branch(branch)?;
    }
    for fog in &p.fog {
        validate_fog(fog)?;
    }
    for item in &p.out_of_scope {
        validate_out_of_scope(item)?;
    }
    for resolution in &p.resolution_pointers {
        validate_resolution(resolution)?;
    }
    if let Some(reconciliation) = &p.reconciliation {
        validate_reconciliation(reconciliation)?;
    }
    for remaining in &p.remaining_uncertainty {
        validate_non_empty(
            &remaining.summary,
            "receipt_schema_invalid",
            "remaining uncertainty summary required",
        )?;
        validate_non_empty(
            &remaining.trigger,
            "receipt_schema_invalid",
            "remaining uncertainty trigger required",
        )?;
    }
    Ok(())
}

fn validate_materialization(value: &str) -> Result<()> {
    match value {
        "R0" | "R1" | "R2" | "R3" => Ok(()),
        _ => Err(PulseError::validation(
            "receipt_schema_invalid",
            "materialization must be R0..R3",
        )),
    }
}

fn validate_destination(destination: &ShapingDestination) -> Result<()> {
    validate_non_empty(
        &destination.summary,
        "shaping_destination_missing",
        "destination summary required",
    )?;
    if destination.exit_conditions.is_empty() {
        return Err(PulseError::validation(
            "shaping_exit_condition_missing",
            "destination exit_conditions required",
        ));
    }
    for condition in &destination.exit_conditions {
        validate_non_empty(
            condition,
            "shaping_exit_condition_missing",
            "destination exit condition required",
        )?;
    }
    for boundary in &destination.scope_boundary {
        validate_non_empty(
            boundary,
            "receipt_schema_invalid",
            "scope boundary required",
        )?;
    }
    Ok(())
}

fn validate_map(receipt: &ReceiptEnvelope, map: &ShapingMapSnapshot) -> Result<()> {
    crate::storage::safe_repo_relative(&map.path)?;
    if map.revision < 1 {
        return Err(PulseError::validation(
            "shaping_map_revision_stale",
            "map revision must be >= 1",
        ));
    }
    validate_content_hash(&map.content_hash, "shaping_map_content_stale")?;
    validate_document_entry_common(receipt, &map.path, &map.content_hash)
}

fn validate_branch(branch: &ShapingBranch) -> Result<()> {
    validate_id_prefix(&branch.id, "BR-", "shaping_branch_duplicate")?;
    validate_non_empty(
        &branch.question,
        "receipt_schema_invalid",
        "branch question required",
    )?;
    validate_gap_kind(&branch.gap_kind)?;
    validate_unique_by(
        "work_binding_missing",
        branch.affected_work.iter().map(|id| id.as_str()),
    )?;
    for id in &branch.affected_work {
        validate_work_id_kind(id, "", "work_binding_missing")?;
    }
    match &branch.disposition {
        BranchDisposition::Resolved { resolution } => validate_resolution(resolution),
        BranchDisposition::Rejected { reason, .. } => validate_non_empty(
            reason,
            "shaping_rejection_reason_missing",
            "rejected branch reason required",
        ),
        BranchDisposition::Delegated { freedom_id, reason } => {
            validate_non_empty(
                freedom_id,
                "shaping_delegation_exceeds_freedom",
                "delegated branch freedom_id required",
            )?;
            validate_non_empty(
                reason,
                "receipt_schema_invalid",
                "delegation reason required",
            )
        }
        BranchDisposition::Deferred {
            reason,
            owner,
            target_work,
            trigger,
            non_blocking_for,
        } => {
            validate_non_empty(
                reason,
                "shaping_defer_reason_missing",
                "defer reason required",
            )?;
            validate_non_empty(owner, "shaping_defer_owner_missing", "defer owner required")?;
            validate_work_id_kind(target_work, "", "shaping_defer_target_missing")?;
            validate_non_empty(
                trigger,
                "shaping_defer_trigger_missing",
                "defer trigger required",
            )?;
            if non_blocking_for.is_empty() {
                return Err(PulseError::validation(
                    "shaping_defer_not_non_blocking",
                    "deferred branch must name non_blocking_for work",
                ));
            }
            for id in non_blocking_for {
                validate_work_id_kind(id, "", "shaping_defer_not_non_blocking")?;
            }
            Ok(())
        }
        BranchDisposition::Blocking { .. } => Ok(()),
    }
}

fn validate_fog(fog: &ShapingFog) -> Result<()> {
    validate_id_prefix(&fog.id, "FOG-", "receipt_schema_invalid")?;
    validate_non_empty(
        &fog.statement,
        "receipt_schema_invalid",
        "fog statement required",
    )?;
    if fog.bounds.is_empty() {
        return Err(PulseError::validation(
            "shaping_fog_unbounded",
            "fog bounds required",
        ));
    }
    for bound in &fog.bounds {
        validate_non_empty(bound, "shaping_fog_unbounded", "fog bound required")?;
    }
    validate_non_empty(
        &fog.why_not_precise,
        "receipt_schema_invalid",
        "fog why_not_precise required",
    )?;
    validate_non_empty(
        &fog.trigger,
        "shaping_fog_trigger_missing",
        "fog trigger required",
    )?;
    for id in &fog.affected_work {
        validate_work_id_kind(id, "", "work_binding_missing")?;
    }
    Ok(())
}

fn validate_out_of_scope(item: &ShapingOutOfScope) -> Result<()> {
    validate_id_prefix(&item.id, "OOS-", "receipt_schema_invalid")?;
    validate_non_empty(
        &item.statement,
        "receipt_schema_invalid",
        "out_of_scope statement required",
    )?;
    validate_non_empty(
        &item.reason,
        "receipt_schema_invalid",
        "out_of_scope reason required",
    )
}

fn validate_resolution(resolution: &ShapingResolutionPointer) -> Result<()> {
    match resolution.kind.as_str() {
        "decision" | "work" | "evidence" | "content" => {}
        _ => {
            return Err(PulseError::validation(
                "shaping_resolution_missing",
                "resolution kind unsupported",
            ))
        }
    }
    validate_non_empty(
        &resolution.id,
        "shaping_resolution_missing",
        "resolution id required",
    )?;
    if resolution.revision < 1 {
        return Err(PulseError::validation(
            "shaping_resolution_missing",
            "resolution revision required",
        ));
    }
    validate_non_empty(
        &resolution.gist,
        "shaping_resolution_missing",
        "resolution gist required",
    )
}

fn validate_reconciliation(reconciliation: &ShapingReconciliation) -> Result<()> {
    if let Some(id) = &reconciliation.supersedes_receipt {
        validate_receipt_id(id)?;
    }
    validate_unique_by(
        "shaping_reconciliation_reference_invalid",
        reconciliation
            .surfaced_branch_ids
            .iter()
            .chain(reconciliation.invalidated_branch_ids.iter())
            .chain(reconciliation.graduated_fog_ids.iter())
            .map(|id| id.as_str()),
    )?;
    for id in &reconciliation.affected_work {
        validate_work_id_kind(id, "", "work_binding_missing")?;
    }
    Ok(())
}

fn validate_gap_kind(value: &str) -> Result<()> {
    match value {
        "fact_gap" | "intent_gap" | "tradeoff_gap" | "fidelity_gap" | "prerequisite_gap" => Ok(()),
        _ => Err(PulseError::validation(
            "receipt_schema_invalid",
            "invalid gap kind",
        )),
    }
}
