//! Typed QA checkpoint payload validation.
//!
//! Evidence owns the immutable envelope and generic bindings. This module owns
//! only behavioral QA payload semantics; Core completion separately resolves
//! the current baseline and proves coverage/currentness before closing work.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::evidence::model::{ReceiptEnvelope, ReceiptResult};
use crate::{PulseError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaCheckpointPayload {
    pub payload_version: u32,
    pub qa_scope: QaExecutionScope,
    pub story_id: String,
    pub ticket_id: String,
    pub baseline_revision: u64,
    pub baseline_content_hash: String,
    pub cases: Vec<QaCaseObservation>,
    pub executor: QaExecutor,
    pub environment: QaRuntimeEnvironment,
    pub observations: Vec<String>,
    pub cleanup_passed: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QaExecutionScope {
    TicketCheckpoint,
    StoryClose,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaCaseObservation {
    pub case_id: String,
    pub case_revision: u64,
    pub outcome: QaCaseOutcome,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QaCaseOutcome {
    Passed,
    ProductFailure,
    TestFailure,
    InfrastructureFailure,
    Inconclusive,
    Flaky,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaExecutor {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaRuntimeEnvironment {
    pub profile: String,
    pub platform: String,
    pub fixture_revision: String,
}

/// Validate QA-specific semantics inside a generic immutable evidence envelope.
///
/// # Errors
///
/// Returns a typed validation error for unsupported versions, incomplete
/// execution identity, inconsistent results, duplicate cases, or missing
/// source/baseline bindings.
pub fn validate_checkpoint_receipt(
    receipt: &ReceiptEnvelope,
    payload: &QaCheckpointPayload,
) -> Result<()> {
    if receipt.receipt_version != 2
        || payload.payload_version != 1
        || payload.qa_scope != QaExecutionScope::TicketCheckpoint
        || payload.story_id.trim().is_empty()
        || payload.ticket_id.trim().is_empty()
        || payload.baseline_revision == 0
        || !is_sha256(&payload.baseline_content_hash)
        || payload.cases.is_empty()
        || payload.executor.name.trim().is_empty()
        || payload.executor.version.trim().is_empty()
        || payload.environment.profile.trim().is_empty()
        || payload.environment.platform.trim().is_empty()
        || payload.environment.fixture_revision.trim().is_empty()
        || payload
            .observations
            .iter()
            .all(|value| value.trim().is_empty())
    {
        return Err(PulseError::validation(
            "qa_receipt_invalid",
            "QA checkpoint receipt is incomplete or uses an unsupported contract",
        ));
    }
    if receipt.subject.kind != "work"
        || receipt.subject.id != payload.ticket_id
        || receipt.bindings.source.is_none()
    {
        return Err(PulseError::validation(
            "qa_receipt_binding_invalid",
            "QA checkpoint must bind its Ticket subject and exact source",
        ));
    }
    let baseline_path = format!("works/{}/qa.md", payload.story_id);
    if !receipt.bindings.content.iter().any(|binding| {
        binding.path == baseline_path && binding.sha256 == payload.baseline_content_hash
    }) {
        return Err(PulseError::validation(
            "qa_receipt_baseline_binding_missing",
            "QA checkpoint must content-bind the exact Story baseline",
        ));
    }
    let mut ids = BTreeSet::new();
    for case in &payload.cases {
        if !ids.insert(&case.case_id) || case.case_id.trim().is_empty() || case.case_revision == 0 {
            return Err(PulseError::validation(
                "qa_receipt_case_invalid",
                "QA checkpoint cases must have unique IDs and positive revisions",
            ));
        }
    }
    let all_passed = payload
        .cases
        .iter()
        .all(|case| case.outcome == QaCaseOutcome::Passed)
        && payload.cleanup_passed;
    let has_failure = payload.cases.iter().any(|case| {
        matches!(
            case.outcome,
            QaCaseOutcome::ProductFailure | QaCaseOutcome::TestFailure
        )
    });
    let consistent = match receipt.result {
        ReceiptResult::Passed => all_passed,
        ReceiptResult::Failed => has_failure && !all_passed,
        ReceiptResult::Inconclusive => !all_passed && !has_failure,
    };
    if !consistent {
        return Err(PulseError::validation(
            "qa_receipt_result_inconsistent",
            "QA envelope result does not match case outcomes and cleanup",
        ));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()))
}
