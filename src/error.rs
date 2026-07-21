use std::path::PathBuf;
use std::time::Duration;

pub type Result<T> = std::result::Result<T, PulseError>;

#[derive(Debug, thiserror::Error)]
pub enum PulseError {
    #[error("io error at {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

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
    LockTimeout { lock_path: PathBuf, timeout: Duration },

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

    #[error("validation failed: {message}")]
    Validation { message: String },
}

impl PulseError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
