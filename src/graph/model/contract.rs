use serde::{Deserialize, Serialize};

use crate::{PulseError, PulseResult};

pub const NODE_SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_SHORT_TEXT: usize = 280;
pub(crate) const MAX_LONG_TEXT: usize = 4096;
pub(crate) const MAX_ID: usize = 64;
pub(crate) const MAX_COLLECTION: usize = 64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ContractValidationMode {
    /// Canonical structural validation for stored graph nodes. This mode permits
    /// draft/incomplete Tickets and missing role-specific contracts so current
    /// storage can preserve bounded uncertainty without fabricating readiness.
    CanonicalStorage,
    /// Public `work create` validation. Ticket classification must be explicit
    /// and assessed, but the role-specific implementation/decision-work
    /// contract may still be added later as a readiness/completeness concern.
    PublicCreate,
    /// Readiness/completeness-oriented validation. This reports missing
    /// role-specific contracts and other gate findings without redefining
    /// canonical storage validity.
    Completeness,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PublicCreateClassification {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<TicketRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk: Option<Risk>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materialization: Option<Materialization>,
}

impl PublicCreateClassification {
    pub fn any_present(&self) -> bool {
        self.role.is_some() || self.risk.is_some() || self.materialization.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractValidationReport {
    pub schema_version: u32,
    pub code: String,
    pub valid: bool,
    pub errors: Vec<ContractFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractFinding {
    pub code: String,
    pub message: String,
}

impl ContractValidationReport {
    pub fn ok() -> Self {
        Self {
            schema_version: 1,
            code: "valid".to_string(),
            valid: true,
            errors: vec![],
        }
    }

    pub fn push(&mut self, code: &'static str, message: impl Into<String>) {
        self.valid = false;
        self.code = "invalid_contract".to_string();
        self.errors.push(ContractFinding {
            code: code.to_string(),
            message: message.into(),
        });
    }

    pub fn extend(&mut self, other: Self) {
        for error in other.errors {
            self.valid = false;
            self.code = "invalid_contract".to_string();
            self.errors.push(error);
        }
    }

    pub fn into_result(self) -> PulseResult<()> {
        if self.valid {
            Ok(())
        } else {
            let first = &self.errors[0];
            Err(PulseError::validation(
                stable_code(&first.code),
                first.message.clone(),
            ))
        }
    }
}

pub(crate) fn stable_code(code: &str) -> &'static str {
    match code {
        "contract_revision_invalid" => "contract_revision_invalid",
        "work_role_invalid" => "work_role_invalid",
        "work_classification_missing" => "work_classification_missing",
        "work_classification_not_allowed" => "work_classification_not_allowed",
        "risk_materialization_unassessed" => "risk_materialization_unassessed",
        "implementation_contract_missing" => "implementation_contract_missing",
        "implementation_mode_missing" => "implementation_mode_missing",
        "implementation_surface_missing" => "implementation_surface_missing",
        "implementation_plan_policy_missing" => "implementation_plan_policy_missing",
        "implementation_anchor_missing" => "implementation_anchor_missing",
        "implementation_invariant_missing" => "implementation_invariant_missing",
        "implementation_acceptance_missing" => "implementation_acceptance_missing",
        "implementation_freedom_missing" => "implementation_freedom_missing",
        "implementation_brief_missing" => "implementation_brief_missing",
        "implementation_brief_hash_stale" => "implementation_brief_hash_stale",
        "implementation_contract_invalid" => "implementation_contract_invalid",
        "required_decision_missing" => "required_decision_missing",
        "required_decision_revision_stale" => "required_decision_revision_stale",
        "decision_acceptance_missing" => "decision_acceptance_missing",
        "decision_acceptance_stale" => "decision_acceptance_stale",
        "decision_work_contract_missing" => "decision_work_contract_missing",
        "decision_work_destination_invalid" => "decision_work_destination_invalid",
        "decision_work_branch_missing" => "decision_work_branch_missing",
        "decision_work_question_invalid" => "decision_work_question_invalid",
        "qa_impact_unknown" => "qa_impact_unknown",
        "qa_impact_invalid" => "qa_impact_invalid",
        "shaping_receipt_missing" => "shaping_receipt_missing",
        "shaping_receipt_hash_mismatch" => "shaping_receipt_hash_mismatch",
        "shaping_map_path_unsafe" => "shaping_map_path_unsafe",
        "shaping_map_revision_stale" => "shaping_map_revision_stale",
        "shaping_map_content_stale" => "shaping_map_content_stale",
        _ => "contract_validation_failed",
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TicketRole {
    Implementation,
    DecisionWork,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Unassessed,
    Low,
    Medium,
    High,
    Critical,
}

impl Risk {
    pub fn is_assessed(self) -> bool {
        self != Self::Unassessed
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Materialization {
    Unassessed,
    #[serde(rename = "R0")]
    R0,
    #[serde(rename = "R1")]
    R1,
    #[serde(rename = "R2")]
    R2,
    #[serde(rename = "R3")]
    R3,
}

impl Materialization {
    pub fn is_assessed(self) -> bool {
        self != Self::Unassessed
    }

    pub fn requires_invariant(self) -> bool {
        matches!(self, Self::R1 | Self::R2 | Self::R3)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ImplementationMode {
    Locked,
    Guided,
    Open,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum WorkSurface {
    Code,
    Documentation,
    Configuration,
    Data,
    Research,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum PlanPolicy {
    None,
    WorkerOptional,
    RequiredBeforeExecution,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ImplementationSemanticImpact {
    NoBehaviorOrPublicRiskChange,
    BehaviorOrPublicRiskChange,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum QaImpactPosture {
    Unknown,
    Required,
    CoveredByStoryClose,
    None,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaMetadata {
    pub impact: QaImpact,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaImpact {
    pub posture: QaImpactPosture,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behavioral_owner: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affected_case_ids: Vec<String>,
}

impl Default for QaImpact {
    fn default() -> Self {
        Self {
            posture: QaImpactPosture::Unknown,
            rationale: None,
            behavioral_owner: None,
            affected_case_ids: vec![],
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum GapKind {
    FactGap,
    IntentGap,
    TradeoffGap,
    FidelityGap,
    PrerequisiteGap,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedEvidence {
    FocusedTestOutput,
    AcceptanceMapping,
    ClientContractInventory,
    PrototypeEvidence,
    ResearchNotes,
    DecisionRecord,
    DocumentationDiff,
    ConfigurationDiff,
    DataSample,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedHandoff {
    SourceSnapshot,
    AcceptanceToEvidence,
    RemainingRisks,
    DocumentationFindings,
    DecisionSummary,
    FollowUpWork,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EffortMetadata {
    #[serde(default, skip_serializing_if = "is_false")]
    pub multi_session: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub multiple_dependent_decisions: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub resume_or_audit_continuity: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl EffortMetadata {
    pub fn requires_r2_map(&self) -> bool {
        self.multi_session || self.multiple_dependent_decisions || self.resume_or_audit_continuity
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContentRef {
    pub path: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SurfaceRef {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

impl SurfaceRef {
    pub fn path(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            symbol: None,
            content_hash: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContractItem {
    pub id: String,
    pub summary: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContractScope {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub included: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReceiptRef {
    pub id: String,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RevisionedWorkRef {
    pub id: String,
    pub contract_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RequiredDecisionRef {
    pub id: String,
    pub contract_revision: u64,
    pub acceptance_receipt: ReceiptRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SharedApproachRef {
    pub owner: RevisionedWorkRef,
    pub path: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ImplementationContract {
    pub mode: ImplementationMode,
    pub work_surface: WorkSurface,
    pub plan_policy: PlanPolicy,
    pub semantic_impact: ImplementationSemanticImpact,
    #[serde(default, skip_serializing_if = "EffortMetadata::is_default")]
    pub effort: EffortMetadata,
    pub verification_profile: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brief: Option<ContentRef>,
    pub objective: String,
    pub current_behavior: String,
    pub target_behavior: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub code_anchors: Vec<SurfaceRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documentation_anchors: Vec<SurfaceRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub configuration_anchors: Vec<SurfaceRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_anchors: Vec<SurfaceRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub research_refs: Vec<SurfaceRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_changes: Vec<ContractItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariants: Vec<ContractItem>,
    pub acceptance: Vec<ContractItem>,
    #[serde(default, skip_serializing_if = "ContractScope::is_empty")]
    pub scope: ContractScope,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implementation_freedom: Vec<ContractItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_decisions: Vec<RequiredDecisionRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_approach_refs: Vec<SharedApproachRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_evidence: Vec<ExpectedEvidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_handoff: Vec<ExpectedHandoff>,
}

impl EffortMetadata {
    fn is_default(value: &Self) -> bool {
        !value.multi_session
            && !value.multiple_dependent_decisions
            && !value.resume_or_audit_continuity
    }
}

impl ContractScope {
    fn is_empty(&self) -> bool {
        self.included.is_empty() && self.excluded.is_empty()
    }
}

impl ImplementationContract {
    pub fn normalize(&mut self) {
        self.code_anchors.sort_by(surface_ref_cmp);
        self.documentation_anchors.sort_by(surface_ref_cmp);
        self.configuration_anchors.sort_by(surface_ref_cmp);
        self.data_anchors.sort_by(surface_ref_cmp);
        self.research_refs.sort_by(surface_ref_cmp);
        self.required_changes.sort_by(|a, b| a.id.cmp(&b.id));
        self.invariants.sort_by(|a, b| a.id.cmp(&b.id));
        self.acceptance.sort_by(|a, b| a.id.cmp(&b.id));
        self.implementation_freedom.sort_by(|a, b| a.id.cmp(&b.id));
        self.required_decisions.sort_by(|a, b| a.id.cmp(&b.id));
        self.shared_approach_refs
            .sort_by(|a, b| a.owner.id.cmp(&b.owner.id).then(a.path.cmp(&b.path)));
        self.expected_evidence.sort();
        self.expected_evidence.dedup();
        self.expected_handoff.sort();
        self.expected_handoff.dedup();
        self.scope.included.sort();
        self.scope.included.dedup();
        self.scope.excluded.sort();
        self.scope.excluded.dedup();
    }
}

fn surface_ref_cmp(left: &SurfaceRef, right: &SurfaceRef) -> std::cmp::Ordering {
    left.path
        .cmp(&right.path)
        .then(left.symbol.cmp(&right.symbol))
        .then(left.content_hash.cmp(&right.content_hash))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DecisionWorkContract {
    pub destination_owner: RevisionedWorkRef,
    pub branch_id: String,
    pub gap_kind: GapKind,
    pub question: String,
    pub expected_output: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_evidence: Vec<ExpectedEvidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_target: Option<ResolutionTarget>,
    pub provenance: DecisionWorkProvenance,
}

impl DecisionWorkContract {
    pub fn normalize(&mut self) {
        self.expected_evidence.sort();
        self.expected_evidence.dedup();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResolutionTarget {
    pub kind: ResolutionTargetKind,
    pub id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionTargetKind {
    Decision,
    Work,
    Evidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DecisionWorkProvenance {
    pub shaping_receipt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fog_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ShapingPointer {
    pub receipt: ReceiptRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<ShapingMapRef>,
    pub applied_at: chrono::DateTime<chrono::Utc>,
    pub applied_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ShapingMapRef {
    pub path: String,
    pub revision: u64,
    pub content_hash: String,
}
