use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::graph::edge::{deterministic_edge_id, Edge, EdgeType};
use crate::graph::manifest::Manifest;
use crate::graph::node::Node;
use crate::id::validate_id_for_kind;
use crate::storage::safe_repo_relative;
use crate::{PulseError, PulseResult};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidationReport {
    pub schema_version: u32,
    pub code: String,
    pub valid: bool,
    pub errors: Vec<ValidationFinding>,
    pub warnings: Vec<ValidationFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationFinding {
    pub code: String,
    pub message: String,
}

impl ValidationReport {
    pub fn ok() -> Self {
        Self {
            schema_version: 1,
            code: "valid".to_string(),
            valid: true,
            errors: vec![],
            warnings: vec![],
        }
    }

    pub fn push_error(&mut self, code: impl Into<String>, message: impl Into<String>) {
        self.valid = false;
        self.code = "invalid_graph".to_string();
        self.errors.push(ValidationFinding {
            code: code.into(),
            message: message.into(),
        });
    }

    pub fn push_warning(&mut self, code: impl Into<String>, message: impl Into<String>) {
        self.warnings.push(ValidationFinding {
            code: code.into(),
            message: message.into(),
        });
    }

    pub fn into_result(self) -> PulseResult<Self> {
        if self.valid {
            Ok(self)
        } else {
            Err(PulseError::validation(
                "invalid_graph",
                serde_json::to_string(&self.errors).unwrap_or_else(|_| "invalid graph".to_string()),
            ))
        }
    }
}

pub fn validate_graph(
    repo_root: &Path,
    manifest: &Manifest,
    nodes: &[Node],
    edges: &[Edge],
) -> ValidationReport {
    let mut report = ValidationReport::ok();
    if manifest.schema_version != 1 {
        report.push_error("unsupported_manifest_version", "manifest schema_version must be 1");
    }
    if manifest.node_schema != "schemas/node.schema.json" {
        report.push_error("invalid_manifest", "manifest node_schema must be schemas/node.schema.json");
    }
    if manifest.edge_schema != "schemas/edge.schema.json" {
        report.push_error("invalid_manifest", "manifest edge_schema must be schemas/edge.schema.json");
    }

    let mut ids = BTreeSet::new();
    for node in nodes {
        if !ids.insert(node.id.clone()) {
            report.push_error("duplicate_node_id", format!("duplicate node id {}", node.id));
        }
        if let Err(e) = validate_node(repo_root, node) {
            report.push_error(e.code(), e.to_string());
        }
    }

    let mut edge_ids = BTreeSet::new();
    let mut parent_by_child: BTreeMap<String, String> = BTreeMap::new();
    for edge in edges {
        if !edge_ids.insert(edge.id.clone()) {
            report.push_error("duplicate_edge_id", format!("duplicate edge id {}", edge.id));
        }
        if !ids.contains(&edge.from) || !ids.contains(&edge.to) {
            report.push_error(
                "dangling_edge",
                format!("edge {} references missing endpoint {} -> {}", edge.id, edge.from, edge.to),
            );
        }
        let expected = deterministic_edge_id(edge.edge_type, &edge.from, &edge.to);
        if edge.id != expected {
            report.push_error(
                "edge_id_mismatch",
                format!("edge {} should have deterministic id {}", edge.id, expected),
            );
        }
        if edge.edge_type == EdgeType::Related && edge.to < edge.from {
            report.push_error(
                "edge_endpoint_order_mismatch",
                format!("related edge {} must store endpoints in lexical order", edge.id),
            );
        }
        if edge.edge_type == EdgeType::Parent {
            if let Some(existing) = parent_by_child.insert(edge.from.clone(), edge.to.clone()) {
                report.push_error(
                    "multiple_parents",
                    format!("node {} has parents {} and {}", edge.from, existing, edge.to),
                );
            }
        }
    }

    for ty in [EdgeType::Parent, EdgeType::BlockedBy, EdgeType::SupersededBy] {
        if let Some(cycle) = find_cycle(edges, ty) {
            report.push_error(
                "cycle_detected",
                format!("{ty:?} cycle detected: {}", cycle.join(" -> ")),
            );
        }
    }

    report
}

pub fn validate_node(repo_root: &Path, node: &Node) -> PulseResult<()> {
    if node.schema_version != 1 {
        return Err(PulseError::validation("unsupported_node_version", "node schema_version must be 1"));
    }
    validate_id_for_kind(&node.id, node.kind)?;
    if node.revision < 1 {
        return Err(PulseError::validation("invalid_revision", "revision must be >= 1"));
    }
    if node.title.trim().is_empty() {
        return Err(PulseError::validation("invalid_title", "title must not be empty"));
    }
    let rel = safe_repo_relative(&node.content_dir)?;
    if !rel.starts_with("works") {
        return Err(PulseError::validation(
            "unsafe_content_dir",
            format!("content_dir must be under works/: {}", node.content_dir),
        ));
    }
    if rel != Path::new("works").join(&node.id) {
        return Err(PulseError::validation(
            "unsafe_content_dir",
            format!("content_dir must be works/{}", node.id),
        ));
    }
    let full = repo_root.join(&rel);
    if !full.exists() {
        // Advisory for draft nodes per slice contract; callers collect warnings in file validation.
    }
    Ok(())
}

pub fn validate_node_filename(path: &Path, node: &Node) -> PulseResult<()> {
    let stem = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
        PulseError::validation("invalid_filename", format!("invalid node filename {}", path.display()))
    })?;
    if stem != node.id {
        return Err(PulseError::validation(
            "filename_id_mismatch",
            format!("node file {} contains id {}", path.display(), node.id),
        ));
    }
    Ok(())
}

pub fn validate_edge_filename(path: &Path, edge: &Edge) -> PulseResult<()> {
    let stem = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
        PulseError::validation("invalid_filename", format!("invalid edge filename {}", path.display()))
    })?;
    if stem != edge.id {
        return Err(PulseError::validation(
            "filename_id_mismatch",
            format!("edge file {} contains id {}", path.display(), edge.id),
        ));
    }
    Ok(())
}

pub fn validate_edge_for_add(nodes: &BTreeMap<String, Node>, edges: &[Edge], new_edge: &Edge) -> PulseResult<()> {
    if !nodes.contains_key(&new_edge.from) || !nodes.contains_key(&new_edge.to) {
        return Err(PulseError::validation(
            "dangling_edge",
            format!("edge references missing endpoint {} -> {}", new_edge.from, new_edge.to),
        ));
    }
    let mut all = edges.to_vec();
    all.push(new_edge.clone());
    let node_values = nodes.values().cloned().collect::<Vec<_>>();
    let report = validate_graph(Path::new("."), &Manifest::default(), &node_values, &all);
    if report.valid {
        Ok(())
    } else {
        Err(PulseError::validation(
            "invalid_graph",
            serde_json::to_string(&report.errors).unwrap_or_default(),
        ))
    }
}

fn find_cycle(edges: &[Edge], ty: EdgeType) -> Option<Vec<String>> {
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in edges.iter().filter(|e| e.edge_type == ty) {
        adj.entry(edge.from.as_str()).or_default().push(edge.to.as_str());
    }
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    let mut stack = Vec::new();
    for node in adj.keys().copied().collect::<Vec<_>>() {
        if dfs(node, &adj, &mut visiting, &mut visited, &mut stack) {
            stack.push(node.to_string());
            stack.reverse();
            return Some(stack);
        }
    }
    None
}

fn dfs<'a>(
    node: &'a str,
    adj: &HashMap<&'a str, Vec<&'a str>>,
    visiting: &mut HashSet<&'a str>,
    visited: &mut HashSet<&'a str>,
    stack: &mut Vec<String>,
) -> bool {
    if visiting.contains(node) {
        stack.push(node.to_string());
        return true;
    }
    if visited.contains(node) {
        return false;
    }
    visiting.insert(node);
    if let Some(nexts) = adj.get(node) {
        for next in nexts {
            if dfs(next, adj, visiting, visited, stack) {
                if stack.first().map(|s| s.as_str()) != Some(node) {
                    stack.push(node.to_string());
                }
                return true;
            }
        }
    }
    visiting.remove(node);
    visited.insert(node);
    false
}
