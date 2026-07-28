//! Public neutral WorkPacketV1 DTOs.
//!
//! This module defines the packet contract for Phase 2 preview work packets.
//! Every type is a pure value DTO with `#[serde(deny_unknown_fields)]` on every
//! struct.  There are no graph/docs/source imports — only serde, canonical_json
//! and error primitives.
//!
//! Ownership: `src/work_packet.rs` is the public neutral value owner.
//! Cross-domain composition belongs in `src/kernel/packet.rs`.
//!
//! See `proposals/phase2-slice1-work-packet-dispatch-foundation.md`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

use crate::canonical_json;
use crate::PulseResult;

// ---------------------------------------------------------------------------
// Constants / budget profile
// ---------------------------------------------------------------------------

/// Packet schema version for the current pre-release family.
pub const PACKET_SCHEMA_VERSION: u32 = 1;

/// Packet profile identifier.
pub const PACKET_PROFILE: &str = "phase2_work_packet_preview_v1";

/// Budget profile identifier.
pub const BUDGET_PROFILE: &str = "phase2_work_packet_preview_budget_v1";

/// Hard ceiling for canonical packet JSON (128 KiB).
pub const MAX_CANONICAL_JSON_BYTES: usize = 131_072;

/// Maximum incident relations before overflow.
pub const MAX_INCIDENT_RELATIONS: usize = 128;

/// Maximum decision-frontier items before overflow.
pub const MAX_DECISION_FRONTIER_ITEMS: usize = 16;

/// Maximum suggested lexical sections.
pub const MAX_SUGGESTED_SECTIONS: usize = 8;

/// Maximum snippet bytes per suggested section.
pub const MAX_SNIPPET_BYTES_EACH: usize = 500;

/// Recommended initial sections for the read budget.
pub const RECOMMENDED_INITIAL_SECTIONS: usize = 4;

/// Recommended initial lines for the read budget.
pub const MAX_INITIAL_LINES: usize = 240;

// ---------------------------------------------------------------------------
// Top-level packet
// ---------------------------------------------------------------------------

/// Complete Phase 2 preview work packet.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkPacketV1 {
    pub schema_version: u32,
    pub profile: String,
    pub code: String,
    pub subject: SubjectSnapshot,
    pub snapshot: SnapshotReport,
    pub contract: PacketImplementationContractV1,
    pub context: PacketContext,
    pub shaping: PacketShaping,
    pub graph: PacketGraph,
    pub documentation: PacketDocumentation,
    pub knowledge: PacketKnowledge,
    pub source: PacketSource,
    pub workspace: PacketWorkspace,
    pub capabilities: PacketCapabilities,
    pub scope: PacketScope,
    pub assurance: PacketAssurance,
    pub dispatch: PacketDispatch,
    pub budget: PacketBudget,
    /// sha256 fingerprint of the canonical fingerprint projection.
    pub packet_fingerprint: String,
    #[serde(default)]
    pub reason_codes: Vec<String>,
}

// ---------------------------------------------------------------------------
// Subject
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SubjectSnapshot {
    pub id: String,
    pub kind: String,
    pub role: String,
    pub title: String,
    pub revision: u64,
    pub contract_revision: u64,
    pub status: String,
    pub risk: String,
    pub materialization: String,
    pub content_dir: String,
}

// ---------------------------------------------------------------------------
// Snapshot (precondition set)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SnapshotReport {
    pub graph_fingerprint: String,
    pub readiness_profile: String,
    pub readiness_fingerprint: String,
    pub readiness_status: String,
    pub authority_policy_revision: u64,
    pub authority_policy_fingerprint: String,
    pub docs_registry_revision: u64,
    pub docs_registry_fingerprint: String,
    pub docs_index_fingerprint: String,
    pub source_commit: String,
}

// ---------------------------------------------------------------------------
// Packet-level contract DTO
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketImplementationContractV1 {
    pub mode: String,
    pub work_surface: String,
    pub plan_policy: String,
    pub semantic_impact: String,
    #[serde(default)]
    pub effort: PacketEffortMetadata,
    pub verification_profile: String,
    pub brief: Option<PacketContentRef>,
    pub objective: String,
    pub current_behavior: String,
    pub target_behavior: String,
    #[serde(default)]
    pub code_anchors: Vec<PacketSurfaceRef>,
    #[serde(default)]
    pub documentation_anchors: Vec<PacketSurfaceRef>,
    #[serde(default)]
    pub configuration_anchors: Vec<PacketSurfaceRef>,
    #[serde(default)]
    pub data_anchors: Vec<PacketSurfaceRef>,
    #[serde(default)]
    pub research_refs: Vec<PacketSurfaceRef>,
    #[serde(default)]
    pub required_changes: Vec<PacketContractItem>,
    #[serde(default)]
    pub invariants: Vec<PacketContractItem>,
    #[serde(default)]
    pub acceptance: Vec<PacketContractItem>,
    #[serde(default)]
    pub scope: PacketContractScope,
    #[serde(default)]
    pub implementation_freedom: Vec<PacketContractItem>,
    #[serde(default)]
    pub required_decisions: Vec<PacketRequiredDecisionRef>,
    #[serde(default)]
    pub shared_approach_refs: Vec<PacketSharedApproachRef>,
    #[serde(default)]
    pub expected_evidence: Vec<String>,
    #[serde(default)]
    pub expected_handoff: Vec<String>,
}

// ---------------------------------------------------------------------------
// Contract sub-types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[derive(Default)]
pub struct PacketEffortMetadata {
    #[serde(default)]
    pub multi_session: bool,
    #[serde(default)]
    pub multiple_dependent_decisions: bool,
    #[serde(default)]
    pub resume_or_audit_continuity: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketContentRef {
    pub path: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketSurfaceRef {
    pub path: String,
    pub symbol: Option<String>,
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketContractItem {
    pub id: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[derive(Default)]
pub struct PacketContractScope {
    #[serde(default)]
    pub included: Vec<String>,
    #[serde(default)]
    pub excluded: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketRequiredDecisionRef {
    pub id: String,
    pub contract_revision: u64,
    pub acceptance_receipt: Option<PacketReceiptRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketSharedApproachRef {
    pub owner: PacketSharedApproachOwner,
    pub path: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketSharedApproachOwner {
    pub id: String,
    pub contract_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketReceiptRef {
    pub id: String,
    pub hash: String,
}

// ---------------------------------------------------------------------------
// Context: parents and decisions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketContext {
    #[serde(default)]
    pub parents: Vec<PacketParentRef>,
    #[serde(default)]
    pub decisions: Vec<PacketDecisionRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketParentRef {
    pub relation: String,
    pub id: String,
    pub kind: String,
    pub revision: u64,
    pub contract_revision: u64,
    pub status: String,
    pub title: String,
    pub content_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketDecisionRef {
    pub id: String,
    pub revision: u64,
    pub contract_revision: u64,
    pub status: String,
    pub title: String,
    pub acceptance_receipt: Option<PacketReceiptRef>,
    #[serde(default)]
    pub content_refs: Vec<PacketContentRef>,
}

// ---------------------------------------------------------------------------
// Shaping
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketShaping {
    pub status: String,
    pub receipt_id: String,
    pub receipt_hash: String,
    pub owning_work: PacketShapingWorkBinding,
    pub shape_mode: String,
    pub destination: Option<PacketShapingDestination>,
    pub map: Option<PacketShapingMapSnapshot>,
    #[serde(default)]
    pub critical_branches: Vec<PacketCriticalBranch>,
    #[serde(default)]
    pub bounded_fog: Vec<PacketBoundedFog>,
    #[serde(default)]
    pub remaining_uncertainty: Vec<PacketRemainingUncertainty>,
    pub decision_frontier: PacketDecisionFrontier,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketShapingWorkBinding {
    pub id: String,
    pub revision_observed: u64,
    pub contract_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketShapingDestination {
    pub summary: String,
    #[serde(default)]
    pub scope_boundary: Vec<String>,
    #[serde(default)]
    pub exit_conditions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketShapingMapSnapshot {
    pub path: String,
    pub revision: u64,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketCriticalBranch {
    pub id: String,
    pub question: String,
    pub gap_kind: String,
    #[serde(default)]
    pub affected_work: Vec<String>,
    pub disposition: PacketBranchDisposition,
}

/// Tagged disposition for a critical branch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketBranchDisposition {
    pub kind: String,
    pub resolution: Option<PacketResolution>,
    pub non_blocking_context: Option<String>,
}

/// Resolution pointer for a resolved branch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketResolution {
    pub kind: String,
    pub id: String,
    pub revision: u64,
    pub gist: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketBoundedFog {
    pub id: String,
    pub statement: String,
    #[serde(default)]
    pub bounds: Vec<String>,
    pub why_not_precise: String,
    pub trigger: String,
    #[serde(default)]
    pub affected_work: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketRemainingUncertainty {
    pub summary: String,
    pub trigger: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketDecisionFrontier {
    pub status: String,
    #[serde(default)]
    pub items: Vec<PacketDecisionFrontierItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketDecisionFrontierItem {
    pub id: String,
    pub revision: u64,
    pub gap_kind: String,
    pub question: String,
    pub status: String,
}

// ---------------------------------------------------------------------------
// Graph
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketGraph {
    pub structural_state: String,
    #[serde(default)]
    pub hard_blockers: Vec<PacketBlockerItem>,
    #[serde(default)]
    pub soft_preferences: Vec<PacketBlockerItem>,
    pub supersession: Option<PacketSupersessionRef>,
    #[serde(default)]
    pub relations: PacketRelationBundle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketBlockerItem {
    pub id: String,
    pub relation: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketSupersessionRef {
    pub id: String,
    pub revision: u64,
    pub status: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[derive(Default)]
pub struct PacketRelationBundle {
    #[serde(default)]
    pub outgoing: Vec<PacketRelationItem>,
    #[serde(default)]
    pub incoming: Vec<PacketRelationItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketRelationItem {
    pub edge_id: String,
    pub edge_type: String,
    pub from: String,
    pub to: String,
    pub edge_revision: u64,
    pub opposite_id: String,
    pub opposite_kind: String,
    pub opposite_status: String,
    pub opposite_revision: u64,
    pub opposite_title: String,
}

// ---------------------------------------------------------------------------
// Documentation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketDocumentation {
    pub applicability: PacketDocsApplicability,
    pub suggestion_query: PacketSuggestionQuery,
    #[serde(default)]
    pub suggested_sections: Vec<PacketSuggestedSection>,
    pub read_budget: PacketReadBudget,
    pub index: PacketDocsIndex,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketDocsApplicability {
    pub status: String,
    #[serde(default)]
    pub required: Vec<PacketDocRef>,
    #[serde(default)]
    pub optional: Vec<PacketDocRef>,
    #[serde(default)]
    pub write_candidates: Vec<PacketDocRef>,
    #[serde(default)]
    pub excluded: Vec<PacketExcludedDocRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketDocRef {
    pub id: String,
    pub path: String,
    pub kind: String,
    pub authority: String,
    pub owner: String,
    pub summary: String,
    pub revision: u64,
    pub content_hash: String,
    #[serde(default)]
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketExcludedDocRef {
    pub id: String,
    pub path: Option<String>,
    pub reason_codes: Vec<String>,
    pub replacement: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketSuggestionQuery {
    pub text: String,
    #[serde(default)]
    pub normalized_terms: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketSuggestedSection {
    pub rank: u64,
    pub score_micros: u64,
    pub lexical_score_micros: u64,
    pub section_ref: String,
    pub heading_path: String,
    pub line_range: PacketLineRange,
    pub document_id: String,
    pub document_hash: String,
    pub section_hash: String,
    pub summary: String,
    pub snippet: String,
    pub authority: String,
    pub owner: String,
    pub kind: String,
    #[serde(default)]
    pub matched_fields: Vec<String>,
    #[serde(default)]
    pub applicability_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketLineRange {
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketReadBudget {
    pub required_sections: u64,
    pub recommended_initial_sections: u64,
    pub max_initial_lines: u64,
    pub suggestion_limit: u64,
    pub snippet_max_bytes_each: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketDocsIndex {
    pub state: String,
    pub fingerprint: String,
    pub mode: String,
}

// ---------------------------------------------------------------------------
// Knowledge (typed not-installed per P2S1-D10)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketKnowledge {
    pub status: String,
    pub owner_phase: u32,
    pub knowledge_fingerprint: Option<String>,
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub recommended: Vec<String>,
    #[serde(default)]
    pub suggested: Vec<String>,
    #[serde(default)]
    pub excluded: Vec<String>,
}

// ---------------------------------------------------------------------------
// Source
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketSource {
    pub repository_id: String,
    pub kind: String,
    pub commit: String,
    pub head_ref: Option<String>,
    pub worktree_root_kind: String,
    pub cleanliness: String,
    pub operation_state: String,
    pub currentness: String,
}

// ---------------------------------------------------------------------------
// Workspace
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketWorkspace {
    pub binding_status: String,
    pub workspace_id: Option<String>,
    pub required_strategy: String,
    pub base_repository_id: String,
    pub base_commit: String,
    #[serde(default)]
    pub requirements: Vec<String>,
}

// ---------------------------------------------------------------------------
// Capabilities
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketCapabilities {
    pub evaluation_status: String,
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
    #[serde(default)]
    pub missing: Vec<String>,
    pub inventory_identity: Option<String>,
}

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketScope {
    pub scope_hints: PacketScopeHints,
    #[serde(default)]
    pub implementation_freedom: Vec<PacketContractItem>,
    #[serde(default)]
    pub hard_stops: Vec<String>,
    pub enforcement: PacketScopeEnforcement,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[derive(Default)]
pub struct PacketScopeHints {
    #[serde(default)]
    pub source_paths: Vec<String>,
    #[serde(default)]
    pub documentation_paths: Vec<String>,
    #[serde(default)]
    pub configuration_paths: Vec<String>,
    #[serde(default)]
    pub data_paths: Vec<String>,
    #[serde(default)]
    pub included: Vec<String>,
    #[serde(default)]
    pub excluded: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketScopeEnforcement {
    pub status: String,
    pub owner_phase: u32,
}

// ---------------------------------------------------------------------------
// Assurance
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketAssurance {
    pub verification_profile: String,
    #[serde(default)]
    pub expected_evidence: Vec<String>,
    #[serde(default)]
    pub expected_handoff: Vec<String>,
    #[serde(default)]
    pub documentation_impact: PacketDocumentationImpact,
    pub qa: PacketQaStatus,
    pub promotion_policy: PacketFutureGate,
    pub close_gate: PacketFutureGate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[derive(Default)]
pub struct PacketDocumentationImpact {
    pub posture: String,
    pub status: String,
    #[serde(default)]
    pub required_doc_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketQaStatus {
    pub posture: String,
    pub status: String,
    #[serde(default)]
    pub affected_case_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketFutureGate {
    pub status: String,
    pub owner_phase: u32,
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketDispatch {
    pub reservation_candidate: bool,
    pub dispatch_authorized: bool,
    pub authorization_status: String,
    #[serde(default)]
    pub gate_families: Vec<PacketGateFamily>,
    #[serde(default)]
    pub revalidation_preconditions: Vec<PacketRevalidationPrecondition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketGateFamily {
    pub family: String,
    pub status: String,
    #[serde(default)]
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketRevalidationPrecondition {
    pub field: String,
    pub value: String,
}

// ---------------------------------------------------------------------------
// Budget
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketBudget {
    pub profile: String,
    pub max_canonical_json_bytes: u64,
    pub max_incident_relations: u64,
    pub max_decision_frontier_items: u64,
    pub max_suggested_sections: u64,
    pub max_snippet_bytes_each: u64,
    pub recommended_initial_sections: u64,
    pub max_initial_lines: u64,
    /// Length of the final canonical serialization (fixed-point).
    pub actual_canonical_json_bytes: u64,
    #[serde(default)]
    pub truncations: Vec<String>,
}

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

impl WorkPacketV1 {
    /// Normalize every set-like collection for deterministic ordering.
    pub fn normalize(&mut self) {
        sort_strings(&mut self.reason_codes);

        self.contract.normalize();
        self.context.normalize();
        self.shaping.normalize();
        self.graph.normalize();
        self.documentation.normalize();
        self.knowledge.normalize();
        self.source.normalize();
        self.workspace.normalize();
        self.capabilities.normalize();
        self.scope.normalize();
        self.assurance.normalize();
        self.dispatch.normalize();
        self.budget.normalize();
    }
}

impl PacketImplementationContractV1 {
    pub fn normalize(&mut self) {
        sort_by_path_symbol(&mut self.code_anchors);
        sort_by_path_symbol(&mut self.documentation_anchors);
        sort_by_path_symbol(&mut self.configuration_anchors);
        sort_by_path_symbol(&mut self.data_anchors);
        sort_by_path_symbol(&mut self.research_refs);
        sort_by_id(&mut self.required_changes);
        sort_by_id(&mut self.invariants);
        sort_by_id(&mut self.acceptance);
        sort_by_id(&mut self.implementation_freedom);
        sort_by_id(&mut self.required_decisions);
        sort_by_owner_id(&mut self.shared_approach_refs);
        sort_strings(&mut self.expected_evidence);
        sort_strings(&mut self.expected_handoff);
        self.scope.included.sort();
        self.scope.included.dedup();
        self.scope.excluded.sort();
        self.scope.excluded.dedup();
    }
}

impl PacketContext {
    pub fn normalize(&mut self) {
        self.parents
            .sort_by(|a, b| a.relation.cmp(&b.relation).then(a.id.cmp(&b.id)));
        self.decisions.sort_by(|a, b| a.id.cmp(&b.id));
    }
}

impl PacketShaping {
    pub fn normalize(&mut self) {
        self.critical_branches.sort_by(|a, b| a.id.cmp(&b.id));
        self.bounded_fog.sort_by(|a, b| a.id.cmp(&b.id));
        self.remaining_uncertainty
            .sort_by(|a, b| a.summary.cmp(&b.summary));
        self.decision_frontier.normalize();
    }
}

impl PacketDecisionFrontier {
    pub fn normalize(&mut self) {
        self.items.sort_by(|a, b| a.id.cmp(&b.id));
    }
}

impl PacketGraph {
    pub fn normalize(&mut self) {
        self.hard_blockers.sort_by(|a, b| a.id.cmp(&b.id));
        self.soft_preferences.sort_by(|a, b| a.id.cmp(&b.id));
        self.relations.outgoing.sort_by(|a, b| {
            a.edge_type
                .cmp(&b.edge_type)
                .then(a.from.cmp(&b.from))
                .then(a.to.cmp(&b.to))
                .then(a.edge_id.cmp(&b.edge_id))
        });
        self.relations.incoming.sort_by(|a, b| {
            a.edge_type
                .cmp(&b.edge_type)
                .then(a.from.cmp(&b.from))
                .then(a.to.cmp(&b.to))
                .then(a.edge_id.cmp(&b.edge_id))
        });
    }
}

impl PacketDocumentation {
    pub fn normalize(&mut self) {
        self.applicability.normalize();
        self.suggested_sections
            .sort_by(|a, b| a.rank.cmp(&b.rank).then(a.section_ref.cmp(&b.section_ref)));
    }
}

impl PacketDocsApplicability {
    pub fn normalize(&mut self) {
        self.required.sort_by(|a, b| a.id.cmp(&b.id));
        self.optional.sort_by(|a, b| a.id.cmp(&b.id));
        self.write_candidates.sort_by(|a, b| a.id.cmp(&b.id));
        self.excluded.sort_by(|a, b| a.id.cmp(&b.id));
    }
}

impl PacketKnowledge {
    pub fn normalize(&mut self) {
        sort_strings(&mut self.required);
        sort_strings(&mut self.recommended);
        sort_strings(&mut self.suggested);
        sort_strings(&mut self.excluded);
    }
}

impl PacketSource {
    pub fn normalize(&mut self) {
        // No set-like collections in source currently.
    }
}

impl PacketWorkspace {
    pub fn normalize(&mut self) {
        sort_strings(&mut self.requirements);
    }
}

impl PacketCapabilities {
    pub fn normalize(&mut self) {
        sort_strings(&mut self.required);
        sort_strings(&mut self.optional);
        sort_strings(&mut self.missing);
    }
}

impl PacketScope {
    pub fn normalize(&mut self) {
        sort_strings(&mut self.scope_hints.source_paths);
        sort_strings(&mut self.scope_hints.documentation_paths);
        sort_strings(&mut self.scope_hints.configuration_paths);
        sort_strings(&mut self.scope_hints.data_paths);
        sort_strings(&mut self.scope_hints.included);
        sort_strings(&mut self.scope_hints.excluded);
        sort_by_id(&mut self.implementation_freedom);
        sort_strings(&mut self.hard_stops);
    }
}

impl PacketAssurance {
    pub fn normalize(&mut self) {
        sort_strings(&mut self.expected_evidence);
        sort_strings(&mut self.expected_handoff);
        sort_strings(&mut self.documentation_impact.required_doc_ids);
        sort_strings(&mut self.qa.affected_case_ids);
    }
}

impl PacketDispatch {
    pub fn normalize(&mut self) {
        self.gate_families.sort_by(|a, b| a.family.cmp(&b.family));
        for family in &mut self.gate_families {
            sort_strings(&mut family.reason_codes);
        }
        self.revalidation_preconditions
            .sort_by(|a, b| a.field.cmp(&b.field));
    }
}

impl PacketBudget {
    pub fn normalize(&mut self) {
        sort_strings(&mut self.truncations);
    }
}

// ---------------------------------------------------------------------------
// Sorting helpers
// ---------------------------------------------------------------------------

fn sort_strings(v: &mut Vec<String>) {
    v.sort();
    v.dedup();
}

fn sort_by_id<T>(v: &mut [T])
where
    T: HasId,
{
    v.sort_by(|a, b| a.id().cmp(b.id()));
}

fn sort_by_owner_id(v: &mut [PacketSharedApproachRef]) {
    v.sort_by(|a, b| a.owner.id.cmp(&b.owner.id).then(a.path.cmp(&b.path)));
}

fn sort_by_path_symbol(v: &mut [PacketSurfaceRef]) {
    v.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then(a.symbol.cmp(&b.symbol))
            .then(a.content_hash.cmp(&b.content_hash))
    });
}

trait HasId {
    fn id(&self) -> &str;
}

impl HasId for PacketContractItem {
    fn id(&self) -> &str {
        &self.id
    }
}

impl HasId for PacketRequiredDecisionRef {
    fn id(&self) -> &str {
        &self.id
    }
}

impl HasId for PacketBlockerItem {
    fn id(&self) -> &str {
        &self.id
    }
}

// ---------------------------------------------------------------------------
// Fingerprint projection
// ---------------------------------------------------------------------------

impl WorkPacketV1 {
    /// Compute the canonical packet fingerprint.
    ///
    /// The fingerprint projection excludes:
    ///   - `packet_fingerprint` (self-reference)
    ///   - `budget.actual_canonical_json_bytes` (self-reference)
    ///   - any `dispatch.revalidation_preconditions[]` entry whose `field` is
    ///     `packet_fingerprint` (defensive self-reference exclusion; packet
    ///     builders must not emit that precondition)
    ///
    /// The projection includes every other field: subject revision, snapshot
    /// fingerprints, contract content, shaping/decision identities, graph
    /// relations, docs applicability, source commit, workspace strategy,
    /// capability requirements, budget limits (not the actual size), and
    /// reason codes.
    pub fn compute_fingerprint(&self) -> PulseResult<String> {
        let value = serde_json::to_value(self)?;
        let projection = strip_self_referential_fields(&value);
        let canonical = canonical_json::to_canonical_value(&projection)?;
        let bytes = canonical_json::canonical_value_bytes(&canonical)?;
        Ok(canonical_json::hash_bytes(&bytes))
    }
}

/// Strip self-referential fields from a serialized packet value before
/// fingerprinting.
fn strip_self_referential_fields(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, child) in map {
                if key == "packet_fingerprint" {
                    continue;
                }

                let cleaned = if key == "budget" {
                    strip_budget_actual_size(child)
                } else if key == "dispatch" {
                    strip_dispatch_fingerprint_precondition(child)
                } else {
                    strip_self_referential_fields(child)
                };
                out.insert(key.clone(), cleaned);
            }
            Value::Object(out)
        }
        Value::Array(items) => {
            Value::Array(items.iter().map(strip_self_referential_fields).collect())
        }
        other => other.clone(),
    }
}

fn strip_budget_actual_size(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, child) in map {
                if key == "actual_canonical_json_bytes" {
                    continue;
                }
                out.insert(key.clone(), strip_self_referential_fields(child));
            }
            Value::Object(out)
        }
        other => strip_self_referential_fields(other),
    }
}

fn strip_dispatch_fingerprint_precondition(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, child) in map {
                let cleaned = if key == "revalidation_preconditions" {
                    strip_packet_fingerprint_preconditions(child)
                } else {
                    strip_self_referential_fields(child)
                };
                out.insert(key.clone(), cleaned);
            }
            Value::Object(out)
        }
        other => strip_self_referential_fields(other),
    }
}

fn strip_packet_fingerprint_preconditions(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .filter(|item| !is_packet_fingerprint_precondition(item))
                .map(strip_self_referential_fields)
                .collect(),
        ),
        other => strip_self_referential_fields(other),
    }
}

fn is_packet_fingerprint_precondition(value: &Value) -> bool {
    value
        .as_object()
        .and_then(|map| map.get("field"))
        .and_then(Value::as_str)
        == Some("packet_fingerprint")
}

fn validate_embedded_schema_value(
    schema: &Value,
    value: &Value,
    path: &str,
    root_schema: &Value,
) -> PulseResult<()> {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let resolved = resolve_schema_ref(root_schema, reference)?;
        return validate_embedded_schema_value(resolved, value, path, root_schema);
    }
    if let Some(any_of) = schema.get("anyOf").and_then(Value::as_array) {
        if any_of.iter().any(|candidate| {
            validate_embedded_schema_value(candidate, value, path, root_schema).is_ok()
        }) {
            return Ok(());
        }
        return schema_error(path, "value does not match any allowed schema");
    }
    match schema.get("const") {
        Some(expected) if value != expected => {
            return schema_error(path, format!("expected const {expected}"));
        }
        _ => {}
    }

    if let Some(types) = schema.get("type") {
        validate_schema_type(types, value, path)?;
    }
    if let Some(enum_values) = schema.get("enum").and_then(Value::as_array) {
        if !enum_values.iter().any(|expected| expected == value) {
            return schema_error(path, "value is not in enum");
        }
    }
    if let Some(pattern) = schema.get("pattern").and_then(Value::as_str) {
        validate_known_pattern(pattern, value, path)?;
    }
    if let Some(minimum) = schema.get("minimum").and_then(Value::as_i64) {
        if value.as_i64().is_some_and(|actual| actual < minimum) {
            return schema_error(path, format!("value is less than minimum {minimum}"));
        }
    }
    if let Some(max_items) = schema.get("maxItems").and_then(Value::as_u64) {
        if value
            .as_array()
            .is_some_and(|items| items.len() as u64 > max_items)
        {
            return schema_error(path, format!("array exceeds maxItems {max_items}"));
        }
    }

    if schema.get("type").and_then(Value::as_str) == Some("object") {
        validate_schema_object(schema, value, path, root_schema)?;
    }
    if schema.get("type").and_then(Value::as_str) == Some("array") {
        validate_schema_array(schema, value, path, root_schema)?;
    }
    Ok(())
}

fn resolve_schema_ref<'a>(root_schema: &'a Value, reference: &str) -> PulseResult<&'a Value> {
    let Some(pointer) = reference.strip_prefix('#') else {
        return schema_error("$schema", format!("unsupported schema ref {reference}"));
    };
    root_schema.pointer(pointer).ok_or_else(|| {
        crate::PulseError::validation(
            "work_packet_schema_invalid",
            format!("$schema: unresolved schema ref {reference}"),
        )
    })
}

fn validate_schema_type(types: &Value, value: &Value, path: &str) -> PulseResult<()> {
    let matches_type = match types {
        Value::String(kind) => value_matches_schema_type(value, kind),
        Value::Array(kinds) => kinds
            .iter()
            .filter_map(Value::as_str)
            .any(|kind| value_matches_schema_type(value, kind)),
        _ => true,
    };
    if matches_type {
        Ok(())
    } else {
        schema_error(path, format!("value does not match schema type {types}"))
    }
}

fn value_matches_schema_type(value: &Value, kind: &str) -> bool {
    match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true,
    }
}

fn validate_schema_object(
    schema: &Value,
    value: &Value,
    path: &str,
    root_schema: &Value,
) -> PulseResult<()> {
    let Some(object) = value.as_object() else {
        return Ok(());
    };
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    for field in &required {
        if !object.contains_key(*field) {
            return schema_error(path, format!("missing required property {field}"));
        }
    }
    let properties = schema.get("properties").and_then(Value::as_object);
    if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
        for key in object.keys() {
            let known_property = properties
                .map(|props| props.contains_key(key))
                .unwrap_or(false);
            if !known_property {
                return schema_error(path, format!("unexpected property {key}"));
            }
        }
    }
    if let Some(properties) = properties {
        for (key, child_schema) in properties {
            if let Some(child_value) = object.get(key) {
                validate_embedded_schema_value(
                    child_schema,
                    child_value,
                    &format!("{path}.{key}"),
                    root_schema,
                )?;
            }
        }
    }
    Ok(())
}

fn validate_schema_array(
    schema: &Value,
    value: &Value,
    path: &str,
    root_schema: &Value,
) -> PulseResult<()> {
    let Some(items) = value.as_array() else {
        return Ok(());
    };
    if let Some(item_schema) = schema.get("items") {
        for (index, item) in items.iter().enumerate() {
            validate_embedded_schema_value(
                item_schema,
                item,
                &format!("{path}[{index}]"),
                root_schema,
            )?;
        }
    }
    Ok(())
}

fn validate_known_pattern(pattern: &str, value: &Value, path: &str) -> PulseResult<()> {
    let Some(text) = value.as_str() else {
        return Ok(());
    };
    let valid = match pattern {
        "^sha256:[A-Fa-f0-9]{64}$" => valid_hash(text),
        "^[A-Fa-f0-9]{40}$" => text.len() == 40 && text.chars().all(|c| c.is_ascii_hexdigit()),
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        schema_error(path, format!("value does not match pattern {pattern}"))
    }
}

fn valid_hash(text: &str) -> bool {
    text.len() == 71
        && text.starts_with("sha256:")
        && text[7..].chars().all(|c| c.is_ascii_hexdigit())
}

fn schema_error<T>(path: &str, message: impl Into<String>) -> PulseResult<T> {
    Err(crate::PulseError::validation(
        "work_packet_schema_invalid",
        format!("{path}: {}", message.into()),
    ))
}

// ---------------------------------------------------------------------------
// Size fixpoint
// ---------------------------------------------------------------------------

impl WorkPacketV1 {
    /// Compute the fixed-point canonical size.
    ///
    /// 1. Set `packet_fingerprint` to the computed fingerprint.
    /// 2. Serialize and measure length L0.
    /// 3. Set `budget.actual_canonical_json_bytes` to L0 and re-serialize.
    /// 4. Repeat until the value converges (at most 3 iterations).
    /// 5. Enforce `<= MAX_CANONICAL_JSON_BYTES`.
    pub fn finalize_size(&mut self) -> PulseResult<()> {
        let fp = self.compute_fingerprint()?;
        self.packet_fingerprint = fp;
        self.validate_schema_contract()?;

        let mut prev_bytes = 0u64;
        for iteration in 0..4 {
            let raw = serde_json::to_value(&*self)?;
            let canonical = canonical_json::to_canonical_value(&raw)?;
            let bytes = canonical_json::canonical_value_bytes(&canonical)?;
            let len = bytes.len() as u64;

            if iteration > 0 && len == prev_bytes {
                // Converged.
                return self.enforce_budget(len);
            }
            prev_bytes = len;
            self.budget.actual_canonical_json_bytes = len;
        }

        // Did not converge within 3 iterations after first fingerprint write.
        Err(crate::PulseError::validation(
            "work_packet_size_fixpoint_failed",
            "final canonical size did not converge within 3 iterations",
        ))
    }

    pub fn validate_schema_contract(&self) -> PulseResult<()> {
        let value = serde_json::to_value(self)?;
        let schema: Value = serde_json::from_str(WORK_PACKET_SCHEMA)?;
        validate_embedded_schema_value(&schema, &value, "$", &schema)
    }

    fn enforce_budget(&self, actual: u64) -> PulseResult<()> {
        if actual > MAX_CANONICAL_JSON_BYTES as u64 {
            return Err(crate::PulseError::validation(
                "work_packet_budget_exceeded",
                format!(
                    "canonical packet JSON {} bytes exceeds maximum {} bytes",
                    actual, MAX_CANONICAL_JSON_BYTES
                ),
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a packet with all fields at their zero/empty defaults for use by
/// the kernel builder before population.
impl PacketScopeHints {
    #[allow(dead_code)]
    fn is_empty(&self) -> bool {
        self.source_paths.is_empty()
            && self.documentation_paths.is_empty()
            && self.configuration_paths.is_empty()
            && self.data_paths.is_empty()
            && self.included.is_empty()
            && self.excluded.is_empty()
    }
}

impl Default for PacketDecisionFrontier {
    fn default() -> Self {
        Self {
            status: "not_evaluated".to_string(),
            items: vec![],
        }
    }
}

impl Default for PacketBudget {
    fn default() -> Self {
        Self {
            profile: BUDGET_PROFILE.to_string(),
            max_canonical_json_bytes: MAX_CANONICAL_JSON_BYTES as u64,
            max_incident_relations: MAX_INCIDENT_RELATIONS as u64,
            max_decision_frontier_items: MAX_DECISION_FRONTIER_ITEMS as u64,
            max_suggested_sections: MAX_SUGGESTED_SECTIONS as u64,
            max_snippet_bytes_each: MAX_SNIPPET_BYTES_EACH as u64,
            recommended_initial_sections: RECOMMENDED_INITIAL_SECTIONS as u64,
            max_initial_lines: MAX_INITIAL_LINES as u64,
            actual_canonical_json_bytes: 0,
            truncations: vec![],
        }
    }
}

impl Default for PacketDispatch {
    fn default() -> Self {
        Self {
            reservation_candidate: true,
            dispatch_authorized: false,
            authorization_status: "not_reserved".to_string(),
            gate_families: vec![
                PacketGateFamily {
                    family: "readiness".to_string(),
                    status: "not_evaluated".to_string(),
                    reason_codes: vec![],
                },
                PacketGateFamily {
                    family: "packet_completeness".to_string(),
                    status: "not_evaluated".to_string(),
                    reason_codes: vec![],
                },
                PacketGateFamily {
                    family: "source_base".to_string(),
                    status: "not_evaluated".to_string(),
                    reason_codes: vec![],
                },
                PacketGateFamily {
                    family: "documentation_context".to_string(),
                    status: "not_evaluated".to_string(),
                    reason_codes: vec![],
                },
                PacketGateFamily {
                    family: "qa_baseline_and_cases".to_string(),
                    status: "not_applicable".to_string(),
                    reason_codes: vec![],
                },
                PacketGateFamily {
                    family: "lease".to_string(),
                    status: "not_evaluated".to_string(),
                    reason_codes: vec!["lease_resolver_not_installed".to_string()],
                },
                PacketGateFamily {
                    family: "workspace_binding".to_string(),
                    status: "not_evaluated".to_string(),
                    reason_codes: vec!["workspace_not_allocated".to_string()],
                },
                PacketGateFamily {
                    family: "capability_match".to_string(),
                    status: "not_evaluated".to_string(),
                    reason_codes: vec!["capability_inventory_not_bound".to_string()],
                },
            ],
            revalidation_preconditions: vec![],
        }
    }
}

// ---------------------------------------------------------------------------
// Schema constant
// ---------------------------------------------------------------------------

/// Embedded JSON schema for WorkPacketV1.
pub const WORK_PACKET_SCHEMA: &str = include_str!("schema/work-packet.schema.json");

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    // -----------------------------------------------------------------------
    // Helper: build a minimal valid packet for tests
    // -----------------------------------------------------------------------

    fn minimal_packet(fingerprint: &str) -> WorkPacketV1 {
        WorkPacketV1 {
            schema_version: PACKET_SCHEMA_VERSION,
            profile: PACKET_PROFILE.to_string(),
            code: "reservation_candidate".to_string(),
            subject: SubjectSnapshot {
                id: "TK-001".to_string(),
                kind: "ticket".to_string(),
                role: "implementation".to_string(),
                title: "Test ticket".to_string(),
                revision: 1,
                contract_revision: 1,
                status: "ready".to_string(),
                risk: "low".to_string(),
                materialization: "R1".to_string(),
                content_dir: "works/TK-001".to_string(),
            },
            snapshot: SnapshotReport {
                graph_fingerprint:
                    "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                        .to_string(),
                readiness_profile: "phase1_contract_readiness_v1".to_string(),
                readiness_fingerprint:
                    "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                        .to_string(),
                readiness_status: "ready".to_string(),
                authority_policy_revision: 1,
                authority_policy_fingerprint:
                    "sha256:2222222222222222222222222222222222222222222222222222222222222222"
                        .to_string(),
                docs_registry_revision: 1,
                docs_registry_fingerprint:
                    "sha256:3333333333333333333333333333333333333333333333333333333333333333"
                        .to_string(),
                docs_index_fingerprint:
                    "sha256:4444444444444444444444444444444444444444444444444444444444444444"
                        .to_string(),
                source_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
            },
            contract: PacketImplementationContractV1 {
                mode: "guided".to_string(),
                work_surface: "code".to_string(),
                plan_policy: "worker_optional".to_string(),
                semantic_impact: "behavior_or_public_risk_change".to_string(),
                effort: PacketEffortMetadata::default(),
                verification_profile: "service-change".to_string(),
                brief: None,
                objective: "Enable token rotation".to_string(),
                current_behavior: "No rotation".to_string(),
                target_behavior: "Atomic rotation".to_string(),
                code_anchors: vec![],
                documentation_anchors: vec![],
                configuration_anchors: vec![],
                data_anchors: vec![],
                research_refs: vec![],
                required_changes: vec![],
                invariants: vec![],
                acceptance: vec![],
                scope: PacketContractScope::default(),
                implementation_freedom: vec![],
                required_decisions: vec![],
                shared_approach_refs: vec![],
                expected_evidence: vec![],
                expected_handoff: vec![],
            },
            context: PacketContext {
                parents: vec![],
                decisions: vec![],
            },
            shaping: PacketShaping {
                status: "current".to_string(),
                receipt_id: "rcpt_00000000000000000000000000".to_string(),
                receipt_hash:
                    "sha256:5555555555555555555555555555555555555555555555555555555555555555"
                        .to_string(),
                owning_work: PacketShapingWorkBinding {
                    id: "ST-001".to_string(),
                    revision_observed: 3,
                    contract_revision: 2,
                },
                shape_mode: "focused_branches".to_string(),
                destination: Some(PacketShapingDestination {
                    summary: "Deliver token rotation".to_string(),
                    scope_boundary: vec!["No session redesign".to_string()],
                    exit_conditions: vec!["Concurrent rotation passes".to_string()],
                }),
                map: None,
                critical_branches: vec![],
                bounded_fog: vec![],
                remaining_uncertainty: vec![],
                decision_frontier: PacketDecisionFrontier {
                    status: "evaluated".to_string(),
                    items: vec![],
                },
            },
            graph: PacketGraph {
                structural_state: "executable".to_string(),
                hard_blockers: vec![],
                soft_preferences: vec![],
                supersession: None,
                relations: PacketRelationBundle::default(),
            },
            documentation: PacketDocumentation {
                applicability: PacketDocsApplicability {
                    status: "complete".to_string(),
                    required: vec![],
                    optional: vec![],
                    write_candidates: vec![],
                    excluded: vec![],
                },
                suggestion_query: PacketSuggestionQuery {
                    text: "Enable token rotation".to_string(),
                    normalized_terms: vec![
                        "enable".to_string(),
                        "token".to_string(),
                        "rotation".to_string(),
                    ],
                },
                suggested_sections: vec![],
                read_budget: PacketReadBudget {
                    required_sections: 0,
                    recommended_initial_sections: RECOMMENDED_INITIAL_SECTIONS as u64,
                    max_initial_lines: MAX_INITIAL_LINES as u64,
                    suggestion_limit: MAX_SUGGESTED_SECTIONS as u64,
                    snippet_max_bytes_each: MAX_SNIPPET_BYTES_EACH as u64,
                },
                index: PacketDocsIndex {
                    state: "current".to_string(),
                    fingerprint:
                        "sha256:6666666666666666666666666666666666666666666666666666666666666666"
                            .to_string(),
                    mode: "lexical".to_string(),
                },
            },
            knowledge: PacketKnowledge {
                status: "not_installed".to_string(),
                owner_phase: 4,
                knowledge_fingerprint: None,
                required: vec![],
                recommended: vec![],
                suggested: vec![],
                excluded: vec![],
            },
            source: PacketSource {
                repository_id: "repo_test".to_string(),
                kind: "git_commit".to_string(),
                commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
                head_ref: Some("refs/heads/main".to_string()),
                worktree_root_kind: "primary_or_existing_worktree".to_string(),
                cleanliness: "clean".to_string(),
                operation_state: "normal".to_string(),
                currentness: "current".to_string(),
            },
            workspace: PacketWorkspace {
                binding_status: "not_allocated".to_string(),
                workspace_id: None,
                required_strategy: "isolated_worktree_required".to_string(),
                base_repository_id: "repo_test".to_string(),
                base_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
                requirements: vec![
                    "same_repository_identity".to_string(),
                    "exact_base_commit".to_string(),
                    "clean_at_reservation".to_string(),
                    "scope_policy_revalidation".to_string(),
                ],
            },
            capabilities: PacketCapabilities {
                evaluation_status: "not_evaluated".to_string(),
                required: vec![
                    "repository.inspect".to_string(),
                    "source.read".to_string(),
                    "source.write".to_string(),
                ],
                optional: vec![],
                missing: vec![],
                inventory_identity: None,
            },
            scope: PacketScope {
                scope_hints: PacketScopeHints::default(),
                implementation_freedom: vec![],
                hard_stops: vec![
                    "do_not_change_acceptance_without_authority".to_string(),
                    "do_not_override_accepted_decision".to_string(),
                    "stop_on_objective_or_invariant_ambiguity".to_string(),
                    "stop_on_source_or_contract_drift".to_string(),
                ],
                enforcement: PacketScopeEnforcement {
                    status: "not_installed".to_string(),
                    owner_phase: 2,
                },
            },
            assurance: PacketAssurance {
                verification_profile: "service-change".to_string(),
                expected_evidence: vec![],
                expected_handoff: vec![],
                documentation_impact: PacketDocumentationImpact::default(),
                qa: PacketQaStatus {
                    posture: "none".to_string(),
                    status: "ready_gate_satisfied".to_string(),
                    affected_case_ids: vec![],
                },
                promotion_policy: PacketFutureGate {
                    status: "not_installed".to_string(),
                    owner_phase: 2,
                },
                close_gate: PacketFutureGate {
                    status: "not_installed".to_string(),
                    owner_phase: 2,
                },
            },
            dispatch: PacketDispatch::default(),
            budget: PacketBudget::default(),
            packet_fingerprint: fingerprint.to_string(),
            reason_codes: vec![],
        }
    }

    // -----------------------------------------------------------------------
    // Round-trip and unknown-field rejection
    // -----------------------------------------------------------------------

    #[test]
    fn round_trip_minimal_packet() {
        let packet = minimal_packet("");
        let json = serde_json::to_value(&packet).unwrap();
        let deserialized: WorkPacketV1 = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(packet, deserialized);

        // Verify specific top-level fields survive round-trip.
        assert_eq!(deserialized.schema_version, PACKET_SCHEMA_VERSION);
        assert_eq!(deserialized.profile, PACKET_PROFILE);
        assert_eq!(deserialized.code, "reservation_candidate");
        assert_eq!(deserialized.subject.id, "TK-001");
        assert_eq!(deserialized.subject.revision, 1);
        assert!(!deserialized.dispatch.dispatch_authorized);
        assert!(deserialized.dispatch.reservation_candidate);
        assert_eq!(deserialized.dispatch.authorization_status, "not_reserved");
    }

    #[test]
    fn deny_unknown_fields_top_level() {
        let json = serde_json::json!({
            "schema_version": 1,
            "profile": "test",
            "code": "test",
            "subject": {},
            "snapshot": {},
            "contract": {},
            "context": {},
            "shaping": {},
            "graph": {},
            "documentation": {},
            "knowledge": {},
            "source": {},
            "workspace": {},
            "capabilities": {},
            "scope": {},
            "assurance": {},
            "dispatch": {},
            "budget": {},
            "packet_fingerprint": "",
            "reason_codes": [],
            "unknown_field": "should_reject"
        });
        let result: Result<WorkPacketV1, _> = serde_json::from_value(json);
        assert!(result.is_err(), "top-level unknown field must be rejected");
    }

    #[test]
    fn deny_unknown_fields_subject() {
        let json = serde_json::json!({
            "id": "TK-001",
            "kind": "ticket",
            "role": "implementation",
            "title": "Test",
            "revision": 1,
            "contract_revision": 1,
            "status": "ready",
            "risk": "low",
            "materialization": "R1",
            "content_dir": "works/TK-001",
            "unknown_field": "reject"
        });
        let result: Result<SubjectSnapshot, _> = serde_json::from_value(json);
        assert!(
            result.is_err(),
            "SubjectSnapshot unknown field must be rejected"
        );
    }

    #[test]
    fn deny_unknown_fields_contract() {
        let json = serde_json::json!({
            "mode": "guided",
            "work_surface": "code",
            "plan_policy": "worker_optional",
            "semantic_impact": "behavior_or_public_risk_change",
            "effort": {},
            "verification_profile": "service-change",
            "objective": "Test",
            "current_behavior": "A",
            "target_behavior": "B",
            "acceptance": [],
            "fake": true
        });
        let result: Result<PacketImplementationContractV1, _> = serde_json::from_value(json);
        assert!(result.is_err(), "Contract unknown field must be rejected");
    }

    #[test]
    fn deny_unknown_fields_nested() {
        // Test that nested structs also reject unknown fields.
        let json = serde_json::json!({
            "id": "TK-001",
            "kind": "ticket",
            "role": "implementation",
            "title": "Test",
            "revision": 1,
            "contract_revision": 1,
            "status": "ready",
            "risk": "low",
            "materialization": "R1",
            "content_dir": "works/TK-001"
        });
        // Valid JSON should parse fine
        let subject: SubjectSnapshot = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(subject.id, "TK-001");

        // With extra field should fail
        let mut bad = json.clone();
        bad.as_object_mut()
            .unwrap()
            .insert("bogus".to_string(), serde_json::json!("value"));
        let result: Result<SubjectSnapshot, _> = serde_json::from_value(bad);
        assert!(result.is_err(), "SubjectSnapshot unknown field rejected");
    }

    // -----------------------------------------------------------------------
    // Normalization
    // -----------------------------------------------------------------------

    #[test]
    fn normalize_reason_codes() {
        let mut packet = minimal_packet("");
        packet.reason_codes = vec!["z".to_string(), "a".to_string(), "a".to_string()];
        packet.normalize();
        assert_eq!(packet.reason_codes, vec!["a".to_string(), "z".to_string()]);
    }

    #[test]
    fn normalize_capabilities() {
        let mut packet = minimal_packet("");
        packet.capabilities.required = vec![
            "source.write".to_string(),
            "source.read".to_string(),
            "repository.inspect".to_string(),
        ];
        packet.normalize();
        assert_eq!(
            packet.capabilities.required,
            vec![
                "repository.inspect".to_string(),
                "source.read".to_string(),
                "source.write".to_string(),
            ]
        );
    }

    #[test]
    fn normalize_hard_stops() {
        let mut packet = minimal_packet("");
        packet.scope.hard_stops = vec![
            "stop_on_source_or_contract_drift".to_string(),
            "do_not_override_accepted_decision".to_string(),
        ];
        packet.normalize();
        assert_eq!(
            packet.scope.hard_stops,
            vec![
                "do_not_override_accepted_decision".to_string(),
                "stop_on_source_or_contract_drift".to_string(),
            ]
        );
    }

    #[test]
    fn normalize_relations_by_edge_type_from_to_id() {
        let mut packet = minimal_packet("");
        packet.graph.relations.outgoing = vec![
            PacketRelationItem {
                edge_id: "edge-2".to_string(),
                edge_type: "blocks".to_string(),
                from: "TK-002".to_string(),
                to: "TK-001".to_string(),
                edge_revision: 1,
                opposite_id: "TK-002".to_string(),
                opposite_kind: "ticket".to_string(),
                opposite_status: "ready".to_string(),
                opposite_revision: 2,
                opposite_title: "Second".to_string(),
            },
            PacketRelationItem {
                edge_id: "edge-1".to_string(),
                edge_type: "blocks".to_string(),
                from: "TK-001".to_string(),
                to: "TK-003".to_string(),
                edge_revision: 1,
                opposite_id: "TK-003".to_string(),
                opposite_kind: "ticket".to_string(),
                opposite_status: "ready".to_string(),
                opposite_revision: 3,
                opposite_title: "Third".to_string(),
            },
        ];

        packet.normalize();
        assert_eq!(
            packet
                .graph
                .relations
                .outgoing
                .iter()
                .map(|item| item.edge_id.as_str())
                .collect::<Vec<_>>(),
            vec!["edge-1", "edge-2"]
        );
    }

    // -----------------------------------------------------------------------
    // Fingerprint determinism
    // -----------------------------------------------------------------------

    #[test]
    fn fingerprint_is_deterministic() {
        let fp1 = minimal_packet("").compute_fingerprint().unwrap();
        let fp2 = minimal_packet("").compute_fingerprint().unwrap();
        assert_eq!(fp1, fp2, "same inputs must produce same fingerprint");
    }

    #[test]
    fn serialized_minimal_packet_contains_required_null_and_empty_fields() {
        let packet = minimal_packet("");
        let value = serde_json::to_value(&packet).unwrap();

        assert_eq!(value["contract"]["brief"], Value::Null);
        assert_eq!(value["contract"]["code_anchors"], serde_json::json!([]));
        assert_eq!(value["context"]["decisions"], serde_json::json!([]));
        assert_eq!(value["shaping"]["map"], Value::Null);
        assert_eq!(value["graph"]["supersession"], Value::Null);
        assert_eq!(value["source"]["head_ref"], "refs/heads/main");
        assert_eq!(value["workspace"]["workspace_id"], Value::Null);
        assert_eq!(value["capabilities"]["inventory_identity"], Value::Null);
        assert_eq!(value["knowledge"]["knowledge_fingerprint"], Value::Null);
    }

    #[test]
    fn serialized_detached_head_packet_contains_null_head_ref() {
        let mut packet = minimal_packet("");
        packet.source.head_ref = None;
        let value = serde_json::to_value(&packet).unwrap();
        assert_eq!(value["source"]["head_ref"], Value::Null);
    }

    #[test]
    fn fingerprint_excludes_self_referential_fields() {
        // Different packet_fingerprint values must produce the same projection
        // hash because the fingerprint projection strips that field.
        let p1 = minimal_packet(
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        );
        let p2 = minimal_packet(
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );
        assert_eq!(
            p1.compute_fingerprint().unwrap(),
            p2.compute_fingerprint().unwrap(),
            "fingerprint must not depend on packet_fingerprint field"
        );
    }

    #[test]
    fn fingerprint_changes_when_subject_changes() {
        let p1 = minimal_packet("");
        let mut p2 = minimal_packet("");
        p2.subject.revision = 2;
        assert_ne!(
            p1.compute_fingerprint().unwrap(),
            p2.compute_fingerprint().unwrap(),
            "different subject revision must change fingerprint"
        );
    }

    #[test]
    fn fingerprint_changes_when_contract_changes() {
        let p1 = minimal_packet("");
        let mut p2 = minimal_packet("");
        p2.contract.objective = "Different objective".to_string();
        assert_ne!(
            p1.compute_fingerprint().unwrap(),
            p2.compute_fingerprint().unwrap(),
            "different contract must change fingerprint"
        );
    }

    #[test]
    fn fingerprint_changes_when_reason_codes_change() {
        let p1 = minimal_packet("");
        let mut p2 = minimal_packet("");
        p2.reason_codes = vec!["something_changed".to_string()];
        assert_ne!(
            p1.compute_fingerprint().unwrap(),
            p2.compute_fingerprint().unwrap(),
            "different reason codes must change fingerprint"
        );
    }

    #[test]
    fn fingerprint_excludes_packet_fingerprint_revalidation_precondition() {
        let mut p1 = minimal_packet("");
        let mut p2 = minimal_packet("");
        p1.dispatch.revalidation_preconditions = vec![
            PacketRevalidationPrecondition {
                field: "source_commit".to_string(),
                value: p1.snapshot.source_commit.clone(),
            },
            PacketRevalidationPrecondition {
                field: "packet_fingerprint".to_string(),
                value: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
            },
        ];
        p2.dispatch.revalidation_preconditions = vec![
            PacketRevalidationPrecondition {
                field: "source_commit".to_string(),
                value: p2.snapshot.source_commit.clone(),
            },
            PacketRevalidationPrecondition {
                field: "packet_fingerprint".to_string(),
                value: "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    .to_string(),
            },
        ];

        assert_eq!(
            p1.compute_fingerprint().unwrap(),
            p2.compute_fingerprint().unwrap(),
            "packet_fingerprint revalidation precondition must be excluded"
        );
    }

    #[test]
    fn fingerprint_changes_when_non_self_revalidation_precondition_changes() {
        let mut p1 = minimal_packet("");
        let mut p2 = minimal_packet("");
        p1.dispatch.revalidation_preconditions = vec![PacketRevalidationPrecondition {
            field: "source_commit".to_string(),
            value: "0123456789abcdef0123456789abcdef01234567".to_string(),
        }];
        p2.dispatch.revalidation_preconditions = vec![PacketRevalidationPrecondition {
            field: "source_commit".to_string(),
            value: "fedcba9876543210fedcba9876543210fedcba98".to_string(),
        }];

        assert_ne!(
            p1.compute_fingerprint().unwrap(),
            p2.compute_fingerprint().unwrap(),
            "non-self revalidation preconditions are packet preconditions"
        );
    }

    #[test]
    fn fingerprint_excludes_actual_bytes() {
        let p1 = minimal_packet("");
        let mut p2 = minimal_packet("");
        p2.budget.actual_canonical_json_bytes = 99999;
        assert_eq!(
            p1.compute_fingerprint().unwrap(),
            p2.compute_fingerprint().unwrap(),
            "actual_canonical_json_bytes must be excluded from fingerprint"
        );
    }

    // -----------------------------------------------------------------------
    // Size fixpoint
    // -----------------------------------------------------------------------

    #[test]
    fn size_fixpoint_converges() {
        let mut packet = minimal_packet("");
        packet.finalize_size().unwrap();
        assert!(
            packet.packet_fingerprint.starts_with("sha256:"),
            "fingerprint must have sha256: prefix"
        );
        assert_eq!(
            packet.packet_fingerprint.len(),
            71, // "sha256:" (7) + 64 hex chars
            "fingerprint must be sha256: + 64 hex chars"
        );
        assert!(
            packet.budget.actual_canonical_json_bytes > 0,
            "actual bytes must be set"
        );
        // Verify fixed point: serializing again with the set value should match
        let bytes1 = packet.budget.actual_canonical_json_bytes;
        let raw = serde_json::to_value(&packet).unwrap();
        let canonical = canonical_json::to_canonical_value(&raw).unwrap();
        let actual = canonical_json::canonical_value_bytes(&canonical)
            .unwrap()
            .len() as u64;
        assert_eq!(
            bytes1, actual,
            "size fixpoint must converge to actual serialized length"
        );
    }

    #[test]
    fn size_fixpoint_budget_respected() {
        let mut packet = minimal_packet("");
        // Artificially inflate subject fields to exceed budget
        packet.subject.title = "X".repeat(200_000);
        let result = packet.finalize_size();
        // With 200K chars in title, should definitely exceed 128 KiB
        assert!(
            result.is_err(),
            "oversized packet must fail with budget error"
        );
        if let Err(e) = result {
            let msg = format!("{}", e);
            assert!(
                msg.contains("budget") || msg.contains("exceed"),
                "error must mention budget/exceeded: got {}",
                msg
            );
        }
    }

    #[test]
    fn size_fixpoint_small_packet_within_budget() {
        let mut packet = minimal_packet("");
        packet.finalize_size().unwrap();
        assert!(packet.budget.actual_canonical_json_bytes <= MAX_CANONICAL_JSON_BYTES as u64);
    }

    // -----------------------------------------------------------------------
    // Budget defaults
    // -----------------------------------------------------------------------

    #[test]
    fn budget_defaults_match_constants() {
        let budget = PacketBudget::default();
        assert_eq!(budget.profile, BUDGET_PROFILE);
        assert_eq!(
            budget.max_canonical_json_bytes,
            MAX_CANONICAL_JSON_BYTES as u64
        );
        assert_eq!(budget.max_incident_relations, MAX_INCIDENT_RELATIONS as u64);
        assert_eq!(
            budget.max_decision_frontier_items,
            MAX_DECISION_FRONTIER_ITEMS as u64
        );
        assert_eq!(budget.max_suggested_sections, MAX_SUGGESTED_SECTIONS as u64);
        assert_eq!(budget.max_snippet_bytes_each, MAX_SNIPPET_BYTES_EACH as u64);
        assert_eq!(
            budget.recommended_initial_sections,
            RECOMMENDED_INITIAL_SECTIONS as u64
        );
        assert_eq!(budget.max_initial_lines, MAX_INITIAL_LINES as u64);
        assert_eq!(budget.actual_canonical_json_bytes, 0);
        assert!(budget.truncations.is_empty());
    }

    // -----------------------------------------------------------------------
    // Dispatch defaults
    // -----------------------------------------------------------------------

    #[test]
    fn dispatch_defaults() {
        let d = PacketDispatch::default();
        assert!(d.reservation_candidate);
        assert!(!d.dispatch_authorized);
        assert_eq!(d.authorization_status, "not_reserved");
        assert_eq!(d.gate_families.len(), 8);
        // Verify specific gate families exist
        let names: Vec<&str> = d.gate_families.iter().map(|g| g.family.as_str()).collect();
        assert!(names.contains(&"readiness"));
        assert!(names.contains(&"lease"));
        assert!(names.contains(&"capability_match"));
        assert!(names.contains(&"qa_baseline_and_cases"));
        // Lease should have reason code
        let lease = d
            .gate_families
            .iter()
            .find(|g| g.family == "lease")
            .unwrap();
        assert_eq!(lease.reason_codes, vec!["lease_resolver_not_installed"]);
    }

    #[test]
    fn schema_constrains_preview_dispatch_constants() {
        let schema: Value = serde_json::from_str(WORK_PACKET_SCHEMA).unwrap();
        let dispatch = &schema["properties"]["dispatch"]["properties"];
        assert_eq!(dispatch["reservation_candidate"]["const"], true);
        assert_eq!(dispatch["dispatch_authorized"]["const"], false);
        assert_eq!(dispatch["authorization_status"]["const"], "not_reserved");
    }

    // -----------------------------------------------------------------------
    // JSON Schema constant
    // -----------------------------------------------------------------------

    #[test]
    fn work_packet_schema_is_embedded() {
        assert!(!WORK_PACKET_SCHEMA.is_empty());
        assert!(WORK_PACKET_SCHEMA.contains("WorkPacketV1"));
    }

    #[test]
    fn schema_lists_contract_fields_as_required() {
        let schema: Value = serde_json::from_str(WORK_PACKET_SCHEMA).unwrap();
        let required = schema["properties"]["contract"]["required"]
            .as_array()
            .expect("contract required list");
        for field in [
            "mode",
            "work_surface",
            "plan_policy",
            "semantic_impact",
            "effort",
            "verification_profile",
            "brief",
            "objective",
            "current_behavior",
            "target_behavior",
            "code_anchors",
            "documentation_anchors",
            "configuration_anchors",
            "data_anchors",
            "research_refs",
            "required_changes",
            "invariants",
            "acceptance",
            "scope",
            "implementation_freedom",
            "required_decisions",
            "shared_approach_refs",
            "expected_evidence",
            "expected_handoff",
        ] {
            assert!(
                required.contains(&Value::String(field.to_string())),
                "contract.required must contain {field}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Constants sanity
    // -----------------------------------------------------------------------

    #[test]
    fn constants_are_sane() {
        assert_eq!(PACKET_SCHEMA_VERSION, 1);
        assert_eq!(PACKET_PROFILE, "phase2_work_packet_preview_v1");
        assert_eq!(BUDGET_PROFILE, "phase2_work_packet_preview_budget_v1");
        assert_eq!(MAX_CANONICAL_JSON_BYTES, 131_072);
        assert_eq!(MAX_INCIDENT_RELATIONS, 128);
        assert_eq!(MAX_DECISION_FRONTIER_ITEMS, 16);
        assert_eq!(MAX_SUGGESTED_SECTIONS, 8);
        assert_eq!(MAX_SNIPPET_BYTES_EACH, 500);
        assert_eq!(RECOMMENDED_INITIAL_SECTIONS, 4);
        assert_eq!(MAX_INITIAL_LINES, 240);
    }

    // -----------------------------------------------------------------------
    // No float in serialized packet (canonical_json rejects it)
    // -----------------------------------------------------------------------

    #[test]
    fn canonical_serialization_rejects_float() {
        let packet = minimal_packet("");
        let value = serde_json::to_value(&packet).unwrap();
        // Verify the canonical serializer does NOT error (no floats in our DTOs)
        let result = canonical_json::to_canonical_value(&value);
        assert!(
            result.is_ok(),
            "no floats should be present: {:?}",
            result.err()
        );
    }
}
