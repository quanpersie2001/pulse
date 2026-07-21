use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::graph::edge::{Edge, EdgeType};
use crate::graph::executability::{structural_executability, StructuralExecutabilityReport};
use crate::graph::lifecycle::{status_class, StatusClass};
use crate::graph::manifest::Manifest;
use crate::graph::node::{Node, NodeStatus};
use crate::graph::rollup::{rollup, RollupReport};
use crate::id::WorkKind;
use crate::storage;
use crate::{PulseError, PulseResult};

pub const PROJECTION_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphProjection {
    pub schema_version: u32,
    pub graph_fingerprint: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub inverse: InverseIndexes,
    pub lifecycle: LifecycleProjection,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct LifecycleProjection {
    pub status_classes: BTreeMap<StatusClass, Vec<String>>,
    pub structural_executability: BTreeMap<String, StructuralExecutabilityReport>,
    pub rollups: BTreeMap<String, RollupReport>,
    pub by_status: BTreeMap<NodeStatus, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct InverseIndexes {
    pub children: BTreeMap<String, Vec<String>>,
    pub blocks: BTreeMap<String, Vec<String>>,
    pub preferred_before: BTreeMap<String, Vec<String>>,
    pub supersedes: BTreeMap<String, Vec<String>>,
    pub has_duplicate: BTreeMap<String, Vec<String>>,
    pub related: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedProjection {
    cache_schema_version: u32,
    graph_fingerprint: String,
    projection_schema_version: u32,
    projection: GraphProjection,
}

pub fn export_with_cache(
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
                && cache.projection_schema_version == PROJECTION_SCHEMA_VERSION
                && cache.graph_fingerprint == fingerprint
                && cache.projection.graph_fingerprint == fingerprint
            {
                return Ok(cache.projection);
            }
        }
    }
    let projection = build_projection(fingerprint, node_files, edge_files);
    let cache = CachedProjection {
        cache_schema_version: 1,
        graph_fingerprint: projection.graph_fingerprint.clone(),
        projection_schema_version: PROJECTION_SCHEMA_VERSION,
        projection: projection.clone(),
    };
    let bytes = to_canonical_bytes(&cache)?;
    storage::atomic_write(&cache_path, &bytes)?;
    Ok(projection)
}

pub fn build_projection(
    graph_fingerprint: String,
    node_files: &[(PathBuf, Node)],
    edge_files: &[(PathBuf, Edge)],
) -> GraphProjection {
    let mut nodes: Vec<Node> = node_files.iter().map(|(_, n)| n.clone()).collect();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    let mut edges: Vec<Edge> = edge_files.iter().map(|(_, e)| e.clone()).collect();
    edges.sort_by(|a, b| a.id.cmp(&b.id));

    let mut inverse = InverseIndexes::default();
    for edge in &edges {
        match edge.edge_type {
            EdgeType::Parent => push_sorted(&mut inverse.children, &edge.to, &edge.from),
            EdgeType::BlockedBy => push_sorted(&mut inverse.blocks, &edge.to, &edge.from),
            EdgeType::PreferredAfter => {
                push_sorted(&mut inverse.preferred_before, &edge.to, &edge.from)
            }
            EdgeType::SupersededBy => push_sorted(&mut inverse.supersedes, &edge.to, &edge.from),
            EdgeType::Duplicates => push_sorted(&mut inverse.has_duplicate, &edge.to, &edge.from),
            EdgeType::Related => {
                push_sorted(&mut inverse.related, &edge.from, &edge.to);
                push_sorted(&mut inverse.related, &edge.to, &edge.from);
            }
        }
    }

    let mut projection = GraphProjection {
        schema_version: PROJECTION_SCHEMA_VERSION,
        graph_fingerprint,
        nodes,
        edges,
        inverse,
        lifecycle: LifecycleProjection::default(),
    };
    projection.lifecycle = build_lifecycle_projection(&projection);
    projection
}

fn build_lifecycle_projection(projection: &GraphProjection) -> LifecycleProjection {
    let mut lifecycle = LifecycleProjection::default();
    for node in &projection.nodes {
        push_sorted_enum(&mut lifecycle.by_status, node.status, &node.id);
        push_sorted_enum(
            &mut lifecycle.status_classes,
            status_class(node.status),
            &node.id,
        );
        if node.kind == WorkKind::Ticket {
            if let Ok(report) = structural_executability(projection, &node.id) {
                lifecycle
                    .structural_executability
                    .insert(node.id.clone(), report);
            }
        }
        if matches!(node.kind, WorkKind::Epic | WorkKind::Story) {
            if let Ok(report) = rollup(projection, &node.id) {
                lifecycle.rollups.insert(node.id.clone(), report);
            }
        }
    }
    lifecycle
}

fn push_sorted_enum<K>(map: &mut BTreeMap<K, Vec<String>>, key: K, value: &str)
where
    K: Ord,
{
    let values = map.entry(key).or_default();
    if values.iter().all(|existing| existing != value) {
        values.push(value.to_string());
        values.sort();
    }
}

pub fn graph_fingerprint(
    manifest: &Manifest,
    node_files: &[(PathBuf, Node)],
    edge_files: &[(PathBuf, Edge)],
) -> PulseResult<String> {
    let manifest_hash = crate::canonical_json::hash_value(manifest)?;
    let mut nodes = vec![];
    for (path, node) in node_files {
        nodes.push(path_hash(path, node)?);
    }
    nodes.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    let mut edges = vec![];
    for (path, edge) in edge_files {
        edges.push(path_hash(path, edge)?);
    }
    edges.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    let value = json!({
        "fingerprint_version": 1,
        "manifest_hash": manifest_hash,
        "nodes": nodes,
        "edges": edges,
    });
    Ok(hash_bytes(&to_canonical_bytes(&value)?))
}

fn path_hash<T: Serialize>(path: &Path, value: &T) -> PulseResult<Value> {
    let path_str = path
        .to_str()
        .ok_or_else(|| PulseError::validation("invalid_path", "path is not UTF-8"))?;
    Ok(json!({
        "path": path_str.replace('\\', "/"),
        "hash": crate::canonical_json::hash_value(value)?,
    }))
}

fn push_sorted(map: &mut BTreeMap<String, Vec<String>>, key: &str, value: &str) {
    let values = map.entry(key.to_string()).or_default();
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
        values.sort();
    }
}
