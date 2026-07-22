use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde_json::json;

use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::docs::model::RetrievalConfig;
use crate::docs::model::{DocsRegistryEnvelope, DOCS_REGISTRY_SCHEMA_VERSION_V2};
use crate::docs::validate::validate_registry;
use crate::event::{event_path, EventEnvelope};
use crate::evidence::manifest as evidence_manifest;
use crate::id::new_event_id;
use crate::storage::transaction::{
    commit_prepared_multi_target_transaction, prepare_multi_target_transaction,
    recover_prepared_transactions, FileState, MultiTargetTransactionIntent, TransactionTarget,
};
use crate::storage::WriteGuard;
use crate::{PulseError, Result};

/// Current (v2) document registry JSON schema. Embedded so drift is detectable.
pub const DOCUMENT_SCHEMA: &str = include_str!("../schema/docs/document.schema.json");

/// Exact known predecessor: the Slice 4 (v1) document registry JSON schema.
/// A registry whose on-disk schema canonical-hash matches this predecessor is
/// migrated deliberately to v2. It is never silently reinterpreted as current.
/// Raw formatting does not matter: migration compares canonical hashes.
const DOCUMENT_SCHEMA_V1_PREDECESSOR: &str = r##"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://pulse.dev/schema/docs/document.schema.json",
  "title": "Pulse Document Registry",
  "type": "object",
  "additionalProperties": false,
  "required": ["schema_version", "revision", "repository_id", "documents"],
  "properties": {
    "schema_version": { "const": 1 },
    "revision": { "type": "integer", "minimum": 1 },
    "repository_id": { "type": "string", "pattern": "^repo_" },
    "documents": {
      "type": "array",
      "items": { "$ref": "#/$defs/document" }
    }
  },
  "$defs": {
    "document": {
      "type": "object",
      "additionalProperties": false,
      "required": [
        "id", "revision", "path", "kind", "authority", "lifecycle", "owner",
        "summary", "aliases", "scope", "review_policy", "verification_profile",
        "generated", "superseded_by"
      ],
      "properties": {
        "id": { "type": "string", "pattern": "^DOC-[A-Z0-9][A-Z0-9-]{2,63}$" },
        "revision": { "type": "integer", "minimum": 1 },
        "path": { "type": "string", "minLength": 1 },
        "kind": { "enum": ["repository_map", "policy", "product", "architecture", "domain", "operations", "reference", "decision_projection", "generated", "informational"] },
        "authority": { "enum": ["draft", "approved", "informational", "generated"] },
        "lifecycle": { "enum": ["current", "suspected_stale", "stale", "retired", "superseded"] },
        "owner": { "type": "string" },
        "summary": { "type": "string", "minLength": 1, "maxLength": 500 },
        "aliases": { "type": "array", "maxItems": 32, "items": { "type": "string", "minLength": 1, "maxLength": 120 } },
        "scope": { "$ref": "#/$defs/scope" },
        "review_policy": { "enum": ["none", "light", "standard", "independent", "human"] },
        "verification_profile": { "type": "string" },
        "generated": { "anyOf": [{ "type": "null" }, { "$ref": "#/$defs/generated" }] },
        "superseded_by": { "anyOf": [{ "type": "null" }, { "type": "string" }] }
      }
    },
    "scope": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "paths": { "type": "array", "items": { "type": "string" } },
        "domains": { "type": "array", "items": { "type": "string" } },
        "work_labels": { "type": "array", "items": { "type": "string" } }
      }
    },
    "generated": {
      "type": "object",
      "additionalProperties": false,
      "required": ["sources", "command", "outputs", "editable", "freshness_check"],
      "properties": {
        "sources": { "type": "array", "items": { "type": "string" }, "minItems": 1 },
        "command": { "type": "string", "minLength": 1 },
        "outputs": { "type": "array", "items": { "type": "string" }, "minItems": 1 },
        "editable": { "type": "boolean" },
        "freshness_check": { "type": "string", "minLength": 1 }
      }
    }
  }
}
"##;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaVersion {
    /// On-disk schema matches the embedded current v2 schema.
    Current,
    /// On-disk schema matches the exact known v1 predecessor and must migrate.
    KnownPredecessor,
}

#[derive(Debug, Clone)]
pub struct DocsBootstrapOutcome {
    pub schema_version: u32,
    pub created: Vec<PathBuf>,
    pub preserved: Vec<PathBuf>,
    pub registry: DocsRegistryEnvelope,
}

/// Outcome of a deliberate registry schema migration (v1 -> v2).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DocsMigrationOutcome {
    pub schema_version: u32,
    pub code: String,
    pub status: MigrationStatus,
    pub registry_revision_before: u64,
    pub registry_revision_after: u64,
    pub schema_hash_before: String,
    pub schema_hash_after: String,
    pub registry: DocsRegistryEnvelope,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MigrationStatus {
    /// Migration ran and bumped schema + registry revision exactly once.
    Migrated,
    /// Registry was already v2; nothing to do (idempotent retry).
    AlreadyCurrent,
}

pub fn bootstrap(repo_root: &Path) -> Result<DocsBootstrapOutcome> {
    let _guard = WriteGuard::acquire(repo_root)?;
    bootstrap_unlocked(repo_root)
}

pub(crate) fn bootstrap_unlocked(repo_root: &Path) -> Result<DocsBootstrapOutcome> {
    let evidence = evidence_manifest::load(repo_root)?;
    recover_prepared_transactions(repo_root)?;

    let docs = repo_root.join(".pulse/docs");
    let schemas = docs.join("schemas");
    let mut created = Vec::new();
    let mut preserved = Vec::new();

    create_dir_if_missing(&docs, &mut created, &mut preserved)?;
    create_dir_if_missing(&schemas, &mut created, &mut preserved)?;
    write_schema_if_absent(
        &schemas.join("document.schema.json"),
        DOCUMENT_SCHEMA,
        &mut created,
        &mut preserved,
    )?;
    // Classify the schema now so that an unknown/modified schema file is
    // rejected even when there is no registry yet. A v1 predecessor is allowed
    // here and migrated on load.
    let _ = schema_version(repo_root)?;

    let registry_path = docs.join("registry.json");
    let registry = if registry_path.exists() {
        preserved.push(registry_path.clone());
        load_existing_registry(repo_root, &evidence.repository_id)?
    } else {
        let registry = DocsRegistryEnvelope::empty(evidence.repository_id.clone());
        let bytes = to_canonical_bytes(&registry)?;
        crate::storage::create_new(&registry_path, &bytes)?;
        created.push(registry_path);
        registry
    };

    Ok(DocsBootstrapOutcome {
        schema_version: DOCS_REGISTRY_SCHEMA_VERSION_V2,
        created,
        preserved,
        registry,
    })
}

pub fn load(repo_root: &Path) -> Result<DocsRegistryEnvelope> {
    let _guard = WriteGuard::acquire(repo_root)?;
    load_unlocked(repo_root)
}

pub(crate) fn load_unlocked(repo_root: &Path) -> Result<DocsRegistryEnvelope> {
    let registry_path = repo_root.join(".pulse/docs/registry.json");
    if !registry_path.exists() {
        return Ok(bootstrap_unlocked(repo_root)?.registry);
    }
    let evidence = evidence_manifest::load(repo_root)?;
    recover_prepared_transactions(repo_root)?;
    load_existing_registry(repo_root, &evidence.repository_id)
}

pub(crate) fn load_existing_registry(
    repo_root: &Path,
    repository_id: &str,
) -> Result<DocsRegistryEnvelope> {
    // Schema evolution gate: only the current v2 schema or the exact known v1
    // predecessor may load. Unknown/modified predecessors are rejected and the
    // canonical files are preserved untouched.
    match schema_version(repo_root)? {
        SchemaVersion::Current => {}
        SchemaVersion::KnownPredecessor => {
            migrate_registry_unlocked(repo_root, repository_id)?;
        }
    }
    let registry_path = repo_root.join(".pulse/docs/registry.json");
    let registry: DocsRegistryEnvelope = crate::storage::read_json(&registry_path)?;
    if registry.repository_id != repository_id {
        return Err(PulseError::validation(
            "docs_registry_schema_invalid",
            "docs registry repository_id does not match evidence manifest",
        ));
    }
    if registry.schema_version != DOCS_REGISTRY_SCHEMA_VERSION_V2 {
        return Err(PulseError::validation(
            "docs_registry_schema_invalid",
            "docs registry schema_version must be 2 after migration",
        ));
    }
    validate_registry(repo_root, repository_id, &registry)?.into_result()?;
    Ok(registry)
}

/// Deliberately migrate an exact known v1 predecessor registry to v2. Acquires
/// the repository write guard, recovers pending transactions, adds retrieval
/// defaults without changing document identities/revisions, bumps the registry
/// schema version + revision exactly once via a recoverable multi-target
/// transaction, and emits one immutable `docs.registry.schema_migrated` event.
/// Idempotent: an already-v2 registry reports `already_current` unchanged.
pub fn migrate_registry(repo_root: &Path, repository_id: &str) -> Result<DocsMigrationOutcome> {
    let _guard = WriteGuard::acquire(repo_root)?;
    migrate_registry_unlocked(repo_root, repository_id)
}

pub(crate) fn migrate_registry_unlocked(
    repo_root: &Path,
    repository_id: &str,
) -> Result<DocsMigrationOutcome> {
    recover_prepared_transactions(repo_root)?;
    let schema_path = repo_root.join(".pulse/docs/schemas/document.schema.json");
    if !schema_path.exists() {
        return Err(PulseError::validation(
            "docs_registry_schema_invalid",
            "missing document schema; cannot evaluate migration predecessor",
        ));
    }
    let on_disk_schema_hash = schema_file_hash(repo_root)?;

    // Idempotency / current check.
    if on_disk_schema_hash == current_schema_hash()? {
        let registry = read_registry_only(repo_root, repository_id)?;
        return Ok(DocsMigrationOutcome {
            schema_version: DOCS_REGISTRY_SCHEMA_VERSION_V2,
            code: "already_current".to_string(),
            status: MigrationStatus::AlreadyCurrent,
            registry_revision_before: registry.revision,
            registry_revision_after: registry.revision,
            schema_hash_before: on_disk_schema_hash.clone(),
            schema_hash_after: on_disk_schema_hash,
            registry,
        });
    }

    // Only the exact known v1 predecessor may migrate. Anything else is drift.
    let v1_hash = predecessor_schema_hash()?;
    if on_disk_schema_hash != v1_hash {
        return Err(PulseError::validation(
            "docs_registry_schema_invalid",
            "unknown or modified registry schema predecessor; refusing migration",
        ));
    }

    let registry_path = repo_root.join(".pulse/docs/registry.json");
    let before_registry: DocsRegistryEnvelope = crate::storage::read_json(&registry_path)?;
    if before_registry.repository_id != repository_id {
        return Err(PulseError::validation(
            "docs_registry_schema_invalid",
            "docs registry repository_id does not match evidence manifest",
        ));
    }
    if before_registry.schema_version != 1 {
        return Err(PulseError::validation(
            "docs_registry_schema_invalid",
            "migration predecessor must be schema_version 1",
        ));
    }

    let registry_revision_before = before_registry.revision;
    let before_registry_bytes = to_canonical_bytes(&before_registry)?;
    let before_registry_hash = hash_bytes(&before_registry_bytes);

    // Build v2 registry: add deterministic retrieval defaults without changing
    // document IDs, document revisions or other semantic metadata.
    let mut after_registry = before_registry.clone();
    after_registry.schema_version = DOCS_REGISTRY_SCHEMA_VERSION_V2;
    if after_registry.retrieval.is_none() {
        after_registry.retrieval = Some(RetrievalConfig::defaults());
    }
    after_registry.revision += 1;
    after_registry.normalize();
    validate_registry(repo_root, repository_id, &after_registry)?.into_result()?;
    let after_registry_bytes = to_canonical_bytes(&after_registry)?;
    let after_registry_hash = hash_bytes(&after_registry_bytes);

    let current_schema_hash = current_schema_hash()?;
    let v2_schema_bytes = current_schema_bytes()?;

    let schema_target = TransactionTarget::new(
        schema_path.clone(),
        FileState::Present {
            hash: v1_hash.clone(),
            revision: 1,
        },
        FileState::Present {
            hash: current_schema_hash.clone(),
            revision: 1,
        },
        &v2_schema_bytes,
    );
    let registry_target = TransactionTarget::new(
        registry_path.clone(),
        FileState::Present {
            hash: before_registry_hash.clone(),
            revision: registry_revision_before,
        },
        FileState::Present {
            hash: after_registry_hash.clone(),
            revision: after_registry.revision,
        },
        &after_registry_bytes,
    );

    let event = EventEnvelope::new(
        new_event_id(),
        "docs.registry.schema_migrated",
        "system:docs".to_string(),
        repository_id.to_string(),
        json!({
            "repository_id": repository_id,
            "schema_version_before": 1,
            "schema_version_after": DOCS_REGISTRY_SCHEMA_VERSION_V2,
            "schema_hash_before": v1_hash,
            "schema_hash_after": current_schema_hash,
            "registry_revision_before": registry_revision_before,
            "registry_revision_after": after_registry.revision,
            "registry_hash_before": before_registry_hash,
            "registry_hash_after": after_registry_hash,
            "document_revisions_preserved": true,
        }),
        Utc::now(),
    );
    let intent = MultiTargetTransactionIntent::prepared(
        event.id.clone(),
        event.event_type.clone(),
        event.actor.clone(),
        vec![schema_target, registry_target],
        event_path(repo_root, &event),
        serde_json::to_value(&event)?,
    )?;
    let prepared = prepare_multi_target_transaction(repo_root, intent)?;
    commit_prepared_multi_target_transaction(&prepared, None)?;

    Ok(DocsMigrationOutcome {
        schema_version: DOCS_REGISTRY_SCHEMA_VERSION_V2,
        code: "schema_migrated".to_string(),
        status: MigrationStatus::Migrated,
        registry_revision_before,
        registry_revision_after: after_registry.revision,
        schema_hash_before: v1_hash,
        schema_hash_after: current_schema_hash,
        registry: after_registry,
    })
}

fn read_registry_only(repo_root: &Path, repository_id: &str) -> Result<DocsRegistryEnvelope> {
    let registry_path = repo_root.join(".pulse/docs/registry.json");
    let registry: DocsRegistryEnvelope = crate::storage::read_json(&registry_path)?;
    if registry.repository_id != repository_id {
        return Err(PulseError::validation(
            "docs_registry_schema_invalid",
            "docs registry repository_id does not match evidence manifest",
        ));
    }
    Ok(registry)
}

fn create_dir_if_missing(
    path: &Path,
    created: &mut Vec<PathBuf>,
    preserved: &mut Vec<PathBuf>,
) -> Result<()> {
    if path.exists() {
        preserved.push(path.to_path_buf());
    } else {
        fs::create_dir_all(path).map_err(|error| PulseError::io(path, error))?;
        created.push(path.to_path_buf());
    }
    Ok(())
}

fn write_schema_if_absent(
    path: &Path,
    schema: &str,
    created: &mut Vec<PathBuf>,
    preserved: &mut Vec<PathBuf>,
) -> Result<()> {
    let value: serde_json::Value = serde_json::from_str(schema)?;
    let bytes = to_canonical_bytes(&value)?;
    if path.exists() {
        preserved.push(path.to_path_buf());
        // An existing schema is validated against the current/predecessor set
        // during load; do not drift-check here so migration can repair a v1 file.
    } else {
        crate::storage::create_new(path, &bytes)?;
        created.push(path.to_path_buf());
    }
    Ok(())
}

/// Classify the on-disk document schema as current (v2) or the known v1
/// predecessor. Any other bytes are drift and produce an error.
pub fn schema_version(repo_root: &Path) -> Result<SchemaVersion> {
    let path = repo_root.join(".pulse/docs/schemas/document.schema.json");
    if !path.exists() {
        return Err(PulseError::validation(
            "docs_registry_schema_invalid",
            format!("missing document schema at {}", path.display()),
        ));
    }
    let actual_hash = schema_file_hash(repo_root)?;
    if actual_hash == current_schema_hash()? {
        Ok(SchemaVersion::Current)
    } else if actual_hash == predecessor_schema_hash()? {
        Ok(SchemaVersion::KnownPredecessor)
    } else {
        Err(PulseError::validation(
            "docs_registry_schema_invalid",
            format!(
                "schema drift at {}: unknown document schema",
                path.display()
            ),
        ))
    }
}

fn schema_file_hash(repo_root: &Path) -> Result<String> {
    let path = repo_root.join(".pulse/docs/schemas/document.schema.json");
    let actual = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
    let value: serde_json::Value =
        serde_json::from_slice(&actual).map_err(|error| PulseError::json(&path, error))?;
    Ok(hash_bytes(&to_canonical_bytes(&value)?))
}

fn current_schema_bytes() -> Result<Vec<u8>> {
    let value: serde_json::Value = serde_json::from_str(DOCUMENT_SCHEMA)?;
    to_canonical_bytes(&value)
}

fn current_schema_hash() -> Result<String> {
    Ok(hash_bytes(&current_schema_bytes()?))
}

fn predecessor_schema_hash() -> Result<String> {
    let value: serde_json::Value = serde_json::from_str(DOCUMENT_SCHEMA_V1_PREDECESSOR)?;
    Ok(hash_bytes(&to_canonical_bytes(&value)?))
}

/// The exact known v1 (Slice 4) predecessor schema text. Exposed for migration
/// test harnesses and diagnostics; production code should use [schema_version]
/// / [migrate_registry].
pub fn predecessor_schema() -> &'static str {
    DOCUMENT_SCHEMA_V1_PREDECESSOR
}
