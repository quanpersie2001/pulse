use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::event::{event_path, EventEnvelope};
use crate::graph::edge::{canonical_endpoints, deterministic_edge_id, Edge, EdgeType};
use crate::graph::executability::{structural_executability, StructuralExecutabilityReport};
use crate::graph::lifecycle::{status_requires_reason, validate_transition, TransitionReason};
use crate::graph::manifest::{Manifest, EDGE_SCHEMA, NODE_SCHEMA};
use crate::graph::node::{Node, NodeStatus, StatusReason};
use crate::graph::projection::PROJECTION_SCHEMA_VERSION;
use crate::graph::projection::{export_with_cache, GraphProjection};
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

const SLICE1_NODE_SCHEMA_HASH: &str =
    "sha256:1590def10b4715549d6d735352f2033bb128808dab7e56a138689bb0a46af589";

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
    pub assertion: SupersessionAssertion,
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

    pub fn bootstrap(&self) -> PulseResult<()> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        Ok(())
    }

    pub fn bootstrap_unlocked(&self) -> PulseResult<()> {
        let wg = self.workgraph_dir();
        fs::create_dir_all(wg.join("nodes")).map_err(|e| PulseError::io(wg.join("nodes"), e))?;
        fs::create_dir_all(wg.join("edges")).map_err(|e| PulseError::io(wg.join("edges"), e))?;
        fs::create_dir_all(wg.join("schemas"))
            .map_err(|e| PulseError::io(wg.join("schemas"), e))?;
        self.write_if_absent(&wg.join("manifest.json"), &Manifest::default())?;
        self.write_or_upgrade_node_schema_unlocked()?;
        self.write_bytes_if_absent(&wg.join("schemas/edge.schema.json"), EDGE_SCHEMA.as_bytes())?;
        Ok(())
    }

    pub fn create_node_with_context(
        &self,
        kind: WorkKind,
        title: String,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let id = self.allocate_id(kind)?;
        let node = Node::new(id.clone(), kind, title, ctx.now)?;
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
                        assertion,
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
                        assertion,
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
                format!("status {:?} cannot be superseded in Slice 2", old.status),
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
                assertion,
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

    pub fn recover(&self) -> PulseResult<()> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        Ok(())
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

    fn write_or_upgrade_node_schema_unlocked(&self) -> PulseResult<()> {
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
        if current_hash != SLICE1_NODE_SCHEMA_HASH {
            return Err(PulseError::validation(
                "node_schema_upgrade_refused",
                format!(
                    "refusing to overwrite unknown node schema {}; expected Slice 1 predecessor {}",
                    current_hash, SLICE1_NODE_SCHEMA_HASH
                ),
            ));
        }
        // Drain any prepared transaction before replacing the schema template so recovery
        // evidence is interpreted against a stable predecessor contract.
        recover_prepared_transactions(&self.repo_root)?;
        // Prove every existing node still parses and validates under the typed Slice 2 model.
        let manifest = self.manifest()?;
        let node_files = self.load_node_files()?;
        for (_, node) in &node_files {
            validate_node_filename(&self.node_path(&node.id), node)?;
            crate::graph::validate::validate_node(&self.repo_root, &manifest, node)?;
        }
        let event = EventEnvelope::new(
            new_event_id(),
            "work.schema.node.upgraded",
            "system:pulse",
            "schemas/node.schema.json",
            json!({
                "from_schema_hash": current_hash,
                "to_schema_hash": hash_bytes(NODE_SCHEMA.as_bytes()),
                "node_count": node_files.len(),
                "projection_schema_version": PROJECTION_SCHEMA_VERSION,
                "gate_coverage": ["write_fence", "transaction_recovery", "known_predecessor_schema", "typed_node_parse"]
            }),
            Utc::now(),
        );
        let event_path = event_path(&self.repo_root, &event);
        let intent = TransactionIntent::prepared(
            event.id.clone(),
            "work.schema.node.upgraded",
            "system:pulse",
            path.clone(),
            event_path,
            FileState::Present {
                hash: current_hash,
                revision: 0,
            },
            FileState::Present {
                hash: hash_bytes(NODE_SCHEMA.as_bytes()),
                revision: 0,
            },
            serde_json::to_value(event)?,
        )?;
        let prepared = prepare_transaction(&self.repo_root, intent)?;
        commit_prepared_transaction(&prepared, NODE_SCHEMA.as_bytes(), self.failpoint)?;
        Ok(())
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
                                "schema {} differs from slice schema template",
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
}

fn file_state_revision(state: &FileState) -> Option<u64> {
    match state {
        FileState::Absent => None,
        FileState::Present { revision, .. } => Some(*revision),
    }
}
