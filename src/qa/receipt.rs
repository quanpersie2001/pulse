//! Typed QA checkpoint payload validation.
//!
//! Evidence owns the immutable envelope and generic bindings. This module owns
//! only behavioral QA payload semantics; Core completion separately resolves
//! the current baseline and proves coverage/currentness before closing work.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::evidence::model::{
    ArtifactBinding, ReceiptEnvelope, ReceiptKind, ReceiptResult, SourceBinding, SubjectRef,
};
use crate::execution::{Finding, FindingSeverity};
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
    pub observations: Vec<String>,
    /// Shaped findings (Decision 0012 §3, 0014 §5): `case_id` instead of
    /// `acceptance_id`; a finding without `check` is `unverifiable`.
    #[serde(default)]
    pub findings: Vec<Finding>,
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
    NotApplicable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaExecutor {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

pub const QA_CHECKPOINT_PAYLOAD_VERSION: u32 = 1;

/// Validate QA-specific semantics inside a generic immutable evidence envelope.
///
/// # Errors
///
/// Returns a typed validation error for unsupported versions, inconsistent
/// results, duplicate cases, or missing source/baseline bindings.
pub fn validate_checkpoint_receipt(
    receipt: &ReceiptEnvelope,
    payload: &QaCheckpointPayload,
) -> Result<()> {
    if receipt.receipt_version != 2
        || payload.payload_version != QA_CHECKPOINT_PAYLOAD_VERSION
        || payload.story_id.trim().is_empty()
        || payload.ticket_id.trim().is_empty()
        || payload.baseline_revision == 0
        || !is_sha256(&payload.baseline_content_hash)
        || payload.cases.is_empty()
        || payload.executor.name.trim().is_empty()
        || payload.executor.version.trim().is_empty()
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
        .all(|case| case.outcome == QaCaseOutcome::Passed);
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
            "QA envelope result does not match case outcomes",
        ));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Build the immutable `qa_checkpoint` envelope from the run input contract,
/// the runner's final output JSON, the artifacts ingested by Pulse and the
/// exact source binding (Decision 0014 §2).
///
/// The runner's final output carries `cases[] {id, status, observation}`,
/// optional `findings[] {case_id?, summary, owner, check?, severity}` and
/// nothing else of consequence; statuses map onto typed outcomes and every
/// reported case must exist in the input with the same revision. `result` is
/// `passed` only when every reported case passed, `failed` on a product or
/// test failure, otherwise `inconclusive`.
///
/// # Errors
///
/// Returns a typed validation error when the input or output contract is
/// malformed, a reported case is absent from the baseline, a status or
/// severity is unknown, or a finding lacks the mandatory shape.
pub fn build_checkpoint_envelope(
    input: &serde_json::Value,
    output: &serde_json::Value,
    artifacts: &[ArtifactBinding],
    source: SourceBinding,
) -> Result<ReceiptEnvelope> {
    let invalid = |detail: String| PulseError::validation("qa_checkpoint_input_invalid", detail);
    let qa_scope = match input.get("qa_scope").and_then(|v| v.as_str()) {
        Some("ticket_checkpoint") => QaExecutionScope::TicketCheckpoint,
        Some("story_close") => QaExecutionScope::StoryClose,
        _ => {
            return Err(invalid(
                "input qa_scope must be ticket_checkpoint or story_close".into(),
            ))
        }
    };
    let story_id =
        str_field(input, "story_id").ok_or_else(|| invalid("input story_id is missing".into()))?;
    let ticket_id = str_field(input, "ticket_id")
        .ok_or_else(|| invalid("input ticket_id is missing".into()))?;
    let baseline_revision = input
        .get("baseline_revision")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| invalid("input baseline_revision is missing".into()))?;
    let baseline_content_hash = str_field(input, "baseline_content_hash")
        .ok_or_else(|| invalid("input baseline_content_hash is missing".into()))?;
    let input_cases = input
        .get("cases")
        .and_then(|v| v.as_array())
        .ok_or_else(|| invalid("input cases must be an array".into()))?;
    let mut baseline: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    for case in input_cases {
        let id = str_field(case, "id").ok_or_else(|| invalid("input case id is missing".into()))?;
        let revision = case
            .get("revision")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| invalid("input case revision is missing".into()))?;
        baseline.insert(id, revision);
    }

    let output_cases = output
        .get("cases")
        .and_then(|v| v.as_array())
        .ok_or_else(|| invalid("output cases must be an array".into()))?;
    let mut cases = Vec::with_capacity(output_cases.len());
    let mut observations = Vec::with_capacity(output_cases.len());
    for case in output_cases {
        let id =
            str_field(case, "id").ok_or_else(|| invalid("output case id is missing".into()))?;
        let revision = baseline
            .get(&id)
            .copied()
            .ok_or_else(|| invalid(format!("case {id} is absent from the run input baseline")))?;
        let status = str_field(case, "status")
            .ok_or_else(|| invalid(format!("case {id} status is missing")))?;
        let outcome = match status.as_str() {
            "passed" => QaCaseOutcome::Passed,
            "failed" => QaCaseOutcome::ProductFailure,
            "inconclusive" => QaCaseOutcome::Inconclusive,
            "flaky" => QaCaseOutcome::Flaky,
            "not_applicable" => QaCaseOutcome::NotApplicable,
            other => {
                return Err(invalid(format!(
                    "case {id} status {other:?} is not a QA output status"
                )))
            }
        };
        let observation = str_field(case, "observation")
            .ok_or_else(|| invalid(format!("case {id} observation is missing")))?;
        observations.push(format!("{id}: {observation}"));
        cases.push(QaCaseObservation {
            case_id: id,
            case_revision: revision,
            outcome,
        });
    }

    let mut findings = Vec::new();
    for finding in output
        .get("findings")
        .and_then(|v| v.as_array())
        .map(|list| list.as_slice())
        .unwrap_or_default()
    {
        let summary = str_field(finding, "summary")
            .ok_or_else(|| invalid("finding summary is missing".into()))?;
        let owner = str_field(finding, "owner")
            .ok_or_else(|| invalid("finding owner is missing".into()))?;
        let severity = match str_field(finding, "severity").as_deref() {
            Some("high") => FindingSeverity::High,
            Some("medium") => FindingSeverity::Medium,
            Some("low") => FindingSeverity::Low,
            _ => {
                return Err(invalid(
                    "finding severity must be high, medium or low".into(),
                ))
            }
        };
        let check = str_field(finding, "check");
        let case_id = str_field(finding, "case_id");
        if let Some(case_id) = &case_id {
            if !baseline.contains_key(case_id.as_str()) {
                return Err(invalid(format!(
                    "finding references case {case_id} absent from the run input baseline"
                )));
            }
        }
        let mut finding = Finding {
            summary,
            owner,
            check,
            severity,
            acceptance_id: None,
            case_id,
            unverifiable: false,
        };
        finding.normalize();
        findings.push(finding);
    }

    let all_passed = !cases.is_empty()
        && cases
            .iter()
            .all(|case| case.outcome == QaCaseOutcome::Passed);
    let has_failure = cases.iter().any(|case| {
        matches!(
            case.outcome,
            QaCaseOutcome::ProductFailure | QaCaseOutcome::TestFailure
        )
    });
    let result = if all_passed {
        ReceiptResult::Passed
    } else if has_failure {
        ReceiptResult::Failed
    } else {
        ReceiptResult::Inconclusive
    };

    let subject_id = match qa_scope {
        QaExecutionScope::TicketCheckpoint => ticket_id.clone(),
        QaExecutionScope::StoryClose => story_id.clone(),
    };
    Ok(ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 2,
        id: crate::evidence::new_receipt_id(),
        kind: ReceiptKind::QaCheckpoint,
        result,
        actor: crate::policy::parse_actor("agent:runner:qa"),
        recorded_at: chrono::Utc::now(),
        subject: SubjectRef {
            kind: "work".to_string(),
            id: subject_id,
        },
        bindings: crate::evidence::model::ReceiptBindings {
            work: Vec::new(),
            source: Some(source),
            content: vec![crate::evidence::model::ContentBinding {
                path: format!("works/{story_id}/qa.md"),
                sha256: baseline_content_hash.clone(),
            }],
            artifacts: artifacts.to_vec(),
            graph_fingerprint_observed: None,
        },
        payload: crate::evidence::model::ReceiptPayload::QaCheckpoint(QaCheckpointPayload {
            payload_version: QA_CHECKPOINT_PAYLOAD_VERSION,
            qa_scope,
            story_id,
            ticket_id,
            baseline_revision,
            baseline_content_hash,
            cases,
            executor: QaExecutor {
                name: "pulse".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                capabilities: vec!["runner".to_string()],
            },
            observations,
            findings,
        }),
    })
}

fn str_field(value: &serde_json::Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}
