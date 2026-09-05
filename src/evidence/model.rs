//! Evidence receipt model.
//!
//! `ActorRef` and `ActorKind` are owned by the neutral identity module and
//! re-exported here for compatibility with the historical
//! `pulse::evidence::model::{ActorRef, ActorKind}` path used by receipts,
//! tests and the CLI.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// Compatibility re-export: identity vocabulary is neutral, not evidence-owned.
pub use crate::identity::actor::{ActorKind, ActorRef};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SubjectRef {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkBinding {
    pub id: String,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceBinding {
    pub kind: String,
    pub commit: String,
    pub repository_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContentBinding {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactBinding {
    pub sha256: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct ReceiptBindings {
    #[serde(default)]
    pub work: Vec<WorkBinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceBinding>,
    #[serde(default)]
    pub content: Vec<ContentBinding>,
    #[serde(default)]
    pub artifacts: Vec<ArtifactBinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_fingerprint_observed: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReceiptEnvelope {
    pub schema_version: u32,
    pub receipt_version: u32,
    pub id: String,
    pub kind: ReceiptKind,
    pub result: ReceiptResult,
    pub actor: ActorRef,
    pub recorded_at: DateTime<Utc>,
    pub subject: SubjectRef,
    pub bindings: ReceiptBindings,
    pub payload: ReceiptPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptKind {
    SupersessionReconciliation,
    DecisionAcceptance,
    DocumentationValidation,
    QaCheckpoint,
}

impl ReceiptKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SupersessionReconciliation => "supersession_reconciliation",
            Self::DecisionAcceptance => "decision_acceptance",
            Self::DocumentationValidation => "documentation_validation",
            Self::QaCheckpoint => "qa_checkpoint",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptResult {
    Passed,
    Failed,
    Inconclusive,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ReceiptPayload {
    SupersessionReconciliation(SupersessionReconciliationPayload),
    DecisionAcceptance(DecisionAcceptancePayload),
    DocumentationValidation(DocumentationValidationPayload),
    QaCheckpoint(crate::qa::QaCheckpointPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkRevisionRef {
    pub id: String,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SupersessionReconciliationPayload {
    pub payload_version: u32,
    pub old: WorkRevisionRef,
    pub target: SupersessionReceiptTarget,
    pub claim: SupersessionReceiptClaim,
    #[serde(default)]
    pub follow_up_work: Vec<WorkRevisionRef>,
    pub review_summary: String,
    #[serde(default)]
    pub reviewed_references: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SupersessionReceiptTarget {
    Replacement { id: String, revision: u64 },
    DecisionExplanation { id: String, revision: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupersessionReceiptClaim {
    Absorbed,
    FollowUpRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourcePosture {
    CleanGitCommit,
    NotRequiredContentBound,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DecisionAcceptancePayload {
    pub payload_version: u32,
    pub decision: DecisionAcceptanceDecision,
    pub accepted_outcome: String,
    pub approver: ActorRef,
    pub source_posture: SourcePosture,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DecisionAcceptanceDecision {
    pub id: String,
    pub revision_observed: u64,
    pub contract_revision: u64,
    pub content: DecisionContentSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DecisionContentSnapshot {
    pub path: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocumentationValidationPayload {
    pub payload_version: u32,
    #[serde(default)]
    pub documents: Vec<DocumentationValidationDocument>,
    #[serde(default)]
    pub checks: Vec<DocumentCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocumentationValidationDocument {
    /// Canonical registry document identity. Required for the documentation
    /// validation payload; optional in the Rust model so validation can return a
    /// domain error instead of failing deserialization first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    /// Receipt-bound document record revision. Required for the documentation
    /// validation payload; optional in the Rust model so validation can return a
    /// domain error instead of failing deserialization first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_revision: Option<u64>,
    pub path: String,
    pub content_hash: String,
    pub result: ReceiptResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocumentCheck {
    pub kind: String,
    pub result: ReceiptResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceiptReference {
    pub id: String,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationReport {
    pub schema_version: u32,
    pub receipt_id: String,
    pub receipt_hash: String,
    pub integrity: ValidationDimension,
    pub bindings: ValidationDimension,
    pub registry: ValidationDimension,
    pub policy: ValidationDimension,
    pub authorization: ValidationDimension,
    pub gate_eligible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationDimension {
    pub status: String,
    pub reason_codes: Vec<String>,
}
