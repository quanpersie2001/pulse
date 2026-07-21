pub mod atomic;
pub mod lock;
pub mod paths;
pub mod transaction;

use crate::canonical_json;
use crate::error::{PulseError, Result};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

pub const MANIFEST_JSON: &str = r#"{
  "schema_version": 1,
  "graph_id": "pulse-main",
  "node_schema": "schemas/node.schema.json",
  "edge_schema": "schemas/edge.schema.json",
  "content_root": "../../works",
  "id_pattern": "^(EP|ST|TK|DEC)-[0-9]{3,}$"
}
"#;

pub const NODE_SCHEMA_JSON: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Pulse work graph node",
  "type": "object",
  "required": [
    "schema_version",
    "id",
    "kind",
    "revision",
    "title",
    "status",
    "content_dir",
    "created_at",
    "updated_at"
  ],
  "properties": {
    "schema_version": { "const": 1 },
    "id": { "type": "string", "pattern": "^(EP|ST|TK|DEC)-[0-9]{3,}$" },
    "kind": { "enum": ["epic", "story", "ticket", "decision"] },
    "revision": { "type": "integer", "minimum": 1 },
    "title": { "type": "string", "minLength": 1 },
    "status": {
      "enum": [
        "draft",
        "shaped",
        "ready",
        "active",
        "verifying",
        "done",
        "rework",
        "blocked",
        "superseded"
      ]
    },
    "content_dir": { "type": "string" },
    "created_at": { "type": "string", "format": "date-time" },
    "updated_at": { "type": "string", "format": "date-time" }
  },
  "additionalProperties": true
}
"#;

pub const EDGE_SCHEMA_JSON: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Pulse work graph edge",
  "type": "object",
  "required": [
    "schema_version",
    "id",
    "type",
    "from",
    "to",
    "revision",
    "created_at",
    "created_by"
  ],
  "properties": {
    "schema_version": { "const": 1 },
    "id": { "type": "string" },
    "type": {
      "enum": [
        "parent",
        "blocked_by",
        "preferred_after",
        "superseded_by",
        "related",
        "duplicates"
      ]
    },
    "from": { "type": "string", "pattern": "^(EP|ST|TK|DEC)-[0-9]{3,}$" },
    "to": { "type": "string", "pattern": "^(EP|ST|TK|DEC)-[0-9]{3,}$" },
    "revision": { "type": "integer", "minimum": 1 },
    "created_at": { "type": "string", "format": "date-time" },
    "created_by": { "type": "string", "minLength": 1 }
  },
  "additionalProperties": true
}
"#;

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
