//! Deterministic readiness composition.
//!
//! This module is a *pure* gate-family evaluator. It consumes a coherent typed
//! snapshot ([`ReadinessInputs`]) that the graph store assembles under the
//! repository fence, and produces one stable [`ReadinessReport`] with
//! explainable per-family statuses, stable reason codes and a *narrow*
//! readiness fingerprint.
//!
//! Boundary rules (see `proposals/phase1-slice7-shaping-readiness-frontier.md`):
//!
//! * readiness never reads raw JSON or performs filesystem I/O — the store owns
//!   coherent snapshot capture;
//! * readiness never mutates state;
//! * the structural executability module must not import readiness (one-way
//!   dependency: readiness consumes the structural report, never the reverse);
//! * only implementation Tickets can become `ready` under
//!   `phase1_contract_readiness_v1`.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::docs::applicability::ApplicableDocsReport;
use crate::evidence::model::{
    BranchCriticality, BranchDisposition, DecisionAcceptancePayload, ReceiptKind, ReceiptResult,
    ShapeMode, ShapingValidationPayload,
};
use crate::graph::contract::{
    validate_node_contract, ContractValidationMode, ImplementationMode, QaImpactPosture, TicketRole,
};
use crate::graph::executability::{StructuralExecutabilityReport, StructuralState};
use crate::graph::node::{Node, NodeStatus};
use crate::id::WorkKind;
use crate::policy::AuthorityPolicyReport;
use crate::PulseResult;

/// Current readiness profile identifier. Only implementation Tickets can be
/// ready under this profile; decision-work Tickets use decision-frontier
/// eligibility instead.
pub const READINESS_PROFILE: &str = "phase1_contract_readiness_v1";

/// Profile identifier recorded on `work.node.transitioned` events that pass the
/// `draft -> shaped` (and blocked resume) shaping gate.
pub const SHAPED_GATE_PROFILE: &str = "phase1_shaped_v1";

pub const READINESS_SCHEMA_VERSION: u32 = 1;

/// Future gate families that Slice 7 intentionally does not evaluate. They are
/// reported as `not_evaluated` so consumers cannot mistake absence for passage.
pub const FUTURE_GATE_FAMILIES: &[(&str, u32)] = &[
    ("qa_baseline_and_cases", 3),
    ("lease", 2),
    ("source_workspace", 2),
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    Passed,
    Failed,
    Stale,
    NotApplicable,
    NotEvaluated,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessStatus {
    Ready,
    NotReady,
    Stale,
    Invalid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GateFamilyReport {
    pub family: String,
    pub status: GateStatus,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FutureGateFamily {
    pub family: String,
    pub owner_phase: u32,
    pub status: GateStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadinessSubject {
    pub id: String,
    pub revision: u64,
    pub contract_revision: u64,
    pub status: NodeStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadinessDestination {
    pub owner: String,
    pub receipt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_revision: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessReport {
    pub schema_version: u32,
    pub code: String,
    pub subject: ReadinessSubject,
    pub profile: String,
    pub status: ReadinessStatus,
    pub transition_eligible: bool,
    pub dispatch_authorized: bool,
    pub readiness_fingerprint: String,
    pub graph_fingerprint_observed: String,
    pub gate_families: Vec<GateFamilyReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<ReadinessDestination>,
    pub remaining_non_blocking_uncertainty: Vec<String>,
    pub future_gate_families: Vec<FutureGateFamily>,
    pub reason_codes: Vec<String>,
}

/// Which gate profile to evaluate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalProfile {
    /// `draft -> shaped` / blocked resume shaping gate.
    Shaped,
    /// Full `phase1_contract_readiness_v1` readiness gate (`shaped -> ready`).
    Ready,
}

/// Immutable snapshot of a current shaping receipt used by readiness.
#[derive(Debug, Clone)]
pub struct ShapingReceiptSnapshot {
    pub receipt_id: String,
    pub receipt_hash: String,
    pub payload: ShapingValidationPayload,
    /// Integrity/result/version checks pass for a current pointer.
    pub integrity_valid: bool,
    /// Content/source binding staleness codes from the evidence plane.
    pub binding_codes: Vec<String>,
    /// Map snapshot content still matches the bound hash.
    pub map_current: bool,
}

/// Immutable snapshot of a Decision acceptance proof referenced by an
/// implementation contract.
#[derive(Debug, Clone)]
pub struct DecisionProofSnapshot {
    pub decision_id: String,
    pub required_contract_revision: u64,
    pub receipt_id: String,
    pub receipt_hash: String,
    pub payload: DecisionAcceptancePayload,
    pub integrity_valid: bool,
    pub decision_node_present: bool,
    pub decision_terminal: bool,
    pub decision_contract_revision: u64,
    pub content_current: bool,
}

/// A single content binding (brief/map/shared approach/Decision prose) and its
/// current on-disk hash, used for content-reference currentness.
#[derive(Debug, Clone)]
pub struct ContentHashBinding {
    pub label: String,
    pub path: String,
    pub bound_hash: String,
    /// `None` when the bound content file is missing.
    pub current_hash: Option<String>,
}

/// Coherent typed snapshot consumed by the pure readiness evaluator. The graph
/// store assembles every field under the repository fence before evaluation.
pub struct ReadinessInputs<'a> {
    pub subject: &'a Node,
    pub graph_valid: bool,
    pub structural: &'a StructuralExecutabilityReport,
    pub shaping: Option<&'a ShapingReceiptSnapshot>,
    pub decision_proofs: Vec<DecisionProofSnapshot>,
    pub docs: &'a ApplicableDocsReport,
    pub authority: &'a AuthorityPolicyReport,
    pub content_bindings: Vec<ContentHashBinding>,
    pub graph_fingerprint: String,
}

impl GateStatus {
    fn is_passing(self) -> bool {
        matches!(self, GateStatus::Passed | GateStatus::NotApplicable)
    }
}

/// Evaluate readiness for the given coherent snapshot under the requested
/// profile. Pure: no I/O, no mutation.
pub fn evaluate(inputs: &ReadinessInputs, profile: EvalProfile) -> PulseResult<ReadinessReport> {
    let mut families = Vec::new();

    let active = active_families(profile);
    let mut evaluator = FamilyEvaluator::new(inputs, profile);

    for (family, active_for_profile) in ALL_FAMILIES {
        let status = if *active_for_profile {
            evaluator.evaluate(family)?
        } else {
            GateStatus::NotEvaluated
        };
        let reason_codes = evaluator.take_codes(family);
        families.push(GateFamilyReport {
            family: family.to_string(),
            status,
            reason_codes,
        });
    }

    let transition_eligible = families
        .iter()
        .filter(|f| active.contains(&f.family.as_str()))
        .all(|f| f.status.is_passing());

    let any_failed = active_families_status(&families, &active, |s| {
        matches!(s, GateStatus::Failed | GateStatus::Unavailable)
    });
    let any_stale = active_families_status(&families, &active, |s| s == GateStatus::Stale);

    let subject_is_ready_lifecycle = inputs.subject.status == NodeStatus::Ready;
    let status = if transition_eligible {
        ReadinessStatus::Ready
    } else if any_stale {
        ReadinessStatus::Stale
    } else if subject_is_ready_lifecycle && any_failed {
        // A `ready` node whose current inputs no longer pass is stale, not
        // silently demoted. Status is retained for audit; the report flags it.
        ReadinessStatus::Stale
    } else {
        ReadinessStatus::NotReady
    };

    let mut reason_codes: Vec<String> = families
        .iter()
        .flat_map(|f| f.reason_codes.iter().cloned())
        .collect();
    // A `ready` node whose current inputs no longer pass is flagged stale, not
    // silently demoted. Status is retained for audit; the reason surfaces the
    // divergence regardless of which family drove it (failed/stale/unavailable).
    if subject_is_ready_lifecycle && !transition_eligible {
        reason_codes.push("ready_state_stale".to_string());
    }
    reason_codes.sort();
    reason_codes.dedup();

    let destination = destination_projection(inputs);
    let remaining_non_blocking_uncertainty = remaining_uncertainty(inputs);
    let readiness_fingerprint = fingerprint(inputs, profile)?;
    let future_gate_families = FUTURE_GATE_FAMILIES
        .iter()
        .map(|(family, owner_phase)| FutureGateFamily {
            family: family.to_string(),
            owner_phase: *owner_phase,
            status: GateStatus::NotEvaluated,
        })
        .collect();

    let code = match status {
        ReadinessStatus::Ready => "ready",
        ReadinessStatus::NotReady => "not_ready",
        ReadinessStatus::Stale => "stale",
        ReadinessStatus::Invalid => "invalid",
    }
    .to_string();

    Ok(ReadinessReport {
        schema_version: READINESS_SCHEMA_VERSION,
        code,
        subject: ReadinessSubject {
            id: inputs.subject.id.clone(),
            revision: inputs.subject.revision,
            contract_revision: inputs.subject.contract_revision,
            status: inputs.subject.status,
        },
        profile: profile_name(profile).to_string(),
        status,
        transition_eligible,
        dispatch_authorized: false,
        readiness_fingerprint,
        graph_fingerprint_observed: inputs.graph_fingerprint.clone(),
        gate_families: families,
        destination,
        remaining_non_blocking_uncertainty,
        future_gate_families,
        reason_codes,
    })
}

fn profile_name(profile: EvalProfile) -> &'static str {
    match profile {
        EvalProfile::Shaped => SHAPED_GATE_PROFILE,
        EvalProfile::Ready => READINESS_PROFILE,
    }
}

fn active_families(profile: EvalProfile) -> Vec<&'static str> {
    ALL_FAMILIES
        .iter()
        .filter_map(|(family, active)| active.then_some(*family))
        .filter(|family| match profile {
            EvalProfile::Shaped => SHAPED_FAMILIES.contains(family),
            EvalProfile::Ready => true,
        })
        .collect()
}

fn active_families_status(
    families: &[GateFamilyReport],
    active: &[&str],
    predicate: impl Fn(GateStatus) -> bool,
) -> bool {
    families
        .iter()
        .any(|f| active.contains(&f.family.as_str()) && predicate(f.status))
}

const SHAPED_FAMILIES: &[&str] = &[
    "shaping_receipt_integrity",
    "shaping_bindings",
    "branch_dispositions",
    "destination_and_map",
    "bounded_fog",
    "authority",
];

/// All gate families in fixed evaluation order, paired with whether the family
/// is part of the *full readiness* profile. The shaped profile further filters
/// to [`SHAPED_FAMILIES`].
const ALL_FAMILIES: &[(&str, bool)] = &[
    ("graph_validity", true),
    ("work_kind_and_role", true),
    ("lifecycle_eligibility", true),
    ("structural_executability", true),
    ("implementation_contract", true),
    ("required_decisions", true),
    ("shaping_receipt_integrity", true),
    ("shaping_bindings", true),
    ("branch_dispositions", true),
    ("destination_and_map", true),
    ("bounded_fog", true),
    ("authority", true),
    ("documentation_impact", true),
    ("applicable_documents", true),
    ("qa_impact", true),
    ("content_reference_integrity", true),
];

struct FamilyEvaluator<'a> {
    inputs: &'a ReadinessInputs<'a>,
    codes: Vec<String>,
}

impl<'a> FamilyEvaluator<'a> {
    fn new(inputs: &'a ReadinessInputs<'a>, _profile: EvalProfile) -> Self {
        Self {
            inputs,
            codes: Vec::new(),
        }
    }

    fn evaluate(&mut self, family: &str) -> PulseResult<GateStatus> {
        let status = match family {
            "graph_validity" => self.graph_validity(),
            "work_kind_and_role" => self.work_kind_and_role(),
            "lifecycle_eligibility" => self.lifecycle_eligibility(),
            "structural_executability" => self.structural_executability(),
            "implementation_contract" => self.implementation_contract()?,
            "required_decisions" => self.required_decisions(),
            "shaping_receipt_integrity" => self.shaping_receipt_integrity(),
            "shaping_bindings" => self.shaping_bindings(),
            "branch_dispositions" => self.branch_dispositions(),
            "destination_and_map" => self.destination_and_map(),
            "bounded_fog" => self.bounded_fog(),
            "authority" => self.authority(),
            "documentation_impact" => self.documentation_impact(),
            "applicable_documents" => self.applicable_documents(),
            "qa_impact" => self.qa_impact(),
            "content_reference_integrity" => self.content_reference_integrity(),
            _ => GateStatus::NotEvaluated,
        };
        Ok(status)
    }

    fn take_codes(&mut self, _family: &str) -> Vec<String> {
        std::mem::take(&mut self.codes)
    }

    fn note(&mut self, code: &str) {
        self.codes.push(code.to_string());
    }
}

impl FamilyEvaluator<'_> {
    fn graph_validity(&mut self) -> GateStatus {
        if self.inputs.graph_valid {
            GateStatus::Passed
        } else {
            self.note("graph_invalid");
            GateStatus::Failed
        }
    }

    fn work_kind_and_role(&mut self) -> GateStatus {
        let node = self.inputs.subject;
        if node.kind != WorkKind::Ticket {
            self.note("work_role_invalid");
            return GateStatus::NotApplicable;
        }
        match node.role {
            Some(TicketRole::Implementation) => GateStatus::Passed,
            Some(TicketRole::DecisionWork) => {
                self.note("work_role_not_implementation");
                GateStatus::NotApplicable
            }
            None => {
                self.note("work_role_invalid");
                GateStatus::Failed
            }
        }
    }

    fn lifecycle_eligibility(&mut self) -> GateStatus {
        match self.inputs.subject.status {
            NodeStatus::Draft | NodeStatus::Shaped | NodeStatus::Ready => GateStatus::Passed,
            NodeStatus::Blocked => {
                self.note("lifecycle_blocked");
                GateStatus::Failed
            }
            NodeStatus::Active | NodeStatus::Verifying | NodeStatus::Rework => {
                self.note("lifecycle_in_execution");
                GateStatus::Failed
            }
            NodeStatus::Done | NodeStatus::Cancelled | NodeStatus::Superseded => {
                self.note("lifecycle_terminal");
                GateStatus::Failed
            }
        }
    }

    fn structural_executability(&mut self) -> GateStatus {
        let report = self.inputs.structural;
        match report.structural_state {
            StructuralState::Candidate => GateStatus::Passed,
            StructuralState::Blocked => {
                if report.hard_blockers.iter().any(|b| {
                    b.resolution != crate::graph::executability::BlockerResolution::Satisfied
                }) {
                    self.note("hard_blocker_open");
                }
                GateStatus::Failed
            }
            StructuralState::Paused => {
                self.note("structural_paused");
                GateStatus::Failed
            }
            StructuralState::Terminal => {
                self.note("structural_terminal");
                GateStatus::Failed
            }
            StructuralState::NotExecutableKind => {
                self.note("not_executable_kind");
                GateStatus::NotApplicable
            }
            StructuralState::Invalid => {
                self.note("structural_invalid");
                GateStatus::Failed
            }
        }
    }

    fn implementation_contract(&mut self) -> PulseResult<GateStatus> {
        let node = self.inputs.subject;
        if node.role != Some(TicketRole::Implementation) {
            return Ok(GateStatus::NotApplicable);
        }
        let contract = match &node.implementation {
            Some(contract) => contract,
            None => {
                self.note("implementation_contract_missing");
                return Ok(GateStatus::Failed);
            }
        };
        // Structural canonical validation catches missing brief/anchors/
        // invariants/acceptance and hash shape problems.
        let report = validate_node_contract(node, ContractValidationMode::CanonicalStorage);
        if !report.valid {
            for finding in &report.errors {
                self.note(finding.code.as_str());
            }
            return Ok(GateStatus::Failed);
        }
        // Completeness: locked work must have a Decision/approach proof that the
        // required_decisions family will resolve; surface it here only when the
        // contract literally has none and mode demands it.
        if contract.mode == ImplementationMode::Locked
            && contract.required_decisions.is_empty()
            && contract.shared_approach_refs.is_empty()
        {
            self.note("required_decision_missing");
            return Ok(GateStatus::Failed);
        }
        Ok(GateStatus::Passed)
    }

    fn required_decisions(&mut self) -> GateStatus {
        let node = self.inputs.subject;
        let contract = match &node.implementation {
            Some(contract) => contract,
            None => return GateStatus::NotApplicable,
        };
        if contract.required_decisions.is_empty() {
            return GateStatus::Passed;
        }
        let mut worst = GateStatus::Passed;
        for decision in &contract.required_decisions {
            let proof = self
                .inputs
                .decision_proofs
                .iter()
                .find(|proof| proof.decision_id == decision.id);
            let Some(proof) = proof else {
                self.note("decision_acceptance_missing");
                worst = worst.or_unavailable();
                continue;
            };
            if !proof.integrity_valid {
                self.note("decision_acceptance_stale");
                worst = worst.or_stale();
                continue;
            }
            if !proof.decision_node_present {
                self.note("required_decision_missing");
                worst = worst.or_failed();
                continue;
            }
            if proof.decision_terminal {
                self.note("required_decision_superseded");
                worst = worst.or_failed();
                continue;
            }
            if proof.decision_contract_revision != decision.contract_revision {
                self.note("required_decision_revision_stale");
                worst = worst.or_stale();
                continue;
            }
            if !proof.content_current {
                self.note("decision_acceptance_stale");
                worst = worst.or_stale();
            }
        }
        worst
    }

    fn shaping_receipt_integrity(&mut self) -> GateStatus {
        let Some(shaping) = self.inputs.shaping else {
            self.note("shaping_receipt_missing");
            return GateStatus::Failed;
        };
        let payload = &shaping.payload;
        if payload.payload_version != 1 {
            self.note("shaping_receipt_version_ineligible");
            return GateStatus::Failed;
        }
        if !shaping.integrity_valid {
            self.note("shaping_receipt_hash_mismatch");
            return GateStatus::Stale;
        }
        // Materialization/shape-mode policy: persisted_map requires destination
        // and map; focused/concise may omit them.
        if payload.shape_mode == ShapeMode::PersistedMap {
            if payload.destination.is_none() {
                self.note("shaping_destination_missing");
                return GateStatus::Failed;
            }
            if payload.map.is_none() {
                self.note("shaping_map_required");
                return GateStatus::Failed;
            }
        }
        GateStatus::Passed
    }

    fn shaping_bindings(&mut self) -> GateStatus {
        let Some(shaping) = self.inputs.shaping else {
            return GateStatus::NotApplicable;
        };
        let payload = &shaping.payload;
        // Shaping currentness is by contract_revision: the receipt must bind the
        // subject's current contract revision.
        if payload.owning_work.contract_revision != self.inputs.subject.contract_revision {
            self.note("shaping_receipt_stale");
            return GateStatus::Stale;
        }
        if !shaping.binding_codes.is_empty() {
            for code in &shaping.binding_codes {
                self.note(crate::evidence::receipt::code_to_static(code));
            }
            return GateStatus::Stale;
        }
        if let Some(map) = &payload.map {
            if !shaping.map_current {
                self.note("shaping_map_content_stale");
                return GateStatus::Stale;
            }
            let _ = map;
        }
        GateStatus::Passed
    }

    fn branch_dispositions(&mut self) -> GateStatus {
        let Some(shaping) = self.inputs.shaping else {
            return GateStatus::NotApplicable;
        };
        let payload = &shaping.payload;
        let subject = &self.inputs.subject.id;
        let implementation = self.inputs.subject.implementation.as_ref();
        let mut worst = GateStatus::Passed;
        for branch in &payload.branches {
            if !branch.affected_work.iter().any(|w| w == subject) {
                continue;
            }
            if branch.criticality == BranchCriticality::Critical {
                match &branch.disposition {
                    BranchDisposition::Blocking { .. } => {
                        self.note("shaping_blocking_branch_open");
                        worst = worst.or_failed();
                    }
                    BranchDisposition::Delegated { freedom_id, .. } => {
                        let valid = implementation
                            .map(|c| c.implementation_freedom.iter().any(|f| &f.id == freedom_id))
                            .unwrap_or(false);
                        if !valid {
                            self.note("shaping_delegation_exceeds_freedom");
                            worst = worst.or_failed();
                        }
                        if matches!(
                            implementation.map(|c| c.mode),
                            Some(ImplementationMode::Locked)
                        ) {
                            self.note("shaping_delegation_exceeds_freedom");
                            worst = worst.or_failed();
                        }
                    }
                    BranchDisposition::Deferred {
                        reason,
                        owner,
                        target_work,
                        trigger,
                        non_blocking_for,
                    } => {
                        if reason.trim().is_empty() {
                            self.note("shaping_defer_reason_missing");
                            worst = worst.or_failed();
                        }
                        if owner.trim().is_empty() {
                            self.note("shaping_defer_owner_missing");
                            worst = worst.or_failed();
                        }
                        if target_work.trim().is_empty() {
                            self.note("shaping_defer_target_missing");
                            worst = worst.or_failed();
                        }
                        if trigger.trim().is_empty() {
                            self.note("shaping_defer_trigger_missing");
                            worst = worst.or_failed();
                        }
                        if !non_blocking_for.iter().any(|w| w == subject) {
                            self.note("shaping_defer_not_non_blocking");
                            worst = worst.or_failed();
                        }
                    }
                    BranchDisposition::Resolved { resolution } => {
                        if resolution.gist.trim().is_empty() {
                            self.note("shaping_resolution_missing");
                            worst = worst.or_failed();
                        }
                    }
                    BranchDisposition::Rejected { reason, .. } => {
                        if reason.trim().is_empty() {
                            self.note("shaping_rejection_reason_missing");
                            worst = worst.or_failed();
                        }
                    }
                }
            }
        }
        worst
    }

    fn destination_and_map(&mut self) -> GateStatus {
        let Some(shaping) = self.inputs.shaping else {
            return GateStatus::NotApplicable;
        };
        let payload = &shaping.payload;
        if let Some(destination) = &payload.destination {
            if destination.summary.trim().is_empty() {
                self.note("shaping_destination_missing");
                return GateStatus::Failed;
            }
            if destination
                .exit_conditions
                .iter()
                .all(|c| c.trim().is_empty())
            {
                self.note("shaping_exit_condition_missing");
                return GateStatus::Failed;
            }
        }
        GateStatus::Passed
    }

    fn bounded_fog(&mut self) -> GateStatus {
        let Some(shaping) = self.inputs.shaping else {
            return GateStatus::NotApplicable;
        };
        for fog in &shaping.payload.fog {
            if fog.bounds.iter().all(|b| b.trim().is_empty()) {
                self.note("shaping_fog_unbounded");
                return GateStatus::Failed;
            }
            if fog.trigger.trim().is_empty() {
                self.note("shaping_fog_trigger_missing");
                return GateStatus::Failed;
            }
        }
        GateStatus::Passed
    }

    fn authority(&mut self) -> GateStatus {
        let report = self.inputs.authority;
        if !report.available {
            self.note("readiness_policy_missing");
            return GateStatus::Unavailable;
        }
        if !report.valid {
            self.note("readiness_policy_invalid");
            return GateStatus::Unavailable;
        }
        // Staleness: the shaping approver must still hold the kernel-derived
        // materialization grant. This catches a policy revocation after apply.
        if let Some(shaping) = self.inputs.shaping {
            let materialization = &shaping.payload.materialization;
            match crate::graph::shaping::materialization_approve_grant(materialization) {
                Ok(grant) => {
                    if !report.principals.iter().any(|principal| {
                        principal.kind == shaping.payload.approval.approved_by.kind
                            && principal.id == shaping.payload.approval.approved_by.id
                            && principal.grants.iter().any(|g| g == &grant)
                    }) {
                        self.note("readiness_authority_denied");
                        return GateStatus::Failed;
                    }
                }
                Err(_) => {
                    self.note("shaping_receipt_version_ineligible");
                    return GateStatus::Failed;
                }
            }
        }
        GateStatus::Passed
    }

    fn documentation_impact(&mut self) -> GateStatus {
        let posture = self.inputs.subject.documentation_posture();
        match posture {
            crate::graph::node::DocumentationImpactPosture::Unknown => {
                self.note("documentation_impact_unknown");
                GateStatus::Failed
            }
            crate::graph::node::DocumentationImpactPosture::Required
            | crate::graph::node::DocumentationImpactPosture::None
            | crate::graph::node::DocumentationImpactPosture::Deferred => GateStatus::Passed,
        }
    }

    fn applicable_documents(&mut self) -> GateStatus {
        let posture = self.inputs.subject.documentation_posture();
        let has_required = self
            .inputs
            .subject
            .documentation
            .as_ref()
            .map(|d| !d.impact.required_documents.is_empty())
            .unwrap_or(false);
        if !has_required && posture != crate::graph::node::DocumentationImpactPosture::Required {
            return GateStatus::NotApplicable;
        }
        let gate = &self.inputs.docs.gate;
        if gate.status == "complete" && gate.reason_codes.is_empty() {
            GateStatus::Passed
        } else {
            for code in &gate.reason_codes {
                self.note(code);
            }
            GateStatus::Failed
        }
    }

    fn qa_impact(&mut self) -> GateStatus {
        let qa = self
            .inputs
            .subject
            .qa
            .as_ref()
            .map(|q| q.impact.posture)
            .unwrap_or(QaImpactPosture::Unknown);
        match qa {
            QaImpactPosture::Unknown => {
                self.note("qa_impact_unknown");
                GateStatus::Failed
            }
            QaImpactPosture::None | QaImpactPosture::CoveredByStoryClose => GateStatus::Passed,
            QaImpactPosture::Required => {
                // Baseline/case resolver belongs to Phase 3. Until then the
                // gate is unavailable and the Ticket cannot transition ready.
                self.note("qa_baseline_resolver_unavailable");
                GateStatus::Unavailable
            }
        }
    }

    fn content_reference_integrity(&mut self) -> GateStatus {
        if self.inputs.content_bindings.is_empty() {
            return GateStatus::NotApplicable;
        }
        let mut worst = GateStatus::Passed;
        for binding in &self.inputs.content_bindings {
            match &binding.current_hash {
                Some(current) if current == &binding.bound_hash => {}
                Some(_) => {
                    self.note(content_stale_code(&binding.label));
                    worst = worst.or_stale();
                }
                None => {
                    self.note(content_missing_code(&binding.label));
                    worst = worst.or_failed();
                }
            }
        }
        worst
    }
}

fn content_stale_code(label: &str) -> &'static str {
    match label {
        "brief" => "implementation_brief_hash_stale",
        "map" => "shaping_map_content_stale",
        s if s.starts_with("decision:") => "decision_acceptance_stale",
        s if s.starts_with("shared_approach") => "implementation_brief_hash_stale",
        _ => "content_reference_stale",
    }
}

fn content_missing_code(label: &str) -> &'static str {
    match label {
        "brief" => "implementation_brief_missing",
        "map" => "shaping_map_missing",
        s if s.starts_with("decision:") => "decision_acceptance_missing",
        _ => "content_reference_missing",
    }
}

impl GateStatus {
    /// Combine two statuses by severity, keeping the worse one.
    /// Severity rank used to combine multiple findings within a family: the
    /// worst (highest) severity wins so a single definite failure is not masked
    /// by a later stale/unavailable finding, and vice-versa.
    fn severity(self) -> u8 {
        match self {
            GateStatus::Failed => 4,
            GateStatus::Unavailable => 3,
            GateStatus::Stale => 2,
            GateStatus::Passed => 1,
            GateStatus::NotApplicable | GateStatus::NotEvaluated => 0,
        }
    }

    /// Combine two statuses, keeping the more severe one.
    fn combine_worst(self, other: GateStatus) -> GateStatus {
        if self.severity() >= other.severity() {
            self
        } else {
            other
        }
    }

    fn or_failed(self) -> GateStatus {
        self.combine_worst(GateStatus::Failed)
    }

    fn or_stale(self) -> GateStatus {
        self.combine_worst(GateStatus::Stale)
    }

    fn or_unavailable(self) -> GateStatus {
        self.combine_worst(GateStatus::Unavailable)
    }
}

fn destination_projection(inputs: &ReadinessInputs) -> Option<ReadinessDestination> {
    let shaping = inputs.shaping?;
    Some(ReadinessDestination {
        owner: inputs.subject.id.clone(),
        receipt: shaping.receipt_id.clone(),
        map_revision: shaping.payload.map.as_ref().map(|m| m.revision),
    })
}

fn remaining_uncertainty(inputs: &ReadinessInputs) -> Vec<String> {
    let Some(shaping) = inputs.shaping else {
        return Vec::new();
    };
    let mut ids: Vec<String> = shaping.payload.fog.iter().map(|f| f.id.clone()).collect();
    ids.sort();
    ids.dedup();
    ids
}

/// Compute the narrow readiness fingerprint from explicit gate projections.
///
/// Excluded by design: subject normal revision, lifecycle status, status
/// reason, timestamps, shaping applied_at/by, unrelated graph nodes/edges,
/// events, cache/runtime state and the global graph fingerprint (which is
/// reported separately for audit only).
fn fingerprint(inputs: &ReadinessInputs, profile: EvalProfile) -> PulseResult<String> {
    let node = inputs.subject;
    let mut value = serde_json::Map::new();
    value.insert("profile".to_string(), json!(profile_name(profile)));
    value.insert(
        "subject".to_string(),
        json!({
            "id": node.id,
            "contract_revision": node.contract_revision,
            "role": node.role,
            "risk": node.risk,
            "materialization": node.materialization,
        }),
    );

    if let Some(doc) = &node.documentation {
        value.insert("documentation".to_string(), documentation_projection(doc));
    }
    if let Some(qa) = &node.qa {
        value.insert("qa".to_string(), qa_projection(qa));
    }
    if let Some(contract) = &node.implementation {
        value.insert(
            "implementation".to_string(),
            implementation_projection(contract),
        );
    }
    if let Some(contract) = &node.decision_work {
        value.insert(
            "decision_work".to_string(),
            decision_work_projection(contract),
        );
    }

    // Structural: relevant hard blocker edges + supersession replacement only.
    value.insert(
        "structural".to_string(),
        structural_projection(inputs.structural),
    );

    // Shaping receipt + branches/fog/map.
    if let Some(shaping) = inputs.shaping {
        value.insert("shaping".to_string(), shaping_projection(shaping));
    }

    // Required Decision acceptance proofs.
    let proofs: Vec<Value> = inputs
        .decision_proofs
        .iter()
        .map(|proof| {
            json!({
                "decision_id": proof.decision_id,
                "required_contract_revision": proof.required_contract_revision,
                "receipt_id": proof.receipt_id,
                "receipt_hash": proof.receipt_hash,
                "content_hash": proof.payload.decision.content.content_hash,
                "content_current": proof.content_current,
            })
        })
        .collect();
    if !proofs.is_empty() {
        value.insert("decision_proofs".to_string(), Value::Array(proofs));
    }

    // Content bindings (brief/map/shared approach/decision prose).
    let bindings: Vec<Value> = inputs
        .content_bindings
        .iter()
        .map(|binding| {
            json!({
                "label": binding.label,
                "path": binding.path,
                "bound_hash": binding.bound_hash,
                "current_hash": binding.current_hash,
            })
        })
        .collect();
    if !bindings.is_empty() {
        value.insert("content_bindings".to_string(), Value::Array(bindings));
    }

    // Authority policy fingerprint participates in readiness freshness.
    if let Some(fingerprint) = &inputs.authority.fingerprint {
        value.insert(
            "authority_policy".to_string(),
            json!({
                "revision": inputs.authority.policy_revision,
                "fingerprint": fingerprint,
            }),
        );
    }

    // Required docs registry records/revisions/content hashes.
    value.insert("applicable_docs".to_string(), docs_projection(inputs.docs));

    let canonical = to_canonical_bytes(&Value::Object(value))?;
    Ok(hash_bytes(&canonical))
}

fn documentation_projection(doc: &crate::graph::node::DocumentationMetadata) -> Value {
    json!({
        "posture": doc.impact.posture,
        "rationale": doc.impact.rationale,
        "required_documents": doc.impact.required_documents,
        "deferred_to": doc.impact.deferred_to,
        "routing_paths": doc.routing.paths,
        "routing_domains": doc.routing.domains,
        "routing_labels": doc.routing.labels,
    })
}

fn qa_projection(qa: &crate::graph::contract::QaMetadata) -> Value {
    json!({
        "posture": qa.impact.posture,
        "rationale": qa.impact.rationale,
        "behavioral_owner": qa.impact.behavioral_owner,
        "affected_case_ids": qa.impact.affected_case_ids,
    })
}

fn implementation_projection(contract: &crate::graph::contract::ImplementationContract) -> Value {
    let acceptance: Vec<Value> = contract
        .acceptance
        .iter()
        .map(|item| json!({"id": item.id, "summary": item.summary}))
        .collect();
    let invariants: Vec<Value> = contract
        .invariants
        .iter()
        .map(|item| json!({"id": item.id}))
        .collect();
    let freedom: Vec<Value> = contract
        .implementation_freedom
        .iter()
        .map(|item| json!({"id": item.id}))
        .collect();
    let required_decisions: Vec<Value> = contract
        .required_decisions
        .iter()
        .map(|decision| {
            json!({
                "id": decision.id,
                "contract_revision": decision.contract_revision,
                "acceptance_receipt": decision.acceptance_receipt,
            })
        })
        .collect();
    let shared: Vec<Value> = contract
        .shared_approach_refs
        .iter()
        .map(|approach| {
            json!({
                "owner": approach.owner,
                "path": approach.path,
                "content_hash": approach.content_hash,
            })
        })
        .collect();
    let anchors: Vec<Value> = contract
        .code_anchors
        .iter()
        .map(|anchor| json!({"path": anchor.path, "symbol": anchor.symbol}))
        .collect();
    json!({
        "mode": contract.mode,
        "work_surface": contract.work_surface,
        "plan_policy": contract.plan_policy,
        "semantic_impact": contract.semantic_impact,
        "verification_profile": contract.verification_profile,
        "objective": contract.objective,
        "brief": contract.brief,
        "acceptance": acceptance,
        "invariants": invariants,
        "implementation_freedom": freedom,
        "required_decisions": required_decisions,
        "shared_approach_refs": shared,
        "code_anchors": anchors,
    })
}

fn decision_work_projection(contract: &crate::graph::contract::DecisionWorkContract) -> Value {
    json!({
        "destination_owner": contract.destination_owner,
        "branch_id": contract.branch_id,
        "gap_kind": contract.gap_kind,
        "question": contract.question,
        "resolution_target": contract.resolution_target,
        "provenance_shaping_receipt": contract.provenance.shaping_receipt,
        "provenance_fog_id": contract.provenance.fog_id,
    })
}

fn structural_projection(report: &StructuralExecutabilityReport) -> Value {
    let blockers: Vec<Value> = report
        .hard_blockers
        .iter()
        .map(|blocker| {
            json!({
                "id": blocker.id,
                "resolution": blocker.resolution,
            })
        })
        .collect();
    json!({
        "structural_state": report.structural_state,
        "hard_blockers": blockers,
        "supersession_replacement": report
            .supersession
            .as_ref()
            .and_then(|s| s.replacement.clone()),
    })
}

fn shaping_projection(shaping: &ShapingReceiptSnapshot) -> Value {
    let payload = &shaping.payload;
    let branches: Vec<Value> = payload
        .branches
        .iter()
        .map(|branch| {
            let disposition_kind = match &branch.disposition {
                BranchDisposition::Resolved { .. } => "resolved",
                BranchDisposition::Rejected { .. } => "rejected",
                BranchDisposition::Delegated { freedom_id, .. } => {
                    return json!({
                        "id": branch.id,
                        "criticality": branch.criticality,
                        "disposition": "delegated",
                        "freedom_id": freedom_id,
                    });
                }
                BranchDisposition::Deferred { .. } => "deferred",
                BranchDisposition::Blocking { .. } => "blocking",
            };
            json!({
                "id": branch.id,
                "criticality": branch.criticality,
                "disposition": disposition_kind,
            })
        })
        .collect();
    let fog: Vec<Value> = payload.fog.iter().map(|f| json!({"id": f.id})).collect();
    json!({
        "receipt_id": shaping.receipt_id,
        "receipt_hash": shaping.receipt_hash,
        "materialization": payload.materialization,
        "shape_mode": payload.shape_mode,
        "source_posture": payload.source_posture,
        "destination": payload.destination.is_some(),
        "map": payload.map,
        "branches": branches,
        "fog": fog,
    })
}

fn docs_projection(docs: &ApplicableDocsReport) -> Value {
    let required: Vec<Value> = docs
        .required
        .iter()
        .map(|document| {
            json!({
                "id": document.id,
                "revision": document.document_revision,
                "content_hash": document.content_hash,
            })
        })
        .collect();
    json!({
        "registry_revision": docs.registry.revision,
        "registry_fingerprint": docs.registry.fingerprint,
        "posture": docs.work.documentation_posture,
        "required": required,
    })
}

/// Decide whether a shaping receipt is readiness-eligible as a *current* pointer
/// source: kind, result, payload version and subject must match. Used by the
/// store when assembling the readiness snapshot so the pure evaluator stays
/// free of receipt integrity concerns.
pub fn shaping_receipt_eligible(
    receipt: &crate::evidence::model::ReceiptEnvelope,
    subject_id: &str,
) -> bool {
    if receipt.kind != ReceiptKind::ShapingValidation {
        return false;
    }
    if receipt.result != ReceiptResult::Passed {
        return false;
    }
    if receipt.subject.id != subject_id {
        return false;
    }
    matches!(
        &receipt.payload,
        crate::evidence::model::ReceiptPayload::ShapingValidation(payload)
            if payload.payload_version == 1
    )
}

impl ReadinessReport {
    pub fn status_as_word(&self) -> &'static str {
        match self.status {
            ReadinessStatus::Ready => "ready",
            ReadinessStatus::NotReady => "not_ready",
            ReadinessStatus::Stale => "stale",
            ReadinessStatus::Invalid => "invalid",
        }
    }
}
