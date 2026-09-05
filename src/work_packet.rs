//! Public neutral WorkPacket DTOs.
//!
//! Every type is a pure value DTO with `#[serde(deny_unknown_fields)]` on
//! every struct. There are no graph/docs/source imports — only serde,
//! canonical_json and error primitives.
//!
//! Ownership: `src/work_packet.rs` is the public neutral value owner.
//! Cross-domain composition belongs in `src/kernel/packet.rs`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::canonical_json;
use crate::PulseResult;

// ---------------------------------------------------------------------------
// Constants / budget profile
// ---------------------------------------------------------------------------

/// Packet schema version for the current pre-release family.
pub const PACKET_SCHEMA_VERSION: u32 = 1;

/// Hard ceiling for canonical packet JSON (128 KiB).
pub const MAX_CANONICAL_JSON_BYTES: usize = 131_072;

/// Maximum incident relations before overflow.
pub const MAX_INCIDENT_RELATIONS: usize = 128;

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

/// Bounded context packet for one executable Ticket.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkPacket {
    pub schema_version: u32,
    pub profile: String,
    pub code: String,
    pub ticket: PacketTicket,
    #[serde(default)]
    pub parents: Vec<PacketParentSummary>,
    #[serde(default)]
    pub decisions: Vec<PacketDecisionSummary>,
    #[serde(default)]
    pub blockers: Vec<PacketBlockerItem>,
    #[serde(default)]
    pub related: Vec<PacketRelationItem>,
    pub docs: PacketDocs,
    pub qa: PacketQa,
    #[serde(default)]
    pub knowledge: Vec<PacketKnowledgeItem>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub rework: Vec<String>,
    pub source: PacketSource,
    #[serde(default)]
    pub tags_vocabulary: Vec<String>,
    pub handoff: PacketHandoff,
    /// sha256 fingerprint of the canonical packet content.
    pub packet_fingerprint: String,
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

/// A bounded, hash-bound markdown artifact included in a packet.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketRawFile {
    pub path: String,
    pub content_hash: String,
    pub content: String,
}

/// Ticket node plus its real markdown contract and optional plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketTicket {
    pub node: SubjectSnapshot,
    pub brief_hash: Option<String>,
    pub tags: Vec<String>,
    pub ticket_md: PacketRawFile,
    pub plan_md: Option<PacketRawFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketParentSummary {
    pub node: PacketParentRef,
    pub summary: String,
    pub approach_md: Option<PacketRawFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketDecisionSummary {
    pub id: String,
    pub title: String,
    pub status: String,
    pub decision_md: Option<PacketRawFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketDocs {
    pub required: Vec<PacketDocRef>,
    pub suggested: Vec<PacketSuggestedSection>,
    pub write_candidates: Vec<PacketDocRef>,
    pub excluded: Vec<PacketExcludedDocRef>,
    pub read_budget: PacketReadBudget,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketQa {
    pub posture: String,
    pub cases: Vec<PacketRawFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketKnowledgeItem {
    pub summary: String,
    pub why_applicable: String,
    pub required_checks: Vec<String>,
    pub detail_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketHandoff {
    pub commands: Vec<String>,
}

// ---------------------------------------------------------------------------
// Graph
// ---------------------------------------------------------------------------

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
    pub status: String,
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
    pub status: String,
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
// Source
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PacketSource {
    pub repository_id: String,
    pub commit: String,
    pub dirty: bool,
    /// sha256 over the tracked diff plus untracked manifest at packet build
    /// time. Part of the packet fingerprint: any source mutation after the
    /// packet is built changes it and makes the packet stale.
    pub dirty_hash: String,
}

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

impl WorkPacket {
    /// Normalize set-like collections for deterministic packet bytes.
    pub fn normalize(&mut self) {
        self.parents.sort_by(|a, b| a.node.id.cmp(&b.node.id));
        self.decisions.sort_by(|a, b| a.id.cmp(&b.id));
        self.blockers.sort_by(|a, b| a.id.cmp(&b.id));
        self.related.sort_by(|a, b| a.edge_id.cmp(&b.edge_id));
        self.docs.required.sort_by(|a, b| a.id.cmp(&b.id));
        self.docs.suggested.sort_by_key(|a| a.rank);
        self.docs.write_candidates.sort_by(|a, b| a.id.cmp(&b.id));
        self.docs.excluded.sort_by(|a, b| a.id.cmp(&b.id));
        sort_strings(&mut self.ticket.tags);
        sort_strings(&mut self.tags_vocabulary);
        sort_strings(&mut self.notes);
        sort_strings(&mut self.rework);
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

// ---------------------------------------------------------------------------
// Sorting helpers
// ---------------------------------------------------------------------------

fn sort_strings(v: &mut Vec<String>) {
    v.sort();
    v.dedup();
}

// ---------------------------------------------------------------------------
// Fingerprint projection
// ---------------------------------------------------------------------------

impl WorkPacket {
    /// Compute the canonical fingerprint, excluding only its self-reference.
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

                let cleaned = strip_self_referential_fields(child);
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

fn validate_json_schema_contract(value: &Value) -> PulseResult<()> {
    let schema: Value = serde_json::from_str(WORK_PACKET_SCHEMA)?;
    let compiled = jsonschema::JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .compile(&schema)
        .map_err(|error| {
            crate::PulseError::validation(
                "work_packet_schema_invalid",
                format!("embedded work packet schema is invalid: {error}"),
            )
        })?;
    if let Err(errors) = compiled.validate(value) {
        let messages = errors
            .take(8)
            .map(|error| format!("{}: {}", error.instance_path, error))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(crate::PulseError::validation(
            "work_packet_schema_invalid",
            format!("work packet JSON failed schema validation: {messages}"),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Size fixpoint
// ---------------------------------------------------------------------------

impl WorkPacket {
    /// Set the fingerprint, validate the schema, and enforce the packet bound.
    pub fn finalize_size(&mut self) -> PulseResult<()> {
        self.packet_fingerprint = self.compute_fingerprint()?;
        self.validate_schema_contract()?;
        let raw = serde_json::to_value(&*self)?;
        let canonical = canonical_json::to_canonical_value(&raw)?;
        let bytes = canonical_json::canonical_value_bytes(&canonical)?;
        self.enforce_budget(bytes.len() as u64)
    }

    pub fn validate_schema_contract(&self) -> PulseResult<()> {
        let value = serde_json::to_value(self)?;
        validate_json_schema_contract(&value)
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
// Schema constant
// ---------------------------------------------------------------------------

/// Embedded JSON schema for WorkPacket.
pub const WORK_PACKET_SCHEMA: &str = include_str!("schema/work-packet.schema.json");
