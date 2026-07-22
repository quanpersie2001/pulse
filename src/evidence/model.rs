use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActorRef {
    pub kind: ActorKind,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    Human,
    Agent,
    System,
}

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
    ShapingValidation,
    DocumentationValidation,
}

impl ReceiptKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SupersessionReconciliation => "supersession_reconciliation",
            Self::ShapingValidation => "shaping_validation",
            Self::DocumentationValidation => "documentation_validation",
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ReceiptPayload {
    SupersessionReconciliation(SupersessionReconciliationPayload),
    ShapingValidation(ShapingValidationPayload),
    DocumentationValidation(DocumentationValidationPayload),
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
#[serde(deny_unknown_fields)]
pub struct ShapingValidationPayload {
    pub payload_version: u32,
    pub owning_work: WorkRevisionRef,
    pub risk: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    pub branch_summary: BranchSummary,
    #[serde(default)]
    pub remaining_uncertainty: Vec<String>,
    pub approval_assertion: ApprovalAssertion,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct BranchSummary {
    #[serde(default)]
    pub resolved: Vec<String>,
    #[serde(default)]
    pub rejected: Vec<String>,
    #[serde(default)]
    pub delegated: Vec<String>,
    #[serde(default)]
    pub deferred: Vec<String>,
    #[serde(default)]
    pub blocking: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ApprovalAssertion {
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
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
    /// Historical Slice 3 payload v1 carried an optional, untrusted document ID
    /// proposal. It remains parseable for immutable receipts but is not treated
    /// as canonical registry identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_document_id: Option<String>,
    /// Canonical Slice 4+ registry document identity. Required for payload v2;
    /// optional in the Rust model so legacy v1 receipts can deserialize.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    /// Receipt-bound document record revision. Required for payload v2;
    /// optional in the Rust model so legacy v1 receipts can deserialize.
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
