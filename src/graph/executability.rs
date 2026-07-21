use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::graph::edge::EdgeType;
use crate::graph::node::{Node, NodeStatus};
use crate::graph::projection::GraphProjection;
use crate::id::WorkKind;
use crate::{PulseError, PulseResult};

pub const MISSING_GATE_FAMILIES: &[&str] = &[
    "implementation_contract",
    "shaping_authority",
    "documentation_impact",
    "qa_impact",
    "receipts",
    "lease",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StructuralState {
    Candidate,
    Blocked,
    Paused,
    Terminal,
    NotExecutableKind,
    Invalid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructuralExecutabilityReport {
    pub schema_version: u32,
    pub subject: String,
    pub graph_fingerprint: String,
    pub structural_state: StructuralState,
    pub dispatch_authorized: bool,
    pub lifecycle: LifecycleSummary,
    pub hard_blockers: Vec<HardBlockerReport>,
    pub soft_preferences: Vec<SoftPreferenceReport>,
    pub supersession: Option<SupersessionReport>,
    pub gate_coverage: Vec<String>,
    pub missing_gate_families: Vec<String>,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LifecycleSummary {
    pub status: NodeStatus,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardBlockerReport {
    pub id: String,
    pub status: Option<NodeStatus>,
    pub resolution: BlockerResolution,
    pub resolution_basis: String,
    pub path: Vec<String>,
    pub missing_resolver_families: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlockerResolution {
    Satisfied,
    Unsatisfied,
    UnknownToSlice,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SoftPreferenceReport {
    pub preferred_after: String,
    pub status: Option<NodeStatus>,
    pub resolution_basis: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupersessionReport {
    pub replacement: Option<String>,
    pub resolution_basis: String,
    pub path: Vec<String>,
    pub missing_resolver_families: Vec<String>,
}

pub fn structural_executability(
    projection: &GraphProjection,
    subject: &str,
) -> PulseResult<StructuralExecutabilityReport> {
    let nodes = node_index(projection);
    let node = nodes.get(subject).ok_or_else(|| PulseError::NotFound {
        subject: subject.to_string(),
    })?;

    let mut reason_codes = BTreeSet::new();
    let mut structural_state = StructuralState::Candidate;

    if node.kind != WorkKind::Ticket {
        structural_state = StructuralState::NotExecutableKind;
        reason_codes.insert("not_executable_kind".to_string());
    }

    let supersession = supersession_report(projection, &nodes, subject);
    if supersession.is_some() && !matches!(node.status, NodeStatus::Superseded) {
        reason_codes.insert("superseded_by_replacement".to_string());
    }

    let hard_blockers = hard_blockers(projection, &nodes, subject);
    if hard_blockers
        .iter()
        .any(|b| b.resolution != BlockerResolution::Satisfied)
    {
        reason_codes.insert("hard_blocker_open".to_string());
    }
    let soft_preferences = soft_preferences(projection, &nodes, subject);

    if node.kind == WorkKind::Ticket {
        structural_state = match node.status {
            NodeStatus::Done | NodeStatus::Cancelled | NodeStatus::Superseded => {
                reason_codes.insert("terminal_lifecycle".to_string());
                StructuralState::Terminal
            }
            NodeStatus::Blocked => {
                reason_codes.insert("explicitly_blocked".to_string());
                StructuralState::Paused
            }
            NodeStatus::Draft => {
                reason_codes.insert("work_not_shaped".to_string());
                StructuralState::Blocked
            }
            NodeStatus::Shaped | NodeStatus::Ready => {
                if hard_blockers
                    .iter()
                    .any(|b| b.resolution != BlockerResolution::Satisfied)
                    || supersession.is_some()
                {
                    StructuralState::Blocked
                } else {
                    StructuralState::Candidate
                }
            }
            NodeStatus::Active | NodeStatus::Verifying | NodeStatus::Rework => {
                reason_codes.insert("lifecycle_not_new_dispatch_candidate".to_string());
                StructuralState::Paused
            }
        };
    }

    Ok(StructuralExecutabilityReport {
        schema_version: 1,
        subject: subject.to_string(),
        graph_fingerprint: projection.graph_fingerprint.clone(),
        structural_state,
        dispatch_authorized: false,
        lifecycle: LifecycleSummary {
            status: node.status,
            revision: node.revision,
        },
        hard_blockers,
        soft_preferences,
        supersession,
        gate_coverage: [
            "graph_validity",
            "lifecycle_state",
            "hard_dependencies",
            "soft_preferences",
            "supersession",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        missing_gate_families: MISSING_GATE_FAMILIES
            .iter()
            .map(|s| s.to_string())
            .collect(),
        reason_codes: reason_codes.into_iter().collect(),
    })
}

fn node_index(projection: &GraphProjection) -> BTreeMap<String, Node> {
    projection
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.clone()))
        .collect()
}

fn hard_blockers(
    projection: &GraphProjection,
    nodes: &BTreeMap<String, Node>,
    subject: &str,
) -> Vec<HardBlockerReport> {
    let mut blockers = projection
        .edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::BlockedBy && edge.from == subject)
        .map(|edge| resolve_blocker(projection, nodes, subject, &edge.to))
        .collect::<Vec<_>>();
    blockers.sort_by(|a, b| a.id.cmp(&b.id).then(a.path.cmp(&b.path)));
    blockers
}

fn soft_preferences(
    projection: &GraphProjection,
    nodes: &BTreeMap<String, Node>,
    subject: &str,
) -> Vec<SoftPreferenceReport> {
    let mut preferences = projection
        .edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::PreferredAfter && edge.from == subject)
        .map(|edge| SoftPreferenceReport {
            preferred_after: edge.to.clone(),
            status: nodes.get(&edge.to).map(|node| node.status),
            resolution_basis: "advisory_only".to_string(),
        })
        .collect::<Vec<_>>();
    preferences.sort_by(|a, b| a.preferred_after.cmp(&b.preferred_after));
    preferences
}

fn supersession_report(
    projection: &GraphProjection,
    nodes: &BTreeMap<String, Node>,
    subject: &str,
) -> Option<SupersessionReport> {
    let mut replacements = projection
        .edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::SupersededBy && edge.from == subject)
        .map(|edge| edge.to.clone())
        .collect::<Vec<_>>();
    replacements.sort();
    let replacement = replacements.first()?.clone();
    let resolved = resolve_supersession_chain(projection, nodes, subject, &replacement);
    Some(SupersessionReport {
        replacement: Some(replacement),
        resolution_basis: resolved.basis,
        path: resolved.path,
        missing_resolver_families: resolved.missing_resolver_families,
    })
}

fn resolve_blocker(
    projection: &GraphProjection,
    nodes: &BTreeMap<String, Node>,
    subject: &str,
    blocker: &str,
) -> HardBlockerReport {
    let status = nodes.get(blocker).map(|node| node.status);
    let (resolution, basis, path, missing_resolver_families) = match nodes.get(blocker) {
        None => (
            BlockerResolution::Unsatisfied,
            "missing_blocker_node".to_string(),
            vec![subject.to_string(), blocker.to_string()],
            vec![],
        ),
        Some(node) if node.kind != WorkKind::Ticket => (
            BlockerResolution::UnknownToSlice,
            "non_ticket_blocker".to_string(),
            vec![subject.to_string(), blocker.to_string()],
            vec!["typed_prerequisite_resolver".to_string()],
        ),
        Some(node) => match node.status {
            NodeStatus::Done => (
                BlockerResolution::Satisfied,
                "terminal_done".to_string(),
                vec![subject.to_string(), blocker.to_string()],
                vec![],
            ),
            NodeStatus::Superseded => {
                let replacement = first_superseded_by(projection, blocker);
                match replacement {
                    Some(replacement) => {
                        let chain =
                            resolve_supersession_chain(projection, nodes, blocker, &replacement);
                        let resolution = if chain.satisfied {
                            BlockerResolution::Satisfied
                        } else if chain.unknown {
                            BlockerResolution::UnknownToSlice
                        } else {
                            BlockerResolution::Unsatisfied
                        };
                        let mut path = vec![subject.to_string()];
                        path.extend(chain.path);
                        (
                            resolution,
                            chain.basis,
                            path,
                            chain.missing_resolver_families,
                        )
                    }
                    None => (
                        BlockerResolution::UnknownToSlice,
                        "superseded_decision_or_missing_replacement_resolver".to_string(),
                        vec![subject.to_string(), blocker.to_string()],
                        vec!["decision_resolution".to_string()],
                    ),
                }
            }
            NodeStatus::Cancelled => (
                BlockerResolution::Unsatisfied,
                "terminal_cancelled".to_string(),
                vec![subject.to_string(), blocker.to_string()],
                vec![],
            ),
            _ => (
                BlockerResolution::Unsatisfied,
                "open_lifecycle".to_string(),
                vec![subject.to_string(), blocker.to_string()],
                vec![],
            ),
        },
    };

    HardBlockerReport {
        id: blocker.to_string(),
        status,
        resolution,
        resolution_basis: basis,
        path,
        missing_resolver_families,
    }
}

struct ChainResolution {
    satisfied: bool,
    unknown: bool,
    basis: String,
    path: Vec<String>,
    missing_resolver_families: Vec<String>,
}

fn resolve_supersession_chain(
    projection: &GraphProjection,
    nodes: &BTreeMap<String, Node>,
    start: &str,
    replacement: &str,
) -> ChainResolution {
    let mut path = vec![start.to_string(), replacement.to_string()];
    let mut seen = BTreeSet::from([start.to_string()]);
    let mut current = replacement.to_string();

    loop {
        if !seen.insert(current.clone()) {
            return ChainResolution {
                satisfied: false,
                unknown: false,
                basis: "supersession_cycle".to_string(),
                path,
                missing_resolver_families: vec![],
            };
        }
        let Some(node) = nodes.get(&current) else {
            return ChainResolution {
                satisfied: false,
                unknown: false,
                basis: "missing_replacement_node".to_string(),
                path,
                missing_resolver_families: vec![],
            };
        };
        if node.kind != WorkKind::Ticket {
            return ChainResolution {
                satisfied: false,
                unknown: true,
                basis: "replacement_non_ticket".to_string(),
                path,
                missing_resolver_families: vec!["typed_outcome_resolver".to_string()],
            };
        }
        match node.status {
            NodeStatus::Done => {
                return ChainResolution {
                    satisfied: true,
                    unknown: false,
                    basis: "superseded_chain_terminal_done".to_string(),
                    path,
                    missing_resolver_families: vec![],
                };
            }
            NodeStatus::Superseded => match first_superseded_by(projection, &current) {
                Some(next) => {
                    path.push(next.clone());
                    current = next;
                }
                None => {
                    return ChainResolution {
                        satisfied: false,
                        unknown: true,
                        basis: "superseded_decision_or_missing_replacement_resolver".to_string(),
                        path,
                        missing_resolver_families: vec!["decision_resolution".to_string()],
                    };
                }
            },
            NodeStatus::Cancelled => {
                return ChainResolution {
                    satisfied: false,
                    unknown: false,
                    basis: "superseded_chain_terminal_cancelled".to_string(),
                    path,
                    missing_resolver_families: vec![],
                };
            }
            _ => {
                return ChainResolution {
                    satisfied: false,
                    unknown: false,
                    basis: "superseded_chain_open".to_string(),
                    path,
                    missing_resolver_families: vec![],
                };
            }
        }
    }
}

fn first_superseded_by(projection: &GraphProjection, subject: &str) -> Option<String> {
    let mut replacements = projection
        .edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::SupersededBy && edge.from == subject)
        .map(|edge| edge.to.clone())
        .collect::<Vec<_>>();
    replacements.sort();
    replacements.into_iter().next()
}
