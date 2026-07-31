//! Durable daemon-global runtime repositories.

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::daemon::assignment::{AssignmentSagaRecord, DeliveryRecord};
use crate::daemon::process::ManagedProcessRecord;
use crate::daemon::project::ProjectRecord;
use crate::daemon::session::{CommunicationGrantRecord, SessionMessageRecord, SessionRecord};
use crate::daemon::timeline::TimelineEvent;
use crate::daemon::workspace::WorkspaceRecord;
use crate::{PulseError, Result};

pub const DAEMON_STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DaemonState {
    pub schema_version: u32,
    pub epoch: String,
    pub next_sequence: u64,
    pub projects: BTreeMap<String, ProjectRecord>,
    pub workspaces: BTreeMap<String, WorkspaceRecord>,
    pub sessions: BTreeMap<String, SessionRecord>,
    #[serde(default)]
    pub communication_grants: BTreeMap<String, CommunicationGrantRecord>,
    #[serde(default)]
    pub session_messages: BTreeMap<String, SessionMessageRecord>,
    pub processes: BTreeMap<String, ManagedProcessRecord>,
    pub assignment_sagas: BTreeMap<String, AssignmentSagaRecord>,
    #[serde(default)]
    pub deliveries: BTreeMap<String, DeliveryRecord>,
    pub timeline: Vec<TimelineEvent>,
    #[serde(default)]
    pub idempotency_results: BTreeMap<String, IdempotencyRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdempotencyRecord {
    pub request_fingerprint: String,
    pub response: Value,
    pub recorded_at: String,
}

impl DaemonState {
    pub fn new() -> Self {
        Self {
            schema_version: DAEMON_STATE_SCHEMA_VERSION,
            epoch: format!("epoch_{}", ulid::Ulid::new()),
            next_sequence: 1,
            projects: BTreeMap::new(),
            workspaces: BTreeMap::new(),
            sessions: BTreeMap::new(),
            communication_grants: BTreeMap::new(),
            session_messages: BTreeMap::new(),
            processes: BTreeMap::new(),
            assignment_sagas: BTreeMap::new(),
            deliveries: BTreeMap::new(),
            timeline: Vec::new(),
            idempotency_results: BTreeMap::new(),
        }
    }
}

impl Default for DaemonState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn daemon_home() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("PULSE_DAEMON_HOME") {
        if path.is_empty() {
            return Err(PulseError::validation(
                "daemon_home_invalid",
                "PULSE_DAEMON_HOME must not be empty",
            ));
        }
        return Ok(PathBuf::from(path));
    }
    if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
        return Ok(PathBuf::from(path).join("pulse"));
    }
    #[cfg(windows)]
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        return Ok(PathBuf::from(path).join("Pulse"));
    }
    let home = std::env::var_os("HOME").ok_or_else(|| {
        PulseError::validation(
            "daemon_home_unavailable",
            "HOME, XDG_STATE_HOME and PULSE_DAEMON_HOME are unavailable",
        )
    })?;
    Ok(PathBuf::from(home).join(".local/state/pulse"))
}

#[derive(Debug, Clone)]
pub struct StateStore {
    root: PathBuf,
}

impl StateStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn discover() -> Result<Self> {
        Ok(Self::new(daemon_home()?))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn state_path(&self) -> PathBuf {
        self.root.join("runtime-state.json")
    }

    pub fn endpoint_path(&self) -> PathBuf {
        self.root.join("daemon-endpoint.json")
    }

    pub fn owner_lock_path(&self) -> PathBuf {
        self.root.join("daemon-owner.lock")
    }

    pub fn with_state<T>(
        &self,
        mutate: bool,
        operation: impl FnOnce(&mut DaemonState) -> Result<T>,
    ) -> Result<T> {
        fs::create_dir_all(&self.root).map_err(|error| PulseError::io(&self.root, error))?;
        let lock_path = self.root.join("runtime-state.lock");
        let lock = open_lock(&lock_path)?;
        lock_with_timeout(&lock, &lock_path, Duration::from_secs(10))?;
        let mut state = self.load_unlocked()?;
        let result = operation(&mut state)?;
        if mutate {
            self.save_unlocked(&state)?;
        }
        FileExt::unlock(&lock).map_err(|error| PulseError::io(&lock_path, error))?;
        Ok(result)
    }

    pub fn load(&self) -> Result<DaemonState> {
        self.with_state(false, |state| Ok(state.clone()))
    }

    fn load_unlocked(&self) -> Result<DaemonState> {
        let path = self.state_path();
        if !path.exists() {
            return Ok(DaemonState::new());
        }
        let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        let state: DaemonState =
            serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))?;
        if state.schema_version != DAEMON_STATE_SCHEMA_VERSION {
            return Err(PulseError::validation(
                "daemon_state_version_incompatible",
                format!(
                    "daemon state schema {} is incompatible with {}",
                    state.schema_version, DAEMON_STATE_SCHEMA_VERSION
                ),
            ));
        }
        Ok(state)
    }

    fn save_unlocked(&self, state: &DaemonState) -> Result<()> {
        let bytes = crate::canonical_json::to_canonical_bytes(state)?;
        crate::storage::atomic_write_private(&self.state_path(), &bytes)
    }

    pub fn acquire_owner(&self) -> Result<DaemonOwnerGuard> {
        fs::create_dir_all(&self.root).map_err(|error| PulseError::io(&self.root, error))?;
        let path = self.owner_lock_path();
        let file = open_lock(&path)?;
        file.try_lock_exclusive().map_err(|error| {
            if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) {
                PulseError::validation(
                    "daemon_already_running",
                    "another Pulse daemon owns this daemon home",
                )
            } else {
                PulseError::io(&path, error)
            }
        })?;
        Ok(DaemonOwnerGuard { path, file })
    }

    pub fn acquire_idempotency(&self, key: &str) -> Result<IdempotencyGuard> {
        let digest = crate::canonical_json::hash_bytes(key.as_bytes());
        let filename = format!("{}.lock", digest.trim_start_matches("sha256:"));
        let path = self.root.join("locks/idempotency").join(filename);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
        }
        let file = open_lock(&path)?;
        lock_with_timeout(&file, &path, Duration::from_secs(30))?;
        Ok(IdempotencyGuard { path, file })
    }

    /// Test-only deterministic fault injection for crash-consistency tests.
    ///
    /// A marker file at `<root>/failpoints/<name>.panic` makes the next
    /// failpoint call panic (simulating a daemon crash between two durable
    /// commits); `<name>.error` makes it return an injected store failure.
    /// Without marker files this is a no-op, so production behavior is
    /// unaffected. Tests arm markers with [`Self::arm_failpoint`].
    pub fn check_failpoint(&self, name: &str) -> Result<()> {
        let panic_path = self.root.join("failpoints").join(format!("{name}.panic"));
        let error_path = self.root.join("failpoints").join(format!("{name}.error"));
        if panic_path.exists() {
            panic!("injected daemon crash at failpoint {name:?}");
        }
        if error_path.exists() {
            return Err(PulseError::validation(
                "injected_failpoint",
                format!("injected store failure at failpoint {name:?}"),
            ));
        }
        Ok(())
    }

    /// Test-only: create a failpoint marker (see [`Self::check_failpoint`]).
    pub fn arm_failpoint(&self, name: &str, mode: FailpointMode) -> Result<()> {
        let path = self
            .root
            .join("failpoints")
            .join(format!("{name}.{}", mode.as_str()));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
        }
        fs::write(&path, b"").map_err(|error| PulseError::io(&path, error))
    }

    /// Test-only: remove a failpoint marker (see [`Self::check_failpoint`]).
    pub fn disarm_failpoint(&self, name: &str, mode: FailpointMode) -> Result<()> {
        let path = self
            .root
            .join("failpoints")
            .join(format!("{name}.{}", mode.as_str()));
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(PulseError::io(&path, error)),
        }
    }
}

pub struct DaemonOwnerGuard {
    path: PathBuf,
    file: File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailpointMode {
    Panic,
    Error,
}

impl FailpointMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Panic => "panic",
            Self::Error => "error",
        }
    }
}

pub struct IdempotencyGuard {
    path: PathBuf,
    file: File,
}

impl Drop for IdempotencyGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
        let _ = &self.path;
    }
}

impl Drop for DaemonOwnerGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
        let _ = &self.path;
    }
}

fn open_lock(path: &Path) -> Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|error| PulseError::io(path, error))
}

fn lock_with_timeout(file: &File, path: &Path, timeout: Duration) -> Result<()> {
    let start = Instant::now();
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(()),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) && start.elapsed() < timeout =>
            {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Err(PulseError::LockTimeout {
                    lock_path: path.to_path_buf(),
                    timeout,
                });
            }
            Err(error) => return Err(PulseError::io(path, error)),
        }
    }
}
