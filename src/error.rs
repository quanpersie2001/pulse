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

    #[error("repository write lock timed out after {timeout:?}: {lock_path:?}")]
    LockTimeout {
        lock_path: PathBuf,
        timeout: Duration,
    },

    #[error("validation failed: {message}")]
    Validation { code: &'static str, message: String },

    /// v3 kernel/store error: every code here MUST carry a `hint` (plan 0022
    /// §6 — "hint là bắt buộc cho mọi mã lỗi"). The v2 variants that used to
    /// share this enum (transactions, CAS, content roots, failpoints) were
    /// deleted with their machinery at the error-code audit (P3.4); new v3
    /// code always constructs errors through [`PulseError::kernel`].
    #[error("{message}")]
    Kernel {
        code: &'static str,
        message: String,
        hint: &'static str,
    },
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
            Self::LockTimeout { .. } => "lock_timeout",
            Self::Validation { code, .. } => code,
            Self::Kernel { code, .. } => code,
        }
    }

    /// Operator-facing "how to fix this" text. Only [`Self::Kernel`] carries
    /// one today; every new v3 error code goes through [`PulseError::kernel`], so
    /// this is never `None` for a code introduced after plan 0022.
    pub fn hint(&self) -> Option<&'static str> {
        match self {
            Self::Kernel { hint, .. } => Some(hint),
            _ => None,
        }
    }

    pub fn validation(code: &'static str, message: impl Into<String>) -> Self {
        Self::Validation {
            code,
            message: message.into(),
        }
    }

    pub fn kernel(code: &'static str, message: impl Into<String>, hint: &'static str) -> Self {
        Self::Kernel {
            code,
            message: message.into(),
            hint,
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
