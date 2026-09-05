//! Deterministic readiness composition.
//!
//! This module is a *pure* gate-family evaluator. It consumes a coherent typed
//! snapshot ([`ReadinessInputs`]) that the graph store assembles under the
//! repository fence, and produces one stable [`ReadinessReport`] with
//! explainable per-family statuses, stable reason codes and a *narrow*
//! readiness fingerprint.
//!
//! Boundary rules:
//!
//! * readiness never reads raw JSON or performs filesystem I/O — the store owns
//!   coherent snapshot capture;
//! * readiness never mutates state;
//! * the structural executability module must not import readiness (one-way
//!   dependency: readiness consumes the structural report, never the reverse);
//! * only implementation Tickets can become `ready` under
//!   `contract_readiness`;
//! * the Ticket contract itself is `works/<id>/ticket.md`; readiness consumes
//!   its parse via `ticket_brief` and never a stored JSON contract.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::docs::applicability::ApplicableDocsReport;
use crate::graph::model::brief::TicketBrief;
use crate::graph::model::contract::{QaImpactPosture, TicketRole};
use crate::graph::model::node::{Node, NodeStatus};
use crate::graph::read::executability::StructuralExecutabilityReport;
use crate::id::WorkKind;
use crate::policy::AuthorityPolicyReport;
use crate::PulseResult;

/// Current readiness profile identifier. Only implementation Tickets can be
/// ready under this profile.
pub const READINESS_PROFILE: &str = "contract_readiness";

/// Profile identifier recorded on `work.node.transitioned` events that pass the
/// `draft -> shaped` (and blocked resume) shaping gate.
pub const SHAPED_GATE_PROFILE: &str = "shaped";

pub const READINESS_SCHEMA_VERSION: u32 = 1;

/// Future gate families that are intentionally not evaluated yet. They are
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
    pub future_gate_families: Vec<FutureGateFamily>,
    pub reason_codes: Vec<String>,
}

/// Which gate profile to evaluate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalProfile {
    /// `draft -> shaped` / blocked resume shaping gate.
    Shaped,
    /// Full `contract_readiness` readiness gate (`shaped -> ready`).
    Ready,
}

/// Current Story QA baseline resolution for a required Ticket checkpoint.
#[derive(Debug, Clone)]
pub struct QaCaseResolutionSnapshot {
    pub owner_id: String,
    pub baseline_revision: u64,
    pub baseline_content_hash: String,
    pub selected_cases: Vec<(String, u64)>,
    pub error_code: Option<String>,
}

/// A single content binding (the ticket brief) and its current on-disk hash,
/// used for content-reference currentness.
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
    /// The current ticket.md parse and materialization validation result.
    /// Shaping (the ambiguity gate) and the full ready gate both evaluate
    /// against this markdown contract.
    pub ticket_brief: Option<&'a TicketBrief>,
    pub ticket_brief_error: Option<&'a str>,
    pub qa_resolution: Option<&'a QaCaseResolutionSnapshot>,
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
    let mut evaluator = FamilyEvaluator::new(inputs);

    for (family, active_for_profile) in ALL_FAMILIES {
        let status = if *active_for_profile {
            evaluator.evaluate(family)?
        } else {
            GateStatus::NotEvaluated
        };
        let reason_codes = evaluator.take_codes();
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

const SHAPED_FAMILIES: &[&str] = &["ticket_ambiguity"];

/// All gate families in fixed evaluation order, paired with whether the family
/// is part of the *full readiness* profile. The shaped profile further filters
/// to [`SHAPED_FAMILIES`].
const ALL_FAMILIES: &[(&str, bool)] = &[
    ("graph_validity", true),
    ("work_kind_and_role", true),
    ("lifecycle_eligibility", true),
    ("structural_executability", true),
    ("ticket_ambiguity", true),
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
    fn new(inputs: &'a ReadinessInputs<'a>) -> Self {
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
            "ticket_ambiguity" => self.ticket_ambiguity(),
            "authority" => self.authority(),
            "documentation_impact" => self.documentation_impact(),
            "applicable_documents" => self.applicable_documents(),
            "qa_impact" => self.qa_impact(),
            "content_reference_integrity" => self.content_reference_integrity(),
            _ => GateStatus::NotEvaluated,
        };
        Ok(status)
    }

    fn take_codes(&mut self) -> Vec<String> {
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
            crate::graph::read::executability::StructuralState::Candidate => GateStatus::Passed,
            crate::graph::read::executability::StructuralState::Blocked => {
                if report.hard_blockers.iter().any(|b| {
                    b.resolution != crate::graph::read::executability::BlockerResolution::Satisfied
                }) {
                    self.note("hard_blocker_open");
                }
                GateStatus::Failed
            }
            crate::graph::read::executability::StructuralState::Paused => {
                self.note("structural_paused");
                GateStatus::Failed
            }
            crate::graph::read::executability::StructuralState::NotExecutableKind => {
                self.note("not_executable_kind");
                GateStatus::NotApplicable
            }
            crate::graph::read::executability::StructuralState::Terminal => {
                self.note("structural_terminal");
                GateStatus::Failed
            }
            crate::graph::read::executability::StructuralState::Invalid => {
                self.note("structural_invalid");
                GateStatus::Failed
            }
        }
    }

    fn ticket_ambiguity(&mut self) -> GateStatus {
        if self.inputs.subject.role != Some(TicketRole::Implementation) {
            return GateStatus::NotApplicable;
        }
        if let Some(code) = self.inputs.ticket_brief_error {
            self.note(code);
            return GateStatus::Failed;
        }
        if self.inputs.ticket_brief.is_none() {
            self.note("ticket_brief_missing");
            return GateStatus::Failed;
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
        GateStatus::Passed
    }

    fn documentation_impact(&mut self) -> GateStatus {
        let posture = self.inputs.subject.documentation_posture();
        match posture {
            crate::graph::model::node::DocumentationImpactPosture::Unknown => {
                self.note("documentation_impact_unknown");
                GateStatus::Failed
            }
            crate::graph::model::node::DocumentationImpactPosture::Required
            | crate::graph::model::node::DocumentationImpactPosture::None
            | crate::graph::model::node::DocumentationImpactPosture::Deferred => GateStatus::Passed,
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
        if !has_required
            && posture != crate::graph::model::node::DocumentationImpactPosture::Required
        {
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
                let Some(resolution) = self.inputs.qa_resolution else {
                    self.note("qa_baseline_missing");
                    return GateStatus::Failed;
                };
                if let Some(code) = &resolution.error_code {
                    self.note(code);
                    GateStatus::Failed
                } else if resolution.selected_cases.is_empty() {
                    self.note("qa_case_selection_empty");
                    GateStatus::Failed
                } else {
                    GateStatus::Passed
                }
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
        _ => "content_reference_stale",
    }
}

fn content_missing_code(label: &str) -> &'static str {
    match label {
        "brief" => "implementation_brief_missing",
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

    fn or_stale(self) -> GateStatus {
        self.combine_worst(GateStatus::Stale)
    }

    fn or_failed(self) -> GateStatus {
        self.combine_worst(GateStatus::Failed)
    }
}

/// Compute the narrow readiness fingerprint from explicit gate projections.
///
/// Excluded by design: subject normal revision, lifecycle status, status
/// reason, timestamps, unrelated graph nodes/edges, events, cache/runtime
/// state and the global graph fingerprint (which is reported separately for
/// audit only).
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
    if let Some(resolution) = inputs.qa_resolution {
        value.insert(
            "qa_baseline".to_string(),
            json!({
                "owner_id": resolution.owner_id,
                "revision": resolution.baseline_revision,
                "content_hash": resolution.baseline_content_hash,
                "selected_cases": resolution.selected_cases,
                "error_code": resolution.error_code,
            }),
        );
    }

    // Structural: relevant hard blocker edges + supersession replacement only.
    value.insert(
        "structural".to_string(),
        structural_projection(inputs.structural),
    );

    // The ticket.md contract binding (the only contract content binding).
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

fn documentation_projection(doc: &crate::graph::model::node::DocumentationMetadata) -> Value {
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

fn qa_projection(qa: &crate::graph::model::contract::QaMetadata) -> Value {
    json!({
        "posture": qa.impact.posture,
        "rationale": qa.impact.rationale,
        "behavioral_owner": qa.impact.behavioral_owner,
        "affected_case_ids": qa.impact.affected_case_ids,
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
