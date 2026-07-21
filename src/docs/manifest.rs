use std::fs;
use std::path::{Path, PathBuf};

use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::docs::model::DocsRegistryEnvelope;
use crate::docs::validate::validate_registry;
use crate::evidence::manifest as evidence_manifest;
use crate::storage::transaction::recover_prepared_transactions;
use crate::storage::WriteGuard;
use crate::{PulseError, Result};

pub const DOCUMENT_SCHEMA: &str = include_str!("../schema/docs/document.schema.json");

#[derive(Debug, Clone)]
pub struct DocsBootstrapOutcome {
    pub schema_version: u32,
    pub created: Vec<PathBuf>,
    pub preserved: Vec<PathBuf>,
    pub registry: DocsRegistryEnvelope,
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
        schema_version: 1,
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
    validate_schema(repo_root)?;
    load_existing_registry(repo_root, &evidence.repository_id)
}

pub(crate) fn load_existing_registry(
    repo_root: &Path,
    repository_id: &str,
) -> Result<DocsRegistryEnvelope> {
    validate_schema(repo_root)?;
    let registry_path = repo_root.join(".pulse/docs/registry.json");
    let registry: DocsRegistryEnvelope = crate::storage::read_json(&registry_path)?;
    if registry.repository_id != repository_id {
        return Err(PulseError::validation(
            "docs_registry_schema_invalid",
            "docs registry repository_id does not match evidence manifest",
        ));
    }
    validate_registry(repo_root, repository_id, &registry)?.into_result()?;
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
        let existing = fs::read(path).map_err(|error| PulseError::io(path, error))?;
        if hash_bytes(&existing) != hash_bytes(&bytes) {
            return Err(PulseError::validation(
                "docs_registry_schema_invalid",
                format!("schema drift at {}", path.display()),
            ));
        }
    } else {
        crate::storage::create_new(path, &bytes)?;
        created.push(path.to_path_buf());
    }
    Ok(())
}

fn validate_schema(repo_root: &Path) -> Result<()> {
    let path = repo_root.join(".pulse/docs/schemas/document.schema.json");
    if !path.exists() {
        return Err(PulseError::validation(
            "docs_registry_schema_invalid",
            format!("missing document schema at {}", path.display()),
        ));
    }
    let expected_value: serde_json::Value = serde_json::from_str(DOCUMENT_SCHEMA)?;
    let expected_bytes = to_canonical_bytes(&expected_value)?;
    let actual = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
    if hash_bytes(&actual) != hash_bytes(&expected_bytes) {
        return Err(PulseError::validation(
            "docs_registry_schema_invalid",
            format!("schema drift at {}", path.display()),
        ));
    }
    Ok(())
}
