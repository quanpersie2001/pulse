use crate::error::{PulseError, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkKind {
    Epic,
    Story,
    Ticket,
    Decision,
}

impl WorkKind {
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Epic => "EP",
            Self::Story => "ST",
            Self::Ticket => "TK",
            Self::Decision => "DEC",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Epic => "epic",
            Self::Story => "story",
            Self::Ticket => "ticket",
            Self::Decision => "decision",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkId(String);

impl WorkId {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_work_id(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn kind(&self) -> Result<WorkKind> {
        let prefix = self
            .0
            .split_once('-')
            .map(|(prefix, _)| prefix)
            .ok_or_else(|| PulseError::Validation {
                message: format!("invalid work id {}", self.0),
            })?;
        match prefix {
            "EP" => Ok(WorkKind::Epic),
            "ST" => Ok(WorkKind::Story),
            "TK" => Ok(WorkKind::Ticket),
            "DEC" => Ok(WorkKind::Decision),
            _ => Err(PulseError::Validation {
                message: format!("unknown work id prefix {prefix}"),
            }),
        }
    }
}

impl fmt::Display for WorkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for WorkId {
    type Err = PulseError;

    fn from_str(value: &str) -> Result<Self> {
        Self::new(value)
    }
}

pub fn validate_work_id(value: &str) -> Result<()> {
    let Some((prefix, digits)) = value.split_once('-') else {
        return Err(PulseError::Validation {
            message: format!("work id must contain a prefix and number: {value}"),
        });
    };
    if !matches!(prefix, "EP" | "ST" | "TK" | "DEC") {
        return Err(PulseError::Validation {
            message: format!("unsupported work id prefix: {prefix}"),
        });
    }
    if digits.len() < 3 || !digits.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(PulseError::Validation {
            message: format!("work id number must have at least three digits: {value}"),
        });
    }
    Ok(())
}

pub fn new_transaction_id() -> String {
    format!("txn_{}", ulid::Ulid::new())
}

pub fn new_event_id() -> String {
    format!("evt_{}", ulid::Ulid::new())
}

pub fn edge_id(edge_type: &str, from: &WorkId, to: &WorkId) -> String {
    let slug = edge_type.replace('_', "-");
    if edge_type == "related" && to < from {
        format!("{slug}--{}--{}", to.as_str(), from.as_str())
    } else {
        format!("{slug}--{}--{}", from.as_str(), to.as_str())
    }
}
