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
    DecisionAcceptance,
    DocumentationValidation,
}

impl ReceiptKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SupersessionReconciliation => "supersession_reconciliation",
            Self::ShapingValidation => "shaping_validation",
            Self::DecisionAcceptance => "decision_acceptance",
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

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ReceiptPayload {
    SupersessionReconciliation(SupersessionReconciliationPayload),
    ShapingValidation(ShapingValidationPayload),
    DecisionAcceptance(DecisionAcceptancePayload),
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
    pub owning_work: ShapingWorkBinding,
    pub materialization: String,
    pub shape_mode: ShapeMode,
    pub source_posture: SourcePosture,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<ShapingDestination>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<ShapingMapSnapshot>,
    #[serde(default)]
    pub affected_work: Vec<ShapingWorkBinding>,
    #[serde(default)]
    pub branches: Vec<ShapingBranch>,
    #[serde(default)]
    pub fog: Vec<ShapingFog>,
    #[serde(default)]
    pub out_of_scope: Vec<ShapingOutOfScope>,
    #[serde(default)]
    pub resolution_pointers: Vec<ShapingResolutionPointer>,
    pub approval: ShapingApproval,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconciliation: Option<ShapingReconciliation>,
    #[serde(default)]
    pub remaining_uncertainty: Vec<RemainingUncertainty>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ShapingWorkBinding {
    pub id: String,
    pub revision_observed: u64,
    pub contract_revision: u64,
}

impl ShapingWorkBinding {
    pub fn revision_ref(&self) -> WorkRevisionRef {
        WorkRevisionRef {
            id: self.id.clone(),
            revision: self.revision_observed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShapeMode {
    ConciseSelfCheck,
    FocusedBranches,
    PersistedMap,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourcePosture {
    CleanGitCommit,
    NotRequiredContentBound,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ShapingDestination {
    pub summary: String,
    #[serde(default)]
    pub scope_boundary: Vec<String>,
    pub exit_conditions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ShapingMapSnapshot {
    pub path: String,
    pub revision: u64,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ShapingBranch {
    pub id: String,
    pub question: String,
    pub gap_kind: String,
    pub criticality: BranchCriticality,
    #[serde(default)]
    pub affected_work: Vec<String>,
    pub disposition: BranchDisposition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BranchCriticality {
    Critical,
    NonCritical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BranchDisposition {
    Resolved {
        resolution: ShapingResolutionPointer,
    },
    Rejected {
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        reference: Option<String>,
    },
    Delegated {
        freedom_id: String,
        reason: String,
    },
    Deferred {
        reason: String,
        owner: String,
        target_work: String,
        trigger: String,
        non_blocking_for: Vec<String>,
    },
    Blocking {
        #[serde(skip_serializing_if = "Option::is_none")]
        linked_decision_work: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ShapingResolutionPointer {
    pub kind: String,
    pub id: String,
    pub revision: u64,
    pub gist: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ShapingFog {
    pub id: String,
    pub statement: String,
    pub bounds: Vec<String>,
    pub why_not_precise: String,
    pub review: FogReview,
    pub trigger: String,
    #[serde(default)]
    pub affected_work: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FogReview {
    BoundedNonBlocking,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ShapingOutOfScope {
    pub id: String,
    pub statement: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ShapingApproval {
    pub approved_by: ActorRef,
    pub reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ShapingReconciliation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes_receipt: Option<String>,
    #[serde(default)]
    pub surfaced_branch_ids: Vec<String>,
    #[serde(default)]
    pub invalidated_branch_ids: Vec<String>,
    #[serde(default)]
    pub graduated_fog_ids: Vec<String>,
    #[serde(default)]
    pub affected_work: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RemainingUncertainty {
    pub summary: String,
    pub trigger: String,
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
