use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::graph::edge::{Edge, EdgeType};
use crate::graph::projection::GraphProjection;
use crate::{PulseError, PulseResult};

pub const MAX_NEIGHBORHOOD_DEPTH: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NeighborhoodReport {
    pub schema_version: u32,
    pub subject: String,
    pub graph_fingerprint: String,
    pub depth: usize,
    pub nodes: Vec<String>,
    pub edges: Vec<NeighborhoodEdge>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct NeighborhoodEdge {
    pub id: String,
    pub edge_type: EdgeType,
    pub from: String,
    pub to: String,
    pub direction: TraversalDirection,
    pub depth: usize,
    pub path: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TraversalDirection {
    Outgoing,
    Incoming,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AffectedByReport {
    pub schema_version: u32,
    pub subject: String,
    pub graph_fingerprint: String,
    pub relation_filter: Option<EdgeType>,
    pub hard: Vec<AffectedNode>,
    pub rollup: Vec<AffectedNode>,
    pub supersession: Vec<AffectedNode>,
    pub advisory: Vec<AffectedNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct AffectedNode {
    pub id: String,
    pub relation: EdgeType,
    pub basis: String,
    pub path: Vec<String>,
}

pub fn neighborhood(
    projection: &GraphProjection,
    subject: &str,
    requested_depth: usize,
) -> PulseResult<NeighborhoodReport> {
    ensure_node(projection, subject)?;
    let depth = requested_depth.min(MAX_NEIGHBORHOOD_DEPTH);
    let truncated = requested_depth > MAX_NEIGHBORHOOD_DEPTH;
    let mut seen_nodes = BTreeSet::from([subject.to_string()]);
    let mut seen_edges = BTreeSet::new();
    let mut out_edges = Vec::new();
    let mut queue = VecDeque::from([(subject.to_string(), 0usize, vec![subject.to_string()])]);

    while let Some((current, current_depth, path)) = queue.pop_front() {
        if current_depth >= depth {
            continue;
        }
        let mut adjacent = adjacent_edges(projection, &current);
        adjacent.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.id.cmp(&b.1.id)).then(a.2.cmp(&b.2)));
        for (direction, edge, next) in adjacent {
            let next_depth = current_depth + 1;
            let mut edge_path = path.clone();
            edge_path.push(next.clone());
            if seen_edges.insert((edge.id.clone(), direction)) {
                out_edges.push(NeighborhoodEdge {
                    id: edge.id.clone(),
                    edge_type: edge.edge_type,
                    from: edge.from.clone(),
                    to: edge.to.clone(),
                    direction,
                    depth: next_depth,
                    path: edge_path.clone(),
                });
            }
            if seen_nodes.insert(next.clone()) {
                queue.push_back((next, next_depth, edge_path));
            }
        }
    }

    out_edges.sort();
    Ok(NeighborhoodReport {
        schema_version: 1,
        subject: subject.to_string(),
        graph_fingerprint: projection.graph_fingerprint.clone(),
        depth,
        nodes: seen_nodes.into_iter().collect(),
        edges: out_edges,
        truncated,
    })
}

pub fn affected_by(
    projection: &GraphProjection,
    subject: &str,
    relation_filter: Option<EdgeType>,
) -> PulseResult<AffectedByReport> {
    ensure_node(projection, subject)?;
    let mut hard = Vec::new();
    let mut rollup = Vec::new();
    let mut supersession = Vec::new();
    let mut advisory = Vec::new();

    if relation_matches(relation_filter, EdgeType::BlockedBy) {
        for edge in projection
            .edges
            .iter()
            .filter(|edge| edge.edge_type == EdgeType::BlockedBy && edge.to == subject)
        {
            hard.push(AffectedNode {
                id: edge.from.clone(),
                relation: EdgeType::BlockedBy,
                basis: "reverse_blocked_by".to_string(),
                path: vec![subject.to_string(), edge.from.clone()],
            });
        }
    }

    if relation_matches(relation_filter, EdgeType::Parent) {
        for node in hierarchy_related(projection, subject) {
            rollup.push(node);
        }
    }

    if relation_matches(relation_filter, EdgeType::SupersededBy) {
        for edge in projection
            .edges
            .iter()
            .filter(|edge| edge.edge_type == EdgeType::SupersededBy && edge.to == subject)
        {
            supersession.push(AffectedNode {
                id: edge.from.clone(),
                relation: EdgeType::SupersededBy,
                basis: "reverse_superseded_by".to_string(),
                path: vec![subject.to_string(), edge.from.clone()],
            });
        }
    }
    if relation_matches(relation_filter, EdgeType::Duplicates) {
        for edge in projection
            .edges
            .iter()
            .filter(|edge| edge.edge_type == EdgeType::Duplicates && edge.to == subject)
        {
            supersession.push(AffectedNode {
                id: edge.from.clone(),
                relation: EdgeType::Duplicates,
                basis: "reverse_duplicates".to_string(),
                path: vec![subject.to_string(), edge.from.clone()],
            });
        }
    }

    if relation_matches(relation_filter, EdgeType::PreferredAfter) {
        for edge in projection
            .edges
            .iter()
            .filter(|edge| edge.edge_type == EdgeType::PreferredAfter && edge.to == subject)
        {
            advisory.push(AffectedNode {
                id: edge.from.clone(),
                relation: EdgeType::PreferredAfter,
                basis: "advisory_preferred_before".to_string(),
                path: vec![subject.to_string(), edge.from.clone()],
            });
        }
    }

    hard.sort();
    rollup.sort();
    supersession.sort();
    advisory.sort();
    Ok(AffectedByReport {
        schema_version: 1,
        subject: subject.to_string(),
        graph_fingerprint: projection.graph_fingerprint.clone(),
        relation_filter,
        hard,
        rollup,
        supersession,
        advisory,
    })
}

fn ensure_node(projection: &GraphProjection, subject: &str) -> PulseResult<()> {
    if projection.nodes.iter().any(|node| node.id == subject) {
        Ok(())
    } else {
        Err(PulseError::NotFound {
            subject: subject.to_string(),
        })
    }
}

fn adjacent_edges<'a>(
    projection: &'a GraphProjection,
    subject: &str,
) -> Vec<(TraversalDirection, &'a Edge, String)> {
    let mut edges = Vec::new();
    for edge in &projection.edges {
        if edge.from == subject {
            edges.push((TraversalDirection::Outgoing, edge, edge.to.clone()));
        }
        if edge.to == subject {
            edges.push((TraversalDirection::Incoming, edge, edge.from.clone()));
        }
    }
    edges
}

fn relation_matches(filter: Option<EdgeType>, relation: EdgeType) -> bool {
    match filter {
        Some(wanted) => wanted == relation,
        None => true,
    }
}

fn hierarchy_related(projection: &GraphProjection, subject: &str) -> Vec<AffectedNode> {
    let mut out = Vec::new();
    let mut parent_by_child = BTreeMap::new();
    let mut children_by_parent: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for edge in projection
        .edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::Parent)
    {
        parent_by_child.insert(edge.from.clone(), edge.to.clone());
        children_by_parent
            .entry(edge.to.clone())
            .or_default()
            .push(edge.from.clone());
    }

    let mut current = subject.to_string();
    let mut path = vec![subject.to_string()];
    let mut seen = BTreeSet::new();
    while let Some(parent) = parent_by_child.get(&current) {
        if !seen.insert(parent.clone()) {
            break;
        }
        path.push(parent.clone());
        out.push(AffectedNode {
            id: parent.clone(),
            relation: EdgeType::Parent,
            basis: "ancestor_rollup".to_string(),
            path: path.clone(),
        });
        current = parent.clone();
    }

    let mut queue = VecDeque::from([(subject.to_string(), vec![subject.to_string()])]);
    let mut seen_desc = BTreeSet::from([subject.to_string()]);
    while let Some((node, path)) = queue.pop_front() {
        if let Some(children) = children_by_parent.get(&node) {
            let mut children = children.clone();
            children.sort();
            for child in children {
                if seen_desc.insert(child.clone()) {
                    let mut child_path = path.clone();
                    child_path.push(child.clone());
                    out.push(AffectedNode {
                        id: child.clone(),
                        relation: EdgeType::Parent,
                        basis: "descendant_rollup".to_string(),
                        path: child_path.clone(),
                    });
                    queue.push_back((child, child_path));
                }
            }
        }
    }
    out
}
