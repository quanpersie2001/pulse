use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::evidence::manifest as evidence_manifest;
use crate::{PulseError, Result};

pub const LEARNING_SCHEMA: &str = include_str!("../schema/knowledge/learning.schema.json");
pub const RELATION_SCHEMA: &str = include_str!("../schema/knowledge/relation.schema.json");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeManifest {
    pub schema_version: u32,
    pub repository_id: String,
    pub learning_schema: SchemaRef,
    pub relation_schema: SchemaRef,
    pub id_pattern: String,
    pub content_root: String,
    pub projection_schema_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SchemaRef {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeBootstrapOutcome {
    pub schema_version: u32,
    pub created: Vec<PathBuf>,
    pub preserved: Vec<PathBuf>,
    pub manifest: KnowledgeManifest,
}

pub fn bootstrap(repo_root: &Path) -> Result<KnowledgeBootstrapOutcome> {
    let _guard = crate::storage::WriteGuard::acquire(repo_root)?;
    bootstrap_unlocked(repo_root)
}

pub(crate) fn bootstrap_unlocked(repo_root: &Path) -> Result<KnowledgeBootstrapOutcome> {
    let evidence = evidence_manifest::load(repo_root)?;
    let root = repo_root.join(".pulse/knowledge");
    let schemas = root.join("schemas");
    let entries = root.join("entries");
    let relations = root.join("relations");
    let cache = repo_root.join(".pulse/cache");
    let mut created = Vec::new();
    let mut preserved = Vec::new();

    for dir in [&root, &schemas, &entries, &relations, &cache] {
        create_dir_if_missing(dir, &mut created, &mut preserved)?;
    }
    write_schema_if_absent(
        &schemas.join("learning.schema.json"),
        LEARNING_SCHEMA,
        &mut created,
        &mut preserved,
    )?;
    write_schema_if_absent(
        &schemas.join("relation.schema.json"),
        RELATION_SCHEMA,
        &mut created,
        &mut preserved,
    )?;
    verify_schema_hashes(repo_root)?;

    let manifest_path = root.join("manifest.json");
    let manifest = if manifest_path.exists() {
        preserved.push(manifest_path.clone());
        let manifest: KnowledgeManifest = crate::storage::read_json(&manifest_path)?;
        validate_manifest(repo_root, &manifest, &evidence.repository_id)?;
        manifest
    } else {
        let manifest = default_manifest(evidence.repository_id)?;
        let bytes = to_canonical_bytes(&manifest)?;
        crate::storage::create_new(&manifest_path, &bytes)?;
        created.push(manifest_path);
        manifest
    };

    Ok(KnowledgeBootstrapOutcome {
        schema_version: 1,
        created,
        preserved,
        manifest,
    })
}

pub fn load(repo_root: &Path) -> Result<KnowledgeManifest> {
    let manifest_path = repo_root.join(".pulse/knowledge/manifest.json");
    if !manifest_path.exists() {
        return Ok(bootstrap(repo_root)?.manifest);
    }
    let evidence = evidence_manifest::load(repo_root)?;
    let manifest: KnowledgeManifest = crate::storage::read_json(&manifest_path)?;
    validate_manifest(repo_root, &manifest, &evidence.repository_id)?;
    Ok(manifest)
}

/// Load the knowledge manifest **without bootstrapping** any canonical plane.
///
/// Returns `Ok(None)` when the knowledge manifest file is absent, typed error
/// when the file is present but malformed/unsupported, and
/// `Ok(Some(manifest))` on success.
pub fn load_existing(repo_root: &Path) -> Result<Option<KnowledgeManifest>> {
    let manifest_path = repo_root.join(".pulse/knowledge/manifest.json");
    if !manifest_path.exists() {
        return Ok(None);
    }
    let evidence = evidence_manifest::load_existing(repo_root)?.ok_or_else(|| {
        PulseError::validation(
            "knowledge_manifest_evidence_missing",
            "knowledge manifest exists but evidence manifest is missing",
        )
    })?;
    let manifest: KnowledgeManifest = crate::storage::read_json(&manifest_path)?;
    validate_manifest(repo_root, &manifest, &evidence.repository_id)?;
    Ok(Some(manifest))
}

pub fn default_manifest(repository_id: String) -> Result<KnowledgeManifest> {
    Ok(KnowledgeManifest {
        schema_version: 1,
        repository_id,
        learning_schema: SchemaRef {
            path: "schemas/learning.schema.json".to_string(),
            sha256: schema_hash(LEARNING_SCHEMA)?,
        },
        relation_schema: SchemaRef {
            path: "schemas/relation.schema.json".to_string(),
            sha256: schema_hash(RELATION_SCHEMA)?,
        },
        id_pattern: "^LRN-[0-9]{3,}$".to_string(),
        content_root: "../../knowledge/learnings".to_string(),
        projection_schema_version: 1,
    })
}

pub fn validate_manifest(
    repo_root: &Path,
    manifest: &KnowledgeManifest,
    repository_id: &str,
) -> Result<()> {
    if manifest.schema_version != 1 || manifest.projection_schema_version != 1 {
        return Err(PulseError::validation(
            "knowledge_manifest_schema_invalid",
            "unsupported knowledge manifest version",
        ));
    }
    if manifest.repository_id != repository_id {
        return Err(PulseError::validation(
            "knowledge_manifest_repository_mismatch",
            "knowledge manifest repository_id does not match evidence manifest",
        ));
    }
    let expected = default_manifest(repository_id.to_string())?;
    if manifest.id_pattern != expected.id_pattern || manifest.content_root != expected.content_root
    {
        return Err(PulseError::validation(
            "knowledge_manifest_schema_invalid",
            "knowledge manifest contract fields differ from current contract",
        ));
    }
    if manifest.learning_schema != expected.learning_schema
        || manifest.relation_schema != expected.relation_schema
    {
        return Err(PulseError::validation(
            "knowledge_schema_hash_mismatch",
            "knowledge schema refs differ from embedded contract",
        ));
    }
    verify_schema_hashes(repo_root)
}

pub fn schema_hash(schema: &str) -> Result<String> {
    let value: serde_json::Value = serde_json::from_str(schema)?;
    Ok(hash_bytes(&to_canonical_bytes(&value)?))
}

fn verify_schema_hashes(repo_root: &Path) -> Result<()> {
    for (relative, schema) in [
        ("learning.schema.json", LEARNING_SCHEMA),
        ("relation.schema.json", RELATION_SCHEMA),
    ] {
        let path = repo_root.join(".pulse/knowledge/schemas").join(relative);
        let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        let expected = schema_hash(schema)?;
        if hash_bytes(&bytes) != expected {
            return Err(PulseError::validation(
                "knowledge_schema_hash_mismatch",
                format!("schema drift at {}", path.display()),
            ));
        }
    }
    Ok(())
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
        if hash_bytes(&fs::read(path).map_err(|error| PulseError::io(path, error))?)
            != hash_bytes(&bytes)
        {
            return Err(PulseError::validation(
                "knowledge_schema_hash_mismatch",
                format!("schema drift at {}", path.display()),
            ));
        }
    } else {
        crate::storage::create_new(path, &bytes)?;
        created.push(path.to_path_buf());
    }
    Ok(())
}
