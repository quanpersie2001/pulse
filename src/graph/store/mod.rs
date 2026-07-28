use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::event::new_event_id;
use crate::event::{event_path, EventEnvelope};
use crate::graph::contract::{
    validate_node_contract_result, validate_public_create_classification, ContractValidationMode,
    DecisionWorkContract, ImplementationContract, PublicCreateClassification, QaImpactPosture,
    QaMetadata, ShapingPointer, TicketRole,
};
use crate::graph::edge::{canonical_endpoints, deterministic_edge_id, Edge, EdgeType};
use crate::graph::executability::{structural_executability, StructuralExecutabilityReport};
use crate::graph::manifest::{Manifest, EDGE_SCHEMA, NODE_SCHEMA};
use crate::graph::node::{
    DocumentationImpact, DocumentationImpactPosture, DocumentationMetadata, DocumentationRouting,
    Node, NodeStatus, StatusReason,
};
use crate::graph::projection::{graph_fingerprint, GraphProjection};
use crate::graph::rollup::{rollup, RollupReport};
use crate::graph::traversal::{affected_by, neighborhood, AffectedByReport, NeighborhoodReport};
use crate::graph::validate::{
    validate_edge_filename, validate_edge_for_add, validate_graph, validate_node_filename,
    ValidationReport,
};
use crate::id::{format_id, parse_numeric, validate_id_for_kind, WorkKind};
use crate::storage::transaction::{
    commit_prepared_multi_target_transaction, commit_prepared_transaction, current_file_state,
    prepare_multi_target_transaction, prepare_transaction, recover_prepared_transactions,
    FileState, MultiTargetTransactionIntent, TransactionFailpoint, TransactionIntent,
    TransactionTarget,
};
use crate::storage::{self, WriteGuard};
use crate::{PulseError, PulseResult};

mod bootstrap;
mod claim;
mod contracts;
mod edges;
mod nodes;
mod repository;
mod supersession;

pub use bootstrap::{
    bootstrap, default_manifest_value, BootstrapOutcome, EDGE_SCHEMA_JSON, MANIFEST_JSON,
    NODE_SCHEMA_JSON,
};
pub use claim::{ClaimArgs, ClaimWorkOutcome};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedProjection {
    cache_schema_version: u32,
    graph_fingerprint: String,
    projection_schema_version: u32,
    projection: GraphProjection,
}

fn export_with_cache(
    repo_root: &Path,
    manifest: &Manifest,
    node_files: &[(PathBuf, Node)],
    edge_files: &[(PathBuf, Edge)],
) -> PulseResult<GraphProjection> {
    let fingerprint = graph_fingerprint(manifest, node_files, edge_files)?;
    let cache_path = repo_root.join(".pulse/cache/workgraph.snapshot.json");
    if let Ok(bytes) = fs::read(&cache_path) {
        if let Ok(cache) = serde_json::from_slice::<CachedProjection>(&bytes) {
            if cache.cache_schema_version == 1
                && cache.projection_schema_version
                    == crate::graph::projection::PROJECTION_SCHEMA_VERSION
                && cache.graph_fingerprint == fingerprint
                && cache.projection.graph_fingerprint == fingerprint
            {
                return Ok(cache.projection);
            }
        }
    }
    let projection =
        crate::graph::projection::build_projection(fingerprint, node_files, edge_files);
    let cache = CachedProjection {
        cache_schema_version: 1,
        graph_fingerprint: projection.graph_fingerprint.clone(),
        projection_schema_version: crate::graph::projection::PROJECTION_SCHEMA_VERSION,
        projection: projection.clone(),
    };
    let bytes = to_canonical_bytes(&cache)?;
    storage::atomic_write(&cache_path, &bytes)?;
    Ok(projection)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MutationStatus {
    Created,
    Updated,
    Unchanged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationOutcome<T> {
    pub schema_version: u32,
    pub code: String,
    pub status: MutationStatus,
    pub value: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListOutcome<T> {
    pub schema_version: u32,
    pub code: String,
    pub items: Vec<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentationImpactUpdate {
    pub posture: DocumentationImpactPosture,
    pub rationale: Option<String>,
    pub required_documents: Vec<String>,
    pub deferred_to: Vec<String>,
    pub paths: Vec<String>,
    pub domains: Vec<String>,
    pub labels: Vec<String>,
}

/// Typed whole-replacement request for `work contract set`.
///
/// The contract setter is a safe typed replacement rather than an arbitrary
/// JSON Patch: exactly one role-specific contract may be supplied and it must
/// match the Ticket's declared role. Contract mutation bumps both the normal
/// CAS `revision` and the semantic `contract_revision`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContractSetRequest {
    pub role: TicketRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implementation: Option<ImplementationContract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_work: Option<DecisionWorkContract>,
}

/// Minimal readiness-only QA impact update for `work qa-impact set`.
///
/// QA impact is a semantic contract input: mutation bumps both `revision` and
/// `contract_revision`. The `none` and `covered_by_story_close` postures are
/// authority-gated; baseline/case resolution remains a future Phase 3 family.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaImpactUpdate {
    pub posture: QaImpactPosture,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behavioral_owner: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affected_case_ids: Vec<String>,
}

/// Read-only view returned by `work shaping show`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShapingView {
    pub schema_version: u32,
    pub code: String,
    pub owner_id: String,
    pub revision: u64,
    pub contract_revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shaping: Option<ShapingPointer>,
}

/// Read-only view returned by `work contract show`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractView {
    pub schema_version: u32,
    pub code: String,
    pub ticket_id: String,
    pub revision: u64,
    pub contract_revision: u64,
    pub role: Option<TicketRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implementation: Option<ImplementationContract>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_work: Option<DecisionWorkContract>,
}

/// Read-only view returned by `work qa-impact show`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QaImpactView {
    pub schema_version: u32,
    pub code: String,
    pub ticket_id: String,
    pub revision: u64,
    pub contract_revision: u64,
    pub qa: Option<QaMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SupersessionAssertion {
    pub assertion_version: u32,
    pub asserted_by: String,
    pub source_revisions: Vec<String>,
    pub claim: SupersessionClaim,
    pub references: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupersessionClaim {
    Absorbed,
    FollowUpRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupersessionTarget {
    Replacement { id: String },
    Decision { id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupersededWork {
    pub node: Node,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<Edge>,
    pub target: SupersessionTarget,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assertion: Option<SupersessionAssertion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconciliation_receipt: Option<crate::evidence::model::ReceiptReference>,
}

#[derive(Debug, Clone)]
pub struct OperationContext {
    pub actor: String,
    pub now: DateTime<Utc>,
}

impl Default for OperationContext {
    fn default() -> Self {
        Self {
            actor: "human:unknown".to_string(),
            now: Utc::now(),
        }
    }
}

pub struct JsonGraphStore {
    pub(crate) repo_root: PathBuf,
    pub(crate) failpoint: Option<TransactionFailpoint>,
    pub(crate) work_packet_after_first_fence_failpoint: bool,
}

impl JsonGraphStore {
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
            failpoint: None,
            work_packet_after_first_fence_failpoint: false,
        }
    }

    #[cfg(any(test, debug_assertions))]
    pub fn with_failpoint(repo_root: impl Into<PathBuf>, failpoint: TransactionFailpoint) -> Self {
        Self {
            repo_root: repo_root.into(),
            failpoint: Some(failpoint),
            work_packet_after_first_fence_failpoint: false,
        }
    }

    #[cfg(any(test, debug_assertions))]
    pub fn with_work_packet_after_first_fence_failpoint(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
            failpoint: None,
            work_packet_after_first_fence_failpoint: true,
        }
    }

    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    pub fn failpoint(&self) -> Option<TransactionFailpoint> {
        self.failpoint
    }
}
