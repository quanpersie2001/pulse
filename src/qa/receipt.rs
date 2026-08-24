//! Typed QA checkpoint payload validation.
//!
//! Evidence owns the immutable envelope and generic bindings. This module owns
//! only behavioral QA payload semantics; Core completion separately resolves
//! the current baseline and proves coverage/currentness before closing work.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::QaBrowserReport;
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<QaBrowserReport>,
    pub observations: Vec<String>,
    pub cleanup_passed: bool,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QaExecutionScope {
    #[default]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<QaEnvironmentLifecycle>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaEnvironmentIdentity {
    pub environment_instance_id: String,
    pub source_commit: String,
    pub fixture_revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaEnvironmentLifecycle {
    pub identity: QaEnvironmentIdentity,
    pub start_passed: bool,
    pub healthcheck_passed: bool,
    pub reset_passed: bool,
    pub cleanup_passed: bool,
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
        || !matches!(payload.payload_version, 1..=3)
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
    if (payload.payload_version == 1
        && (payload.environment.lifecycle.is_some() || payload.browser.is_some()))
        || (payload.payload_version == 2
            && (payload.environment.lifecycle.is_none() || payload.browser.is_some()))
        || (payload.payload_version == 3
            && (payload.environment.lifecycle.is_none() || payload.browser.is_none()))
    {
        return Err(PulseError::validation(
            "qa_receipt_environment_invalid",
            "QA payload version and environment lifecycle contract do not match",
        ));
    }
    let expected_subject = match payload.qa_scope {
        QaExecutionScope::TicketCheckpoint => payload.ticket_id.as_str(),
        QaExecutionScope::StoryClose => payload.story_id.as_str(),
    };
    if receipt.subject.kind != "work"
        || receipt.subject.id != expected_subject
        || receipt.bindings.source.is_none()
    {
        return Err(PulseError::validation(
            "qa_receipt_binding_invalid",
            "QA receipt must bind its exact execution-scope owner and source",
        ));
    }
    if let Some(lifecycle) = &payload.environment.lifecycle {
        let source_commit = receipt
            .bindings
            .source
            .as_ref()
            .map(|source| source.commit.as_str());
        if lifecycle.identity.environment_instance_id.trim().is_empty()
            || lifecycle.identity.source_commit.trim().is_empty()
            || lifecycle.identity.fixture_revision != payload.environment.fixture_revision
            || source_commit != Some(lifecycle.identity.source_commit.as_str())
            || (payload.cleanup_passed && !lifecycle.cleanup_passed)
            || (receipt.result == ReceiptResult::Passed
                && !(lifecycle.start_passed
                    && lifecycle.healthcheck_passed
                    && lifecycle.reset_passed
                    && lifecycle.cleanup_passed))
        {
            return Err(PulseError::validation(
                "qa_receipt_environment_invalid",
                "QA environment lifecycle identity or required step result is invalid",
            ));
        }
    }
    if let Some(browser) = &payload.browser {
        validate_browser_receipt(receipt, payload, browser)?;
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

fn validate_browser_receipt(
    receipt: &ReceiptEnvelope,
    payload: &QaCheckpointPayload,
    browser: &QaBrowserReport,
) -> Result<()> {
    let capabilities = payload
        .executor
        .capabilities
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if browser.base_url.trim().is_empty()
        || browser.trace_role.trim().is_empty()
        || browser.assertions.is_empty()
        || !["browser", "deterministic-assertion", "playwright"]
            .iter()
            .all(|required| capabilities.contains(required))
        || !receipt
            .bindings
            .artifacts
            .iter()
            .any(|artifact| artifact.role == browser.trace_role)
    {
        return Err(PulseError::validation(
            "qa_receipt_browser_invalid",
            "browser QA receipt requires execution identity, deterministic assertions, and its bound trace artifact",
        ));
    }
    let outcomes = payload
        .cases
        .iter()
        .map(|case| (case.case_id.as_str(), case.outcome))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut covered = BTreeSet::new();
    let mut assertion_keys = BTreeSet::new();
    for assertion in &browser.assertions {
        let Some(outcome) = outcomes.get(assertion.case_id.as_str()) else {
            return Err(PulseError::validation(
                "qa_receipt_browser_invalid",
                "browser assertion references a case outside the receipt",
            ));
        };
        if !assertion_keys.insert((assertion.case_id.as_str(), assertion.kind.as_str()))
            || assertion.kind.trim().is_empty()
            || assertion.expected.trim().is_empty()
            || assertion.actual.trim().is_empty()
            || (*outcome == QaCaseOutcome::Passed && !assertion.passed)
        {
            return Err(PulseError::validation(
                "qa_receipt_browser_invalid",
                "browser assertions are incomplete or inconsistent with case outcomes",
            ));
        }
        covered.insert(assertion.case_id.as_str());
    }
    if covered != outcomes.keys().copied().collect() {
        return Err(PulseError::validation(
            "qa_receipt_browser_invalid",
            "browser assertions must cover every receipt case",
        ));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()))
}
