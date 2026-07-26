use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::graph::node::Node;
use crate::id::{kind_for_id, validate_work_id, WorkKind};
use crate::{PulseError, PulseResult};

pub const NODE_SCHEMA_VERSION: u32 = 1;
const MAX_SHORT_TEXT: usize = 280;
const MAX_LONG_TEXT: usize = 4096;
const MAX_ID: usize = 64;
const MAX_COLLECTION: usize = 64;

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

fn stable_code(code: &str) -> &'static str {
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

pub fn validate_node_contract(
    node: &Node,
    mode: ContractValidationMode,
) -> ContractValidationReport {
    let mut report = ContractValidationReport::ok();

    if node.contract_revision < 1 {
        report.push(
            "contract_revision_invalid",
            "contract_revision must be >= 1 and distinct from normal revision",
        );
    }

    match node.kind {
        WorkKind::Ticket => validate_ticket_contract(node, mode, &mut report),
        WorkKind::Epic | WorkKind::Story => validate_non_ticket_contract_fields(node, &mut report),
        WorkKind::Decision => {
            validate_non_ticket_contract_fields(node, &mut report);
            if node.shaping.is_some() {
                report.push(
                    "work_role_invalid",
                    "current shaping pointer is only allowed on Epic, Story, or Ticket nodes",
                );
            }
        }
    }

    if let Some(shaping) = &node.shaping {
        validate_shaping_pointer(node, shaping, &mut report);
    }

    report
}

pub fn validate_node_contract_result(node: &Node, mode: ContractValidationMode) -> PulseResult<()> {
    validate_node_contract(node, mode).into_result()
}

pub fn stable_contract_code(code: &str) -> &'static str {
    stable_code(code)
}

pub fn validate_public_create_classification(
    kind: WorkKind,
    classification: &PublicCreateClassification,
) -> PulseResult<()> {
    if kind != WorkKind::Ticket {
        if classification.any_present() {
            return Err(PulseError::validation(
                "work_classification_not_allowed",
                "role, risk, and materialization classification flags are only valid for Ticket creation",
            ));
        }
        return Ok(());
    }

    match (classification.role, classification.risk, classification.materialization) {
        (Some(_), Some(risk), Some(materialization))
            if risk.is_assessed() && materialization.is_assessed() => Ok(()),
        (Some(_), Some(_), Some(_)) => Err(PulseError::validation(
            "risk_materialization_unassessed",
            "public Ticket creation requires assessed risk and materialization; unassessed is only valid for canonical draft storage",
        )),
        _ => Err(PulseError::validation(
            "work_classification_missing",
            "public Ticket creation requires explicit --role, --risk, and --materialization",
        )),
    }
}

fn validate_non_ticket_contract_fields(node: &Node, report: &mut ContractValidationReport) {
    if node.role.is_some()
        || node.risk.is_some()
        || node.materialization.is_some()
        || node.qa.is_some()
        || node.implementation.is_some()
        || node.decision_work.is_some()
    {
        report.push(
            "work_role_invalid",
            "role, risk, materialization, QA, implementation, and decision_work are Ticket-only fields",
        );
    }
}

fn validate_ticket_contract(
    node: &Node,
    mode: ContractValidationMode,
    report: &mut ContractValidationReport,
) {
    let Some(role) = node.role else {
        report.push("work_role_invalid", "Ticket nodes must declare a role");
        return;
    };
    let Some(risk) = node.risk else {
        report.push(
            "work_classification_missing",
            "Ticket nodes must declare risk",
        );
        return;
    };
    let Some(materialization) = node.materialization else {
        report.push(
            "work_classification_missing",
            "Ticket nodes must declare materialization",
        );
        return;
    };
    let Some(qa) = &node.qa else {
        report.push(
            "qa_impact_unknown",
            "Ticket nodes must carry QA impact metadata",
        );
        return;
    };

    if mode == ContractValidationMode::PublicCreate
        && (!risk.is_assessed() || !materialization.is_assessed())
    {
        report.push(
            "risk_materialization_unassessed",
            "public Ticket creation requires assessed risk and materialization",
        );
    }

    if node.implementation.is_some() && node.decision_work.is_some() {
        report.push(
            "work_role_invalid",
            "implementation and decision_work contracts are mutually exclusive",
        );
    }

    let require_role_contract = mode == ContractValidationMode::Completeness;
    match role {
        TicketRole::Implementation => {
            if node.decision_work.is_some() {
                report.push(
                    "work_role_invalid",
                    "implementation role must not carry a decision_work contract",
                );
            }
            match &node.implementation {
                Some(implementation) => {
                    report.extend(validate_implementation_contract(
                        implementation,
                        materialization,
                        node,
                    ));
                    validate_qa_impact(qa, Some(implementation), mode, report);
                }
                None if require_role_contract => {
                    report.push(
                        "implementation_contract_missing",
                        "complete implementation Ticket requires an implementation contract",
                    );
                    validate_qa_impact(qa, None, mode, report);
                }
                None => validate_qa_impact(qa, None, mode, report),
            }
        }
        TicketRole::DecisionWork => {
            if node.implementation.is_some() {
                report.push(
                    "work_role_invalid",
                    "decision_work role must not carry an implementation contract",
                );
            }
            match &node.decision_work {
                Some(decision_work) => {
                    report.extend(validate_decision_work_contract(decision_work));
                    validate_qa_impact(qa, None, mode, report);
                }
                None if require_role_contract => {
                    report.push(
                        "decision_work_contract_missing",
                        "complete decision-work Ticket requires a decision_work contract",
                    );
                    validate_qa_impact(qa, None, mode, report);
                }
                None => validate_qa_impact(qa, None, mode, report),
            }
        }
    }
}

fn validate_implementation_contract(
    contract: &ImplementationContract,
    materialization: Materialization,
    node: &Node,
) -> ContractValidationReport {
    let mut report = ContractValidationReport::ok();
    validate_non_empty_bounded(
        &contract.objective,
        "implementation_contract_invalid",
        "implementation objective must be non-empty and bounded",
        MAX_LONG_TEXT,
        &mut report,
    );
    validate_non_empty_bounded(
        &contract.current_behavior,
        "implementation_contract_invalid",
        "current_behavior must be non-empty and bounded",
        MAX_LONG_TEXT,
        &mut report,
    );
    validate_non_empty_bounded(
        &contract.target_behavior,
        "implementation_contract_invalid",
        "target_behavior must be non-empty and bounded",
        MAX_LONG_TEXT,
        &mut report,
    );
    validate_slugish(
        &contract.verification_profile,
        "implementation_contract_invalid",
        "verification_profile must be a bounded profile identifier",
        &mut report,
    );

    match &contract.brief {
        Some(brief) => validate_content_ref(
            brief,
            Some(&node.content_dir),
            "implementation_brief_hash_stale",
            &mut report,
        ),
        None => report.push(
            "implementation_brief_missing",
            "implementation contract requires a content-bound brief reference",
        ),
    }

    if matches!(
        contract.mode,
        ImplementationMode::Guided | ImplementationMode::Locked
    ) {
        let anchors = anchors_for_surface(contract, contract.work_surface);
        if anchors.is_empty() {
            report.push(
                "implementation_anchor_missing",
                "guided or locked implementation requires at least one typed anchor/reference for its work surface",
            );
        }
    }

    validate_surface_refs("code_anchors", &contract.code_anchors, &mut report);
    validate_surface_refs(
        "documentation_anchors",
        &contract.documentation_anchors,
        &mut report,
    );
    validate_surface_refs(
        "configuration_anchors",
        &contract.configuration_anchors,
        &mut report,
    );
    validate_surface_refs("data_anchors", &contract.data_anchors, &mut report);
    validate_surface_refs("research_refs", &contract.research_refs, &mut report);

    validate_items(
        "required_changes",
        &contract.required_changes,
        false,
        "implementation_contract_invalid",
        &mut report,
    );
    validate_items(
        "invariants",
        &contract.invariants,
        materialization.requires_invariant(),
        "implementation_invariant_missing",
        &mut report,
    );
    validate_items(
        "acceptance",
        &contract.acceptance,
        true,
        "implementation_acceptance_missing",
        &mut report,
    );
    validate_items(
        "implementation_freedom",
        &contract.implementation_freedom,
        false,
        "implementation_freedom_missing",
        &mut report,
    );
    validate_unique_texts("scope.included", &contract.scope.included, &mut report);
    validate_unique_texts("scope.excluded", &contract.scope.excluded, &mut report);

    if contract.mode == ImplementationMode::Locked
        && contract.required_decisions.is_empty()
        && contract.shared_approach_refs.is_empty()
    {
        report.push(
            "required_decision_missing",
            "locked implementation requires at least one required Decision or shared approach reference",
        );
    }

    if contract.required_decisions.len() > MAX_COLLECTION {
        report.push(
            "required_decision_missing",
            "required_decisions exceeds the bounded collection limit",
        );
    }
    let mut decision_ids = BTreeSet::new();
    for decision in &contract.required_decisions {
        if !decision_ids.insert(&decision.id) {
            report.push(
                "required_decision_missing",
                format!("duplicate required Decision reference {}", decision.id),
            );
        }
        match kind_for_id(&decision.id) {
            Ok(WorkKind::Decision) => {}
            _ => report.push(
                "required_decision_missing",
                format!("required Decision id must use DEC prefix: {}", decision.id),
            ),
        }
        if decision.contract_revision < 1 {
            report.push(
                "required_decision_revision_stale",
                "required Decision contract_revision must be >= 1",
            );
        }
        validate_receipt_ref_with_codes(
            &decision.acceptance_receipt,
            "decision_acceptance_missing",
            "decision_acceptance_stale",
            &mut report,
        );
    }

    if contract.shared_approach_refs.len() > MAX_COLLECTION {
        report.push(
            "implementation_contract_invalid",
            "shared_approach_refs exceeds the bounded collection limit",
        );
    }
    let mut approach_keys = BTreeSet::new();
    for approach in &contract.shared_approach_refs {
        if !approach_keys.insert((&approach.owner.id, &approach.path)) {
            report.push(
                "implementation_contract_invalid",
                format!("duplicate shared approach reference {}", approach.path),
            );
        }
        validate_work_ref(&approach.owner, None, &mut report);
        validate_path(
            &approach.path,
            None,
            "implementation_contract_invalid",
            &mut report,
        );
        validate_hash(
            &approach.content_hash,
            "implementation_brief_hash_stale",
            "shared approach content_hash must be sha256:<hex>",
            &mut report,
        );
    }

    validate_unique_enums(
        "expected_evidence",
        &contract.expected_evidence,
        "implementation_contract_invalid",
        &mut report,
    );
    validate_unique_enums(
        "expected_handoff",
        &contract.expected_handoff,
        "implementation_contract_invalid",
        &mut report,
    );

    report
}

fn anchors_for_surface(contract: &ImplementationContract, surface: WorkSurface) -> &[SurfaceRef] {
    match surface {
        WorkSurface::Code => &contract.code_anchors,
        WorkSurface::Documentation => &contract.documentation_anchors,
        WorkSurface::Configuration => &contract.configuration_anchors,
        WorkSurface::Data => &contract.data_anchors,
        WorkSurface::Research => &contract.research_refs,
    }
}

fn validate_decision_work_contract(contract: &DecisionWorkContract) -> ContractValidationReport {
    let mut report = ContractValidationReport::ok();
    validate_destination_owner(&contract.destination_owner, &mut report);
    if !is_portable_branch_id(&contract.branch_id) {
        report.push(
            "decision_work_branch_missing",
            "decision-work branch_id must be a 1-64 character portable uppercase branch identifier starting with BR- and ending with an uppercase letter or digit",
        );
    }
    validate_non_empty_bounded(
        &contract.question,
        "decision_work_question_invalid",
        "decision-work question must be precise and non-empty",
        MAX_LONG_TEXT,
        &mut report,
    );
    validate_non_empty_bounded(
        &contract.expected_output,
        "decision_work_question_invalid",
        "decision-work expected_output must be non-empty",
        MAX_LONG_TEXT,
        &mut report,
    );
    validate_unique_enums(
        "expected_evidence",
        &contract.expected_evidence,
        "decision_work_question_invalid",
        &mut report,
    );
    if let Some(target) = &contract.resolution_target {
        match target.kind {
            ResolutionTargetKind::Decision => match kind_for_id(&target.id) {
                Ok(WorkKind::Decision) => {}
                _ => report.push(
                    "decision_work_question_invalid",
                    "decision resolution_target must use a DEC id",
                ),
            },
            ResolutionTargetKind::Work => {
                if let Err(error) = validate_work_id(&target.id) {
                    report.push("decision_work_question_invalid", error.to_string());
                }
            }
            ResolutionTargetKind::Evidence => validate_non_empty_bounded(
                &target.id,
                "decision_work_question_invalid",
                "evidence resolution_target id must be non-empty",
                MAX_ID,
                &mut report,
            ),
        }
    }
    validate_receipt_id_with_code(
        &contract.provenance.shaping_receipt,
        "shaping_receipt_missing",
        &mut report,
    );
    if let Some(fog_id) = &contract.provenance.fog_id {
        validate_stable_id(
            "decision_work_question_invalid",
            fog_id,
            "fog provenance id must be a stable identifier",
            &mut report,
        );
    }
    report
}

fn validate_qa_impact(
    qa: &QaMetadata,
    implementation: Option<&ImplementationContract>,
    mode: ContractValidationMode,
    report: &mut ContractValidationReport,
) {
    let rationale_present = qa
        .impact
        .rationale
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    validate_case_ids(&qa.impact.affected_case_ids, report);
    match qa.impact.posture {
        QaImpactPosture::Unknown => {
            if mode == ContractValidationMode::Completeness {
                report.push(
                    "qa_impact_unknown",
                    "complete Ticket requires assessed QA impact metadata",
                );
            }
            if rationale_present
                || qa.impact.behavioral_owner.is_some()
                || !qa.impact.affected_case_ids.is_empty()
            {
                report.push(
                    "qa_impact_invalid",
                    "unknown QA impact must not carry rationale, owner, or case ids",
                );
            }
        }
        QaImpactPosture::None => {
            if !rationale_present {
                report.push("qa_impact_invalid", "qa=none requires a rationale");
            }
            if let Some(contract) = implementation {
                match contract.semantic_impact {
                    ImplementationSemanticImpact::NoBehaviorOrPublicRiskChange => {}
                    ImplementationSemanticImpact::BehaviorOrPublicRiskChange => report.push(
                        "qa_impact_invalid",
                        "qa=none requires semantic_impact=no_behavior_or_public_risk_change",
                    ),
                }
            }
            if qa.impact.behavioral_owner.is_some() || !qa.impact.affected_case_ids.is_empty() {
                report.push(
                    "qa_impact_invalid",
                    "qa=none must not carry behavioral owner or case ids",
                );
            }
        }
        QaImpactPosture::CoveredByStoryClose => {
            if !rationale_present {
                report.push(
                    "qa_impact_invalid",
                    "covered_by_story_close requires a rationale",
                );
            }
            match qa.impact.behavioral_owner.as_deref() {
                Some(owner) => match kind_for_id(owner) {
                    Ok(WorkKind::Story) => {}
                    _ => report.push(
                        "qa_impact_invalid",
                        "covered_by_story_close behavioral_owner must be a Story id",
                    ),
                },
                None => report.push(
                    "qa_impact_invalid",
                    "covered_by_story_close requires a behavioral_owner Story id",
                ),
            }
        }
        QaImpactPosture::Required => {
            match qa.impact.behavioral_owner.as_deref() {
                Some(owner) => match kind_for_id(owner) {
                    Ok(WorkKind::Story) => {}
                    _ => report.push(
                        "qa_impact_invalid",
                        "required QA impact behavioral_owner must be a Story id",
                    ),
                },
                None => report.push(
                    "qa_impact_invalid",
                    "required QA impact requires a behavioral_owner Story id",
                ),
            }
            if qa.impact.affected_case_ids.is_empty() {
                report.push(
                    "qa_impact_invalid",
                    "required QA impact requires at least one affected case id",
                );
            }
        }
    }
}

fn validate_shaping_pointer(
    node: &Node,
    shaping: &ShapingPointer,
    report: &mut ContractValidationReport,
) {
    validate_receipt_ref(&shaping.receipt, report);
    if shaping.applied_by.trim().is_empty() {
        report.push(
            "shaping_receipt_missing",
            "shaping applied_by must be non-empty",
        );
    }
    if let Some(map) = &shaping.map {
        validate_path(
            &map.path,
            Some(&node.content_dir),
            "shaping_map_path_unsafe",
            report,
        );
        if map.revision < 1 {
            report.push(
                "shaping_map_revision_stale",
                "shaping map revision must be >= 1",
            );
        }
        validate_hash(
            &map.content_hash,
            "shaping_map_content_stale",
            "shaping map content_hash must be sha256:<hex>",
            report,
        );
    }
}

fn validate_content_ref(
    content: &ContentRef,
    required_prefix: Option<&str>,
    hash_code: &'static str,
    report: &mut ContractValidationReport,
) {
    validate_path(
        &content.path,
        required_prefix,
        "implementation_contract_invalid",
        report,
    );
    validate_hash(
        &content.content_hash,
        hash_code,
        "content_hash must be sha256:<hex>",
        report,
    );
}

fn validate_surface_refs(
    field: &'static str,
    refs: &[SurfaceRef],
    report: &mut ContractValidationReport,
) {
    if refs.len() > MAX_COLLECTION {
        report.push(
            "implementation_contract_invalid",
            format!("{field} exceeds the bounded collection limit"),
        );
    }
    let mut seen = BTreeSet::new();
    for reference in refs {
        validate_path(
            &reference.path,
            None,
            "implementation_contract_invalid",
            report,
        );
        if let Some(symbol) = &reference.symbol {
            validate_non_empty_bounded(
                symbol,
                "implementation_contract_invalid",
                "anchor symbol must be non-empty and bounded",
                MAX_SHORT_TEXT,
                report,
            );
        }
        if let Some(hash) = &reference.content_hash {
            validate_hash(
                hash,
                "implementation_brief_hash_stale",
                "anchor content_hash must be sha256:<hex>",
                report,
            );
        }
        if !seen.insert((&reference.path, &reference.symbol)) {
            report.push(
                "implementation_contract_invalid",
                format!("{field} contains duplicate anchor {}", reference.path),
            );
        }
    }
}

fn validate_items(
    field: &'static str,
    items: &[ContractItem],
    required: bool,
    missing_code: &'static str,
    report: &mut ContractValidationReport,
) {
    if required && items.is_empty() {
        report.push(missing_code, format!("{field} requires at least one item"));
    }
    if items.len() > MAX_COLLECTION {
        report.push(
            missing_code,
            format!("{field} exceeds the bounded collection limit"),
        );
    }
    let mut seen = BTreeSet::new();
    for item in items {
        validate_stable_id(
            missing_code,
            &item.id,
            format!("{field} item id must be stable and bounded"),
            report,
        );
        validate_non_empty_bounded(
            &item.summary,
            missing_code,
            format!("{field} item summary must be non-empty and bounded"),
            MAX_SHORT_TEXT,
            report,
        );
        if !seen.insert(&item.id) {
            report.push(
                missing_code,
                format!("{field} contains duplicate item id {}", item.id),
            );
        }
    }
}

fn validate_work_ref(
    reference: &RevisionedWorkRef,
    expected_kind: Option<WorkKind>,
    report: &mut ContractValidationReport,
) {
    match (kind_for_id(&reference.id), expected_kind) {
        (Ok(kind), Some(expected)) if kind == expected => {}
        (Ok(_), Some(_)) | (Err(_), _) => report.push(
            "implementation_contract_invalid",
            format!("work reference id has unexpected kind: {}", reference.id),
        ),
        (Ok(_), None) => {}
    }
    if reference.contract_revision < 1 {
        report.push(
            "contract_revision_invalid",
            "referenced contract_revision must be >= 1",
        );
    }
}

fn validate_destination_owner(
    reference: &RevisionedWorkRef,
    report: &mut ContractValidationReport,
) {
    match kind_for_id(&reference.id) {
        Ok(WorkKind::Epic | WorkKind::Story) => {}
        _ => report.push(
            "decision_work_destination_invalid",
            "decision-work destination_owner must be an Epic or Story id",
        ),
    }
    if reference.contract_revision < 1 {
        report.push(
            "decision_work_destination_invalid",
            "decision-work destination_owner contract_revision must be >= 1",
        );
    }
}

fn validate_receipt_ref(reference: &ReceiptRef, report: &mut ContractValidationReport) {
    validate_receipt_ref_with_codes(
        reference,
        "shaping_receipt_missing",
        "shaping_receipt_hash_mismatch",
        report,
    );
}

fn validate_receipt_ref_with_codes(
    reference: &ReceiptRef,
    id_code: &'static str,
    hash_code: &'static str,
    report: &mut ContractValidationReport,
) {
    validate_receipt_id_with_code(&reference.id, id_code, report);
    validate_hash(
        &reference.hash,
        hash_code,
        "receipt hash must be sha256:<hex>",
        report,
    );
}

fn validate_receipt_id_with_code(
    value: &str,
    code: &'static str,
    report: &mut ContractValidationReport,
) {
    if value.trim().is_empty() || value.len() > 128 || !value.starts_with("rcpt_") {
        report.push(
            code,
            format!("receipt id must be a bounded rcpt_ identifier: {value}"),
        );
    }
}

fn validate_hash(
    value: &str,
    code: &'static str,
    message: &'static str,
    report: &mut ContractValidationReport,
) {
    let Some(hex) = value.strip_prefix("sha256:") else {
        report.push(code, message);
        return;
    };
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        report.push(code, message);
    }
}

fn validate_path(
    value: &str,
    required_prefix: Option<&str>,
    code: &'static str,
    report: &mut ContractValidationReport,
) {
    match crate::storage::safe_repo_relative(value) {
        Ok(path) => {
            if let Some(prefix) = required_prefix {
                if !path.starts_with(Path::new(prefix)) {
                    report.push(
                        code,
                        format!("path {value} must live under content_dir {prefix}"),
                    );
                }
            }
        }
        Err(error) => report.push(code, error.to_string()),
    }
}

fn validate_unique_texts(
    field: &'static str,
    values: &[String],
    report: &mut ContractValidationReport,
) {
    if values.len() > MAX_COLLECTION {
        report.push(
            "implementation_contract_invalid",
            format!("{field} exceeds the bounded collection limit"),
        );
    }
    let mut seen = BTreeSet::new();
    for value in values {
        validate_non_empty_bounded(
            value,
            "implementation_contract_invalid",
            format!("{field} entries must be non-empty and bounded"),
            MAX_SHORT_TEXT,
            report,
        );
        if !seen.insert(value) {
            report.push(
                "implementation_contract_invalid",
                format!("{field} contains duplicate entry {value}"),
            );
        }
    }
}

fn validate_case_ids(values: &[String], report: &mut ContractValidationReport) {
    if values.len() > MAX_COLLECTION {
        report.push(
            "qa_impact_invalid",
            "affected_case_ids exceeds the bounded collection limit",
        );
    }
    let mut seen = BTreeSet::new();
    for value in values {
        if !is_portable_case_id(value) {
            report.push(
                "qa_impact_invalid",
                format!(
                    "affected_case_id must be a 1-64 character portable uppercase case identifier: {value}"
                ),
            );
        }
        if !seen.insert(value) {
            report.push(
                "qa_impact_invalid",
                format!("affected_case_ids contains duplicate case id {value}"),
            );
        }
    }
}

fn is_portable_case_id(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_ID {
        return false;
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !is_case_id_boundary_char(first) {
        return false;
    }
    let mut last = first;
    for character in chars {
        if !is_case_id_char(character) {
            return false;
        }
        last = character;
    }
    is_case_id_boundary_char(last)
}

fn is_case_id_char(character: char) -> bool {
    character.is_ascii_uppercase() || character.is_ascii_digit() || character == '-'
}

fn is_case_id_boundary_char(character: char) -> bool {
    character.is_ascii_uppercase() || character.is_ascii_digit()
}

fn is_portable_branch_id(value: &str) -> bool {
    value.len() <= MAX_ID && value.strip_prefix("BR-").is_some_and(is_portable_case_id)
}

fn validate_unique_enums<T>(
    field: &'static str,
    values: &[T],
    code: &'static str,
    report: &mut ContractValidationReport,
) where
    T: Ord + std::fmt::Debug,
{
    if values.len() > MAX_COLLECTION {
        report.push(
            code,
            format!("{field} exceeds the bounded collection limit"),
        );
    }
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            report.push(code, format!("{field} contains duplicate entry {value:?}"));
        }
    }
}

fn validate_stable_id(
    code: &'static str,
    value: &str,
    message: impl Into<String>,
    report: &mut ContractValidationReport,
) {
    if value.len() < 2
        || value.len() > MAX_ID
        || value.starts_with('-')
        || value.ends_with('-')
        || !value
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
    {
        report.push(code, message.into());
    }
}

fn validate_slugish(
    value: &str,
    code: &'static str,
    message: impl Into<String>,
    report: &mut ContractValidationReport,
) {
    if value.trim().is_empty()
        || value.len() > MAX_ID
        || value.starts_with('-')
        || value.ends_with('-')
        || !value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        report.push(code, message.into());
    }
}

fn validate_non_empty_bounded(
    value: &str,
    code: &'static str,
    message: impl Into<String>,
    max: usize,
    report: &mut ContractValidationReport,
) {
    if value.trim().is_empty() || value.len() > max {
        report.push(code, message.into());
    }
}
