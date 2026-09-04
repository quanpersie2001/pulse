use std::fs;
use std::path::{Path, PathBuf};

use crate::canonical_json::to_canonical_bytes;
use crate::docs::model::{DocsRegistryEnvelope, DOCS_REGISTRY_SCHEMA_VERSION};
use crate::docs::validate::validate_registry;
use crate::evidence::manifest as evidence_manifest;
use crate::storage::transaction::recover_prepared_transactions;
use crate::storage::WriteGuard;
use crate::{PulseError, Result};

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
    let mut created = Vec::new();
    let mut preserved = Vec::new();

    if docs.exists() {
        preserved.push(docs.clone());
    } else {
        fs::create_dir_all(&docs).map_err(|error| PulseError::io(&docs, error))?;
        created.push(docs.clone());
    }

    let tags_path = docs.join("tags.json");
    if !tags_path.exists() {
        let bytes = to_canonical_bytes(&crate::docs::tags::TagsRegistry::default())?;
        crate::storage::create_new(&tags_path, &bytes)?;
        created.push(tags_path);
    } else {
        preserved.push(tags_path.clone());
        let tags: crate::docs::tags::TagsRegistry = crate::storage::read_json(&tags_path)?;
        crate::docs::tags::validate_registry_shape(&tags)?;
    }

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

/// Validate existing documentation-registry bootstrap state without writing.
///
/// A missing registry may be completed only when the managed directory is
/// empty or contains the current document schema and no other state.
pub(crate) fn preflight_bootstrap(repo_root: &Path) -> Result<()> {
    let root = repo_root.join(".pulse/docs");
    if !root.exists() {
        return Ok(());
    }
    if !root.is_dir() {
        return Err(PulseError::validation(
            "repository_init_docs_conflict",
            ".pulse/docs exists but is not a directory",
        ));
    }
    if load_existing(repo_root)?.is_some() {
        return Ok(());
    }
    ensure_only_known_partial_entries(&root, &["registry.json", "tags.json"])?;
    Ok(())
}

pub(crate) fn load_unlocked_preserve(repo_root: &Path) -> Result<Option<DocsRegistryEnvelope>> {
    load_existing(repo_root)
}

pub(crate) fn load_existing_registry(
    repo_root: &Path,
    repository_id: &str,
) -> Result<DocsRegistryEnvelope> {
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

fn ensure_only_known_partial_entries(root: &Path, allowed: &[&str]) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root).map_err(|error| PulseError::io(root, error))? {
        let entry = entry.map_err(|error| PulseError::io(root, error))?;
        let name = entry.file_name();
        if !allowed.iter().any(|allowed| name == *allowed) {
            return Err(PulseError::validation(
                "repository_init_docs_partial_refused",
                format!(
                    "documentation registry state without a registry contains unknown entry {}",
                    entry.path().display()
                ),
            ));
        }
    }
    Ok(())
}
