use std::path::PathBuf;
use std::time::Duration;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, PulseError>;
pub type PulseResult<T> = std::result::Result<T, PulseError>;

#[derive(Debug, Error)]
pub enum PulseError {
    #[error("io error at {path:?}: {source}")]
    Io {
        code: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("json error at {path:?}: {source}")]
    Json {
        code: &'static str,
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("canonical JSON rejects floating point value at {path}")]
    FloatRejected { path: String },

    #[error("path must be repository-relative: {path:?}")]
    AbsolutePath { path: PathBuf },

    #[error("path escapes repository root: {path:?}")]
    PathEscape { path: PathBuf },

    #[error("path traversal is not allowed: {path:?}")]
    PathTraversal { path: PathBuf },

    #[error("content path must be under works/: {path:?}")]
    ContentRootViolation { path: PathBuf },

    #[error("repository write lock timed out after {timeout:?}: {lock_path:?}")]
    LockTimeout {
        lock_path: PathBuf,
        timeout: Duration,
    },

    #[error("durability support boundary: {message}")]
    DurabilityUnsupported { message: String },

    #[error("transaction recovery is ambiguous for {transaction_id}: {message}")]
    AmbiguousTransaction {
        transaction_id: String,
        message: String,
    },

    #[error("transaction event mismatch for {transaction_id}: {message}")]
    EventMismatch {
        transaction_id: String,
        message: String,
    },

    #[error("invalid transaction intent: {message}")]
    InvalidTransaction { message: String },

    #[error("test failpoint reached: {name}")]
    Failpoint { name: &'static str },

    #[error("validation failed: {message}")]
    Validation { code: &'static str, message: String },

    #[error("CAS conflict for {subject}: expected revision {expected_revision}, current revision {current_revision}")]
    CasConflict {
        subject: String,
        expected_revision: u64,
        current_revision: u64,
    },

    #[error("not found: {subject}")]
    NotFound { subject: String },

    #[error("already exists: {subject}")]
    AlreadyExists { subject: String },
}

impl PulseError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io { code, .. } => code,
            Self::Json { code, .. } => code,
            Self::FloatRejected { .. } => "non_canonical_number",
            Self::AbsolutePath { .. } | Self::PathEscape { .. } | Self::PathTraversal { .. } => {
                "unsafe_path"
            }
            Self::ContentRootViolation { .. } => "content_root_violation",
            Self::LockTimeout { .. } => "lock_timeout",
            Self::DurabilityUnsupported { .. } => "durability_unsupported",
            Self::AmbiguousTransaction { .. } => "ambiguous_transaction",
            Self::EventMismatch { .. } => "event_mismatch",
            Self::InvalidTransaction { .. } => "invalid_transaction",
            Self::Failpoint { .. } => "failpoint",
            Self::Validation { code, .. } => code,
            Self::CasConflict { .. } => "cas_conflict",
            Self::NotFound { .. } => "not_found",
            Self::AlreadyExists { .. } => "already_exists",
        }
    }

    pub fn validation(code: &'static str, message: impl Into<String>) -> Self {
        Self::Validation {
            code,
            message: message.into(),
        }
    }

    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            code: "io_error",
            path: path.into(),
            source,
        }
    }

    pub fn json(path: impl Into<PathBuf>, source: serde_json::Error) -> Self {
        Self::Json {
            code: "json_error",
            path: path.into(),
            source,
        }
    }
}

impl From<serde_json::Error> for PulseError {
    fn from(source: serde_json::Error) -> Self {
        Self::Json {
            code: "json_error",
            path: PathBuf::from("<memory>"),
            source,
        }
    }
}
