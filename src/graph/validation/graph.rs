use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use crate::graph::contract::{self, ContractValidationMode, NODE_SCHEMA_VERSION};
use crate::graph::edge::{deterministic_edge_id, Edge, EdgeType};
use crate::graph::lifecycle::{status_requires_reason, validate_reason, TransitionReason};
use crate::graph::manifest::Manifest;
use crate::graph::node::{Node, NodeStatus};
use crate::id::validate_id_for_kind;
use crate::{PulseError, PulseResult};
use serde::{Deserialize, Serialize};

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
        report.push_error(
            "unsupported_manifest_version",
            "manifest schema_version must be 1",
        );
    }
    if manifest.node_schema != "schemas/node.schema.json" {
        report.push_error(
            "invalid_manifest",
            "manifest node_schema must be schemas/node.schema.json",
        );
    }
    if manifest.edge_schema != "schemas/edge.schema.json" {
        report.push_error(
            "invalid_manifest",
            "manifest edge_schema must be schemas/edge.schema.json",
        );
    }

    let mut ids = BTreeSet::new();
    for node in nodes {
        if !ids.insert(node.id.clone()) {
            report.push_error(
                "duplicate_node_id",
                format!("duplicate node id {}", node.id),
            );
        }
        if let Err(e) = validate_node(repo_root, manifest, node) {
            report.push_error(e.code(), e.to_string());
        }
    }

    let mut edge_ids = BTreeSet::new();
    let mut parent_by_child: BTreeMap<String, String> = BTreeMap::new();
    for edge in edges {
        if !edge_ids.insert(edge.id.clone()) {
            report.push_error(
                "duplicate_edge_id",
                format!("duplicate edge id {}", edge.id),
            );
        }
        if let Err(e) = validate_edge_schema_semantics(edge) {
            report.push_error(e.code(), e.to_string());
        }
        if !ids.contains(&edge.from) || !ids.contains(&edge.to) {
            report.push_error(
                "dangling_edge",
                format!(
                    "edge {} references missing endpoint {} -> {}",
                    edge.id, edge.from, edge.to
                ),
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
                format!(
                    "related edge {} must store endpoints in lexical order",
                    edge.id
                ),
            );
        }
        if edge.edge_type == EdgeType::Parent {
            if let Some(existing) = parent_by_child.insert(edge.from.clone(), edge.to.clone()) {
                report.push_error(
                    "multiple_parents",
                    format!(
                        "node {} has parents {} and {}",
                        edge.from, existing, edge.to
                    ),
                );
            }
        }
    }

    for ty in [
        EdgeType::Parent,
        EdgeType::BlockedBy,
        EdgeType::SupersededBy,
    ] {
        if let Some(cycle) = find_cycle(edges, ty) {
            report.push_error(
                "cycle_detected",
                format!("{ty:?} cycle detected: {}", cycle.join(" -> ")),
            );
        }
    }

    let nodes_by_id = nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut superseded_by_count: BTreeMap<&str, usize> = BTreeMap::new();
    for edge in edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::SupersededBy)
    {
        *superseded_by_count.entry(edge.from.as_str()).or_default() += 1;
        match nodes_by_id.get(edge.from.as_str()) {
            Some(node) if node.status == NodeStatus::Superseded => {}
            Some(_) => report.push_error(
                "supersession_status_mismatch",
                format!(
                    "node {} has outgoing superseded_by edge but is not superseded",
                    edge.from
                ),
            ),
            None => {}
        }
        match nodes_by_id.get(edge.to.as_str()) {
            Some(node) if matches!(node.status, NodeStatus::Done | NodeStatus::Cancelled) => {
                report.push_error(
                    "invalid_supersession_target",
                    format!("supersession target {} is terminal", edge.to),
                );
            }
            _ => {}
        }
    }
    for node in nodes {
        let outgoing = superseded_by_count
            .get(node.id.as_str())
            .copied()
            .unwrap_or(0);
        if outgoing > 1 {
            report.push_error(
                "multiple_supersession_targets",
                format!("node {} has more than one superseded_by edge", node.id),
            );
        }
        if node.status == NodeStatus::Superseded {
            let decision_reference = node.status_reason.as_ref().and_then(|reason| {
                reason
                    .reference
                    .as_ref()
                    .and_then(|reference| nodes_by_id.get(reference.as_str()))
                    .filter(|target| target.kind == crate::id::WorkKind::Decision)
            });
            match (outgoing, decision_reference) {
                (1, None) => {}
                (0, Some(_)) => {}
                (1, Some(_)) => report.push_error(
                    "ambiguous_supersession_target",
                    format!(
                        "node {} has both superseded_by edge and Decision status_reason reference",
                        node.id
                    ),
                ),
                _ => report.push_error(
                    "missing_supersession_target",
                    format!(
                        "superseded node {} must have exactly one supersession target form",
                        node.id
                    ),
                ),
            }
        }
    }

    report
}

pub fn validate_node(repo_root: &Path, manifest: &Manifest, node: &Node) -> PulseResult<()> {
    validate_node_schema_semantics(node)?;
    let rel = crate::graph::model::node::safe_repo_relative(&node.content_dir)?;
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
    crate::storage::paths::resolve_content_path_under(repo_root, &manifest.content_root, &rel)?;
    Ok(())
}

pub fn validate_node_schema_semantics(node: &Node) -> PulseResult<()> {
    if node.schema_version != NODE_SCHEMA_VERSION {
        return Err(PulseError::validation(
            "unsupported_node_version",
            "node schema_version must be 1",
        ));
    }
    validate_id_for_kind(&node.id, node.kind)?;
    if node.revision < 1 {
        return Err(PulseError::validation(
            "invalid_revision",
            "revision must be >= 1",
        ));
    }
    if node.title.trim().is_empty() {
        return Err(PulseError::validation(
            "invalid_title",
            "title must not be empty",
        ));
    }
    match (&node.status_reason, status_requires_reason(node.status)) {
        (Some(reason), _) => validate_reason(&TransitionReason {
            code: reason.code.clone(),
            summary: reason.summary.clone(),
            reference: reason.reference.clone(),
        })?,
        (None, true) => {
            return Err(PulseError::validation(
                "missing_status_reason",
                format!("status {:?} requires status_reason", node.status),
            ));
        }
        (None, false) => {}
    }
    if node.status_reason.is_some() && !status_requires_reason(node.status) {
        return Err(PulseError::validation(
            "stale_status_reason",
            format!("status {:?} must not persist status_reason", node.status),
        ));
    }
    if let Some(documentation) = &node.documentation {
        documentation.validate(false)?;
    }
    contract::validate_node_contract_result(node, ContractValidationMode::CanonicalStorage)?;
    Ok(())
}

pub fn validate_edge_schema_semantics(edge: &Edge) -> PulseResult<()> {
    if edge.schema_version != 1 {
        return Err(PulseError::validation(
            "unsupported_edge_version",
            "edge schema_version must be 1",
        ));
    }
    crate::id::validate_work_id(&edge.from)?;
    crate::id::validate_work_id(&edge.to)?;
    if edge.revision < 1 {
        return Err(PulseError::validation(
            "invalid_revision",
            "edge revision must be >= 1",
        ));
    }
    if edge.created_by.trim().is_empty() {
        return Err(PulseError::validation(
            "invalid_actor",
            "created_by must not be empty",
        ));
    }
    Ok(())
}

pub fn validate_node_filename(path: &Path, node: &Node) -> PulseResult<()> {
    let stem = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
        PulseError::validation(
            "invalid_filename",
            format!("invalid node filename {}", path.display()),
        )
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
        PulseError::validation(
            "invalid_filename",
            format!("invalid edge filename {}", path.display()),
        )
    })?;
    if stem != edge.id {
        return Err(PulseError::validation(
            "filename_id_mismatch",
            format!("edge file {} contains id {}", path.display(), edge.id),
        ));
    }
    Ok(())
}

pub fn validate_edge_for_add(
    nodes: &BTreeMap<String, Node>,
    edges: &[Edge],
    new_edge: &Edge,
) -> PulseResult<()> {
    if !nodes.contains_key(&new_edge.from) || !nodes.contains_key(&new_edge.to) {
        return Err(PulseError::validation(
            "dangling_edge",
            format!(
                "edge references missing endpoint {} -> {}",
                new_edge.from, new_edge.to
            ),
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
        adj.entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
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
