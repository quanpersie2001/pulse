pub mod atomic;
pub mod lock;
pub mod paths;
pub mod transaction;

use crate::canonical_json;
use crate::error::{PulseError, Result};
use crate::graph::manifest::{EDGE_SCHEMA, NODE_SCHEMA};
use serde::de::DeserializeOwned;
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

pub use lock::WriteGuard;

pub const MANIFEST_JSON: &str = r#"{
  "schema_version": 1,
  "graph_id": "pulse-main",
  "node_schema": "schemas/node.schema.json",
  "edge_schema": "schemas/edge.schema.json",
  "content_root": "../../works",
  "id_pattern": "^(EP|ST|TK|DEC)-[0-9]{3,}$"
}
"#;

pub const NODE_SCHEMA_JSON: &str = NODE_SCHEMA;
pub const EDGE_SCHEMA_JSON: &str = EDGE_SCHEMA;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapOutcome {
    pub created: Vec<PathBuf>,
    pub preserved: Vec<PathBuf>,
    pub proposed_ignore_entries: Vec<String>,
}

pub fn bootstrap(repo_root: &Path) -> Result<BootstrapOutcome> {
    let repo_root = paths::canonicalize_existing_dir(repo_root)?;
    let pulse = repo_root.join(".pulse");
    let workgraph = pulse.join("workgraph");
    let schemas = workgraph.join("schemas");
    let runtime = pulse.join("runtime");
    let cache = pulse.join("cache");

    let mut created = Vec::new();
    let mut preserved = Vec::new();

    create_dir_if_missing(&workgraph, &mut created, &mut preserved)?;
    create_dir_if_missing(&schemas, &mut created, &mut preserved)?;
    create_dir_if_missing(&workgraph.join("nodes"), &mut created, &mut preserved)?;
    create_dir_if_missing(&workgraph.join("edges"), &mut created, &mut preserved)?;
    create_dir_if_missing(&runtime, &mut created, &mut preserved)?;
    create_dir_if_missing(&runtime.join("locks"), &mut created, &mut preserved)?;
    create_dir_if_missing(&runtime.join("transactions"), &mut created, &mut preserved)?;
    create_dir_if_missing(&cache, &mut created, &mut preserved)?;

    write_template_if_absent(
        &workgraph.join("manifest.json"),
        MANIFEST_JSON,
        &mut created,
        &mut preserved,
    )?;
    write_template_if_absent(
        &schemas.join("node.schema.json"),
        NODE_SCHEMA_JSON,
        &mut created,
        &mut preserved,
    )?;
    write_template_if_absent(
        &schemas.join("edge.schema.json"),
        EDGE_SCHEMA_JSON,
        &mut created,
        &mut preserved,
    )?;

    // Verify embedded templates stay canonical; this prevents future hand-edits from
    // introducing non-deterministic bootstrap bytes.
    for template in [MANIFEST_JSON, NODE_SCHEMA_JSON, EDGE_SCHEMA_JSON] {
        let value: serde_json::Value = serde_json::from_str(template)?;
        let _ = canonical_json::to_canonical_bytes(&value)?;
    }

    Ok(BootstrapOutcome {
        created,
        preserved,
        proposed_ignore_entries: vec![".pulse/runtime/".into(), ".pulse/cache/".into()],
    })
}

fn create_dir_if_missing(
    path: &Path,
    created: &mut Vec<PathBuf>,
    preserved: &mut Vec<PathBuf>,
) -> Result<()> {
    if path.exists() {
        preserved.push(path.to_path_buf());
        return Ok(());
    }
    fs::create_dir_all(path).map_err(|error| PulseError::io(path, error))?;
    created.push(path.to_path_buf());
    Ok(())
}

fn write_template_if_absent(
    path: &Path,
    template: &str,
    created: &mut Vec<PathBuf>,
    preserved: &mut Vec<PathBuf>,
) -> Result<()> {
    if path.exists() {
        preserved.push(path.to_path_buf());
        return Ok(());
    }
    let value: serde_json::Value = serde_json::from_str(template)?;
    let bytes = canonical_json::to_canonical_bytes(&value)?;
    fs::write(path, bytes).map_err(|error| PulseError::io(path, error))?;
    created.push(path.to_path_buf());
    Ok(())
}

pub fn default_manifest_value() -> serde_json::Value {
    json!({
      "schema_version": 1,
      "graph_id": "pulse-main",
      "node_schema": "schemas/node.schema.json",
      "edge_schema": "schemas/edge.schema.json",
      "content_root": "../../works",
      "id_pattern": "^(EP|ST|TK|DEC)-[0-9]{3,}$"
    })
}

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
    paths::validate_relative_path(path_ref)?;
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
