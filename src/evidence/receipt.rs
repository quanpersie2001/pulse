//! One receipt family (plan 0022 §5.1), replacing the v2 handoff/
//! verification/close/decision/documentation receipt hierarchy.
//!
//! A receipt is immutable once written: `.pulse/receipts/<ulid>.json`.
//! Writing the same id with the same canonical content is idempotent;
//! writing the same id with different content is `receipt_conflict`.
//! `artifacts[]` are declared by path (relative to the repo root, under
//! `.pulse/evidence/<subject-id>/`), hashed at seal time — a declared path
//! that does not exist fails the whole write with `artifact_missing`
//! (plan §5.1: "File khai mà không tồn tại -> artifact_missing, receipt
//! không ghi"). Every string in `payload` is redacted through
//! `evidence::redaction` before anything is written.

use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::error::{PulseError, Result};
use crate::evidence::redaction::clean_json_strings;
use crate::storage;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceiptSubject {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceiptSource {
    pub commit: String,
    pub dirty_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceiptArtifact {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceiptEnvelope {
    pub id: String,
    pub kind: String,
    pub subject: ReceiptSubject,
    pub actor: String,
    pub source: ReceiptSource,
    pub recorded_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub payload: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ReceiptArtifact>,
}

/// Everything a caller supplies to seal a new receipt; `id` and
/// `recorded_at` are assigned by [`record_receipt`].
#[derive(Debug, Clone)]
pub struct NewReceipt {
    pub kind: String,
    pub subject: ReceiptSubject,
    pub actor: String,
    pub source: ReceiptSource,
    pub run_id: Option<String>,
    pub payload: Value,
    /// Paths relative to the repo root, expected under
    /// `.pulse/evidence/<subject.id>/`. Hashed at seal time.
    pub artifact_paths: Vec<String>,
}

fn receipts_dir(repo_root: &Path) -> std::path::PathBuf {
    repo_root.join(".pulse/receipts")
}

fn receipt_path(repo_root: &Path, id: &str) -> std::path::PathBuf {
    receipts_dir(repo_root).join(format!("{id}.json"))
}

/// Seal a new receipt, or confirm an existing one with identical content.
///
/// `id` lets a caller retry an interrupted seal idempotently (the same
/// `run_id` + the same intended content should produce the same receipt id);
/// omit it to always mint a fresh ULID.
///
/// # Errors
/// `artifact_missing` if a declared artifact path does not exist.
/// `receipt_privacy_violation` if a payload string fails redaction.
/// `receipt_conflict` if `id` already names a receipt with different
/// content.
pub fn record_receipt(
    repo_root: &Path,
    id: Option<String>,
    new: NewReceipt,
) -> Result<ReceiptEnvelope> {
    let mut artifacts = Vec::with_capacity(new.artifact_paths.len());
    for path in &new.artifact_paths {
        let full = repo_root.join(path);
        let bytes = fs::read(&full).map_err(|_| {
            PulseError::kernel(
                "artifact_missing",
                format!("declared artifact does not exist: {path}"),
                "record every artifact under .pulse/evidence/<id>/ before sealing the receipt, \
                 or drop it from the declaration",
            )
        })?;
        artifacts.push(ReceiptArtifact {
            path: path.clone(),
            sha256: hash_bytes(&bytes).trim_start_matches("sha256:").to_string(),
        });
    }

    let mut payload = new.payload;
    clean_json_strings(repo_root, &mut payload)?;

    let id = id.unwrap_or_else(|| ulid::Ulid::new().to_string());
    let envelope = ReceiptEnvelope {
        id: id.clone(),
        kind: new.kind,
        subject: new.subject,
        actor: new.actor,
        source: new.source,
        recorded_at: Utc::now(),
        run_id: new.run_id,
        payload,
        artifacts,
    };

    let path = receipt_path(repo_root, &id);
    if path.exists() {
        let existing = load_receipt(repo_root, &id)?;
        if same_content(&existing, &envelope) {
            return Ok(existing);
        }
        return Err(PulseError::kernel(
            "receipt_conflict",
            format!("receipt {id} already exists with different content"),
            "receipt ids are content-addressed by intent, not reused across different content; \
             mint a fresh id",
        ));
    }

    let bytes = to_canonical_bytes(&envelope)?;
    storage::create_new(&path, &bytes)?;
    Ok(envelope)
}

/// Two receipts are the same content if everything except `recorded_at`
/// matches (the seal timestamp is not part of the intent).
fn same_content(a: &ReceiptEnvelope, b: &ReceiptEnvelope) -> bool {
    a.id == b.id
        && a.kind == b.kind
        && a.subject == b.subject
        && a.actor == b.actor
        && a.source == b.source
        && a.run_id == b.run_id
        && a.payload == b.payload
        && a.artifacts == b.artifacts
}

/// # Errors
/// Propagates an I/O or JSON error reading `.pulse/receipts/<id>.json`.
pub fn load_receipt(repo_root: &Path, id: &str) -> Result<ReceiptEnvelope> {
    storage::read_json(&receipt_path(repo_root, id))
}

/// A receipt that failed to parse: reported, never silently dropped
/// (Decision 0017 — an always-empty list from a broken store must never
/// read as "nothing here").
#[derive(Debug, Clone, Serialize)]
pub struct UnreadableReceipt {
    pub id: String,
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ReceiptList {
    pub receipts: Vec<ReceiptEnvelope>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unreadable: Vec<UnreadableReceipt>,
}

/// List every receipt, sorted by id (ULID order == chronological order).
///
/// # Errors
/// Propagates an I/O error listing the receipts directory. A single
/// unreadable receipt file does not fail the read; it is reported in
/// `unreadable` instead.
pub fn list_receipts(repo_root: &Path) -> Result<ReceiptList> {
    let dir = receipts_dir(repo_root);
    if !dir.exists() {
        return Ok(ReceiptList::default());
    }
    let mut list = ReceiptList::default();
    for entry in fs::read_dir(&dir).map_err(|error| PulseError::io(&dir, error))? {
        let path = entry.map_err(|error| PulseError::io(&dir, error))?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_string();
        match fs::read(&path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                serde_json::from_slice::<ReceiptEnvelope>(&bytes).map_err(|error| error.to_string())
            }) {
            Ok(receipt) => list.receipts.push(receipt),
            Err(reason) => list.unreadable.push(UnreadableReceipt {
                id,
                path: path.display().to_string(),
                reason,
            }),
        }
    }
    list.receipts.sort_by(|a, b| a.id.cmp(&b.id));
    list.unreadable.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn new_receipt(payload: Value) -> NewReceipt {
        NewReceipt {
            kind: "handoff".to_string(),
            subject: ReceiptSubject {
                id: "TK-a3f9".to_string(),
                revision: Some(1),
            },
            actor: "agent:worker".to_string(),
            source: ReceiptSource {
                commit: "d4e5f6".to_string(),
                dirty_hash: "sha256:0".to_string(),
            },
            run_id: Some("run_1".to_string()),
            payload,
            artifact_paths: Vec::new(),
        }
    }

    #[test]
    fn records_and_loads_a_receipt() {
        let repo = tempfile::tempdir().unwrap();
        let envelope =
            record_receipt(repo.path(), None, new_receipt(json!({"summary": "ok"}))).unwrap();
        let loaded = load_receipt(repo.path(), &envelope.id).unwrap();
        assert_eq!(loaded, envelope);
    }

    #[test]
    fn same_id_and_content_is_idempotent() {
        let repo = tempfile::tempdir().unwrap();
        let id = "01JIDEMPOTENT00000000000000".to_string();
        let first =
            record_receipt(repo.path(), Some(id.clone()), new_receipt(json!({"a": 1}))).unwrap();
        let second =
            record_receipt(repo.path(), Some(id.clone()), new_receipt(json!({"a": 1}))).unwrap();
        assert_eq!(first.id, second.id);
    }

    #[test]
    fn same_id_different_content_is_a_conflict() {
        let repo = tempfile::tempdir().unwrap();
        let id = "01JCONFLICT000000000000000".to_string();
        record_receipt(repo.path(), Some(id.clone()), new_receipt(json!({"a": 1}))).unwrap();
        let err = record_receipt(repo.path(), Some(id), new_receipt(json!({"a": 2}))).unwrap_err();
        assert_eq!(err.code(), "receipt_conflict");
    }

    #[test]
    fn a_declared_artifact_that_does_not_exist_fails_the_whole_seal() {
        let repo = tempfile::tempdir().unwrap();
        let mut new = new_receipt(json!({}));
        new.artifact_paths = vec![".pulse/evidence/TK-a3f9/shots/QA-001.png".to_string()];
        let err = record_receipt(repo.path(), None, new).unwrap_err();
        assert_eq!(err.code(), "artifact_missing");
        assert!(list_receipts(repo.path()).unwrap().receipts.is_empty());
    }

    #[test]
    fn an_existing_declared_artifact_is_hashed_into_the_receipt() {
        let repo = tempfile::tempdir().unwrap();
        let artifact_dir = repo.path().join(".pulse/evidence/TK-a3f9/shots");
        fs::create_dir_all(&artifact_dir).unwrap();
        fs::write(artifact_dir.join("QA-001.png"), b"fake png bytes").unwrap();
        let mut new = new_receipt(json!({}));
        new.artifact_paths = vec![".pulse/evidence/TK-a3f9/shots/QA-001.png".to_string()];
        let envelope = record_receipt(repo.path(), None, new).unwrap();
        assert_eq!(envelope.artifacts.len(), 1);
        assert_eq!(
            envelope.artifacts[0].sha256,
            hash_bytes(b"fake png bytes").trim_start_matches("sha256:")
        );
    }

    #[test]
    fn a_secret_shaped_payload_string_is_refused() {
        let repo = tempfile::tempdir().unwrap();
        let new = new_receipt(json!({"summary": "key AKIAIOSFODNN7EXAMPLE leaked"}));
        let err = record_receipt(repo.path(), None, new).unwrap_err();
        assert_eq!(err.code(), "receipt_privacy_violation");
    }

    #[test]
    fn list_receipts_reports_an_unreadable_file_instead_of_hiding_it() {
        let repo = tempfile::tempdir().unwrap();
        record_receipt(repo.path(), None, new_receipt(json!({"a": 1}))).unwrap();
        let broken_path = receipts_dir(repo.path()).join("01JBROKEN0000000000000000.json");
        fs::write(&broken_path, b"not json").unwrap();

        let list = list_receipts(repo.path()).unwrap();
        assert_eq!(list.receipts.len(), 1);
        assert_eq!(list.unreadable.len(), 1);
        assert_eq!(list.unreadable[0].id, "01JBROKEN0000000000000000");
    }

    #[test]
    fn list_receipts_sorts_by_id() {
        let repo = tempfile::tempdir().unwrap();
        record_receipt(
            repo.path(),
            Some("01JBBBB0000000000000000000".to_string()),
            new_receipt(json!({})),
        )
        .unwrap();
        record_receipt(
            repo.path(),
            Some("01JAAAA0000000000000000000".to_string()),
            new_receipt(json!({})),
        )
        .unwrap();
        let list = list_receipts(repo.path()).unwrap();
        let ids: Vec<&str> = list.receipts.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            ["01JAAAA0000000000000000000", "01JBBBB0000000000000000000"]
        );
    }
}
