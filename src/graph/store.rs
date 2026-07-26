use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::event::{event_path, EventEnvelope};
use crate::graph::contract::{
    validate_public_create_classification, ContractValidationMode, PublicCreateClassification,
};
use crate::graph::edge::{canonical_endpoints, deterministic_edge_id, Edge, EdgeType};
use crate::graph::executability::{structural_executability, StructuralExecutabilityReport};
use crate::graph::lifecycle::{status_requires_reason, validate_transition, TransitionReason};
use crate::graph::manifest::{Manifest, EDGE_SCHEMA, NODE_SCHEMA};
use crate::graph::node::{
    DocumentationImpact, DocumentationImpactPosture, DocumentationMetadata, DocumentationRouting,
    Node, NodeStatus, StatusReason,
};
use crate::graph::projection::{export_with_cache, graph_fingerprint, GraphProjection};
use crate::graph::rollup::{rollup, RollupReport};
use crate::graph::traversal::{affected_by, neighborhood, AffectedByReport, NeighborhoodReport};
use crate::graph::validate::{
    validate_edge_filename, validate_edge_for_add, validate_graph, validate_node_filename,
    ValidationReport,
};
use crate::id::{format_id, new_event_id, parse_numeric, validate_id_for_kind, WorkKind};
use crate::storage::transaction::{
    commit_prepared_multi_target_transaction, commit_prepared_transaction, current_file_state,
    prepare_multi_target_transaction, prepare_transaction, recover_prepared_transactions,
    FileState, MultiTargetTransactionIntent, TransactionFailpoint, TransactionIntent,
    TransactionTarget,
};
use crate::storage::{self, WriteGuard};
use crate::{PulseError, PulseResult};

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
    repo_root: PathBuf,
    failpoint: Option<TransactionFailpoint>,
}

impl JsonGraphStore {
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
            failpoint: None,
        }
    }

    #[cfg(any(test, debug_assertions))]
    pub fn with_failpoint(repo_root: impl Into<PathBuf>, failpoint: TransactionFailpoint) -> Self {
        Self {
            repo_root: repo_root.into(),
            failpoint: Some(failpoint),
        }
    }

    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    pub fn failpoint(&self) -> Option<TransactionFailpoint> {
        self.failpoint
    }

    pub fn bootstrap(&self) -> PulseResult<()> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        Ok(())
    }

    pub fn bootstrap_unlocked(&self) -> PulseResult<()> {
        // Fresh baseline initialization writes the node schema before durable
        // manifest/edge schema markers. If interrupted, only the safe current
        // partial layout may be completed; unknown existing state remains
        // refused without overwrite.
        match self.classify_workgraph_bootstrap_state()? {
            WorkgraphBootstrapState::Empty | WorkgraphBootstrapState::SafePartialCurrent => {
                self.ensure_current_workgraph_baseline_unlocked()?;
            }
            WorkgraphBootstrapState::ExistingCurrent => {
                self.ensure_workgraph_layout_unlocked()?;
            }
            WorkgraphBootstrapState::MissingNodeSchemaWithState => {
                return Err(PulseError::validation(
                    "node_schema_missing_refused",
                    "node schema is missing while existing workgraph state is present; refusing bootstrap without overwrite",
                ));
            }
            WorkgraphBootstrapState::NodeSchemaDrift { hash } => {
                return Err(PulseError::validation(
                    "node_schema_drift_refused",
                    format!(
                        "refusing to overwrite node schema drift {}; resolve schema state explicitly",
                        hash
                    ),
                ));
            }
            WorkgraphBootstrapState::UnexpectedPartialState => {
                return Err(PulseError::validation(
                    "workgraph_partial_state_refused",
                    "workgraph contains partial state that is not a safe current baseline initialization; refusing bootstrap without overwrite",
                ));
            }
        }
        Ok(())
    }

    /// Create the complete current baseline. The node schema is written first so
    /// a fresh initialization interrupted after durable marker writes remains a
    /// recognizable current partial layout.
    fn ensure_current_workgraph_baseline_unlocked(&self) -> PulseResult<()> {
        let wg = self.workgraph_dir();
        fs::create_dir_all(wg.join("schemas"))
            .map_err(|e| PulseError::io(wg.join("schemas"), e))?;
        self.write_current_node_schema_if_absent_unlocked()?;
        self.ensure_workgraph_layout_unlocked()
    }

    /// Create workgraph directories, manifest, and edge schema without touching an
    /// existing node schema. Used after classification has already proven the
    /// repository is either fresh or already on the current baseline.
    fn ensure_workgraph_layout_unlocked(&self) -> PulseResult<()> {
        let wg = self.workgraph_dir();
        fs::create_dir_all(wg.join("nodes")).map_err(|e| PulseError::io(wg.join("nodes"), e))?;
        fs::create_dir_all(wg.join("edges")).map_err(|e| PulseError::io(wg.join("edges"), e))?;
        fs::create_dir_all(wg.join("schemas"))
            .map_err(|e| PulseError::io(wg.join("schemas"), e))?;
        self.write_if_absent(&wg.join("manifest.json"), &Manifest::default())?;
        self.write_bytes_if_absent(&wg.join("schemas/edge.schema.json"), EDGE_SCHEMA.as_bytes())?;
        Ok(())
    }

    pub fn create_node_with_context(
        &self,
        kind: WorkKind,
        title: String,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        self.create_node_with_classification_context(
            kind,
            title,
            PublicCreateClassification::default(),
            ContractValidationMode::CanonicalStorage,
            ctx,
        )
    }

    pub fn create_node_public_with_context(
        &self,
        kind: WorkKind,
        title: String,
        classification: PublicCreateClassification,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        validate_public_create_classification(kind, &classification)?;
        self.create_node_with_classification_context(
            kind,
            title,
            classification,
            ContractValidationMode::PublicCreate,
            ctx,
        )
    }

    fn create_node_with_classification_context(
        &self,
        kind: WorkKind,
        title: String,
        classification: PublicCreateClassification,
        validation_mode: ContractValidationMode,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let id = self.allocate_id(kind)?;
        let mut node = Node::new(id.clone(), kind, title, ctx.now)?;
        if kind == WorkKind::Ticket && classification.any_present() {
            node.role = classification.role;
            node.risk = classification.risk;
            node.materialization = classification.materialization;
        }
        let nodes = self.load_nodes()?;
        let edges = self.load_edges()?;
        validate_id_for_kind(&id, kind)?;
        let path = self.node_path(&id);
        if path.exists() {
            return Err(PulseError::AlreadyExists { subject: id });
        }
        let mut all_nodes = nodes.clone();
        all_nodes.insert(node.id.clone(), node.clone());
        let all_node_values = all_nodes.values().cloned().collect::<Vec<_>>();
        let edge_values = edges.iter().map(|(_, e)| e.clone()).collect::<Vec<_>>();
        validate_graph(
            &self.repo_root,
            &self.manifest()?,
            &all_node_values,
            &edge_values,
        )
        .into_result()?;
        crate::graph::contract::validate_node_contract_result(&node, validation_mode)?;
        let after_bytes = to_canonical_bytes(&node)?;
        self.commit_mutation(
            "work.node.created",
            ctx.actor,
            &node.id,
            json!({"node": node}),
            &path,
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&after_bytes),
                revision: node.revision,
            },
            &after_bytes,
            ctx.now,
        )?;
        Ok(MutationOutcome {
            schema_version: 1,
            code: "created".to_string(),
            status: MutationStatus::Created,
            value: node,
        })
    }

    pub fn create_node(&self, kind: WorkKind, title: String) -> PulseResult<MutationOutcome<Node>> {
        self.create_node_with_context(kind, title, OperationContext::default())
    }

    pub fn show_node(&self, id: &str) -> PulseResult<Node> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let path = self.node_path(id);
        if !path.exists() {
            return Err(PulseError::NotFound {
                subject: id.to_string(),
            });
        }
        storage::read_json(&path)
    }

    pub fn list_nodes(&self, kind: Option<WorkKind>) -> PulseResult<ListOutcome<Node>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let mut nodes: Vec<_> = self.load_nodes()?.into_values().collect();
        if let Some(kind) = kind {
            nodes.retain(|n| n.kind == kind);
        }
        nodes.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(ListOutcome {
            schema_version: 1,
            code: "ok".to_string(),
            items: nodes,
        })
    }

    pub fn edit_title_with_context(
        &self,
        id: &str,
        expected_revision: u64,
        title: String,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        if title.trim().is_empty() {
            return Err(PulseError::validation(
                "invalid_title",
                "title must not be empty",
            ));
        }
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let path = self.node_path(id);
        if !path.exists() {
            return Err(PulseError::NotFound {
                subject: id.to_string(),
            });
        }
        let before_bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        let mut node: Node = serde_json::from_slice(&before_bytes)
            .map_err(|error| PulseError::json(&path, error))?;
        if node.revision != expected_revision {
            return Err(PulseError::CasConflict {
                subject: id.to_string(),
                expected_revision,
                current_revision: node.revision,
            });
        }
        node.title = title;
        node.revision += 1;
        node.updated_at = ctx.now;
        let node_values = self
            .load_nodes_with_override(node.clone())?
            .into_values()
            .collect::<Vec<_>>();
        let edge_values = self
            .load_edges()?
            .iter()
            .map(|(_, e)| e.clone())
            .collect::<Vec<_>>();
        validate_graph(
            &self.repo_root,
            &self.manifest()?,
            &node_values,
            &edge_values,
        )
        .into_result()?;
        let after_bytes = to_canonical_bytes(&node)?;
        self.commit_mutation(
            "work.node.updated",
            ctx.actor,
            id,
            json!({"node": node, "expected_revision": expected_revision}),
            &path,
            FileState::Present {
                hash: hash_bytes(&before_bytes),
                revision: expected_revision,
            },
            FileState::Present {
                hash: hash_bytes(&after_bytes),
                revision: expected_revision + 1,
            },
            &after_bytes,
            ctx.now,
        )?;
        Ok(MutationOutcome {
            schema_version: 1,
            code: "updated".to_string(),
            status: MutationStatus::Updated,
            value: node,
        })
    }

    pub fn edit_title(
        &self,
        id: &str,
        expected_revision: u64,
        title: String,
    ) -> PulseResult<MutationOutcome<Node>> {
        self.edit_title_with_context(id, expected_revision, title, OperationContext::default())
    }

    pub fn update_documentation_impact_with_context(
        &self,
        ticket_id: &str,
        expected_revision: u64,
        update: DocumentationImpactUpdate,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        let documentation = DocumentationMetadata {
            impact: DocumentationImpact {
                posture: update.posture,
                rationale: update.rationale,
                required_documents: update.required_documents,
                deferred_to: update.deferred_to,
            },
            routing: DocumentationRouting {
                paths: update.paths,
                domains: update.domains,
                labels: update.labels,
            },
        };
        documentation.validate(true)?;
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let path = self.node_path(ticket_id);
        if !path.exists() {
            return Err(PulseError::NotFound {
                subject: ticket_id.to_string(),
            });
        }
        let before_bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        let mut node: Node = serde_json::from_slice(&before_bytes)
            .map_err(|error| PulseError::json(&path, error))?;
        if node.kind != WorkKind::Ticket {
            return Err(PulseError::validation(
                "documentation_impact_requires_ticket",
                format!("documentation impact can only be set on tickets: {ticket_id}"),
            ));
        }
        if node.revision != expected_revision {
            return Err(PulseError::CasConflict {
                subject: ticket_id.to_string(),
                expected_revision,
                current_revision: node.revision,
            });
        }
        let nodes = self.load_nodes()?;
        for target in &documentation.impact.deferred_to {
            if !nodes.contains_key(target) {
                return Err(PulseError::validation(
                    "documentation_defer_target_missing",
                    format!("deferred documentation target does not exist: {target}"),
                ));
            }
        }
        let previous_documentation = node.documentation.clone();
        node.documentation = Some(documentation.clone());
        node.contract_revision += 1;
        node.revision += 1;
        node.updated_at = ctx.now;
        let node_values = self
            .load_nodes_with_override(node.clone())?
            .into_values()
            .collect::<Vec<_>>();
        let edge_values = self
            .load_edges()?
            .iter()
            .map(|(_, e)| e.clone())
            .collect::<Vec<_>>();
        validate_graph(
            &self.repo_root,
            &self.manifest()?,
            &node_values,
            &edge_values,
        )
        .into_result()?;
        let after_bytes = to_canonical_bytes(&node)?;
        self.commit_mutation(
            "work.documentation_impact.updated",
            ctx.actor,
            ticket_id,
            json!({
                "ticket_id": ticket_id,
                "expected_revision": expected_revision,
                "new_revision": node.revision,
                "previous_documentation": previous_documentation,
                "documentation": documentation,
                "gate_coverage": ["ticket_kind", "node_revision_cas", "documentation_impact_validation", "deferred_work_refs", "graph_integrity"]
            }),
            &path,
            FileState::Present {
                hash: hash_bytes(&before_bytes),
                revision: expected_revision,
            },
            FileState::Present {
                hash: hash_bytes(&after_bytes),
                revision: expected_revision + 1,
            },
            &after_bytes,
            ctx.now,
        )?;
        Ok(MutationOutcome {
            schema_version: 1,
            code: "updated".to_string(),
            status: MutationStatus::Updated,
            value: node,
        })
    }

    pub fn update_documentation_impact(
        &self,
        ticket_id: &str,
        expected_revision: u64,
        update: DocumentationImpactUpdate,
        actor: String,
    ) -> PulseResult<MutationOutcome<Node>> {
        self.update_documentation_impact_with_context(
            ticket_id,
            expected_revision,
            update,
            OperationContext {
                actor,
                now: Utc::now(),
            },
        )
    }

    pub fn transition_node_with_context(
        &self,
        id: &str,
        to: crate::graph::node::NodeStatus,
        expected_revision: u64,
        reason: Option<TransitionReason>,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let path = self.node_path(id);
        if !path.exists() {
            return Err(PulseError::NotFound {
                subject: id.to_string(),
            });
        }
        let before_bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        let mut node: Node = serde_json::from_slice(&before_bytes)
            .map_err(|error| PulseError::json(&path, error))?;
        if node.revision != expected_revision {
            return Err(PulseError::CasConflict {
                subject: id.to_string(),
                expected_revision,
                current_revision: node.revision,
            });
        }
        let from = node.status;
        let exp = validate_transition(from, to, reason.as_ref())?;
        let transition_reason = reason.clone();
        node.status = to;
        node.status_reason = if status_requires_reason(to) {
            Some(
                reason
                    .clone()
                    .ok_or_else(|| {
                        PulseError::validation(
                            "missing_status_reason",
                            "transition requires a non-empty reason",
                        )
                    })?
                    .into_status_reason(),
            )
        } else {
            None
        };
        node.revision += 1;
        node.updated_at = ctx.now;
        let node_values = self
            .load_nodes_with_override(node.clone())?
            .into_values()
            .collect::<Vec<_>>();
        let edge_values = self
            .load_edges()?
            .iter()
            .map(|(_, e)| e.clone())
            .collect::<Vec<_>>();
        validate_graph(
            &self.repo_root,
            &self.manifest()?,
            &node_values,
            &edge_values,
        )
        .into_result()?;
        let graph_fingerprint_before = self.graph_fingerprint_current_unlocked()?;
        let graph_fingerprint_after = self.graph_fingerprint_with_planned_workgraph(&node, None)?;
        let after_bytes = to_canonical_bytes(&node)?;
        self.commit_mutation(
            "work.node.transitioned",
            ctx.actor,
            id,
            json!({
                "from": from,
                "to": to,
                "expected_revision": expected_revision,
                "reason": transition_reason,
                "graph_fingerprint_before": graph_fingerprint_before,
                "graph_fingerprint_after": graph_fingerprint_after,
                "gate_coverage": ["transition_direction", "graph_integrity"],
                "target_requires_status_reason": exp.target_requires_status_reason,
            }),
            &path,
            FileState::Present {
                hash: hash_bytes(&before_bytes),
                revision: expected_revision,
            },
            FileState::Present {
                hash: hash_bytes(&after_bytes),
                revision: expected_revision + 1,
            },
            &after_bytes,
            ctx.now,
        )?;
        Ok(MutationOutcome {
            schema_version: 1,
            code: "transitioned".to_string(),
            status: MutationStatus::Updated,
            value: node,
        })
    }

    pub fn transition_node(
        &self,
        id: &str,
        to: crate::graph::node::NodeStatus,
        expected_revision: u64,
        reason: Option<TransitionReason>,
        actor: String,
    ) -> PulseResult<MutationOutcome<Node>> {
        self.transition_node_with_context(
            id,
            to,
            expected_revision,
            reason,
            OperationContext {
                actor,
                now: Utc::now(),
            },
        )
    }

    pub fn supersede_work_with_context(
        &self,
        old_id: &str,
        target: SupersessionTarget,
        expected_revision: u64,
        reason: String,
        assertion: SupersessionAssertion,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<SupersededWork>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let old_path = self.node_path(old_id);
        if !old_path.exists() {
            return Err(PulseError::NotFound {
                subject: old_id.to_string(),
            });
        }
        let before_bytes = fs::read(&old_path).map_err(|error| PulseError::io(&old_path, error))?;
        let mut old: Node = serde_json::from_slice(&before_bytes)
            .map_err(|error| PulseError::json(&old_path, error))?;

        let nodes = self.load_nodes()?;
        let edges = self
            .load_edges()?
            .into_iter()
            .map(|(_, e)| e)
            .collect::<Vec<_>>();
        if old.revision != expected_revision {
            if let Some(existing) = self.same_supersession(&old, &target, &assertion, &edges) {
                return Ok(MutationOutcome {
                    schema_version: 1,
                    code: "unchanged".to_string(),
                    status: MutationStatus::Unchanged,
                    value: SupersededWork {
                        node: old,
                        edge: existing,
                        target,
                        assertion: Some(assertion),
                        reconciliation_receipt: None,
                    },
                });
            }
            return Err(PulseError::CasConflict {
                subject: old_id.to_string(),
                expected_revision,
                current_revision: old.revision,
            });
        }
        if reason.trim().is_empty() {
            return Err(PulseError::validation(
                "reason_required",
                "supersession requires a non-empty reason",
            ));
        }
        validate_supersession_assertion(&assertion, &nodes)?;
        let existing_outgoing = superseded_by_edges(&edges, old_id);
        if old.status == NodeStatus::Superseded {
            if let Some(existing) = self.same_supersession(&old, &target, &assertion, &edges) {
                return Ok(MutationOutcome {
                    schema_version: 1,
                    code: "unchanged".to_string(),
                    status: MutationStatus::Unchanged,
                    value: SupersededWork {
                        node: old,
                        edge: existing,
                        target,
                        assertion: Some(assertion),
                        reconciliation_receipt: None,
                    },
                });
            }
            return Err(PulseError::validation(
                "supersession_conflict",
                "work is already superseded by a different target or assertion",
            ));
        }
        if !matches!(
            old.status,
            NodeStatus::Draft | NodeStatus::Shaped | NodeStatus::Ready | NodeStatus::Blocked
        ) {
            return Err(PulseError::validation(
                "supersession_unavailable",
                format!("status {:?} cannot be superseded", old.status),
            ));
        }
        if !existing_outgoing.is_empty() {
            return Err(PulseError::validation(
                "supersession_conflict",
                "work already has an outgoing superseded_by edge",
            ));
        }

        let edge = match &target {
            SupersessionTarget::Replacement { id } => {
                let replacement = nodes.get(id).ok_or_else(|| PulseError::NotFound {
                    subject: id.clone(),
                })?;
                if id == old_id {
                    return Err(PulseError::validation(
                        "supersession_cycle",
                        "work cannot supersede itself",
                    ));
                }
                if matches!(
                    replacement.status,
                    NodeStatus::Cancelled | NodeStatus::Superseded
                ) {
                    return Err(PulseError::validation(
                        "invalid_supersession_target",
                        "replacement must not be cancelled or superseded",
                    ));
                }
                let planned_edge = Edge::new(
                    EdgeType::SupersededBy,
                    old_id.to_string(),
                    id.clone(),
                    ctx.actor.clone(),
                    ctx.now,
                )?;
                let mut all_edges = edges.clone();
                all_edges.push(planned_edge.clone());
                if supersession_reaches(&all_edges, id, old_id) {
                    return Err(PulseError::validation(
                        "supersession_cycle",
                        "supersession edge would create a cycle",
                    ));
                }
                Some(planned_edge)
            }
            SupersessionTarget::Decision { id } => {
                let decision = nodes.get(id).ok_or_else(|| PulseError::NotFound {
                    subject: id.clone(),
                })?;
                if decision.kind != WorkKind::Decision {
                    return Err(PulseError::validation(
                        "invalid_supersession_target",
                        "decision target must have kind Decision",
                    ));
                }
                None
            }
        };

        old.status = NodeStatus::Superseded;
        old.status_reason = Some(StatusReason::new(
            "superseded",
            reason.clone(),
            match &target {
                SupersessionTarget::Replacement { .. } => None,
                SupersessionTarget::Decision { id } => Some(id.clone()),
            },
        )?);
        old.revision += 1;
        old.updated_at = ctx.now;

        let mut all_nodes = nodes.clone();
        all_nodes.insert(old.id.clone(), old.clone());
        let mut all_edges = edges.clone();
        if let Some(edge) = &edge {
            all_edges.push(edge.clone());
        }
        validate_graph(
            &self.repo_root,
            &self.manifest()?,
            &all_nodes.values().cloned().collect::<Vec<_>>(),
            &all_edges,
        )
        .into_result()?;

        let graph_fingerprint_before = self.graph_fingerprint_current_unlocked()?;
        let graph_fingerprint_after =
            self.graph_fingerprint_with_planned_workgraph(&old, edge.as_ref())?;
        let old_after_bytes = to_canonical_bytes(&old)?;
        let event = EventEnvelope::new(
            new_event_id(),
            "work.node.superseded",
            ctx.actor.clone(),
            old_id,
            json!({
                "old_id": old_id,
                "expected_revision": expected_revision,
                "new_revision": old.revision,
                "target": target,
                "reason": reason,
                "assertion": assertion,
                "graph_fingerprint_before": graph_fingerprint_before,
                "graph_fingerprint_after": graph_fingerprint_after,
                "gate_coverage": ["supersession_preconditions", "assertion_identity", "graph_integrity"],
            }),
            ctx.now,
        );
        match &edge {
            Some(edge) => {
                let edge_path = self.edge_path(&edge.id);
                if edge_path.exists() {
                    return Err(PulseError::AlreadyExists {
                        subject: edge.id.clone(),
                    });
                }
                let edge_after_bytes = to_canonical_bytes(edge)?;
                let targets = vec![
                    TransactionTarget::new(
                        old_path.clone(),
                        FileState::Present {
                            hash: hash_bytes(&before_bytes),
                            revision: expected_revision,
                        },
                        FileState::Present {
                            hash: hash_bytes(&old_after_bytes),
                            revision: expected_revision + 1,
                        },
                        &old_after_bytes,
                    ),
                    TransactionTarget::new(
                        edge_path,
                        FileState::Absent,
                        FileState::Present {
                            hash: hash_bytes(&edge_after_bytes),
                            revision: edge.revision,
                        },
                        &edge_after_bytes,
                    ),
                ];
                let intent = MultiTargetTransactionIntent::prepared(
                    event.id.clone(),
                    event.event_type.clone(),
                    ctx.actor,
                    targets,
                    event_path(&self.repo_root, &event),
                    serde_json::to_value(&event)?,
                )?;
                let prepared = prepare_multi_target_transaction(&self.repo_root, intent)?;
                commit_prepared_multi_target_transaction(&prepared, self.failpoint)?;
            }
            None => {
                self.commit_mutation(
                    "work.node.superseded",
                    ctx.actor,
                    old_id,
                    serde_json::to_value(&event.payload)?,
                    &old_path,
                    FileState::Present {
                        hash: hash_bytes(&before_bytes),
                        revision: expected_revision,
                    },
                    FileState::Present {
                        hash: hash_bytes(&old_after_bytes),
                        revision: expected_revision + 1,
                    },
                    &old_after_bytes,
                    ctx.now,
                )?;
            }
        }

        Ok(MutationOutcome {
            schema_version: 1,
            code: "superseded".to_string(),
            status: MutationStatus::Updated,
            value: SupersededWork {
                node: old,
                edge,
                target,
                assertion: Some(assertion),
                reconciliation_receipt: None,
            },
        })
    }

    pub fn supersede_work(
        &self,
        old_id: &str,
        target: SupersessionTarget,
        expected_revision: u64,
        reason: String,
        assertion: SupersessionAssertion,
        actor: String,
    ) -> PulseResult<MutationOutcome<SupersededWork>> {
        self.supersede_work_with_context(
            old_id,
            target,
            expected_revision,
            reason,
            assertion,
            OperationContext {
                actor,
                now: Utc::now(),
            },
        )
    }

    pub fn supersede_work_with_receipt(
        &self,
        old_id: &str,
        target: SupersessionTarget,
        expected_revision: u64,
        reason: String,
        receipt_id: String,
        actor: String,
    ) -> PulseResult<MutationOutcome<SupersededWork>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        crate::evidence::manifest::bootstrap(&self.repo_root)?;
        let old_path = self.node_path(old_id);
        if !old_path.exists() {
            return Err(PulseError::NotFound {
                subject: old_id.to_string(),
            });
        }
        let before_bytes = fs::read(&old_path).map_err(|error| PulseError::io(&old_path, error))?;
        let mut old: Node = serde_json::from_slice(&before_bytes)
            .map_err(|error| PulseError::json(&old_path, error))?;
        let nodes = self.load_nodes()?;
        let edges = self
            .load_edges()?
            .into_iter()
            .map(|(_, e)| e)
            .collect::<Vec<_>>();
        let target_id = match &target {
            SupersessionTarget::Replacement { id } | SupersessionTarget::Decision { id } => {
                id.clone()
            }
        };
        if old.revision != expected_revision {
            if let Some((existing_edge, receipt_ref)) =
                self.same_supersession_receipt(old_id, &target, &receipt_id, &edges)?
            {
                return Ok(MutationOutcome {
                    schema_version: 1,
                    code: "unchanged".to_string(),
                    status: MutationStatus::Unchanged,
                    value: SupersededWork {
                        node: old,
                        edge: existing_edge,
                        target,
                        assertion: None,
                        reconciliation_receipt: Some(receipt_ref),
                    },
                });
            }
            return Err(PulseError::CasConflict {
                subject: old_id.to_string(),
                expected_revision,
                current_revision: old.revision,
            });
        }
        if reason.trim().is_empty() {
            return Err(PulseError::validation(
                "reason_required",
                "supersession requires a non-empty reason",
            ));
        }
        let target_node = nodes.get(&target_id).ok_or_else(|| PulseError::NotFound {
            subject: target_id.clone(),
        })?;
        let receipt_ref = crate::evidence::receipt::validate_for_supersession(
            &self.repo_root,
            &receipt_id,
            old_id,
            expected_revision,
            &target_id,
            target_node.revision,
        )?;
        let existing_outgoing = superseded_by_edges(&edges, old_id);
        if old.status == NodeStatus::Superseded {
            if let Some((existing_edge, receipt_ref)) =
                self.same_supersession_receipt(old_id, &target, &receipt_id, &edges)?
            {
                return Ok(MutationOutcome {
                    schema_version: 1,
                    code: "unchanged".to_string(),
                    status: MutationStatus::Unchanged,
                    value: SupersededWork {
                        node: old,
                        edge: existing_edge,
                        target,
                        assertion: None,
                        reconciliation_receipt: Some(receipt_ref),
                    },
                });
            }
            return Err(PulseError::validation(
                "supersession_conflict",
                "work is already superseded by a different target or receipt",
            ));
        }
        if !matches!(
            old.status,
            NodeStatus::Draft | NodeStatus::Shaped | NodeStatus::Ready | NodeStatus::Blocked
        ) {
            return Err(PulseError::validation(
                "supersession_unavailable",
                format!("status {:?} cannot be superseded", old.status),
            ));
        }
        if !existing_outgoing.is_empty() {
            return Err(PulseError::validation(
                "supersession_conflict",
                "work already has an outgoing superseded_by edge",
            ));
        }
        let edge = match &target {
            SupersessionTarget::Replacement { id } => {
                if id == old_id {
                    return Err(PulseError::validation(
                        "supersession_cycle",
                        "work cannot supersede itself",
                    ));
                }
                let replacement = nodes.get(id).ok_or_else(|| PulseError::NotFound {
                    subject: id.clone(),
                })?;
                if matches!(
                    replacement.status,
                    NodeStatus::Cancelled | NodeStatus::Superseded
                ) {
                    return Err(PulseError::validation(
                        "invalid_supersession_target",
                        "replacement must not be cancelled or superseded",
                    ));
                }
                let planned_edge = Edge::new(
                    EdgeType::SupersededBy,
                    old_id.to_string(),
                    id.clone(),
                    actor.clone(),
                    Utc::now(),
                )?;
                let mut all_edges = edges.clone();
                all_edges.push(planned_edge.clone());
                if supersession_reaches(&all_edges, id, old_id) {
                    return Err(PulseError::validation(
                        "supersession_cycle",
                        "supersession edge would create a cycle",
                    ));
                }
                Some(planned_edge)
            }
            SupersessionTarget::Decision { id } => {
                if target_node.kind != WorkKind::Decision {
                    return Err(PulseError::validation(
                        "invalid_supersession_target",
                        "decision target must have kind Decision",
                    ));
                }
                let _ = id;
                None
            }
        };
        let now = Utc::now();
        old.status = NodeStatus::Superseded;
        old.status_reason = Some(StatusReason::new(
            "superseded",
            reason.clone(),
            match &target {
                SupersessionTarget::Replacement { .. } => None,
                SupersessionTarget::Decision { id } => Some(id.clone()),
            },
        )?);
        old.revision += 1;
        old.updated_at = now;
        let graph_fingerprint_before = self.graph_fingerprint_current_unlocked()?;
        let graph_fingerprint_after =
            self.graph_fingerprint_with_planned_workgraph(&old, edge.as_ref())?;
        let old_after_bytes = to_canonical_bytes(&old)?;
        let event = EventEnvelope::new(
            new_event_id(),
            "work.node.superseded",
            actor.clone(),
            old_id,
            json!({
                "old_id": old_id, "expected_revision": expected_revision, "new_revision": old.revision, "target": target, "reason": reason,
                "reconciliation_receipt": receipt_ref, "graph_fingerprint_before": graph_fingerprint_before,
                "graph_fingerprint_after": graph_fingerprint_after,
                "gate_coverage": ["supersession_preconditions", "receipt_identity", "graph_integrity"]
            }),
            now,
        );
        match &edge {
            Some(edge) => {
                let edge_after_bytes = to_canonical_bytes(edge)?;
                let targets = vec![
                    TransactionTarget::new(
                        old_path.clone(),
                        FileState::Present {
                            hash: hash_bytes(&before_bytes),
                            revision: expected_revision,
                        },
                        FileState::Present {
                            hash: hash_bytes(&old_after_bytes),
                            revision: expected_revision + 1,
                        },
                        &old_after_bytes,
                    ),
                    TransactionTarget::new(
                        self.edge_path(&edge.id),
                        FileState::Absent,
                        FileState::Present {
                            hash: hash_bytes(&edge_after_bytes),
                            revision: edge.revision,
                        },
                        &edge_after_bytes,
                    ),
                ];
                let intent = MultiTargetTransactionIntent::prepared(
                    event.id.clone(),
                    event.event_type.clone(),
                    actor,
                    targets,
                    event_path(&self.repo_root, &event),
                    serde_json::to_value(&event)?,
                )?;
                let prepared = prepare_multi_target_transaction(&self.repo_root, intent)?;
                commit_prepared_multi_target_transaction(&prepared, self.failpoint)?;
            }
            None => self.commit_mutation(
                "work.node.superseded",
                actor,
                old_id,
                serde_json::to_value(&event.payload)?,
                &old_path,
                FileState::Present {
                    hash: hash_bytes(&before_bytes),
                    revision: expected_revision,
                },
                FileState::Present {
                    hash: hash_bytes(&old_after_bytes),
                    revision: expected_revision + 1,
                },
                &old_after_bytes,
                now,
            )?,
        }
        Ok(MutationOutcome {
            schema_version: 1,
            code: "superseded".to_string(),
            status: MutationStatus::Updated,
            value: SupersededWork {
                node: old,
                edge,
                target,
                assertion: None,
                reconciliation_receipt: Some(receipt_ref),
            },
        })
    }

    pub fn add_edge_with_context(
        &self,
        edge_type: EdgeType,
        from: String,
        to: String,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Edge>> {
        if edge_type == EdgeType::SupersededBy {
            return Err(PulseError::validation(
                "superseded_by_lifecycle_owned",
                "superseded_by edges are lifecycle-owned; use pulse work supersede",
            ));
        }
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let (from, to) = canonical_endpoints(edge_type, from, to);
        let id = deterministic_edge_id(edge_type, &from, &to);
        let path = self.edge_path(&id);
        if path.exists() {
            let existing: Edge = storage::read_json(&path)?;
            if existing.edge_type == edge_type && existing.from == from && existing.to == to {
                return Ok(MutationOutcome {
                    schema_version: 1,
                    code: "unchanged".to_string(),
                    status: MutationStatus::Unchanged,
                    value: existing,
                });
            }
            return Err(PulseError::validation(
                "edge_identity_conflict",
                format!("edge id {id} already exists with different payload"),
            ));
        }
        let edge = Edge::new(edge_type, from, to, ctx.actor.clone(), ctx.now)?;
        let nodes = self.load_nodes()?;
        let edges = self
            .load_edges()?
            .into_iter()
            .map(|(_, e)| e)
            .collect::<Vec<_>>();
        validate_edge_for_add(&nodes, &edges, &edge)?;
        let after_bytes = to_canonical_bytes(&edge)?;
        self.commit_mutation(
            "work.edge.created",
            ctx.actor,
            &edge.id,
            json!({"edge": edge}),
            &path,
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&after_bytes),
                revision: edge.revision,
            },
            &after_bytes,
            ctx.now,
        )?;
        Ok(MutationOutcome {
            schema_version: 1,
            code: "created".to_string(),
            status: MutationStatus::Created,
            value: edge,
        })
    }

    pub fn add_edge(
        &self,
        edge_type: EdgeType,
        from: String,
        to: String,
        actor: String,
    ) -> PulseResult<MutationOutcome<Edge>> {
        self.add_edge_with_context(
            edge_type,
            from,
            to,
            OperationContext {
                actor,
                now: Utc::now(),
            },
        )
    }

    pub fn validate(&self) -> PulseResult<ValidationReport> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let manifest = self.manifest()?;
        let node_files = self.load_node_files()?;
        let edge_files = self.load_edge_files()?;
        let node_values = node_files
            .iter()
            .map(|(_, n)| n.clone())
            .collect::<Vec<_>>();
        let edge_values = edge_files
            .iter()
            .map(|(_, e)| e.clone())
            .collect::<Vec<_>>();
        let mut report = validate_graph(&self.repo_root, &manifest, &node_values, &edge_values);
        self.validate_manifest_files(&manifest, &mut report);
        for (path, node) in &node_files {
            if let Err(e) = validate_node_filename(path, node) {
                report.push_error(e.code(), e.to_string());
            }
            self.validate_canonical_file(path, node, "node_canonical_drift", &mut report);
            if !self.repo_root.join(&node.content_dir).exists() {
                report.push_warning(
                    "missing_draft_content_dir",
                    format!("draft content directory missing: {}", node.content_dir),
                );
            }
        }
        for (path, edge) in &edge_files {
            if let Err(e) = validate_edge_filename(path, edge) {
                report.push_error(e.code(), e.to_string());
            }
            self.validate_canonical_file(path, edge, "edge_canonical_drift", &mut report);
        }
        self.validate_runtime_state(&mut report);
        Ok(report)
    }

    pub fn export(&self) -> PulseResult<GraphProjection> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let manifest = self.manifest()?;
        let node_files = self.load_node_files()?;
        let edge_files = self.load_edge_files()?;
        let node_values = node_files
            .iter()
            .map(|(_, n)| n.clone())
            .collect::<Vec<_>>();
        let edge_values = edge_files
            .iter()
            .map(|(_, e)| e.clone())
            .collect::<Vec<_>>();
        validate_graph(&self.repo_root, &manifest, &node_values, &edge_values).into_result()?;
        let node_files = self.load_node_files_rel()?;
        let edge_files = self.load_edge_files_rel()?;
        export_with_cache(&self.repo_root, &manifest, &node_files, &edge_files)
    }

    pub fn executability(&self, id: &str) -> PulseResult<StructuralExecutabilityReport> {
        let projection = self.export()?;
        structural_executability(&projection, id)
    }

    pub fn rollup(&self, id: &str) -> PulseResult<RollupReport> {
        let projection = self.export()?;
        rollup(&projection, id)
    }

    pub fn neighborhood(&self, id: &str, depth: usize) -> PulseResult<NeighborhoodReport> {
        let projection = self.export()?;
        neighborhood(&projection, id, depth)
    }

    pub fn affected_by(
        &self,
        id: &str,
        relation_filter: Option<EdgeType>,
    ) -> PulseResult<AffectedByReport> {
        let projection = self.export()?;
        affected_by(&projection, id, relation_filter)
    }

    fn graph_fingerprint_current_unlocked(&self) -> PulseResult<String> {
        let manifest = self.manifest()?;
        let node_files = self.load_node_files_rel()?;
        let edge_files = self.load_edge_files_rel()?;
        graph_fingerprint(&manifest, &node_files, &edge_files)
    }

    fn graph_fingerprint_with_planned_workgraph(
        &self,
        node_override: &Node,
        edge_override: Option<&Edge>,
    ) -> PulseResult<String> {
        let manifest = self.manifest()?;
        let mut node_files = self.load_node_files_rel()?;
        let node_path = self.rel_path(&self.node_path(&node_override.id));
        let mut node_replaced = false;
        for (path, node) in &mut node_files {
            if path == &node_path {
                *node = node_override.clone();
                node_replaced = true;
                break;
            }
        }
        if !node_replaced {
            node_files.push((node_path, node_override.clone()));
        }
        node_files.sort_by(|left, right| left.0.cmp(&right.0));

        let mut edge_files = self.load_edge_files_rel()?;
        if let Some(edge) = edge_override {
            let edge_path = self.rel_path(&self.edge_path(&edge.id));
            let mut edge_replaced = false;
            for (path, existing) in &mut edge_files {
                if path == &edge_path {
                    *existing = edge.clone();
                    edge_replaced = true;
                    break;
                }
            }
            if !edge_replaced {
                edge_files.push((edge_path, edge.clone()));
            }
            edge_files.sort_by(|left, right| left.0.cmp(&right.0));
        }

        graph_fingerprint(&manifest, &node_files, &edge_files)
    }

    pub fn recover(&self) -> PulseResult<()> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        // Recovery must not create layout/bootstrap files just to locate prepared
        // intents. Runtime transaction recovery alone is enough to roll forward
        // partial work.
        recover_prepared_transactions(&self.repo_root)?;
        Ok(())
    }

    fn classify_workgraph_bootstrap_state(&self) -> PulseResult<WorkgraphBootstrapState> {
        classify_workgraph_bootstrap_state(&self.workgraph_dir())
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_mutation(
        &self,
        event_type: &str,
        actor: String,
        subject: &str,
        payload: serde_json::Value,
        target_path: &Path,
        before: FileState,
        after: FileState,
        canonical_bytes: &[u8],
        now: DateTime<Utc>,
    ) -> PulseResult<()> {
        debug_assert_eq!(
            current_file_state(target_path, file_state_revision(&before))?,
            before
        );
        let event = EventEnvelope::new(
            new_event_id(),
            event_type,
            actor.clone(),
            subject,
            payload,
            now,
        );
        let intent = TransactionIntent::prepared(
            event.id.clone(),
            event_type,
            actor,
            target_path.to_path_buf(),
            event_path(&self.repo_root, &event),
            before,
            after,
            serde_json::to_value(event)?,
        )?;
        let prepared = prepare_transaction(&self.repo_root, intent)?;
        commit_prepared_transaction(&prepared, canonical_bytes, self.failpoint)
    }

    fn workgraph_dir(&self) -> PathBuf {
        self.repo_root.join(".pulse/workgraph")
    }

    fn node_path(&self, id: &str) -> PathBuf {
        self.workgraph_dir()
            .join("nodes")
            .join(format!("{id}.json"))
    }

    fn edge_path(&self, id: &str) -> PathBuf {
        self.workgraph_dir()
            .join("edges")
            .join(format!("{id}.json"))
    }

    fn manifest(&self) -> PulseResult<Manifest> {
        storage::read_json(&self.workgraph_dir().join("manifest.json"))
    }

    fn validate_canonical_file<T: Serialize>(
        &self,
        path: &Path,
        value: &T,
        code: &'static str,
        report: &mut ValidationReport,
    ) {
        match (fs::read(path), to_canonical_bytes(value)) {
            (Ok(actual), Ok(expected)) if actual != expected => report.push_warning(
                code,
                format!("{} is not in canonical JSON byte form", path.display()),
            ),
            (Err(error), _) => report.push_error(
                "io_error",
                format!("cannot read {}: {error}", path.display()),
            ),
            (_, Err(error)) => report.push_error(error.code(), error.to_string()),
            _ => {}
        }
    }

    fn validate_runtime_state(&self, report: &mut ValidationReport) {
        match recover_prepared_transactions(&self.repo_root) {
            Ok(actions) => {
                for action in actions {
                    report.push_warning(
                        "transaction_recovered",
                        format!("recovered local transaction state: {action:?}"),
                    );
                }
            }
            Err(error) => report.push_error(error.code(), error.to_string()),
        }
    }

    fn validate_manifest_files(&self, manifest: &Manifest, report: &mut ValidationReport) {
        match crate::storage::paths::configured_content_root(
            &self.repo_root,
            &manifest.content_root,
        ) {
            Ok(root) => {
                match crate::storage::paths::configured_content_root(&self.repo_root, "../../works")
                {
                    Ok(expected) if root != expected => report.push_error(
                        "content_root_violation",
                        format!(
                            "manifest content_root must resolve to repository works/ root, got {}",
                            manifest.content_root
                        ),
                    ),
                    Ok(_) => {}
                    Err(error) => report.push_error(error.code(), error.to_string()),
                }
            }
            Err(error) => report.push_error(error.code(), error.to_string()),
        }
        if manifest.id_pattern != "^(EP|ST|TK|DEC)-[0-9]{3,}$" {
            report.push_error(
                "invalid_manifest",
                format!(
                    "manifest id_pattern is unsupported: {}",
                    manifest.id_pattern
                ),
            );
        }
        self.validate_schema_file(
            &manifest.node_schema,
            "node_schema_drift",
            NODE_SCHEMA,
            report,
        );
        self.validate_schema_file(
            &manifest.edge_schema,
            "edge_schema_drift",
            EDGE_SCHEMA,
            report,
        );
    }

    fn write_current_node_schema_if_absent_unlocked(&self) -> PulseResult<()> {
        let path = self.workgraph_dir().join("schemas/node.schema.json");
        if !path.exists() {
            storage::atomic_write(&path, NODE_SCHEMA.as_bytes())?;
            return Ok(());
        }
        let current = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        if current == NODE_SCHEMA.as_bytes() {
            return Ok(());
        }
        let current_hash = hash_bytes(&current);
        Err(PulseError::validation(
            "node_schema_drift_refused",
            format!(
                "refusing to overwrite node schema drift {}; resolve schema state explicitly",
                current_hash
            ),
        ))
    }

    fn validate_schema_file(
        &self,
        schema_path: &str,
        drift_code: &'static str,
        expected_embedded_schema: &str,
        report: &mut ValidationReport,
    ) {
        let rel = match crate::storage::safe_repo_relative(schema_path) {
            Ok(rel) => rel,
            Err(e) => {
                report.push_error(e.code(), e.to_string());
                return;
            }
        };
        let full = self.workgraph_dir().join(rel);
        match fs::read(&full) {
            Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
                Ok(repo_schema) => {
                    match serde_json::from_str::<serde_json::Value>(expected_embedded_schema) {
                        Ok(embedded_schema) if repo_schema != embedded_schema => report.push_error(
                            drift_code,
                            format!(
                                "schema {} differs from embedded schema template",
                                full.display()
                            ),
                        ),
                        Ok(_) => match to_canonical_bytes(&repo_schema) {
                            Ok(canonical) if canonical != bytes => report.push_warning(
                                "schema_canonical_drift",
                                format!(
                                    "schema {} is not in canonical JSON byte form",
                                    full.display()
                                ),
                            ),
                            Err(error) => report.push_error(error.code(), error.to_string()),
                            _ => {}
                        },
                        Err(e) => report.push_error(
                            "embedded_schema_parse_error",
                            format!("embedded schema is not valid JSON: {e}"),
                        ),
                    }
                }
                Err(e) => report.push_error(
                    "schema_parse_error",
                    format!("schema {} is not valid JSON: {}", full.display(), e),
                ),
            },
            Err(e) => report.push_error(
                "schema_missing",
                format!("cannot read schema {}: {}", full.display(), e),
            ),
        }
    }

    fn allocate_id(&self, kind: WorkKind) -> PulseResult<String> {
        let prefix = kind.prefix();
        let mut max = 0;
        for entry in fs::read_dir(self.workgraph_dir().join("nodes"))
            .map_err(|e| PulseError::io(self.workgraph_dir().join("nodes"), e))?
        {
            let entry = entry.map_err(|e| PulseError::io(self.workgraph_dir().join("nodes"), e))?;
            let Some(stem) = entry
                .path()
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
            else {
                continue;
            };
            if let Some(n) = parse_numeric(&stem, prefix) {
                max = max.max(n);
            }
        }
        Ok(format_id(kind, max + 1))
    }

    fn load_nodes(&self) -> PulseResult<BTreeMap<String, Node>> {
        let mut out = BTreeMap::new();
        for (_, node) in self.load_node_files()? {
            out.insert(node.id.clone(), node);
        }
        Ok(out)
    }

    fn load_nodes_with_override(&self, node: Node) -> PulseResult<BTreeMap<String, Node>> {
        let mut nodes = self.load_nodes()?;
        nodes.insert(node.id.clone(), node);
        Ok(nodes)
    }

    fn load_edges(&self) -> PulseResult<Vec<(PathBuf, Edge)>> {
        self.load_edge_files()
    }

    fn load_node_files(&self) -> PulseResult<Vec<(PathBuf, Node)>> {
        let dir = self.workgraph_dir().join("nodes");
        let mut out = vec![];
        if !dir.exists() {
            return Ok(out);
        }
        for entry in fs::read_dir(&dir).map_err(|e| PulseError::io(&dir, e))? {
            let entry = entry.map_err(|e| PulseError::io(&dir, e))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                out.push((path.clone(), storage::read_json(&path)?));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }

    fn load_edge_files(&self) -> PulseResult<Vec<(PathBuf, Edge)>> {
        let dir = self.workgraph_dir().join("edges");
        let mut out = vec![];
        if !dir.exists() {
            return Ok(out);
        }
        for entry in fs::read_dir(&dir).map_err(|e| PulseError::io(&dir, e))? {
            let entry = entry.map_err(|e| PulseError::io(&dir, e))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                out.push((path.clone(), storage::read_json(&path)?));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }

    fn load_node_files_rel(&self) -> PulseResult<Vec<(PathBuf, Node)>> {
        Ok(self
            .load_node_files()?
            .into_iter()
            .map(|(p, n)| (self.rel_path(&p), n))
            .collect())
    }

    fn load_edge_files_rel(&self) -> PulseResult<Vec<(PathBuf, Edge)>> {
        Ok(self
            .load_edge_files()?
            .into_iter()
            .map(|(p, e)| (self.rel_path(&p), e))
            .collect())
    }

    fn rel_path(&self, path: &Path) -> PathBuf {
        path.strip_prefix(&self.repo_root)
            .unwrap_or(path)
            .to_path_buf()
    }

    fn write_if_absent<T: Serialize>(&self, path: &Path, value: &T) -> PulseResult<()> {
        if path.exists() {
            return Ok(());
        }
        storage::atomic_write(path, &to_canonical_bytes(value)?)
    }

    fn write_bytes_if_absent(&self, path: &Path, bytes: &[u8]) -> PulseResult<()> {
        if path.exists() {
            return Ok(());
        }
        storage::atomic_write(path, bytes)
    }
}

enum WorkgraphBootstrapState {
    Empty,
    SafePartialCurrent,
    ExistingCurrent,
    MissingNodeSchemaWithState,
    NodeSchemaDrift { hash: String },
    UnexpectedPartialState,
}

struct WorkgraphBootstrapInspection {
    has_manifest: bool,
    has_node_schema: bool,
    has_edge_schema: bool,
    has_node_files: bool,
    has_edge_files: bool,
    has_only_safe_entries: bool,
    manifest_matches: bool,
    node_schema_matches: Option<bool>,
    edge_schema_matches: bool,
    node_schema_hash: Option<String>,
}

impl WorkgraphBootstrapInspection {
    fn has_any_current_marker(&self) -> bool {
        self.has_manifest || self.has_node_schema || self.has_edge_schema
    }

    fn all_present_markers_match(&self) -> bool {
        self.manifest_matches && self.node_schema_matches != Some(false) && self.edge_schema_matches
    }
}

fn classify_workgraph_bootstrap_state(wg: &Path) -> PulseResult<WorkgraphBootstrapState> {
    let inspection = inspect_workgraph_bootstrap_state(wg)?;
    if !inspection.has_only_safe_entries {
        return Ok(WorkgraphBootstrapState::UnexpectedPartialState);
    }
    if inspection.node_schema_matches == Some(false) {
        return Ok(WorkgraphBootstrapState::NodeSchemaDrift {
            hash: inspection.node_schema_hash.unwrap_or_default(),
        });
    }
    if !inspection.all_present_markers_match() {
        return Ok(WorkgraphBootstrapState::UnexpectedPartialState);
    }
    if inspection.has_node_files || inspection.has_edge_files {
        return Ok(if inspection.has_node_schema {
            WorkgraphBootstrapState::ExistingCurrent
        } else {
            WorkgraphBootstrapState::MissingNodeSchemaWithState
        });
    }
    if inspection.has_any_current_marker() {
        return Ok(
            if inspection.has_manifest && inspection.has_node_schema && inspection.has_edge_schema {
                WorkgraphBootstrapState::ExistingCurrent
            } else {
                WorkgraphBootstrapState::SafePartialCurrent
            },
        );
    }
    Ok(WorkgraphBootstrapState::Empty)
}

fn inspect_workgraph_bootstrap_state(wg: &Path) -> PulseResult<WorkgraphBootstrapInspection> {
    if !wg.exists() {
        return Ok(WorkgraphBootstrapInspection {
            has_manifest: false,
            has_node_schema: false,
            has_edge_schema: false,
            has_node_files: false,
            has_edge_files: false,
            has_only_safe_entries: true,
            manifest_matches: true,
            node_schema_matches: None,
            edge_schema_matches: true,
            node_schema_hash: None,
        });
    }

    let manifest_path = wg.join("manifest.json");
    let node_schema_path = wg.join("schemas/node.schema.json");
    let edge_schema_path = wg.join("schemas/edge.schema.json");
    let manifest_matches =
        current_marker_matches(&manifest_path, &to_canonical_bytes(&Manifest::default())?)?;
    let node_schema_bytes = read_optional_bytes(&node_schema_path)?;
    let node_schema_matches = node_schema_bytes
        .as_deref()
        .map(|current| current == NODE_SCHEMA.as_bytes());
    let node_schema_hash = node_schema_bytes
        .as_deref()
        .filter(|current| *current != NODE_SCHEMA.as_bytes())
        .map(hash_bytes);
    Ok(WorkgraphBootstrapInspection {
        has_manifest: manifest_path.exists(),
        has_node_schema: node_schema_path.exists(),
        has_edge_schema: edge_schema_path.exists(),
        has_node_files: directory_has_json_files(&wg.join("nodes"))?,
        has_edge_files: directory_has_json_files(&wg.join("edges"))?,
        has_only_safe_entries: workgraph_subtree_has_only_allowed_entries(
            wg,
            &[
                "manifest.json",
                "schemas",
                "schemas/node.schema.json",
                "schemas/edge.schema.json",
                "nodes",
                "edges",
            ],
        )?,
        manifest_matches,
        node_schema_matches,
        edge_schema_matches: current_marker_matches(&edge_schema_path, EDGE_SCHEMA.as_bytes())?,
        node_schema_hash,
    })
}

fn current_marker_matches(path: &Path, expected: &[u8]) -> PulseResult<bool> {
    Ok(match read_optional_bytes(path)? {
        Some(current) => current == expected,
        None => true,
    })
}

fn read_optional_bytes(path: &Path) -> PulseResult<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(PulseError::io(path, error)),
    }
}

fn directory_has_json_files(dir: &Path) -> PulseResult<bool> {
    if !dir.exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(dir).map_err(|e| PulseError::io(dir, e))? {
        let entry = entry.map_err(|e| PulseError::io(dir, e))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            return Ok(true);
        }
    }
    Ok(false)
}

fn workgraph_subtree_has_only_allowed_entries(root: &Path, allowed: &[&str]) -> PulseResult<bool> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).map_err(|e| PulseError::io(&dir, e))? {
            let entry = entry.map_err(|e| PulseError::io(&dir, e))?;
            let path = entry.path();
            let Ok(relative) = path.strip_prefix(root) else {
                return Ok(false);
            };
            let relative = relative.to_string_lossy();
            if !allowed.iter().any(|candidate| *candidate == relative)
                && !relative.starts_with("nodes/")
                && !relative.starts_with("edges/")
            {
                return Ok(false);
            }
            if entry
                .file_type()
                .map_err(|e| PulseError::io(&path, e))?
                .is_dir()
            {
                stack.push(path);
            }
        }
    }
    Ok(true)
}

fn superseded_by_edges(edges: &[Edge], from: &str) -> Vec<Edge> {
    edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::SupersededBy && edge.from == from)
        .cloned()
        .collect()
}

fn supersession_reaches(edges: &[Edge], start: &str, needle: &str) -> bool {
    let mut current = start;
    let mut seen = std::collections::BTreeSet::new();
    while seen.insert(current.to_string()) {
        let Some(next) = edges
            .iter()
            .find(|edge| edge.edge_type == EdgeType::SupersededBy && edge.from == current)
            .map(|edge| edge.to.as_str())
        else {
            return false;
        };
        if next == needle {
            return true;
        }
        current = next;
    }
    false
}

fn validate_supersession_assertion(
    assertion: &SupersessionAssertion,
    nodes: &BTreeMap<String, Node>,
) -> PulseResult<()> {
    if assertion.assertion_version != 1 {
        return Err(PulseError::validation(
            "invalid_supersession_assertion",
            "assertion_version must be 1",
        ));
    }
    if assertion.asserted_by.trim().is_empty() {
        return Err(PulseError::validation(
            "invalid_supersession_assertion",
            "asserted_by must not be empty",
        ));
    }
    for source in &assertion.source_revisions {
        let (id, revision) = source.split_once('@').ok_or_else(|| {
            PulseError::validation(
                "invalid_supersession_assertion",
                format!("source revision must be ID@revision: {source}"),
            )
        })?;
        let revision = revision.parse::<u64>().map_err(|_| {
            PulseError::validation(
                "invalid_supersession_assertion",
                format!("source revision must contain numeric revision: {source}"),
            )
        })?;
        let node = nodes.get(id).ok_or_else(|| PulseError::NotFound {
            subject: id.to_string(),
        })?;
        if node.revision != revision {
            return Err(PulseError::validation(
                "assertion_revision_mismatch",
                format!(
                    "assertion source {id}@{revision} does not match current revision {}",
                    node.revision
                ),
            ));
        }
    }
    for reference in &assertion.references {
        if !nodes.contains_key(reference) {
            return Err(PulseError::NotFound {
                subject: reference.clone(),
            });
        }
    }
    if assertion.claim == SupersessionClaim::FollowUpRequired
        && !assertion.references.iter().any(|reference| {
            nodes
                .get(reference)
                .is_some_and(|node| node.kind != WorkKind::Decision)
        })
    {
        return Err(PulseError::validation(
            "follow_up_reference_required",
            "follow_up_required assertions must reference at least one work item",
        ));
    }
    Ok(())
}

impl JsonGraphStore {
    fn same_supersession(
        &self,
        old: &Node,
        target: &SupersessionTarget,
        assertion: &SupersessionAssertion,
        edges: &[Edge],
    ) -> Option<Option<Edge>> {
        if old.status != NodeStatus::Superseded
            || !self.supersession_event_matches(&old.id, target, assertion)
        {
            return None;
        }
        match target {
            SupersessionTarget::Replacement { id } => {
                let outgoing = superseded_by_edges(edges, &old.id);
                if outgoing.len() == 1 && outgoing[0].to == *id {
                    Some(Some(outgoing[0].clone()))
                } else {
                    None
                }
            }
            SupersessionTarget::Decision { id } => {
                if old
                    .status_reason
                    .as_ref()
                    .and_then(|reason| reason.reference.as_ref())
                    == Some(id)
                {
                    Some(None)
                } else {
                    None
                }
            }
        }
    }

    fn supersession_event_matches(
        &self,
        old_id: &str,
        target: &SupersessionTarget,
        assertion: &SupersessionAssertion,
    ) -> bool {
        let events_dir = self.repo_root.join(".pulse/events");
        let Ok(date_dirs) = fs::read_dir(events_dir) else {
            return false;
        };
        let target_value = match serde_json::to_value(target) {
            Ok(value) => value,
            Err(_) => return false,
        };
        let assertion_value = match serde_json::to_value(assertion) {
            Ok(value) => value,
            Err(_) => return false,
        };
        for date_dir in date_dirs.flatten() {
            let Ok(entries) = fs::read_dir(date_dir.path()) else {
                continue;
            };
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let Ok(event) = storage::read_json::<EventEnvelope>(&entry.path()) else {
                    continue;
                };
                if event.event_type == "work.node.superseded"
                    && event.subject == old_id
                    && event.payload.get("target") == Some(&target_value)
                    && event.payload.get("assertion") == Some(&assertion_value)
                {
                    return true;
                }
            }
        }
        false
    }

    fn same_supersession_receipt(
        &self,
        old_id: &str,
        target: &SupersessionTarget,
        receipt_id: &str,
        edges: &[Edge],
    ) -> PulseResult<Option<(Option<Edge>, crate::evidence::model::ReceiptReference)>> {
        let events_dir = self.repo_root.join(".pulse/events");
        let Ok(date_dirs) = fs::read_dir(events_dir) else {
            return Ok(None);
        };
        let target_value = serde_json::to_value(target)?;
        for date_dir in date_dirs.flatten() {
            let Ok(entries) = fs::read_dir(date_dir.path()) else {
                continue;
            };
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let Ok(event) = storage::read_json::<EventEnvelope>(&entry.path()) else {
                    continue;
                };
                let Some(receipt_value) = event.payload.get("reconciliation_receipt") else {
                    continue;
                };
                if event.event_type == "work.node.superseded"
                    && event.subject == old_id
                    && event.payload.get("target") == Some(&target_value)
                    && receipt_value.get("id").and_then(|v| v.as_str()) == Some(receipt_id)
                {
                    let receipt_ref: crate::evidence::model::ReceiptReference =
                        serde_json::from_value(receipt_value.clone())?;
                    let edge = match target {
                        SupersessionTarget::Replacement { .. } => {
                            superseded_by_edges(edges, old_id).into_iter().next()
                        }
                        SupersessionTarget::Decision { .. } => None,
                    };
                    return Ok(Some((edge, receipt_ref)));
                }
            }
        }
        Ok(None)
    }
}

fn file_state_revision(state: &FileState) -> Option<u64> {
    match state {
        FileState::Absent => None,
        FileState::Present { revision, .. } => Some(*revision),
    }
}
