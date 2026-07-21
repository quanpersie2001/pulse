use std::path::PathBuf;

use thiserror::Error;

pub type PulseResult<T> = Result<T, PulseError>;

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
    #[error("{message}")]
    Validation {
        code: &'static str,
        message: String,
    },
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
