//! Core-owned typed handoff and verification proof contracts.

use serde::{Deserialize, Serialize};

use crate::canonical_json::hash_serializable;
use crate::{PulseError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HandoffReceiptV1 {
    pub schema_version: u32,
    pub handoff_id: String,
    pub idempotency_key_hash: String,
    pub ticket_id: String,
    pub active_revision: u64,
    pub verifying_revision: u64,
    pub lease_id: String,
    pub project_id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub repository_id: String,
    pub source_commit: String,
    pub summary: String,
    #[serde(default)]
    pub changed_paths: Vec<String>,
    #[serde(default)]
    pub evidence_receipt_ids: Vec<String>,
    pub recorded_by: String,
    pub recorded_at: String,
    pub handoff_fingerprint: String,
}

impl HandoffReceiptV1 {
    pub fn compute_fingerprint(&self) -> Result<String> {
        let mut projection = self.clone();
        projection.handoff_fingerprint.clear();
        hash_serializable(&projection)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationDisposition {
    Passed,
    Rework,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VerificationCheck {
    pub name: String,
    pub command: String,
    pub exit_code: i32,
    #[serde(default)]
    pub artifact_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VerificationReceiptV1 {
    pub schema_version: u32,
    pub verification_id: String,
    pub idempotency_key_hash: String,
    pub handoff_id: String,
    pub ticket_id: String,
    pub lease_id: String,
    pub source_commit: String,
    pub disposition: VerificationDisposition,
    pub summary: String,
    pub checks: Vec<VerificationCheck>,
    pub verified_by: String,
    pub recorded_at: String,
    pub resulting_status: String,
    pub resulting_revision: u64,
    pub verification_fingerprint: String,
}

impl VerificationReceiptV1 {
    pub fn compute_fingerprint(&self) -> Result<String> {
        let mut projection = self.clone();
        projection.verification_fingerprint.clear();
        hash_serializable(&projection)
    }
}

#[derive(Debug, Clone)]
pub struct SubmitHandoffArgs {
    pub lease_id: String,
    pub actor: String,
    pub session_id: String,
    pub source_commit: String,
    pub summary: String,
    pub changed_paths: Vec<String>,
    pub evidence_receipt_ids: Vec<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone)]
pub struct CompleteVerificationArgs {
    pub handoff_id: String,
    pub actor: String,
    pub source_commit: String,
    pub disposition: VerificationDisposition,
    pub summary: String,
    pub checks: Vec<VerificationCheck>,
    pub idempotency_key: String,
}

pub fn validate_checks(
    disposition: VerificationDisposition,
    checks: &[VerificationCheck],
) -> Result<()> {
    if checks.is_empty() {
        return Err(PulseError::validation(
            "verification_checks_missing",
            "proof-driven completion requires at least one verification check",
        ));
    }
    for check in checks {
        if check.name.trim().is_empty() || check.command.trim().is_empty() {
            return Err(PulseError::validation(
                "verification_check_invalid",
                "verification check name and command must not be empty",
            ));
        }
    }
    if disposition == VerificationDisposition::Passed
        && checks.iter().any(|check| check.exit_code != 0)
    {
        return Err(PulseError::validation(
            "verification_not_passed",
            "passed disposition requires every verification check to exit zero",
        ));
    }
    Ok(())
}
