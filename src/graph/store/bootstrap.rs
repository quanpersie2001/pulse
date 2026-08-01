//! Workgraph repository bootstrap and embedded schema templates.
//!
//! This is graph-owned infrastructure: the workgraph directory layout, manifest
//! template and embedded node/edge schema templates belong with the graph
//! repository, not with generic storage primitives. Generic storage
//! (`crate::storage`) retains only atomic/lock/path/transaction primitives and
//! re-exports these symbols for compatibility with the historical
//! `pulse::storage::{bootstrap, MANIFEST_JSON, ...}` path used by evidence,
//! docs, knowledge and integration tests.
//!
//! Behavior and persisted bytes are unchanged: this is pure ownership
//! relocation.

use std::fs;
use std::path::{Path, PathBuf};

use crate::canonical_json;
use crate::error::{PulseError, Result};
use crate::graph::manifest::{EDGE_SCHEMA, NODE_SCHEMA};

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
    let repo_root = crate::storage::paths::canonicalize_existing_dir(repo_root)?;
    ensure_bootstrap_state(&repo_root)?;
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

/// Fail closed before any compatibility bootstrap entrypoint writes bytes.
///
/// The store implementation and the historical `storage::bootstrap` path both
/// use this graph-owned classification. Safe partial initialization is allowed;
/// unknown, drifted, or stateful partial layouts are never repaired implicitly.
pub(crate) fn ensure_bootstrap_state(repo_root: &Path) -> Result<()> {
    use super::repository::{classify_workgraph_bootstrap_state, WorkgraphBootstrapState};

    match classify_workgraph_bootstrap_state(&repo_root.join(".pulse/workgraph"))? {
        WorkgraphBootstrapState::Empty | WorkgraphBootstrapState::SafePartialCurrent => Ok(()),
        WorkgraphBootstrapState::ExistingCurrent => Ok(()),
        WorkgraphBootstrapState::MissingNodeSchemaWithState => Err(PulseError::validation(
            "node_schema_missing_refused",
            "node schema is missing while existing workgraph state is present; refusing bootstrap without overwrite",
        )),
        WorkgraphBootstrapState::NodeSchemaDrift { hash } => Err(PulseError::validation(
            "node_schema_drift_refused",
            format!("refusing to overwrite node schema drift {hash}; resolve schema state explicitly"),
        )),
        WorkgraphBootstrapState::UnexpectedPartialState => Err(PulseError::validation(
            "workgraph_partial_state_refused",
            "workgraph contains partial state that is not a safe current baseline initialization; refusing bootstrap without overwrite",
        )),
    }
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
    serde_json::json!({
      "schema_version": 1,
      "graph_id": "pulse-main",
      "node_schema": "schemas/node.schema.json",
      "edge_schema": "schemas/edge.schema.json",
      "content_root": "../../works",
      "id_pattern": "^(EP|ST|TK|DEC)-[0-9]{3,}$"
    })
}
