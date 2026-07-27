//! Generic storage primitives.
//!
//! This module owns only atomic writes, locking, path validation and transaction
//! primitives. Workgraph bootstrap and embedded schema templates are owned by
//! the graph repository (`crate::graph::store::bootstrap`) and re-exported here
//! for compatibility with the historical `pulse::storage::{bootstrap,
//! MANIFEST_JSON, ...}` path used by evidence, docs, knowledge and tests.

pub mod atomic;
pub mod lock;
pub mod paths;
pub mod transaction;

use crate::error::{PulseError, Result};
use serde::de::DeserializeOwned;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

pub use lock::WriteGuard;

// Compatibility re-exports: workgraph bootstrap/schema ownership moved to the
// graph repository. These aliases preserve the public `pulse::storage::*` paths.
// Re-export through the `graph::store` facade (the bootstrap submodule itself is
// private to the graph store).
pub use crate::graph::store::{
    bootstrap, default_manifest_value, BootstrapOutcome, EDGE_SCHEMA_JSON, MANIFEST_JSON,
    NODE_SCHEMA_JSON,
};

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    atomic::atomic_replace(path, bytes).map(|_| ())
}

pub fn create_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        PulseError::validation(
            "invalid_path",
            format!("path has no parent: {}", path.display()),
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| PulseError::io(path, error))?;
    file.write_all(bytes)
        .map_err(|error| PulseError::io(path, error))?;
    file.sync_all()
        .map_err(|error| PulseError::io(path, error))?;
    Ok(())
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).map_err(|error| PulseError::io(path, error))?;
    serde_json::from_slice(&bytes).map_err(|error| PulseError::json(path, error))
}

pub fn safe_repo_relative(path: &str) -> Result<PathBuf> {
    let path_ref = Path::new(path);
    crate::storage::paths::validate_relative_path(path_ref)?;
    let mut out = PathBuf::new();
    for component in path_ref.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => {
                return Err(PulseError::PathTraversal {
                    path: path_ref.to_path_buf(),
                });
            }
        }
    }
    Ok(out)
}
