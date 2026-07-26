use crate::canonical_json::{self, hash_bytes};
use crate::error::{PulseError, Result};
use crate::id::new_transaction_id;
use crate::storage::atomic;
use base64::prelude::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(debug_assertions)]
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionFailpoint {
    AfterIntent,
    AfterCanonical,
    AfterMultiTargetFirst,
    AfterMultiTargetAll,
    AfterEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedTransaction {
    pub intent: TransactionIntent,
    pub intent_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedMultiTargetTransaction {
    pub intent: MultiTargetTransactionIntent,
    pub intent_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileState {
    Absent,
    Present { hash: String, revision: u64 },
}

pub const TRANSACTION_INTENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentState {
    Prepared,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionIntent {
    pub schema_version: u32,
    pub transaction_id: String,
    pub event_id: String,
    pub operation: String,
    pub actor: String,
    pub targets: Vec<TransactionTarget>,
    pub event_path: PathBuf,
    pub event_hash: String,
    pub event_payload: serde_json::Value,
    pub state: IntentState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionTarget {
    pub path: PathBuf,
    pub before: FileState,
    pub after: FileState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_bytes_base64: Option<String>,
}

impl TransactionTarget {
    pub fn new(path: PathBuf, before: FileState, after: FileState, after_bytes: &[u8]) -> Self {
        Self {
            path,
            before,
            after,
            after_bytes_base64: Some(BASE64_STANDARD.encode(after_bytes)),
        }
    }

    pub fn after_bytes(&self) -> Result<Vec<u8>> {
        let encoded =
            self.after_bytes_base64
                .as_deref()
                .ok_or_else(|| PulseError::InvalidTransaction {
                    message: format!(
                        "stored after payload is missing for {}",
                        self.path.display()
                    ),
                })?;
        BASE64_STANDARD
            .decode(encoded)
            .map_err(|error| PulseError::InvalidTransaction {
                message: format!("stored after payload is not base64: {error}"),
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiTargetTransactionIntent {
    pub schema_version: u32,
    pub transaction_id: String,
    pub event_id: String,
    pub operation: String,
    pub actor: String,
    pub targets: Vec<TransactionTarget>,
    pub event_path: PathBuf,
    pub event_hash: String,
    pub event_payload: serde_json::Value,
    pub state: IntentState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    RolledBack {
        intent_path: PathBuf,
    },
    EventCompleted {
        intent_path: PathBuf,
        event_path: PathBuf,
    },
    CleanedComplete {
        intent_path: PathBuf,
    },
}

impl TransactionIntent {
    #[allow(clippy::too_many_arguments)]
    pub fn prepared(
        event_id: impl Into<String>,
        operation: impl Into<String>,
        actor: impl Into<String>,
        target_path: PathBuf,
        event_path: PathBuf,
        before: FileState,
        after: FileState,
        event_payload: serde_json::Value,
    ) -> Result<Self> {
        let event_hash = canonical_json::hash_value(&event_payload)?;
        let now = Utc::now();
        Ok(Self {
            schema_version: TRANSACTION_INTENT_SCHEMA_VERSION,
            transaction_id: new_transaction_id(),
            event_id: event_id.into(),
            operation: operation.into(),
            actor: actor.into(),
            targets: vec![TransactionTarget {
                path: target_path,
                before,
                after,
                after_bytes_base64: None,
            }],
            event_path,
            event_hash,
            event_payload,
            state: IntentState::Prepared,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn intent_path(&self, repo_root: &Path) -> PathBuf {
        repo_root
            .join(".pulse/runtime/transactions")
            .join(format!("{}.json", self.transaction_id))
    }
}

impl MultiTargetTransactionIntent {
    pub fn prepared(
        event_id: impl Into<String>,
        operation: impl Into<String>,
        actor: impl Into<String>,
        mut targets: Vec<TransactionTarget>,
        event_path: PathBuf,
        event_payload: serde_json::Value,
    ) -> Result<Self> {
        if targets.is_empty() {
            return Err(PulseError::InvalidTransaction {
                message: "multi-target transaction requires at least one target".to_string(),
            });
        }
        targets.sort_by(|left, right| left.path.cmp(&right.path));
        for target in &targets {
            let bytes = target.after_bytes()?;
            if !observed_hash_matches(Some(&hash_bytes(&bytes)), &target.after) {
                return Err(PulseError::InvalidTransaction {
                    message: format!(
                        "after payload hash does not match planned state for {}",
                        target.path.display()
                    ),
                });
            }
        }
        let event_hash = canonical_json::hash_value(&event_payload)?;
        let now = Utc::now();
        Ok(Self {
            schema_version: TRANSACTION_INTENT_SCHEMA_VERSION,
            transaction_id: new_transaction_id(),
            event_id: event_id.into(),
            operation: operation.into(),
            actor: actor.into(),
            targets,
            event_path,
            event_hash,
            event_payload,
            state: IntentState::Prepared,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn intent_path(&self, repo_root: &Path) -> PathBuf {
        repo_root
            .join(".pulse/runtime/transactions")
            .join(format!("{}.json", self.transaction_id))
    }
}

pub fn persist_intent(repo_root: &Path, intent: &TransactionIntent) -> Result<PathBuf> {
    validate_intent_schema(intent.schema_version)?;
    let directory = repo_root.join(".pulse/runtime/transactions");
    fs::create_dir_all(&directory).map_err(|error| PulseError::io(&directory, error))?;
    let path = intent.intent_path(repo_root);
    let bytes = canonical_json::to_canonical_bytes_from(intent)?;
    atomic::atomic_replace(&path, &bytes)?;
    Ok(path)
}

pub fn persist_multi_target_intent(
    repo_root: &Path,
    intent: &MultiTargetTransactionIntent,
) -> Result<PathBuf> {
    validate_intent_schema(intent.schema_version)?;
    let directory = repo_root.join(".pulse/runtime/transactions");
    fs::create_dir_all(&directory).map_err(|error| PulseError::io(&directory, error))?;
    let path = intent.intent_path(repo_root);
    let bytes = canonical_json::to_canonical_bytes_from(intent)?;
    atomic::atomic_replace(&path, &bytes)?;
    Ok(path)
}

pub fn prepare_transaction(
    repo_root: &Path,
    intent: TransactionIntent,
) -> Result<PreparedTransaction> {
    validate_intent_schema(intent.schema_version)?;
    let intent_path = persist_intent(repo_root, &intent)?;
    Ok(PreparedTransaction {
        intent,
        intent_path,
    })
}

pub fn prepare_multi_target_transaction(
    repo_root: &Path,
    intent: MultiTargetTransactionIntent,
) -> Result<PreparedMultiTargetTransaction> {
    validate_intent_schema(intent.schema_version)?;
    let intent_path = persist_multi_target_intent(repo_root, &intent)?;
    Ok(PreparedMultiTargetTransaction {
        intent,
        intent_path,
    })
}

pub fn complete_and_cleanup_intent(intent_path: &Path, intent: &TransactionIntent) -> Result<()> {
    validate_intent_schema(intent.schema_version)?;
    let mut complete = intent.clone();
    complete.state = IntentState::Complete;
    complete.updated_at = Utc::now();
    let bytes = canonical_json::to_canonical_bytes_from(&complete)?;
    atomic::atomic_replace(intent_path, &bytes)?;
    fs::remove_file(intent_path).map_err(|error| PulseError::io(intent_path, error))
}

pub fn complete_and_cleanup_multi_target_intent(
    intent_path: &Path,
    intent: &MultiTargetTransactionIntent,
) -> Result<()> {
    validate_intent_schema(intent.schema_version)?;
    let mut complete = intent.clone();
    complete.state = IntentState::Complete;
    complete.updated_at = Utc::now();
    let bytes = canonical_json::to_canonical_bytes_from(&complete)?;
    atomic::atomic_replace(intent_path, &bytes)?;
    fs::remove_file(intent_path).map_err(|error| PulseError::io(intent_path, error))
}

fn validate_intent_schema(schema_version: u32) -> Result<()> {
    if schema_version == TRANSACTION_INTENT_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(PulseError::InvalidTransaction {
            message: format!(
                "unsupported transaction intent schema_version {schema_version}; expected {TRANSACTION_INTENT_SCHEMA_VERSION}"
            ),
        })
    }
}

fn single_target(intent: &TransactionIntent) -> Result<&TransactionTarget> {
    if intent.targets.len() == 1 {
        Ok(&intent.targets[0])
    } else {
        Err(PulseError::InvalidTransaction {
            message: format!(
                "single-target transaction intent contains {} targets",
                intent.targets.len()
            ),
        })
    }
}

pub fn commit_prepared_transaction(
    prepared: &PreparedTransaction,
    canonical_bytes: &[u8],
    failpoint: Option<TransactionFailpoint>,
) -> Result<()> {
    #[cfg(debug_assertions)]
    if failpoint == Some(TransactionFailpoint::AfterIntent) {
        trigger_failpoint("after_intent")?;
    }

    let target = single_target(&prepared.intent)?;
    write_target_respecting_before(&target.path, &target.before, &target.after, canonical_bytes)?;
    #[cfg(debug_assertions)]
    if failpoint == Some(TransactionFailpoint::AfterCanonical) {
        trigger_failpoint("after_canonical")?;
    }

    write_event_create_new(&prepared.intent)?;
    #[cfg(debug_assertions)]
    if failpoint == Some(TransactionFailpoint::AfterEvent) {
        trigger_failpoint("after_event")?;
    }

    complete_and_cleanup_intent(&prepared.intent_path, &prepared.intent)
}

pub fn commit_prepared_multi_target_transaction(
    prepared: &PreparedMultiTargetTransaction,
    failpoint: Option<TransactionFailpoint>,
) -> Result<()> {
    #[cfg(debug_assertions)]
    if failpoint == Some(TransactionFailpoint::AfterIntent) {
        trigger_failpoint("after_intent")?;
    }

    for (index, target) in prepared.intent.targets.iter().enumerate() {
        write_target_respecting_before(
            &target.path,
            &target.before,
            &target.after,
            &target.after_bytes()?,
        )?;
        #[cfg(debug_assertions)]
        if index == 0 && failpoint == Some(TransactionFailpoint::AfterMultiTargetFirst) {
            trigger_failpoint("after_multi_target_first")?;
        }
    }

    #[cfg(debug_assertions)]
    if failpoint == Some(TransactionFailpoint::AfterMultiTargetAll) {
        trigger_failpoint("after_multi_target_all")?;
    }

    write_event_create_new_multi(&prepared.intent)?;
    #[cfg(debug_assertions)]
    if failpoint == Some(TransactionFailpoint::AfterEvent) {
        trigger_failpoint("after_event")?;
    }

    complete_and_cleanup_multi_target_intent(&prepared.intent_path, &prepared.intent)
}

#[cfg(debug_assertions)]
fn trigger_failpoint(name: &'static str) -> Result<()> {
    if std::env::var_os("PULSE_FAILPOINT_SLEEP_MS").is_some() {
        let millis = std::env::var("PULSE_FAILPOINT_SLEEP_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(30_000);
        eprintln!("pulse failpoint reached: {name}");
        std::thread::sleep(Duration::from_millis(millis));
    }
    Err(PulseError::Failpoint { name })
}

pub fn recover_prepared_transactions(repo_root: &Path) -> Result<Vec<RecoveryAction>> {
    let directory = repo_root.join(".pulse/runtime/transactions");
    cleanup_orphan_transaction_temps(&directory)?;
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut actions = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(&directory)
        .map_err(|error| PulseError::io(&directory, error))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| PulseError::io(&directory, error))?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        let intent: MultiTargetTransactionIntent = serde_json::from_slice(&bytes)?;
        validate_intent_schema(intent.schema_version)?;
        let action = recover_one_multi_target(&path, &intent)?;
        actions.push(action);
    }
    Ok(actions)
}

pub fn current_file_state(path: &Path, revision: Option<u64>) -> Result<FileState> {
    if !path.exists() {
        return Ok(FileState::Absent);
    }
    let bytes = fs::read(path).map_err(|error| PulseError::io(path, error))?;
    Ok(FileState::Present {
        hash: hash_bytes(&bytes),
        revision: revision.unwrap_or(0),
    })
}

fn recover_one_multi_target(
    intent_path: &Path,
    intent: &MultiTargetTransactionIntent,
) -> Result<RecoveryAction> {
    if intent.state == IntentState::Complete {
        fs::remove_file(intent_path).map_err(|error| PulseError::io(intent_path, error))?;
        return Ok(RecoveryAction::CleanedComplete {
            intent_path: intent_path.to_path_buf(),
        });
    }

    let mut observed = Vec::with_capacity(intent.targets.len());
    for target in &intent.targets {
        observed.push(observed_target_state(
            &target.path,
            &target.before,
            &target.after,
        )?);
    }
    let event_state = observed_event_state(&intent.event_path, &intent.event_hash)?;

    let after_prefix = observed
        .iter()
        .take_while(|state| **state == ObservedTarget::After)
        .count();
    let planned_shape = observed[..after_prefix]
        .iter()
        .all(|state| *state == ObservedTarget::After)
        && observed[after_prefix..]
            .iter()
            .all(|state| *state == ObservedTarget::Before);
    if !planned_shape {
        return Err(PulseError::AmbiguousTransaction {
            transaction_id: intent.transaction_id.clone(),
            message: "multi-target canonical state is not all-before, prefix-after, or all-after"
                .to_string(),
        });
    }

    match (after_prefix, intent.targets.len(), event_state) {
        (0, _, ObservedEvent::Absent) => {
            for target in &intent.targets {
                cleanup_target_temp(&target.path)?;
            }
            fs::remove_file(intent_path).map_err(|error| PulseError::io(intent_path, error))?;
            Ok(RecoveryAction::RolledBack {
                intent_path: intent_path.to_path_buf(),
            })
        }
        (written, total, ObservedEvent::Absent) if written < total => {
            for target in intent.targets.iter().skip(written) {
                write_target_respecting_before(
                    &target.path,
                    &target.before,
                    &target.after,
                    &target.after_bytes()?,
                )?;
            }
            write_event_create_new_multi(intent)?;
            fs::remove_file(intent_path).map_err(|error| PulseError::io(intent_path, error))?;
            Ok(RecoveryAction::EventCompleted {
                intent_path: intent_path.to_path_buf(),
                event_path: intent.event_path.clone(),
            })
        }
        (written, total, ObservedEvent::Absent) if written == total => {
            write_event_create_new_multi(intent)?;
            fs::remove_file(intent_path).map_err(|error| PulseError::io(intent_path, error))?;
            Ok(RecoveryAction::EventCompleted {
                intent_path: intent_path.to_path_buf(),
                event_path: intent.event_path.clone(),
            })
        }
        (written, total, ObservedEvent::Matching) if written == total => {
            fs::remove_file(intent_path).map_err(|error| PulseError::io(intent_path, error))?;
            Ok(RecoveryAction::CleanedComplete {
                intent_path: intent_path.to_path_buf(),
            })
        }
        (_, _, ObservedEvent::Matching) => Err(PulseError::AmbiguousTransaction {
            transaction_id: intent.transaction_id.clone(),
            message: "event exists before all multi-target files reached after state".to_string(),
        }),
        (_, _, ObservedEvent::Mismatch { actual_hash }) => Err(PulseError::EventMismatch {
            transaction_id: intent.transaction_id.clone(),
            message: format!(
                "event file hash {actual_hash} does not match prepared hash {}",
                intent.event_hash
            ),
        }),
        _ => Err(PulseError::AmbiguousTransaction {
            transaction_id: intent.transaction_id.clone(),
            message: "unrecognized multi-target recovery state".to_string(),
        }),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ObservedTarget {
    Before,
    After,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ObservedEvent {
    Absent,
    Matching,
    Mismatch { actual_hash: String },
}

fn observed_target_state(
    path: &Path,
    before: &FileState,
    after: &FileState,
) -> Result<ObservedTarget> {
    let observed_hash = if path.exists() {
        let bytes = fs::read(path).map_err(|error| PulseError::io(path, error))?;
        Some(hash_bytes(&bytes))
    } else {
        None
    };

    if observed_hash_matches(observed_hash.as_deref(), before) {
        Ok(ObservedTarget::Before)
    } else if observed_hash_matches(observed_hash.as_deref(), after) {
        Ok(ObservedTarget::After)
    } else {
        Ok(ObservedTarget::Other)
    }
}

fn write_target_respecting_before(
    path: &Path,
    before: &FileState,
    after: &FileState,
    bytes: &[u8],
) -> Result<()> {
    let observed = observed_target_state(path, before, after)?;
    match (before, observed) {
        (FileState::Absent, ObservedTarget::Before) => crate::storage::create_new(path, bytes),
        (_, ObservedTarget::Before) => atomic::atomic_replace(path, bytes).map(|_| ()),
        (_, ObservedTarget::After) => Ok(()),
        (_, ObservedTarget::Other) => Err(PulseError::AmbiguousTransaction {
            transaction_id: "commit_precondition".to_string(),
            message: format!(
                "target {} no longer matches prepared before state",
                path.display()
            ),
        }),
    }
}

fn observed_event_state(path: &Path, expected_hash: &str) -> Result<ObservedEvent> {
    if !path.exists() {
        return Ok(ObservedEvent::Absent);
    }
    let bytes = fs::read(path).map_err(|error| PulseError::io(path, error))?;
    let actual_hash = hash_bytes(&bytes);
    if actual_hash == expected_hash {
        Ok(ObservedEvent::Matching)
    } else {
        Ok(ObservedEvent::Mismatch { actual_hash })
    }
}

fn observed_hash_matches(observed_hash: Option<&str>, expected: &FileState) -> bool {
    match (observed_hash, expected) {
        (None, FileState::Absent) => true,
        (Some(observed), FileState::Present { hash, .. }) => observed == hash,
        _ => false,
    }
}

pub fn write_event_create_new(intent: &TransactionIntent) -> Result<()> {
    write_event_create_new_parts(
        &intent.transaction_id,
        &intent.event_path,
        &intent.event_hash,
        &intent.event_payload,
    )
}

pub fn write_event_create_new_multi(intent: &MultiTargetTransactionIntent) -> Result<()> {
    write_event_create_new_parts(
        &intent.transaction_id,
        &intent.event_path,
        &intent.event_hash,
        &intent.event_payload,
    )
}

fn write_event_create_new_parts(
    transaction_id: &str,
    event_path: &Path,
    event_hash: &str,
    event_payload: &serde_json::Value,
) -> Result<()> {
    let bytes = canonical_json::to_canonical_bytes(event_payload)?;
    let actual_hash = hash_bytes(&bytes);
    if actual_hash != event_hash {
        return Err(PulseError::InvalidTransaction {
            message: format!(
                "prepared event payload hash changed from {event_hash} to {actual_hash}"
            ),
        });
    }
    if let Some(parent) = event_path.parent() {
        fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
    }
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(event_path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if event_matches_parts(event_path, event_hash)? {
                return Ok(());
            }
            return Err(PulseError::EventMismatch {
                transaction_id: transaction_id.to_string(),
                message: format!(
                    "event file already exists at {} with different content",
                    event_path.display()
                ),
            });
        }
        Err(error) => return Err(PulseError::io(event_path, error)),
    };
    file.write_all(&bytes)
        .map_err(|error| PulseError::io(event_path, error))?;
    file.flush()
        .map_err(|error| PulseError::io(event_path, error))?;
    file.sync_all()
        .map_err(|error| PulseError::io(event_path, error))?;
    Ok(())
}

fn cleanup_target_temp(target_path: &Path) -> Result<()> {
    if let Some(parent) = target_path.parent() {
        let _ = atomic::cleanup_orphan_temps(parent)?;
    }
    Ok(())
}

fn cleanup_orphan_transaction_temps(directory: &Path) -> Result<()> {
    let _ = atomic::cleanup_orphan_temps(directory)?;
    Ok(())
}

pub fn event_matches_intent(intent: &TransactionIntent) -> Result<bool> {
    event_matches_parts(&intent.event_path, &intent.event_hash)
}

fn event_matches_parts(event_path: &Path, event_hash: &str) -> Result<bool> {
    if !event_path.exists() {
        return Ok(false);
    }
    let bytes = fs::read(event_path).map_err(|error| PulseError::io(event_path, error))?;
    Ok(hash_bytes(&bytes) == event_hash)
}
