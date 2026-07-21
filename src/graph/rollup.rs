use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::graph::edge::EdgeType;
use crate::graph::node::NodeStatus;
use crate::graph::projection::GraphProjection;
use crate::id::WorkKind;
use crate::{PulseError, PulseResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RollupReport {
    pub schema_version: u32,
    pub subject: String,
    pub graph_fingerprint: String,
    pub direct_children: usize,
    pub descendant_tickets: usize,
    pub by_status: BTreeMap<NodeStatus, usize>,
    pub open_hard_blockers: Vec<String>,
    pub terminal_outcomes: TerminalOutcomes,
    pub completion_claim: CompletionClaim,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalOutcomes {
    pub done: usize,
    pub cancelled: usize,
    pub superseded: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompletionClaim {
    NotEvaluated,
}

pub fn rollup(projection: &GraphProjection, subject: &str) -> PulseResult<RollupReport> {
    let node_by_id = projection
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node))
        .collect::<BTreeMap<_, _>>();
    let subject_node = node_by_id
        .get(subject)
        .ok_or_else(|| PulseError::NotFound {
            subject: subject.to_string(),
        })?;
    if !matches!(subject_node.kind, WorkKind::Epic | WorkKind::Story) {
        return Err(PulseError::validation(
            "not_rollup_kind",
            "roll-up is only defined for Epic and Story",
        ));
    }

    let children_by_parent = children_by_parent(projection);
    detect_hierarchy_cycle(&children_by_parent)?;
    let direct = children_by_parent.get(subject).cloned().unwrap_or_default();
    let descendants = descendants(&children_by_parent, subject);

    let mut by_status = BTreeMap::new();
    let mut descendant_tickets = 0usize;
    let mut terminal_outcomes = TerminalOutcomes {
        done: 0,
        cancelled: 0,
        superseded: 0,
    };
    for id in &descendants {
        let Some(node) = node_by_id.get(id) else {
            continue;
        };
        *by_status.entry(node.status).or_insert(0) += 1;
        if node.kind == WorkKind::Ticket {
            descendant_tickets += 1;
            match node.status {
                NodeStatus::Done => terminal_outcomes.done += 1,
                NodeStatus::Cancelled => terminal_outcomes.cancelled += 1,
                NodeStatus::Superseded => terminal_outcomes.superseded += 1,
                _ => {}
            }
        }
    }

    let mut open_hard_blockers = BTreeSet::new();
    let descendant_set = descendants.iter().cloned().collect::<BTreeSet<_>>();
    for edge in projection
        .edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::BlockedBy && descendant_set.contains(&edge.from))
    {
        if let Some(blocker) = node_by_id.get(&edge.to) {
            if !matches!(blocker.status, NodeStatus::Done) {
                open_hard_blockers.insert(edge.to.clone());
            }
        } else {
            open_hard_blockers.insert(edge.to.clone());
        }
    }

    Ok(RollupReport {
        schema_version: 1,
        subject: subject.to_string(),
        graph_fingerprint: projection.graph_fingerprint.clone(),
        direct_children: direct.len(),
        descendant_tickets,
        by_status,
        open_hard_blockers: open_hard_blockers.into_iter().collect(),
        terminal_outcomes,
        completion_claim: CompletionClaim::NotEvaluated,
    })
}

fn children_by_parent(projection: &GraphProjection) -> BTreeMap<String, Vec<String>> {
    let mut children_by_parent: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for edge in projection
        .edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::Parent)
    {
        children_by_parent
            .entry(edge.to.clone())
            .or_default()
            .push(edge.from.clone());
    }
    for children in children_by_parent.values_mut() {
        children.sort();
    }
    children_by_parent
}

fn descendants(children_by_parent: &BTreeMap<String, Vec<String>>, subject: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut queue = VecDeque::from([subject.to_string()]);
    let mut seen = BTreeSet::from([subject.to_string()]);
    while let Some(current) = queue.pop_front() {
        if let Some(children) = children_by_parent.get(&current) {
            for child in children {
                if seen.insert(child.clone()) {
                    out.push(child.clone());
                    queue.push_back(child.clone());
                }
            }
        }
    }
    out.sort();
    out
}

fn detect_hierarchy_cycle(children_by_parent: &BTreeMap<String, Vec<String>>) -> PulseResult<()> {
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut stack = Vec::new();
    for node in children_by_parent.keys() {
        if visit(
            node,
            children_by_parent,
            &mut visiting,
            &mut visited,
            &mut stack,
        ) {
            stack.reverse();
            return Err(PulseError::validation(
                "hierarchy_cycle",
                format!("hierarchy cycle detected: {}", stack.join(" -> ")),
            ));
        }
    }
    Ok(())
}

fn visit(
    node: &str,
    children_by_parent: &BTreeMap<String, Vec<String>>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
    stack: &mut Vec<String>,
) -> bool {
    if visiting.contains(node) {
        stack.push(node.to_string());
        return true;
    }
    if visited.contains(node) {
        return false;
    }
    visiting.insert(node.to_string());
    if let Some(children) = children_by_parent.get(node) {
        for child in children {
            if visit(child, children_by_parent, visiting, visited, stack) {
                if stack.first().map(|id| id.as_str()) != Some(node) {
                    stack.push(node.to_string());
                }
                return true;
            }
        }
    }
    visiting.remove(node);
    visited.insert(node.to_string());
    false
}
