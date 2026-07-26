use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::{PulseError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const RECEIPT_ENVELOPE_SCHEMA: &str =
    include_str!("../schema/evidence/receipt-envelope.v1.schema.json");
pub const SUPERSESSION_SCHEMA: &str =
    include_str!("../schema/evidence/supersession-reconciliation.v1.schema.json");
pub const SHAPING_SCHEMA: &str =
    include_str!("../schema/evidence/shaping-validation.v1.schema.json");
pub const DOCUMENTATION_SCHEMA: &str =
    include_str!("../schema/evidence/documentation-validation.v1.schema.json");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EvidenceManifest {
    pub schema_version: u32,
    pub receipt_schemas: BTreeMap<String, SchemaRef>,
    pub receipt_kinds: BTreeMap<String, BTreeMap<String, SchemaRef>>,
    pub repository_id: String,
    pub artifact_algorithm: String,
    pub max_inline_receipt_bytes: u64,
    pub max_artifact_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SchemaRef {
    pub schema: String,
    pub schema_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceBootstrapOutcome {
    pub schema_version: u32,
    pub created: Vec<PathBuf>,
    pub preserved: Vec<PathBuf>,
    pub manifest: EvidenceManifest,
}

pub fn bootstrap(repo_root: &Path) -> Result<EvidenceBootstrapOutcome> {
    let evidence = repo_root.join(".pulse/evidence");
    let schemas = evidence.join("schemas");
    let receipts = evidence.join("receipts");
    let artifacts = evidence.join("artifacts/sha256");
    let mut created = Vec::new();
    let mut preserved = Vec::new();

    for dir in [&evidence, &schemas, &receipts, &artifacts] {
        if dir.exists() {
            preserved.push(dir.to_path_buf());
        } else {
            fs::create_dir_all(dir).map_err(|error| PulseError::io(dir, error))?;
            created.push(dir.to_path_buf());
        }
    }

    write_schema_if_absent(
        &schemas.join("receipt-envelope.v1.schema.json"),
        RECEIPT_ENVELOPE_SCHEMA,
        &mut created,
        &mut preserved,
    )?;
    write_schema_if_absent(
        &schemas.join("supersession-reconciliation.v1.schema.json"),
        SUPERSESSION_SCHEMA,
        &mut created,
        &mut preserved,
    )?;
    write_schema_if_absent(
        &schemas.join("shaping-validation.v1.schema.json"),
        SHAPING_SCHEMA,
        &mut created,
        &mut preserved,
    )?;
    write_schema_if_absent(
        &schemas.join("documentation-validation.v1.schema.json"),
        DOCUMENTATION_SCHEMA,
        &mut created,
        &mut preserved,
    )?;

    let manifest_path = evidence.join("manifest.json");
    let manifest = if manifest_path.exists() {
        preserved.push(manifest_path.clone());
        let manifest: EvidenceManifest = crate::storage::read_json(&manifest_path)?;
        validate_manifest(repo_root, &manifest)?;
        manifest
    } else {
        let manifest = default_manifest(repo_root)?;
        let bytes = to_canonical_bytes(&manifest)?;
        crate::storage::create_new(&manifest_path, &bytes)?;
        created.push(manifest_path);
        manifest
    };

    Ok(EvidenceBootstrapOutcome {
        schema_version: 1,
        created,
        preserved,
        manifest,
    })
}

pub fn load(repo_root: &Path) -> Result<EvidenceManifest> {
    let manifest_path = repo_root.join(".pulse/evidence/manifest.json");
    if !manifest_path.exists() {
        return Ok(bootstrap(repo_root)?.manifest);
    }
    let manifest: EvidenceManifest = crate::storage::read_json(&manifest_path)?;
    validate_manifest(repo_root, &manifest)?;
    Ok(manifest)
}

fn default_manifest(repo_root: &Path) -> Result<EvidenceManifest> {
    let mut receipt_schemas = BTreeMap::new();
    receipt_schemas.insert(
        "1".to_string(),
        SchemaRef {
            schema: "schemas/receipt-envelope.v1.schema.json".to_string(),
            schema_hash: schema_hash(RECEIPT_ENVELOPE_SCHEMA)?,
        },
    );
    let mut receipt_kinds = BTreeMap::new();
    for (kind, version, path, schema) in [
        (
            "supersession_reconciliation",
            "1",
            "schemas/supersession-reconciliation.v1.schema.json",
            SUPERSESSION_SCHEMA,
        ),
        (
            "shaping_validation",
            "1",
            "schemas/shaping-validation.v1.schema.json",
            SHAPING_SCHEMA,
        ),
        (
            "documentation_validation",
            "1",
            "schemas/documentation-validation.v1.schema.json",
            DOCUMENTATION_SCHEMA,
        ),
    ] {
        receipt_kinds
            .entry(kind.to_string())
            .or_insert_with(BTreeMap::new)
            .insert(
                version.to_string(),
                SchemaRef {
                    schema: path.to_string(),
                    schema_hash: schema_hash(schema)?,
                },
            );
    }
    let _ = repo_root;
    Ok(EvidenceManifest {
        schema_version: 1,
        receipt_schemas,
        receipt_kinds,
        repository_id: format!("repo_{}", ulid::Ulid::new()),
        artifact_algorithm: "sha256".to_string(),
        max_inline_receipt_bytes: 262_144,
        max_artifact_bytes: 16_777_216,
    })
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
                "receipt_schema_invalid",
                format!("schema drift at {}", path.display()),
            ));
        }
    } else {
        crate::storage::create_new(path, &bytes)?;
        created.push(path.to_path_buf());
    }
    Ok(())
}

fn validate_manifest(repo_root: &Path, manifest: &EvidenceManifest) -> Result<()> {
    if manifest.schema_version != 1
        || manifest.artifact_algorithm != "sha256"
        || !manifest.repository_id.starts_with("repo_")
    {
        return Err(PulseError::validation(
            "receipt_schema_invalid",
            "unsupported evidence manifest",
        ));
    }
    for schema_ref in manifest
        .receipt_schemas
        .values()
        .chain(manifest.receipt_kinds.values().flat_map(|m| m.values()))
    {
        let path = repo_root.join(".pulse/evidence").join(&schema_ref.schema);
        let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        if hash_bytes(&bytes) != schema_ref.schema_hash {
            return Err(PulseError::validation(
                "receipt_schema_invalid",
                format!("schema hash mismatch at {}", path.display()),
            ));
        }
    }
    Ok(())
}

fn schema_hash(schema: &str) -> Result<String> {
    let value: serde_json::Value = serde_json::from_str(schema)?;
    Ok(hash_bytes(&to_canonical_bytes(&value)?))
}
