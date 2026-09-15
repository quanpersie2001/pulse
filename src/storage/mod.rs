//! Generic storage primitives.
//!
//! This module owns only atomic writes, locking, path validation and
//! transaction primitives. It does not depend on any higher domain (`store`,
//! `kernel`, `evidence`, ...).

pub mod append;
pub mod atomic;
pub mod lock;
pub mod paths;
pub mod transaction;

use crate::error::{PulseError, Result};
use serde::de::DeserializeOwned;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub use append::append_line_fsync;
pub use lock::WriteGuard;

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    atomic::atomic_replace(path, bytes).map(|_| ())
}

pub fn atomic_write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    atomic::atomic_replace_private(path, bytes).map(|_| ())
}

pub fn create_new(path: &Path, bytes: &[u8]) -> Result<()> {
    atomic::atomic_create_new(path, bytes).map(|_| ())
}

pub fn create_new_private(path: &Path, bytes: &[u8]) -> Result<()> {
    atomic::atomic_create_new_private(path, bytes).map(|_| ())
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
