use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::canonical_json::to_canonical_bytes;
use crate::event::emit_event;
use crate::graph::edge::{canonical_endpoints, deterministic_edge_id, Edge, EdgeType};
use crate::graph::manifest::{Manifest, EDGE_SCHEMA, NODE_SCHEMA};
use crate::graph::node::Node;
use crate::graph::projection::{export_with_cache, GraphProjection};
use crate::graph::validate::{
    validate_edge_filename, validate_edge_for_add, validate_graph, validate_node_filename, ValidationReport,
};
use crate::id::{format_id, parse_numeric, validate_id_for_kind, WorkKind};
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
}

impl JsonGraphStore {
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
        }
    }

    pub fn bootstrap(&self) -> PulseResult<()> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()
    }

    pub fn bootstrap_unlocked(&self) -> PulseResult<()> {
        let wg = self.workgraph_dir();
        fs::create_dir_all(wg.join("nodes")).map_err(|e| PulseError::io(wg.join("nodes"), e))?;
        fs::create_dir_all(wg.join("edges")).map_err(|e| PulseError::io(wg.join("edges"), e))?;
        fs::create_dir_all(wg.join("schemas")).map_err(|e| PulseError::io(wg.join("schemas"), e))?;
        self.write_if_absent(&wg.join("manifest.json"), &Manifest::default())?;
        self.write_bytes_if_absent(&wg.join("schemas/node.schema.json"), NODE_SCHEMA.as_bytes())?;
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
        storage::atomic_write(&path, &to_canonical_bytes(&node)?)?;
        emit_event(
            &self.repo_root,
            "work.node.created",
            ctx.actor,
            &node.id,
            json!({"node": node}),
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
        self.bootstrap_unlocked()?;
        let path = self.node_path(id);
        if !path.exists() {
            return Err(PulseError::NotFound {
                subject: id.to_string(),
            });
        }
        storage::read_json(&path)
    }

    pub fn list_nodes(&self, kind: Option<WorkKind>) -> PulseResult<ListOutcome<Node>> {
        self.bootstrap_unlocked()?;
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
            return Err(PulseError::validation("invalid_title", "title must not be empty"));
        }
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        let path = self.node_path(id);
        if !path.exists() {
            return Err(PulseError::NotFound {
                subject: id.to_string(),
            });
        }
        let mut node: Node = storage::read_json(&path)?;
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
        let node_values = self.load_nodes_with_override(node.clone())?.into_values().collect::<Vec<_>>();
        let edge_values = self.load_edges()?.iter().map(|(_, e)| e.clone()).collect::<Vec<_>>();
        validate_graph(
            &self.repo_root,
            &self.manifest()?,
            &node_values,
            &edge_values,
        )
        .into_result()?;
        storage::atomic_write(&path, &to_canonical_bytes(&node)?)?;
        emit_event(
            &self.repo_root,
            "work.node.updated",
            ctx.actor,
            id,
            json!({"node": node, "expected_revision": expected_revision}),
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

    pub fn add_edge_with_context(
        &self,
        edge_type: EdgeType,
        from: String,
        to: String,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Edge>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
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
        let edges = self.load_edges()?.into_iter().map(|(_, e)| e).collect::<Vec<_>>();
        validate_edge_for_add(&nodes, &edges, &edge)?;
        storage::atomic_write(&path, &to_canonical_bytes(&edge)?)?;
        emit_event(
            &self.repo_root,
            "work.edge.created",
            ctx.actor,
            &edge.id,
            json!({"edge": edge}),
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
        self.bootstrap_unlocked()?;
        let manifest = self.manifest()?;
        let node_files = self.load_node_files()?;
        let edge_files = self.load_edge_files()?;
        let node_values = node_files.iter().map(|(_, n)| n.clone()).collect::<Vec<_>>();
        let edge_values = edge_files.iter().map(|(_, e)| e.clone()).collect::<Vec<_>>();
        let mut report = validate_graph(
            &self.repo_root,
            &manifest,
            &node_values,
            &edge_values,
        );
        self.validate_manifest_files(&manifest, &mut report);
        for (path, node) in &node_files {
            if let Err(e) = validate_node_filename(path, node) {
                report.push_error(e.code(), e.to_string());
            }
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
        }
        Ok(report)
    }

    pub fn export(&self) -> PulseResult<GraphProjection> {
        self.bootstrap_unlocked()?;
        self.validate()?.into_result()?;
        let manifest = self.manifest()?;
        let node_files = self.load_node_files_rel()?;
        let edge_files = self.load_edge_files_rel()?;
        export_with_cache(&self.repo_root, &manifest, &node_files, &edge_files)
    }

    fn workgraph_dir(&self) -> PathBuf {
        self.repo_root.join(".pulse/workgraph")
    }

    fn node_path(&self, id: &str) -> PathBuf {
        self.workgraph_dir().join("nodes").join(format!("{id}.json"))
    }

    fn edge_path(&self, id: &str) -> PathBuf {
        self.workgraph_dir().join("edges").join(format!("{id}.json"))
    }

    fn manifest(&self) -> PulseResult<Manifest> {
        storage::read_json(&self.workgraph_dir().join("manifest.json"))
    }

    fn validate_manifest_files(&self, manifest: &Manifest, report: &mut ValidationReport) {
        if manifest.content_root != "../../works" {
            report.push_error(
                "invalid_manifest",
                format!("manifest content_root must be ../../works, got {}", manifest.content_root),
            );
        }
        for schema_path in [&manifest.node_schema, &manifest.edge_schema] {
            let rel = match crate::storage::safe_repo_relative(schema_path) {
                Ok(rel) => rel,
                Err(e) => {
                    report.push_error(e.code(), e.to_string());
                    continue;
                }
            };
            let full = self.workgraph_dir().join(rel);
            match fs::read(&full) {
                Ok(bytes) => {
                    if let Err(e) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                        report.push_error(
                            "schema_parse_error",
                            format!("schema {} is not valid JSON: {}", full.display(), e),
                        );
                    }
                }
                Err(e) => report.push_error(
                    "schema_missing",
                    format!("cannot read schema {}: {}", full.display(), e),
                ),
            }
        }
    }

    fn allocate_id(&self, kind: WorkKind) -> PulseResult<String> {
        let prefix = kind.prefix();
        let mut max = 0;
        for entry in fs::read_dir(self.workgraph_dir().join("nodes"))
            .map_err(|e| PulseError::io(self.workgraph_dir().join("nodes"), e))?
        {
            let entry = entry.map_err(|e| PulseError::io(self.workgraph_dir().join("nodes"), e))?;
            let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str()).map(str::to_string) else {
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
        path.strip_prefix(&self.repo_root).unwrap_or(path).to_path_buf()
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
