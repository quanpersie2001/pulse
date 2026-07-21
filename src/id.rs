use serde::{Deserialize, Serialize};

use crate::{PulseError, PulseResult};

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
}

pub fn kind_for_id(id: &str) -> PulseResult<WorkKind> {
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

pub fn validate_id_for_kind(id: &str, kind: WorkKind) -> PulseResult<()> {
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
