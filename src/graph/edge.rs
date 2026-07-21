use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::PulseResult;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Edge {
    pub schema_version: u32,
    pub id: String,
    #[serde(rename = "type")]
    pub edge_type: EdgeType,
    pub from: String,
    pub to: String,
    pub revision: u64,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    Parent,
    BlockedBy,
    PreferredAfter,
    SupersededBy,
    Related,
    Duplicates,
}

impl EdgeType {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Parent => "parent",
            Self::BlockedBy => "blocked-by",
            Self::PreferredAfter => "preferred-after",
            Self::SupersededBy => "superseded-by",
            Self::Related => "related",
            Self::Duplicates => "duplicates",
        }
    }
}

impl Edge {
    pub fn new(
        edge_type: EdgeType,
        from: String,
        to: String,
        actor: String,
        now: DateTime<Utc>,
    ) -> PulseResult<Self> {
        let (from, to) = canonical_endpoints(edge_type, from, to);
        let id = deterministic_edge_id(edge_type, &from, &to);
        Ok(Self {
            schema_version: 1,
            id,
            edge_type,
            from,
            to,
            revision: 1,
            created_at: now,
            created_by: actor,
        })
    }
}

pub fn canonical_endpoints(edge_type: EdgeType, from: String, to: String) -> (String, String) {
    if edge_type == EdgeType::Related && to < from {
        (to, from)
    } else {
        (from, to)
    }
}

pub fn deterministic_edge_id(edge_type: EdgeType, from: &str, to: &str) -> String {
    let (from, to) = if edge_type == EdgeType::Related && to < from {
        (to, from)
    } else {
        (from, to)
    };
    format!("{}--{}--{}", edge_type.slug(), from, to)
}
