use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{PulseError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeRelation {
    pub schema_version: u32,
    pub id: String,
    pub revision: u64,
    #[serde(rename = "type")]
    pub relation_type: RelationType,
    pub from: Endpoint,
    pub to: Endpoint,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub kind: EndpointKind,
    pub id: String,
    pub revision: Option<u64>,
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    DerivedFrom,
    Corroborates,
    Contradicts,
    SupersededBy,
    PromotedTo,
    ImplementedBy,
    AppliedTo,
    CausedBy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EndpointKind {
    Learning,
    Work,
    Receipt,
    Commit,
    Document,
    Decision,
}

impl RelationType {
    pub fn slug(self) -> &'static str {
        match self {
            Self::DerivedFrom => "derived-from",
            Self::Corroborates => "corroborates",
            Self::Contradicts => "contradicts",
            Self::SupersededBy => "superseded-by",
            Self::PromotedTo => "promoted-to",
            Self::ImplementedBy => "implemented-by",
            Self::AppliedTo => "applied-to",
            Self::CausedBy => "caused-by",
        }
    }

    pub fn modifies_entry_snapshot(self) -> bool {
        matches!(self, Self::DerivedFrom | Self::PromotedTo)
    }
}

impl EndpointKind {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Learning => "learning",
            Self::Work => "work",
            Self::Receipt => "receipt",
            Self::Commit => "commit",
            Self::Document => "document",
            Self::Decision => "decision",
        }
    }
}

impl KnowledgeRelation {
    pub fn new(
        relation_type: RelationType,
        from_learning_id: String,
        mut to: Endpoint,
        created_at: DateTime<Utc>,
        created_by: String,
    ) -> Result<Self> {
        validate_relation_direction(relation_type, to.kind)?;
        let mut from_id = from_learning_id;
        if relation_type == RelationType::Corroborates
            && to.kind == EndpointKind::Learning
            && to.id < from_id
        {
            std::mem::swap(&mut from_id, &mut to.id);
        }
        if matches!(
            relation_type,
            RelationType::SupersededBy | RelationType::Corroborates | RelationType::CausedBy
        ) && to.kind == EndpointKind::Learning
            && from_id == to.id
        {
            return Err(PulseError::validation(
                "knowledge_relation_cycle",
                "relation cannot target itself",
            ));
        }
        let id = deterministic_relation_id(relation_type, &from_id, to.kind, &to.id)?;
        Ok(Self {
            schema_version: 1,
            id,
            revision: 1,
            relation_type,
            from: Endpoint {
                kind: EndpointKind::Learning,
                id: from_id,
                revision: None,
                content_hash: None,
            },
            to,
            created_at,
            created_by,
        })
    }
}

pub fn deterministic_relation_id(
    relation_type: RelationType,
    from_learning_id: &str,
    target_kind: EndpointKind,
    target_id: &str,
) -> Result<String> {
    validate_filename_safe_id(from_learning_id)?;
    validate_filename_safe_id(target_id)?;
    Ok(format!(
        "{}--{}--{}--{}",
        relation_type.slug(),
        from_learning_id,
        target_kind.slug(),
        target_id
    ))
}

pub fn validate_relation_direction(
    relation_type: RelationType,
    target_kind: EndpointKind,
) -> Result<()> {
    let ok = match relation_type {
        RelationType::DerivedFrom => matches!(
            target_kind,
            EndpointKind::Work
                | EndpointKind::Receipt
                | EndpointKind::Commit
                | EndpointKind::Document
                | EndpointKind::Decision
        ),
        RelationType::Corroborates => target_kind == EndpointKind::Learning,
        RelationType::Contradicts => matches!(
            target_kind,
            EndpointKind::Learning | EndpointKind::Document | EndpointKind::Decision
        ),
        RelationType::SupersededBy => target_kind == EndpointKind::Learning,
        RelationType::PromotedTo => {
            matches!(target_kind, EndpointKind::Document | EndpointKind::Decision)
        }
        RelationType::ImplementedBy | RelationType::AppliedTo => target_kind == EndpointKind::Work,
        RelationType::CausedBy => target_kind == EndpointKind::Learning,
    };
    if !ok {
        return Err(PulseError::validation(
            "knowledge_relation_direction_invalid",
            format!("relation {relation_type:?} cannot target {target_kind:?}"),
        ));
    }
    Ok(())
}

pub fn validate_filename_safe_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.contains("..")
        || value.chars().any(|c| c.is_control())
    {
        return Err(PulseError::validation(
            "knowledge_relation_id_invalid",
            format!("endpoint id is not portable filename-safe: {value:?}"),
        ));
    }
    let reserved = ["con", "prn", "aux", "nul", "com1", "com2", "lpt1", "lpt2"];
    if reserved.iter().any(|r| value.eq_ignore_ascii_case(r)) {
        return Err(PulseError::validation(
            "knowledge_relation_id_invalid",
            format!("endpoint id is reserved: {value}"),
        ));
    }
    Ok(())
}
