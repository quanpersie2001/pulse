use std::fs;
use std::path::{Path, PathBuf};

use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::docs::model::{DocsRegistryEnvelope, DOCS_REGISTRY_SCHEMA_VERSION};
use crate::docs::validate::validate_registry;
use crate::evidence::manifest as evidence_manifest;
use crate::storage::transaction::recover_prepared_transactions;
use crate::storage::WriteGuard;
use crate::{PulseError, Result};

/// Current document registry JSON schema. Embedded so drift is detectable.
pub const DOCUMENT_SCHEMA: &str = include_str!("../schema/docs/document.schema.json");

/// Derived section-record JSON schema for docs-search `sections.jsonl` lines.
/// This documents the disposable cache contract; canonical prose remains the
/// registered Markdown file and registry metadata.
pub const DOCS_SECTION_SCHEMA: &str = include_str!("../schema/docs/docs-section.schema.json");

/// Immutable docs-search generation `state.json` schema. This is cache
/// validation/publication metadata, not a canonical documentation receipt.
pub const DOCS_INDEX_STATE_SCHEMA: &str =
    include_str!("../schema/docs/docs-index-state.schema.json");

/// JSONL fixture-line schema for the deterministic retrieval eval harness.
pub const RETRIEVAL_EVAL_SCHEMA: &str = include_str!("../schema/docs/retrieval-eval.schema.json");

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
    ensure_current_schema(repo_root)?;

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
        schema_version: DOCS_REGISTRY_SCHEMA_VERSION,
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

/// Load the docs registry **without bootstrapping** any canonical plane.
///
/// Returns `Ok(None)` when the docs registry file is absent. This is the
/// read-path loader used by read-only readiness/frontier projections so they
/// never create the docs registry (or, transitively, the evidence manifest) as
/// a side effect of a query.
///
/// When the registry file *is* present, the evidence manifest must already
/// exist. A registry without its evidence identity owner is reported as an
/// invalid existing manifest state and is never repaired by this preserve
/// loader.
pub fn load_existing(repo_root: &Path) -> Result<Option<DocsRegistryEnvelope>> {
    let registry_path = repo_root.join(".pulse/docs/registry.json");
    if !registry_path.exists() {
        return Ok(None);
    }
    let evidence = evidence_manifest::load_existing(repo_root)?.ok_or_else(|| {
        PulseError::validation(
            "docs_registry_evidence_missing",
            "docs registry exists but evidence manifest is missing",
        )
    })?;
    Ok(Some(load_existing_registry(
        repo_root,
        &evidence.repository_id,
    )?))
}

pub(crate) fn load_unlocked_preserve(repo_root: &Path) -> Result<Option<DocsRegistryEnvelope>> {
    load_existing(repo_root)
}

pub(crate) fn load_existing_registry(
    repo_root: &Path,
    repository_id: &str,
) -> Result<DocsRegistryEnvelope> {
    ensure_current_schema(repo_root)?;
    let registry_path = repo_root.join(".pulse/docs/registry.json");
    let registry: DocsRegistryEnvelope = crate::storage::read_json(&registry_path)?;
    if registry.repository_id != repository_id {
        return Err(PulseError::validation(
            "docs_registry_schema_invalid",
            "docs registry repository_id does not match evidence manifest",
        ));
    }
    if registry.schema_version != DOCS_REGISTRY_SCHEMA_VERSION {
        return Err(PulseError::validation(
            "docs_registry_schema_invalid",
            format!(
                "docs registry schema_version must be {}",
                DOCS_REGISTRY_SCHEMA_VERSION
            ),
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
    } else {
        crate::storage::create_new(path, &bytes)?;
        created.push(path.to_path_buf());
    }
    Ok(())
}

/// Require the on-disk document schema to match the embedded current schema.
/// Existing drift is preserved and refused rather than overwritten implicitly.
pub(crate) fn ensure_current_schema(repo_root: &Path) -> Result<()> {
    let path = repo_root.join(".pulse/docs/schemas/document.schema.json");
    if !path.exists() {
        return Err(PulseError::validation(
            "docs_registry_schema_invalid",
            format!("missing document schema at {}", path.display()),
        ));
    }
    let actual_hash = schema_file_hash(repo_root)?;
    let expected_hash = current_schema_hash()?;
    if actual_hash != expected_hash {
        return Err(PulseError::validation(
            "docs_registry_schema_invalid",
            format!(
                "schema drift at {}: expected current document schema hash {}, found {}",
                path.display(),
                expected_hash,
                actual_hash
            ),
        ));
    }
    Ok(())
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
