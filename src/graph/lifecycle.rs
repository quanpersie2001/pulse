use serde::{Deserialize, Serialize};

use crate::graph::node::{NodeStatus, StatusReason};
use crate::{PulseError, PulseResult};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum StatusClass {
    Preparation,
    Execution,
    Paused,
    Terminal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransitionPolicy {
    Supported,
    Gated,
    SupersessionOnly,
    Illegal,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TransitionExpectation {
    pub from: NodeStatus,
    pub to: NodeStatus,
    pub policy: TransitionPolicy,
    pub requires_transition_reason: bool,
    pub target_requires_status_reason: bool,
    pub required_gate_families: Vec<&'static str>,
    pub allowed_targets: Vec<NodeStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransitionReason {
    pub code: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

impl TransitionReason {
    pub fn into_status_reason(self) -> StatusReason {
        StatusReason {
            code: self.code,
            summary: self.summary,
            reference: self.reference,
        }
    }
}

pub fn status_class(status: NodeStatus) -> StatusClass {
    match status {
        NodeStatus::Draft | NodeStatus::Shaped | NodeStatus::Ready => StatusClass::Preparation,
        NodeStatus::Active | NodeStatus::Verifying | NodeStatus::Rework => StatusClass::Execution,
        NodeStatus::Blocked => StatusClass::Paused,
        NodeStatus::Done | NodeStatus::Cancelled | NodeStatus::Superseded => StatusClass::Terminal,
    }
}

pub fn status_requires_reason(status: NodeStatus) -> bool {
    matches!(
        status,
        NodeStatus::Blocked | NodeStatus::Rework | NodeStatus::Cancelled | NodeStatus::Superseded
    )
}

pub fn transition_requires_reason(from: NodeStatus, to: NodeStatus) -> bool {
    status_requires_reason(to)
        || matches!(
            (from, to),
            (NodeStatus::Shaped, NodeStatus::Draft)
                | (NodeStatus::Ready, NodeStatus::Shaped)
                | (NodeStatus::Blocked, NodeStatus::Draft)
                | (NodeStatus::Blocked, NodeStatus::Shaped)
        )
}

pub fn expectation(from: NodeStatus, to: NodeStatus) -> TransitionExpectation {
    let policy = transition_policy(from, to);
    TransitionExpectation {
        from,
        to,
        policy,
        requires_transition_reason: transition_requires_reason(from, to),
        target_requires_status_reason: status_requires_reason(to),
        required_gate_families: required_gate_families(from, to),
        allowed_targets: supported_targets(from),
    }
}

pub fn validate_transition(
    from: NodeStatus,
    to: NodeStatus,
    reason: Option<&TransitionReason>,
) -> PulseResult<TransitionExpectation> {
    let exp = expectation(from, to);
    match exp.policy {
        TransitionPolicy::Supported => {}
        TransitionPolicy::Gated => {
            return Err(PulseError::validation(
                "transition_gate_unavailable",
                format!(
                    "transition direction {from:?} -> {to:?} is valid but required gate capabilities are not installed; required_gate_families={:?}",
                    exp.required_gate_families
                ),
            ));
        }
        TransitionPolicy::SupersessionOnly | TransitionPolicy::Illegal => {
            return Err(PulseError::validation(
                "illegal_transition",
                format!(
                    "transition {from:?} -> {to:?} is not supported by generic transition; allowed_targets={:?}",
                    exp.allowed_targets
                ),
            ));
        }
    }

    if exp.requires_transition_reason {
        let Some(reason) = reason else {
            return Err(PulseError::validation(
                "missing_status_reason",
                "transition requires a non-empty reason",
            ));
        };
        validate_reason(reason)?;
    }
    Ok(exp)
}

pub fn validate_reason(reason: &TransitionReason) -> PulseResult<()> {
    if !is_slug(&reason.code) {
        return Err(PulseError::validation(
            "invalid_status_reason",
            "reason code must be a non-empty slug using lowercase letters, digits, '_' or '-'",
        ));
    }
    let summary = reason.summary.trim();
    if summary.is_empty() {
        return Err(PulseError::validation(
            "missing_status_reason",
            "reason summary must not be empty",
        ));
    }
    if summary.chars().count() > 500 {
        return Err(PulseError::validation(
            "invalid_status_reason",
            "reason summary must be at most 500 characters",
        ));
    }
    if let Some(reference) = &reason.reference {
        if reference.trim().is_empty() {
            return Err(PulseError::validation(
                "invalid_status_reason",
                "reason reference must not be empty when provided",
            ));
        }
    }
    Ok(())
}

fn supported_targets(from: NodeStatus) -> Vec<NodeStatus> {
    match from {
        NodeStatus::Draft => vec![NodeStatus::Cancelled],
        NodeStatus::Shaped => vec![
            NodeStatus::Draft,
            NodeStatus::Blocked,
            NodeStatus::Cancelled,
        ],
        NodeStatus::Ready => vec![
            NodeStatus::Shaped,
            NodeStatus::Blocked,
            NodeStatus::Cancelled,
        ],
        NodeStatus::Blocked => vec![NodeStatus::Draft, NodeStatus::Shaped, NodeStatus::Cancelled],
        _ => vec![],
    }
}

fn transition_policy(from: NodeStatus, to: NodeStatus) -> TransitionPolicy {
    if supported_targets(from).contains(&to) {
        return TransitionPolicy::Supported;
    }
    if to == NodeStatus::Superseded {
        return TransitionPolicy::SupersessionOnly;
    }
    match (from, to) {
        (NodeStatus::Draft, NodeStatus::Shaped)
        | (NodeStatus::Shaped, NodeStatus::Ready)
        | (NodeStatus::Ready, NodeStatus::Active)
        | (NodeStatus::Active, NodeStatus::Blocked)
        | (NodeStatus::Active, NodeStatus::Verifying)
        | (NodeStatus::Active, NodeStatus::Cancelled)
        | (NodeStatus::Verifying, NodeStatus::Done)
        | (NodeStatus::Verifying, NodeStatus::Rework)
        | (NodeStatus::Verifying, NodeStatus::Blocked)
        | (NodeStatus::Rework, NodeStatus::Shaped)
        | (NodeStatus::Rework, NodeStatus::Ready)
        | (NodeStatus::Rework, NodeStatus::Active)
        | (NodeStatus::Rework, NodeStatus::Cancelled)
        | (NodeStatus::Blocked, NodeStatus::Ready)
        | (NodeStatus::Blocked, NodeStatus::Active) => TransitionPolicy::Gated,
        _ => TransitionPolicy::Illegal,
    }
}

fn required_gate_families(from: NodeStatus, to: NodeStatus) -> Vec<&'static str> {
    match (from, to) {
        (NodeStatus::Draft, NodeStatus::Shaped) => {
            vec!["source_revision", "shaping_authority"]
        }
        (NodeStatus::Shaped, NodeStatus::Ready) | (NodeStatus::Blocked, NodeStatus::Ready) => {
            vec![
                "implementation_contract",
                "shaping_authority",
                "documentation_impact",
                "qa_impact",
            ]
        }
        (NodeStatus::Ready, NodeStatus::Active)
        | (NodeStatus::Rework, NodeStatus::Active)
        | (NodeStatus::Blocked, NodeStatus::Active) => vec!["lease"],
        (NodeStatus::Active, NodeStatus::Blocked) | (NodeStatus::Active, NodeStatus::Cancelled) => {
            vec!["lease", "run_authority"]
        }
        (NodeStatus::Active, NodeStatus::Verifying) => vec!["source_snapshot"],
        (NodeStatus::Verifying, NodeStatus::Done) | (NodeStatus::Verifying, NodeStatus::Rework) => {
            vec!["verification_receipt"]
        }
        (NodeStatus::Verifying, NodeStatus::Blocked) => {
            vec!["verification_receipt", "run_authority"]
        }
        (NodeStatus::Rework, NodeStatus::Shaped) => vec!["rework_receipt", "shaping_authority"],
        (NodeStatus::Rework, NodeStatus::Ready) => vec!["rework_receipt", "ready_gate"],
        (NodeStatus::Rework, NodeStatus::Cancelled) => vec!["authority"],
        _ => vec![],
    }
}

fn is_slug(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
}
