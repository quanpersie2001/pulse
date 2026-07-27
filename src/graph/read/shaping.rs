//! Shaping contract helpers shared between the graph store and later readiness
//! composition.
//!
//! This module owns pure shaping-specific derivation helpers. The actual node
//! mutation, transaction and recovery stay in the graph store layer so they
//! reuse the single prepared-transaction path. Receipt integrity/binding
//! validation stays in [`crate::evidence`].

use crate::PulseError;

/// Closed set of materialization levels that can carry a shaping approval grant.
pub const APPROVABLE_MATERIALIZATIONS: [&str; 4] = ["R0", "R1", "R2", "R3"];

/// Derive the kernel shaping-approval grant from a shaping receipt
/// materialization level.
///
/// The required grant is derived by the kernel from the operation and receipt
/// payload; the receipt cannot choose or under-declare it. A receipt approver
/// must own `shape.approve.<materialization>` for the current shaping effort to
/// apply.
pub fn materialization_approve_grant(materialization: &str) -> Result<String, PulseError> {
    if APPROVABLE_MATERIALIZATIONS.contains(&materialization) {
        Ok(format!("shape.approve.{materialization}"))
    } else {
        Err(PulseError::validation(
            "shaping_receipt_version_ineligible",
            format!(
                "shaping receipt materialization must be one of {:?}, got {materialization:?}",
                APPROVABLE_MATERIALIZATIONS
            ),
        ))
    }
}
