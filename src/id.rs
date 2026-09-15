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

/// Plan 0022 §4.2: `<PREFIX>-<4 hex>`, the 16 leading bits of
/// `sha256(kind + title + created_at + 8 random bytes)`. Distinct from the
/// numeric `format_id` above, which the v2 workgraph still uses until it is
/// deleted (plan 0022 P1.3); callers of the new `issues.jsonl` store use this
/// one exclusively.
pub fn generate_hash_id(kind: WorkKind, title: &str, created_at: &str) -> WorkId {
    loop {
        let candidate = hash_id_once(kind, title, created_at);
        if validate_hash_id_for_kind(&candidate, kind).is_ok() {
            return WorkId(candidate);
        }
        // A generated suffix could theoretically miss the hex-lowercase
        // shape only if the RNG or formatting changed; retry rather than
        // panic so callers keep a total, non-panicking constructor.
    }
}

fn hash_id_once(kind: WorkKind, title: &str, created_at: &str) -> String {
    use rand::RngCore;
    use sha2::{Digest, Sha256};

    let mut random = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut random);
    let mut hasher = Sha256::new();
    hasher.update(kind.as_str().as_bytes());
    hasher.update(title.as_bytes());
    hasher.update(created_at.as_bytes());
    hasher.update(random);
    let digest = hasher.finalize();
    format!(
        "{}-{:04x}",
        kind.prefix(),
        u16::from_be_bytes([digest[0], digest[1]])
    )
}

/// Same scheme as [`generate_hash_id`], for the `LRN-<4 hex>` learning id
/// namespace (plan 0022 §4.2), which sits outside `WorkKind`.
pub fn generate_learning_hash_id(title: &str, created_at: &str) -> String {
    loop {
        let mut random = [0_u8; 8];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut random);
        let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
        sha2::Digest::update(&mut hasher, b"learning");
        sha2::Digest::update(&mut hasher, title.as_bytes());
        sha2::Digest::update(&mut hasher, created_at.as_bytes());
        sha2::Digest::update(&mut hasher, random);
        let digest = sha2::Digest::finalize(hasher);
        let candidate = format!("LRN-{:04x}", u16::from_be_bytes([digest[0], digest[1]]));
        if candidate.len() == 8 {
            return candidate;
        }
    }
}

/// Validates the plan 0022 §4.2 hash-id shape: `<prefix>-<4 lowercase hex>`.
pub fn validate_hash_id_for_kind(id: &str, kind: WorkKind) -> Result<()> {
    let expected = kind.prefix();
    let Some(suffix) = id
        .strip_prefix(expected)
        .and_then(|rest| rest.strip_prefix('-'))
    else {
        return Err(PulseError::validation(
            "invalid_id",
            format!("id {id} does not match kind {kind:?}"),
        ));
    };
    let is_lowercase_hex = suffix.len() == 4
        && suffix
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
    if !is_lowercase_hex {
        return Err(PulseError::validation(
            "invalid_id",
            format!("id {id} must match {expected}-[0-9a-f]{{4}}"),
        ));
    }
    Ok(())
}

// Compatibility re-exports: event/transaction identity generation now lives
// with its owning module. These aliases preserve the historical
// `pulse::id::{new_event_id, new_transaction_id}` path.
pub use crate::event::new_event_id;
pub use crate::storage::transaction::new_transaction_id;

#[cfg(test)]
mod hash_id_tests {
    use super::*;

    #[test]
    fn generated_hash_id_has_the_prefix_and_four_lowercase_hex_digits() {
        for kind in [
            WorkKind::Epic,
            WorkKind::Story,
            WorkKind::Ticket,
            WorkKind::Decision,
        ] {
            let id = generate_hash_id(kind, "some title", "2026-09-16T00:00:00Z");
            assert!(id.as_str().starts_with(kind.prefix()));
            assert!(validate_hash_id_for_kind(id.as_str(), kind).is_ok());
            assert_eq!(id.as_str().len(), kind.prefix().len() + 5);
        }
    }

    #[test]
    fn same_inputs_collide_only_because_of_the_random_suffix() {
        let a = generate_hash_id(WorkKind::Ticket, "same title", "2026-09-16T00:00:00Z");
        let b = generate_hash_id(WorkKind::Ticket, "same title", "2026-09-16T00:00:00Z");
        // Not asserting inequality (a collision is legal and rare); asserting
        // both independently validate is the actual contract.
        assert!(validate_hash_id_for_kind(a.as_str(), WorkKind::Ticket).is_ok());
        assert!(validate_hash_id_for_kind(b.as_str(), WorkKind::Ticket).is_ok());
    }

    #[test]
    fn wrong_kind_prefix_is_rejected() {
        let id = generate_hash_id(WorkKind::Ticket, "t", "2026-09-16T00:00:00Z");
        let err = validate_hash_id_for_kind(id.as_str(), WorkKind::Story).unwrap_err();
        assert_eq!(err.code(), "invalid_id");
    }

    #[test]
    fn learning_hash_id_has_the_lrn_prefix_and_four_hex_digits() {
        let id = generate_learning_hash_id("some learning", "2026-09-16T00:00:00Z");
        assert!(id.starts_with("LRN-"));
        assert_eq!(id.len(), 8);
        assert!(id[4..].chars().all(|c| c.is_ascii_hexdigit()));
    }
}
