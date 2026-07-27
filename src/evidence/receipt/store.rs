//! Receipt persistence and read entrypoints: record/show/list/load/verify,
//! receipt paths, recording-event idempotency and transaction integration.
//!
//! Recording is CAS-style and idempotent: a replay of identical canonical bytes
//! for an existing receipt id yields `Unchanged`, while a conflicting replay is
//! rejected. Recording writes a typed `evidence.receipt.recorded` event through
//! a prepared storage transaction, so crash recovery reconciles partial writes.
//! `verify_receipt` assembles the full validation report: integrity (envelope
//! proof + recording-event presence), bindings currentness and the documentation
//! registry/policy/authorization dimensions (delegated to
//! [`crate::docs::receipt_validation`]).

use super::bindings::binding_staleness;
use super::envelope::{
    normalize_bindings, validate_envelope, validate_manifest_kind, validate_receipt_id,
};
use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::event::new_event_id;
use crate::event::{event_path, EventActor, EventActorKind, EventEnvelope, EventSubject};
use crate::evidence::manifest;
use crate::evidence::model::{
    ActorKind, ReceiptEnvelope, ReceiptKind, ReceiptResult, SubjectRef, ValidationDimension,
    ValidationReport,
};
use crate::storage::transaction::{
    commit_prepared_transaction, prepare_transaction, FileState, TransactionFailpoint,
    TransactionIntent,
};
use crate::storage::WriteGuard;
use crate::{PulseError, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Created,
    Unchanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceiptOutcome {
    pub schema_version: u32,
    pub code: String,
    pub status: ReceiptStatus,
    pub receipt: ReceiptEnvelope,
    pub receipt_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceiptList {
    pub schema_version: u32,
    pub receipts: Vec<ReceiptSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceiptSummary {
    pub id: String,
    pub kind: ReceiptKind,
    pub subject: SubjectRef,
    pub result: ReceiptResult,
    pub recorded_at: chrono::DateTime<Utc>,
    pub receipt_hash: String,
}

pub fn record_receipt(
    repo_root: &Path,
    failpoint: Option<TransactionFailpoint>,
    file: &Path,
) -> Result<ReceiptOutcome> {
    let input_bytes = fs::read(file).map_err(|error| PulseError::io(file, error))?;
    if input_bytes.len() > 262_144 {
        return Err(PulseError::validation(
            "receipt_schema_invalid",
            "receipt too large",
        ));
    }
    let mut receipt: ReceiptEnvelope =
        serde_json::from_slice(&input_bytes).map_err(|error| PulseError::json(file, error))?;
    validate_receipt_id(&receipt.id)?;
    normalize_bindings(&mut receipt);
    let manifest = manifest::load(repo_root)?;
    if input_bytes.len() as u64 > manifest.max_inline_receipt_bytes {
        return Err(PulseError::validation(
            "receipt_schema_invalid",
            "receipt exceeds manifest max_inline_receipt_bytes",
        ));
    }
    validate_manifest_kind(&manifest, &receipt)?;
    validate_envelope(repo_root, &receipt, true)?;
    let canonical = to_canonical_bytes(&receipt)?;
    let receipt_hash = hash_bytes(&canonical);
    let receipt_path = receipt_path(repo_root, &receipt.id);

    let _guard = WriteGuard::acquire(repo_root)?;
    crate::storage::bootstrap(repo_root)?;
    crate::storage::transaction::recover_prepared_transactions(repo_root)?;
    manifest::bootstrap(repo_root)?;

    if receipt_path.exists() {
        let existing =
            fs::read(&receipt_path).map_err(|error| PulseError::io(&receipt_path, error))?;
        if existing == canonical {
            return Ok(ReceiptOutcome {
                schema_version: 1,
                code: "unchanged".to_string(),
                status: ReceiptStatus::Unchanged,
                receipt,
                receipt_hash,
            });
        }
        return Err(PulseError::validation(
            "receipt_id_conflict",
            "same receipt id has different bytes",
        ));
    }

    let event = EventEnvelope::new_typed(
        new_event_id(),
        "evidence.receipt.recorded",
        EventActor::new(
            match receipt.actor.kind {
                ActorKind::Human => EventActorKind::Human,
                ActorKind::Agent => EventActorKind::Agent,
                ActorKind::System => EventActorKind::System,
            },
            receipt.actor.id.clone(),
        ),
        EventSubject::new("receipt", receipt.id.clone(), None),
        None,
        json!({
            "receipt_id": receipt.id,
            "receipt_kind": receipt.kind,
            "receipt_hash": receipt_hash,
            "subject": receipt.subject.id,
            "result": receipt.result,
        }),
        Utc::now(),
    );
    let intent = TransactionIntent::prepared(
        event.id.clone(),
        event.event_type.clone(),
        event.actor.legacy_id(),
        receipt_path.clone(),
        event_path(repo_root, &event),
        FileState::Absent,
        FileState::Present {
            hash: receipt_hash.clone(),
            revision: 1,
        },
        serde_json::to_value(&event)?,
    )?;
    let prepared = prepare_transaction(repo_root, intent)?;
    commit_prepared_transaction(&prepared, &canonical, failpoint)?;
    Ok(ReceiptOutcome {
        schema_version: 1,
        code: "receipt_recorded".to_string(),
        status: ReceiptStatus::Created,
        receipt,
        receipt_hash,
    })
}

pub fn show_receipt(repo_root: &Path, id: &str) -> Result<ReceiptOutcome> {
    let (receipt, hash) = load_receipt(repo_root, id)?;
    Ok(ReceiptOutcome {
        schema_version: 1,
        code: "ok".to_string(),
        status: ReceiptStatus::Unchanged,
        receipt,
        receipt_hash: hash,
    })
}

pub fn list_receipts(
    repo_root: &Path,
    kind: Option<ReceiptKind>,
    subject: Option<String>,
    result: Option<ReceiptResult>,
) -> Result<ReceiptList> {
    let dir = repo_root.join(".pulse/evidence/receipts");
    let mut receipts = Vec::new();
    if dir.exists() {
        for entry in fs::read_dir(&dir).map_err(|error| PulseError::io(&dir, error))? {
            let path = entry.map_err(|error| PulseError::io(&dir, error))?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
            let receipt: ReceiptEnvelope =
                serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))?;
            if kind.as_ref().is_some_and(|k| k != &receipt.kind) {
                continue;
            }
            if subject.as_ref().is_some_and(|s| s != &receipt.subject.id) {
                continue;
            }
            if result.as_ref().is_some_and(|r| r != &receipt.result) {
                continue;
            }
            receipts.push(ReceiptSummary {
                id: receipt.id,
                kind: receipt.kind,
                subject: receipt.subject,
                result: receipt.result,
                recorded_at: receipt.recorded_at,
                receipt_hash: hash_bytes(&bytes),
            });
        }
    }
    receipts.sort_by(|a, b| a.recorded_at.cmp(&b.recorded_at).then(a.id.cmp(&b.id)));
    Ok(ReceiptList {
        schema_version: 1,
        receipts,
    })
}

pub fn verify_receipt(
    repo_root: &Path,
    id: &str,
    current: bool,
    source: Option<&str>,
) -> Result<ValidationReport> {
    let (receipt, hash) = load_receipt(repo_root, id)?;
    let mut integrity = Vec::new();
    if let Err(err) = validate_envelope(repo_root, &receipt, false) {
        integrity.push(err.code().to_string());
    }
    if !has_recording_event(repo_root, id, &hash)? {
        integrity.push("receipt_recording_event_missing".to_string());
    }
    let binding_codes = if current {
        binding_staleness(repo_root, &receipt, source)?
    } else {
        Vec::new()
    };
    let bindings = ValidationDimension {
        status: if !current {
            "not_checked"
        } else if binding_codes.is_empty() {
            "current"
        } else {
            "stale"
        }
        .to_string(),
        reason_codes: binding_codes,
    };
    let (registry, policy, authorization, gate_eligible) =
        crate::docs::receipt_validation::documentation_validation_dimensions(
            repo_root,
            &receipt,
            current,
            integrity.is_empty(),
            bindings.status == "current",
        )?;
    Ok(ValidationReport {
        schema_version: 1,
        receipt_id: id.to_string(),
        receipt_hash: hash,
        integrity: ValidationDimension {
            status: if integrity.is_empty() {
                "valid"
            } else {
                "invalid"
            }
            .to_string(),
            reason_codes: integrity,
        },
        bindings,
        registry,
        policy,
        authorization,
        gate_eligible,
    })
}

pub fn load_receipt(repo_root: &Path, id: &str) -> Result<(ReceiptEnvelope, String)> {
    validate_receipt_id(id)?;
    let path = receipt_path(repo_root, id);
    if !path.exists() {
        return Err(PulseError::validation("receipt_not_found", id));
    }
    let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
    let receipt: ReceiptEnvelope =
        serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))?;
    Ok((receipt, hash_bytes(&bytes)))
}

fn receipt_path(repo_root: &Path, id: &str) -> PathBuf {
    repo_root
        .join(".pulse/evidence/receipts")
        .join(format!("{id}.json"))
}

fn has_recording_event(repo_root: &Path, id: &str, hash: &str) -> Result<bool> {
    let dir = repo_root.join(".pulse/events");
    if !dir.exists() {
        return Ok(false);
    }
    let mut count = 0;
    for day in fs::read_dir(&dir).map_err(|error| PulseError::io(&dir, error))? {
        let day = day.map_err(|error| PulseError::io(&dir, error))?.path();
        if !day.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&day).map_err(|error| PulseError::io(&day, error))? {
            let path = entry.map_err(|error| PulseError::io(&day, error))?.path();
            let Ok(event): std::result::Result<crate::event::EventEnvelope, _> =
                crate::storage::read_json(&path)
            else {
                continue;
            };
            if event.event_type == "evidence.receipt.recorded"
                && event.payload.get("receipt_id").and_then(|v| v.as_str()) == Some(id)
                && event.payload.get("receipt_hash").and_then(|v| v.as_str()) == Some(hash)
            {
                count += 1;
            }
        }
    }
    if count > 1 {
        return Err(PulseError::validation(
            "receipt_recording_event_ambiguous",
            id,
        ));
    }
    Ok(count == 1)
}
