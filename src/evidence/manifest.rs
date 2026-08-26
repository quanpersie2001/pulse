use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::{PulseError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const RECEIPT_ENVELOPE_SCHEMA: &str =
    include_str!("../schema/evidence/receipt-envelope.schema.json");
pub const QA_RECEIPT_ENVELOPE_SCHEMA: &str =
    include_str!("../schema/evidence/receipt-envelope-qa.schema.json");
pub const SUPERSESSION_SCHEMA: &str =
    include_str!("../schema/evidence/supersession-reconciliation.schema.json");
pub const SHAPING_SCHEMA: &str = include_str!("../schema/evidence/shaping-validation.schema.json");
pub const DECISION_ACCEPTANCE_SCHEMA: &str =
    include_str!("../schema/evidence/decision-acceptance.schema.json");
pub const DOCUMENTATION_SCHEMA: &str =
    include_str!("../schema/evidence/documentation-validation.schema.json");
pub const QA_CHECKPOINT_SCHEMA: &str = include_str!("../schema/evidence/qa-checkpoint.schema.json");
pub const QA_CHECKPOINT_LIFECYCLE_SCHEMA: &str =
    include_str!("../schema/evidence/qa-checkpoint-lifecycle.schema.json");
pub const QA_CHECKPOINT_BROWSER_SCHEMA: &str =
    include_str!("../schema/evidence/qa-checkpoint-browser.schema.json");
pub const QA_CHECKPOINT_BROWSER_DEPLOYMENT_SCHEMA: &str =
    include_str!("../schema/evidence/qa-checkpoint-browser-deployment.schema.json");

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

    for (name, schema) in schema_contracts() {
        write_schema_if_absent(&schemas.join(name), schema, &mut created, &mut preserved)?;
    }

    let manifest_path = evidence.join("manifest.json");
    let manifest = if manifest_path.exists() {
        preserved.push(manifest_path.clone());
        let mut manifest: EvidenceManifest = crate::storage::read_json(&manifest_path)?;
        let qa_changed = install_qa_contract(&mut manifest)?;
        if qa_changed {
            crate::storage::atomic_write(&manifest_path, &to_canonical_bytes(&manifest)?)?;
        }
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

/// Load the evidence manifest **without bootstrapping** any canonical plane.
///
/// Returns `Ok(None)` when the evidence manifest file is absent, typed error
/// when the file is present but malformed/unsupported, and
/// `Ok(Some(manifest))` on success.
///
/// This is the read-path loader used by packet source snapshot and repository
/// identity checks so they never create the evidence manifest (or its
/// associated directories/schemas) as a side effect of a query.
pub fn load_existing(repo_root: &Path) -> Result<Option<EvidenceManifest>> {
    let manifest_path = repo_root.join(".pulse/evidence/manifest.json");
    if !manifest_path.exists() {
        return Ok(None);
    }
    let manifest: EvidenceManifest = crate::storage::read_json(&manifest_path)?;
    validate_manifest(repo_root, &manifest)?;
    Ok(Some(manifest))
}

/// Validate existing evidence bootstrap state without creating or upgrading it.
///
/// Current partial schema installation is resumable only while no immutable
/// receipts or artifacts exist without their repository identity manifest.
pub(crate) fn preflight_bootstrap(repo_root: &Path) -> Result<()> {
    let root = repo_root.join(".pulse/evidence");
    if !root.exists() {
        return Ok(());
    }
    if !root.is_dir() {
        return Err(PulseError::validation(
            "repository_init_evidence_conflict",
            ".pulse/evidence exists but is not a directory",
        ));
    }

    let manifest = load_existing(repo_root)?;
    let schemas = root.join("schemas");
    for (name, schema) in schema_contracts() {
        let path = schemas.join(name);
        if path.exists() {
            let expected = canonical_schema_bytes(schema)?;
            let actual = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
            if hash_bytes(&actual) != hash_bytes(&expected) {
                return Err(PulseError::validation(
                    "receipt_schema_invalid",
                    format!("schema drift at {}", path.display()),
                ));
            }
        }
    }

    if manifest.is_none() {
        ensure_only_known_partial_entries(&root, &["schemas", "receipts", "artifacts"])?;
        ensure_only_known_partial_entries(
            &schemas,
            &schema_contracts()
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>(),
        )?;
        ensure_no_files(&root.join("receipts"))?;
        ensure_no_files(&root.join("artifacts"))?;
    }
    Ok(())
}

fn schema_contracts() -> [(&'static str, &'static str); 10] {
    [
        ("receipt-envelope.schema.json", RECEIPT_ENVELOPE_SCHEMA),
        (
            "receipt-envelope-qa.schema.json",
            QA_RECEIPT_ENVELOPE_SCHEMA,
        ),
        (
            "supersession-reconciliation.schema.json",
            SUPERSESSION_SCHEMA,
        ),
        ("shaping-validation.schema.json", SHAPING_SCHEMA),
        (
            "decision-acceptance.schema.json",
            DECISION_ACCEPTANCE_SCHEMA,
        ),
        ("documentation-validation.schema.json", DOCUMENTATION_SCHEMA),
        ("qa-checkpoint.schema.json", QA_CHECKPOINT_SCHEMA),
        (
            "qa-checkpoint-lifecycle.schema.json",
            QA_CHECKPOINT_LIFECYCLE_SCHEMA,
        ),
        (
            "qa-checkpoint-browser.schema.json",
            QA_CHECKPOINT_BROWSER_SCHEMA,
        ),
        (
            "qa-checkpoint-browser-deployment.schema.json",
            QA_CHECKPOINT_BROWSER_DEPLOYMENT_SCHEMA,
        ),
    ]
}

fn canonical_schema_bytes(schema: &str) -> Result<Vec<u8>> {
    let value: serde_json::Value = serde_json::from_str(schema)?;
    to_canonical_bytes(&value)
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
                "repository_init_evidence_partial_refused",
                format!(
                    "evidence state without a manifest contains unknown entry {}",
                    entry.path().display()
                ),
            ));
        }
    }
    Ok(())
}

fn ensure_no_files(root: &Path) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root).map_err(|error| PulseError::io(root, error))? {
        let entry = entry.map_err(|error| PulseError::io(root, error))?;
        let file_type = entry
            .file_type()
            .map_err(|error| PulseError::io(entry.path(), error))?;
        if file_type.is_symlink() || file_type.is_file() {
            return Err(PulseError::validation(
                "repository_init_evidence_partial_refused",
                format!(
                    "evidence records exist without a repository identity manifest at {}",
                    entry.path().display()
                ),
            ));
        }
        ensure_no_files(&entry.path())?;
    }
    Ok(())
}

fn default_manifest(repo_root: &Path) -> Result<EvidenceManifest> {
    let mut receipt_schemas = BTreeMap::new();
    receipt_schemas.insert(
        "1".to_string(),
        SchemaRef {
            schema: "schemas/receipt-envelope.schema.json".to_string(),
            schema_hash: schema_hash(RECEIPT_ENVELOPE_SCHEMA)?,
        },
    );
    receipt_schemas.insert(
        "2".to_string(),
        SchemaRef {
            schema: "schemas/receipt-envelope-qa.schema.json".to_string(),
            schema_hash: schema_hash(QA_RECEIPT_ENVELOPE_SCHEMA)?,
        },
    );
    let mut receipt_kinds = BTreeMap::new();
    for (kind, version, path, schema) in [
        (
            "supersession_reconciliation",
            "1",
            "schemas/supersession-reconciliation.schema.json",
            SUPERSESSION_SCHEMA,
        ),
        (
            "shaping_validation",
            "1",
            "schemas/shaping-validation.schema.json",
            SHAPING_SCHEMA,
        ),
        (
            "decision_acceptance",
            "1",
            "schemas/decision-acceptance.schema.json",
            DECISION_ACCEPTANCE_SCHEMA,
        ),
        (
            "documentation_validation",
            "1",
            "schemas/documentation-validation.schema.json",
            DOCUMENTATION_SCHEMA,
        ),
        (
            "qa_checkpoint",
            "1",
            "schemas/qa-checkpoint.schema.json",
            QA_CHECKPOINT_SCHEMA,
        ),
        (
            "qa_checkpoint",
            "2",
            "schemas/qa-checkpoint-lifecycle.schema.json",
            QA_CHECKPOINT_LIFECYCLE_SCHEMA,
        ),
        (
            "qa_checkpoint",
            "3",
            "schemas/qa-checkpoint-browser.schema.json",
            QA_CHECKPOINT_BROWSER_SCHEMA,
        ),
        (
            "qa_checkpoint",
            "4",
            "schemas/qa-checkpoint-browser-deployment.schema.json",
            QA_CHECKPOINT_BROWSER_DEPLOYMENT_SCHEMA,
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

fn install_qa_contract(manifest: &mut EvidenceManifest) -> Result<bool> {
    let mut changed = false;
    if !manifest.receipt_schemas.contains_key("2") {
        manifest.receipt_schemas.insert(
            "2".to_string(),
            SchemaRef {
                schema: "schemas/receipt-envelope-qa.schema.json".to_string(),
                schema_hash: schema_hash(QA_RECEIPT_ENVELOPE_SCHEMA)?,
            },
        );
        changed = true;
    }
    let qa = manifest
        .receipt_kinds
        .entry("qa_checkpoint".to_string())
        .or_default();
    if !qa.contains_key("1") {
        qa.insert(
            "1".to_string(),
            SchemaRef {
                schema: "schemas/qa-checkpoint.schema.json".to_string(),
                schema_hash: schema_hash(QA_CHECKPOINT_SCHEMA)?,
            },
        );
        changed = true;
    }
    if !qa.contains_key("2") {
        qa.insert(
            "2".to_string(),
            SchemaRef {
                schema: "schemas/qa-checkpoint-lifecycle.schema.json".to_string(),
                schema_hash: schema_hash(QA_CHECKPOINT_LIFECYCLE_SCHEMA)?,
            },
        );
        changed = true;
    }
    if !qa.contains_key("3") {
        qa.insert(
            "3".to_string(),
            SchemaRef {
                schema: "schemas/qa-checkpoint-browser.schema.json".to_string(),
                schema_hash: schema_hash(QA_CHECKPOINT_BROWSER_SCHEMA)?,
            },
        );
        changed = true;
    }
    if !qa.contains_key("4") {
        qa.insert(
            "4".to_string(),
            SchemaRef {
                schema: "schemas/qa-checkpoint-browser-deployment.schema.json".to_string(),
                schema_hash: schema_hash(QA_CHECKPOINT_BROWSER_DEPLOYMENT_SCHEMA)?,
            },
        );
        changed = true;
    }
    Ok(changed)
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
