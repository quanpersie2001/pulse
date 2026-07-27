use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::event::new_event_id;
use crate::event::{event_path, EventEnvelope};
use crate::storage::transaction::{
    commit_prepared_multi_target_transaction, prepare_multi_target_transaction, FileState,
    MultiTargetTransactionIntent, TransactionFailpoint, TransactionTarget,
};
use crate::storage::WriteGuard;
use crate::{PulseError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactMetadata {
    pub schema_version: u32,
    pub algorithm: String,
    pub digest: String,
    pub size_bytes: u64,
    pub media_type: String,
    pub kind: String,
    pub original_name: String,
    pub redaction: RedactionMetadata,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RedactionMetadata {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactStatus {
    Created,
    Unchanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactOutcome {
    pub schema_version: u32,
    pub code: String,
    pub status: ArtifactStatus,
    pub artifact: ArtifactMetadata,
}

pub fn put_artifact(
    repo_root: &Path,
    failpoint: Option<TransactionFailpoint>,
    source: &Path,
    kind: String,
    media_type: Option<String>,
    original_name: Option<String>,
    max_bytes: u64,
) -> Result<ArtifactOutcome> {
    let metadata = fs::symlink_metadata(source).map_err(|error| PulseError::io(source, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(PulseError::validation(
            "artifact_path_unsafe",
            "artifact input must be a regular file",
        ));
    }
    if metadata.len() == 0 {
        return Err(PulseError::validation(
            "artifact_path_unsafe",
            "empty artifacts are not accepted by default",
        ));
    }
    if metadata.len() > max_bytes {
        return Err(PulseError::validation(
            "artifact_too_large",
            format!("artifact is {} bytes", metadata.len()),
        ));
    }
    let bytes = fs::read(source).map_err(|error| PulseError::io(source, error))?;
    let digest = hash_bytes(&bytes);
    let digest_hex = digest.strip_prefix("sha256:").expect("hash prefix");
    let dir = artifact_dir(repo_root, &digest)?;
    let content_path = dir.join("content");
    let metadata_path = dir.join("metadata.json");

    let _guard = WriteGuard::acquire(repo_root)?;
    crate::storage::bootstrap(repo_root)?;
    crate::storage::transaction::recover_prepared_transactions(repo_root)?;
    crate::evidence::manifest::bootstrap(repo_root)?;

    if content_path.exists() {
        let existing =
            fs::read(&content_path).map_err(|error| PulseError::io(&content_path, error))?;
        if hash_bytes(&existing) != digest {
            return Err(PulseError::validation(
                "artifact_hash_mismatch",
                "existing artifact content hash mismatch",
            ));
        }
        if !metadata_path.exists() {
            return Err(PulseError::validation(
                "artifact_metadata_conflict",
                "artifact content exists without metadata",
            ));
        }
        let existing_metadata: ArtifactMetadata = crate::storage::read_json(&metadata_path)?;
        return Ok(ArtifactOutcome {
            schema_version: 1,
            code: "unchanged".to_string(),
            status: ArtifactStatus::Unchanged,
            artifact: existing_metadata,
        });
    }

    let now = Utc::now();
    let artifact = ArtifactMetadata {
        schema_version: 1,
        algorithm: "sha256".to_string(),
        digest: digest.clone(),
        size_bytes: bytes.len() as u64,
        media_type: media_type.unwrap_or_else(|| "application/octet-stream".to_string()),
        kind,
        original_name: original_name.unwrap_or_else(|| {
            source
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("artifact")
                .to_string()
        }),
        redaction: RedactionMetadata {
            status: "caller_asserted".to_string(),
            notes: None,
        },
        created_at: now,
    };
    let metadata_bytes = to_canonical_bytes(&artifact)?;
    let event = EventEnvelope::new(
        new_event_id(),
        "evidence.artifact.recorded",
        "system:evidence",
        digest.clone(),
        json!({
            "digest": digest,
            "algorithm": "sha256",
            "size_bytes": bytes.len() as u64,
            "kind": artifact.kind,
            "metadata_hash": hash_bytes(&metadata_bytes),
        }),
        now,
    );
    let targets = vec![
        TransactionTarget::new(
            content_path,
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&bytes),
                revision: 1,
            },
            &bytes,
        ),
        TransactionTarget::new(
            metadata_path,
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&metadata_bytes),
                revision: 1,
            },
            &metadata_bytes,
        ),
    ];
    let intent = MultiTargetTransactionIntent::prepared(
        event.id.clone(),
        event.event_type.clone(),
        "system:evidence",
        targets,
        event_path(repo_root, &event),
        serde_json::to_value(&event)?,
    )?;
    let prepared = prepare_multi_target_transaction(repo_root, intent)?;
    commit_prepared_multi_target_transaction(&prepared, failpoint)?;
    let _ = digest_hex;
    Ok(ArtifactOutcome {
        schema_version: 1,
        code: "artifact_recorded".to_string(),
        status: ArtifactStatus::Created,
        artifact,
    })
}

pub fn show_artifact(repo_root: &Path, digest: &str) -> Result<ArtifactMetadata> {
    validate_digest(digest)?;
    crate::storage::read_json(&artifact_dir(repo_root, digest)?.join("metadata.json"))
}

pub fn verify_artifact(repo_root: &Path, digest: &str) -> Result<ArtifactOutcome> {
    validate_digest(digest)?;
    let artifact = show_artifact(repo_root, digest)?;
    let content_path = artifact_dir(repo_root, digest)?.join("content");
    if artifact.algorithm != "sha256" || artifact.digest != digest || artifact.size_bytes == 0 {
        return Err(PulseError::validation(
            "artifact_metadata_conflict",
            "artifact metadata does not match requested digest",
        ));
    }
    let bytes = fs::read(&content_path).map_err(|error| PulseError::io(&content_path, error))?;
    if hash_bytes(&bytes) != digest {
        return Err(PulseError::validation(
            "artifact_hash_mismatch",
            "artifact bytes do not match digest",
        ));
    }
    if artifact.size_bytes != bytes.len() as u64 {
        return Err(PulseError::validation(
            "artifact_metadata_conflict",
            "artifact metadata size does not match content",
        ));
    }
    Ok(ArtifactOutcome {
        schema_version: 1,
        code: "artifact_valid".to_string(),
        status: ArtifactStatus::Unchanged,
        artifact,
    })
}

pub fn artifact_exists(repo_root: &Path, digest: &str) -> bool {
    artifact_dir(repo_root, digest)
        .map(|d| d.join("content").exists())
        .unwrap_or(false)
}

fn artifact_dir(repo_root: &Path, digest: &str) -> Result<PathBuf> {
    validate_digest(digest)?;
    let hex = digest.trim_start_matches("sha256:");
    Ok(repo_root
        .join(".pulse/evidence/artifacts/sha256")
        .join(&hex[0..2])
        .join(hex))
}

fn validate_digest(digest: &str) -> Result<()> {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return Err(PulseError::validation(
            "artifact_hash_mismatch",
            "digest must start with sha256:",
        ));
    };
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(PulseError::validation(
            "artifact_hash_mismatch",
            "digest must be sha256 hex",
        ));
    }
    Ok(())
}
