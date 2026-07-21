use crate::canonical_json::{self, hash_bytes};
use crate::error::{PulseError, Result};
use crate::id::new_transaction_id;
use crate::storage::atomic;
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
    AfterEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedTransaction {
    pub intent: TransactionIntent,
    pub intent_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileState {
    Absent,
    Present { hash: String, revision: u64 },
}

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
    pub target_path: PathBuf,
    pub event_path: PathBuf,
    pub before: FileState,
    pub after: FileState,
    pub event_hash: String,
    pub event_payload: serde_json::Value,
    pub state: IntentState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    RolledBack { intent_path: PathBuf },
    EventCompleted { intent_path: PathBuf, event_path: PathBuf },
    CleanedComplete { intent_path: PathBuf },
}

impl TransactionIntent {
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
            schema_version: 1,
            transaction_id: new_transaction_id(),
            event_id: event_id.into(),
            operation: operation.into(),
            actor: actor.into(),
            target_path,
            event_path,
            before,
            after,
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
    let directory = repo_root.join(".pulse/runtime/transactions");
    fs::create_dir_all(&directory).map_err(|error| PulseError::io(&directory, error))?;
    let path = intent.intent_path(repo_root);
    let bytes = canonical_json::to_canonical_bytes_from(intent)?;
    atomic::atomic_replace(&path, &bytes)?;
    Ok(path)
}

pub fn prepare_transaction(repo_root: &Path, intent: TransactionIntent) -> Result<PreparedTransaction> {
    let intent_path = persist_intent(repo_root, &intent)?;
    Ok(PreparedTransaction { intent, intent_path })
}

pub fn complete_and_cleanup_intent(intent_path: &Path, intent: &TransactionIntent) -> Result<()> {
    let mut complete = intent.clone();
    complete.state = IntentState::Complete;
    complete.updated_at = Utc::now();
    let bytes = canonical_json::to_canonical_bytes_from(&complete)?;
    atomic::atomic_replace(intent_path, &bytes)?;
    fs::remove_file(intent_path).map_err(|error| PulseError::io(intent_path, error))
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

    atomic::atomic_replace(&prepared.intent.target_path, canonical_bytes)?;
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
        let intent: TransactionIntent = serde_json::from_slice(&bytes)?;
        let action = recover_one(&path, &intent)?;
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

fn recover_one(intent_path: &Path, intent: &TransactionIntent) -> Result<RecoveryAction> {
    if intent.state == IntentState::Complete {
        fs::remove_file(intent_path).map_err(|error| PulseError::io(intent_path, error))?;
        return Ok(RecoveryAction::CleanedComplete {
            intent_path: intent_path.to_path_buf(),
        });
    }

    let target_state = observed_target_state(&intent.target_path, &intent.before, &intent.after)?;
    let event_state = observed_event_state(&intent.event_path, &intent.event_hash)?;

    match (target_state, event_state) {
        (ObservedTarget::Before, ObservedEvent::Absent) => {
            cleanup_target_temp(&intent.target_path)?;
            fs::remove_file(intent_path).map_err(|error| PulseError::io(intent_path, error))?;
            Ok(RecoveryAction::RolledBack {
                intent_path: intent_path.to_path_buf(),
            })
        }
        (ObservedTarget::After, ObservedEvent::Absent) => {
            write_event_create_new(intent)?;
            fs::remove_file(intent_path).map_err(|error| PulseError::io(intent_path, error))?;
            Ok(RecoveryAction::EventCompleted {
                intent_path: intent_path.to_path_buf(),
                event_path: intent.event_path.clone(),
            })
        }
        (ObservedTarget::After, ObservedEvent::Matching) => {
            fs::remove_file(intent_path).map_err(|error| PulseError::io(intent_path, error))?;
            Ok(RecoveryAction::CleanedComplete {
                intent_path: intent_path.to_path_buf(),
            })
        }
        (ObservedTarget::Before, ObservedEvent::Matching) => Err(PulseError::AmbiguousTransaction {
            transaction_id: intent.transaction_id.clone(),
            message: "event exists but canonical target is still at before state".to_string(),
        }),
        (_, ObservedEvent::Mismatch { actual_hash }) => Err(PulseError::EventMismatch {
            transaction_id: intent.transaction_id.clone(),
            message: format!(
                "event file hash {actual_hash} does not match prepared hash {}",
                intent.event_hash
            ),
        }),
        (ObservedTarget::Other, _) => Err(PulseError::AmbiguousTransaction {
            transaction_id: intent.transaction_id.clone(),
            message: "canonical target matches neither before nor after state".to_string(),
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
    let bytes = canonical_json::to_canonical_bytes(&intent.event_payload)?;
    let actual_hash = hash_bytes(&bytes);
    if actual_hash != intent.event_hash {
        return Err(PulseError::InvalidTransaction {
            message: format!(
                "prepared event payload hash changed from {} to {actual_hash}",
                intent.event_hash
            ),
        });
    }
    if let Some(parent) = intent.event_path.parent() {
        fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
    }
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&intent.event_path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if event_matches_intent(intent)? {
                return Ok(());
            }
            return Err(PulseError::EventMismatch {
                transaction_id: intent.transaction_id.clone(),
                message: format!(
                    "event file already exists at {} with different content",
                    intent.event_path.display()
                ),
            });
        }
        Err(error) => return Err(PulseError::io(&intent.event_path, error)),
    };
    file.write_all(&bytes)
        .map_err(|error| PulseError::io(&intent.event_path, error))?;
    file.flush()
        .map_err(|error| PulseError::io(&intent.event_path, error))?;
    file.sync_all()
        .map_err(|error| PulseError::io(&intent.event_path, error))?;
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
    if !intent.event_path.exists() {
        return Ok(false);
    }
    let bytes = fs::read(&intent.event_path)
        .map_err(|error| PulseError::io(&intent.event_path, error))?;
    Ok(hash_bytes(&bytes) == intent.event_hash)
}
