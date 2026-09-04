use crate::canonical_json::to_canonical_bytes;
use crate::{PulseError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EvidenceManifest {
    pub schema_version: u32,
    pub repository_id: String,
    pub artifact_algorithm: String,
    pub max_inline_receipt_bytes: u64,
    pub max_artifact_bytes: u64,
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
    let receipts = evidence.join("receipts");
    let artifacts = evidence.join("artifacts/sha256");
    let mut created = Vec::new();
    let mut preserved = Vec::new();

    for dir in [&evidence, &receipts, &artifacts] {
        if dir.exists() {
            preserved.push(dir.to_path_buf());
        } else {
            fs::create_dir_all(dir).map_err(|error| PulseError::io(dir, error))?;
            created.push(dir.to_path_buf());
        }
    }

    let manifest_path = evidence.join("manifest.json");
    let manifest = if manifest_path.exists() {
        preserved.push(manifest_path.clone());
        let manifest: EvidenceManifest = crate::storage::read_json(&manifest_path)?;
        validate_manifest(repo_root, &manifest)?;
        manifest
    } else {
        let manifest = default_manifest()?;
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
/// Without a repository identity manifest, only empty managed directories are a
/// safe partial state to complete.
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
    if manifest.is_none() {
        ensure_only_known_partial_entries(&root, &["receipts", "artifacts"])?;
        ensure_only_known_partial_entries(&root.join("artifacts"), &["sha256"])?;
        ensure_no_files(&root.join("receipts"))?;
        ensure_no_files(&root.join("artifacts/sha256"))?;
    }
    Ok(())
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

fn default_manifest() -> Result<EvidenceManifest> {
    Ok(EvidenceManifest {
        schema_version: 1,
        repository_id: format!("repo_{}", ulid::Ulid::new()),
        artifact_algorithm: "sha256".to_string(),
        max_inline_receipt_bytes: 262_144,
        max_artifact_bytes: 16_777_216,
    })
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
    let managed_dirs = [
        repo_root.join(".pulse/evidence/receipts"),
        repo_root.join(".pulse/evidence/artifacts"),
    ];
    for dir in managed_dirs {
        if !dir.exists() {
            return Err(PulseError::validation(
                "receipt_plane_missing",
                format!("evidence plane is missing: {}", dir.display()),
            ));
        }
    }
    Ok(())
}
