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
        kind_for_id(&self.0)
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

pub fn kind_for_id(id: &str) -> Result<WorkKind> {
    if id.starts_with("EP-") {
        Ok(WorkKind::Epic)
    } else if id.starts_with("ST-") {
        Ok(WorkKind::Story)
    } else if id.starts_with("TK-") {
        Ok(WorkKind::Ticket)
    } else if id.starts_with("DEC-") {
        Ok(WorkKind::Decision)
    } else {
        Err(PulseError::validation(
            "invalid_id",
            format!("id does not have a supported prefix: {id}"),
        ))
    }
}

pub fn validate_work_id(value: &str) -> Result<()> {
    let kind = kind_for_id(value)?;
    validate_id_for_kind(value, kind)
}

pub fn validate_id_for_kind(id: &str, kind: WorkKind) -> Result<()> {
    let expected = kind.prefix();
    if !id.starts_with(&format!("{expected}-")) {
        return Err(PulseError::validation(
            "id_kind_mismatch",
            format!("id {id} does not match kind {kind:?}"),
        ));
    }
    let suffix = &id[expected.len() + 1..];
    if suffix.len() < 3 || !suffix.chars().all(|c| c.is_ascii_digit()) {
        return Err(PulseError::validation(
            "invalid_id",
            format!("id {id} must match {expected}-[0-9]{{3,}}"),
        ));
    }
    Ok(())
}

pub fn format_id(kind: WorkKind, numeric: u64) -> String {
    format!("{}-{numeric:03}", kind.prefix())
}

pub fn parse_numeric(id: &str, prefix: &str) -> Option<u64> {
    id.strip_prefix(&format!("{prefix}-"))?.parse().ok()
}

// Compatibility re-exports: event/transaction identity generation now lives
// with its owning module. These aliases preserve the historical
// `pulse::id::{new_event_id, new_transaction_id}` path.
pub use crate::event::new_event_id;
pub use crate::storage::transaction::new_transaction_id;
