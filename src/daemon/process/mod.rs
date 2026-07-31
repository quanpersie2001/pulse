//! Daemon-owned provider/helper process ledger and native process control.

mod native;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::{PulseError, Result};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManagedProcessState {
    Starting,
    Running,
    InterruptRequested,
    Exited,
    StaleNeedsOperator,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagedProcessRecord {
    pub schema_version: u32,
    pub process_id: String,
    pub owner_kind: String,
    pub owner_id: String,
    pub provider_id: String,
    pub executable: String,
    pub argv_fingerprint: String,
    pub pid: u32,
    pub process_group_id: Option<i64>,
    pub platform: String,
    pub platform_start_marker: String,
    pub state: ManagedProcessState,
    pub stdout_prefix_path: String,
    pub stdout_tail_path: String,
    pub stderr_prefix_path: String,
    pub stderr_tail_path: String,
    pub created_at: String,
    pub updated_at: String,
}

struct OwnedChild {
    child: Child,
    identity: native::NativeProcessIdentity,
    executable: PathBuf,
    stdout_lines: Receiver<String>,
}

#[derive(Default)]
pub struct ProcessOwner {
    children: Mutex<BTreeMap<String, Arc<Mutex<OwnedChild>>>>,
}

pub struct SpawnRequest<'a> {
    pub owner_kind: &'a str,
    pub owner_id: &'a str,
    pub provider_id: &'a str,
    pub executable: &'a Path,
    pub args: &'a [String],
    pub cwd: &'a Path,
    pub log_root: &'a Path,
    pub max_log_bytes: usize,
}

impl ProcessOwner {
    pub fn spawn(&self, request: SpawnRequest<'_>) -> Result<ManagedProcessRecord> {
        native::ensure_supported_platform()?;
        if request.max_log_bytes == 0 {
            return Err(PulseError::validation(
                "managed_log_limit_invalid",
                "managed log limit must be positive",
            ));
        }
        if !request.executable.is_absolute() {
            return Err(PulseError::validation(
                "provider_executable_not_absolute",
                "ProcessOwner requires a resolved absolute executable",
            ));
        }
        std::fs::create_dir_all(request.log_root)
            .map_err(|error| PulseError::io(request.log_root, error))?;
        let process_id = format!("proc_{}", ulid::Ulid::new());
        let stdout_prefix = request.log_root.join(format!("{process_id}.stdout.prefix"));
        let stdout_tail = request.log_root.join(format!("{process_id}.stdout.tail"));
        let stderr_prefix = request.log_root.join(format!("{process_id}.stderr.prefix"));
        let stderr_tail = request.log_root.join(format!("{process_id}.stderr.tail"));
        for path in [&stdout_prefix, &stdout_tail, &stderr_prefix, &stderr_tail] {
            crate::storage::atomic_write_private(path, &[])?;
        }
        let mut command = Command::new(request.executable);
        command
            .args(request.args)
            .current_dir(request.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = native::spawn_process_group(&mut command)?;
        let (identity, observed_executable) = {
            let deadline = Instant::now() + Duration::from_secs(1);
            let mut previous = None;
            loop {
                if child
                    .try_wait()
                    .map_err(|error| PulseError::io("<managed-process-wait>", error))?
                    .is_some()
                {
                    return Err(PulseError::validation(
                        "managed_process_exited_during_startup",
                        "managed process exited before its identity stabilized",
                    ));
                }
                let current = (
                    native::current_process_identity(child.id()),
                    native::current_process_executable(child.id()),
                );
                if let (Ok(identity), Ok(executable)) = current {
                    let stable = previous.as_ref().is_some_and(
                        |(previous_identity, previous_executable)| {
                            previous_identity == &identity && previous_executable == &executable
                        },
                    );
                    if stable {
                        break (identity, executable);
                    }
                    previous = Some((identity, executable));
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(PulseError::validation(
                        "managed_process_identity_unavailable",
                        "managed process identity did not stabilize after spawn",
                    ));
                }
                thread::sleep(Duration::from_millis(10));
            }
        };
        let stdout = child.stdout.take().ok_or_else(|| {
            PulseError::validation(
                "managed_log_unavailable",
                "child stdout pipe is unavailable",
            )
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            PulseError::validation(
                "managed_log_unavailable",
                "child stderr pipe is unavailable",
            )
        })?;
        let stdout_lines = spawn_provider_output_drain(
            stdout,
            stdout_prefix.clone(),
            stdout_tail.clone(),
            request.max_log_bytes,
        )?;
        spawn_bounded_log_drain(
            stderr,
            stderr_prefix.clone(),
            stderr_tail.clone(),
            request.max_log_bytes,
        )?;
        let now = chrono::Utc::now().to_rfc3339();
        let record = ManagedProcessRecord {
            schema_version: 1,
            process_id: process_id.clone(),
            owner_kind: request.owner_kind.to_string(),
            owner_id: request.owner_id.to_string(),
            provider_id: request.provider_id.to_string(),
            executable: observed_executable.to_string_lossy().to_string(),
            argv_fingerprint: crate::canonical_json::hash_serializable(&request.args.to_vec())?,
            pid: identity.pid,
            process_group_id: identity.process_group_id,
            platform: identity.platform.clone(),
            platform_start_marker: identity.platform_start_marker.clone(),
            state: ManagedProcessState::Running,
            stdout_prefix_path: stdout_prefix.to_string_lossy().to_string(),
            stdout_tail_path: stdout_tail.to_string_lossy().to_string(),
            stderr_prefix_path: stderr_prefix.to_string_lossy().to_string(),
            stderr_tail_path: stderr_tail.to_string_lossy().to_string(),
            created_at: now.clone(),
            updated_at: now,
        };
        self.children.lock().map_err(|_| lock_poisoned())?.insert(
            process_id,
            Arc::new(Mutex::new(OwnedChild {
                child,
                identity,
                executable: observed_executable,
                stdout_lines,
            })),
        );
        Ok(record)
    }

    pub fn send_line(&self, process_id: &str, message: &str) -> Result<()> {
        let owned = self.owned_child(process_id)?;
        let mut owned = owned.lock().map_err(|_| lock_poisoned())?;
        write_provider_line(&mut owned, message)
    }

    pub fn request_json(
        &self,
        process_id: &str,
        request_id: &str,
        message: &str,
        timeout: Duration,
    ) -> Result<(serde_json::Value, Vec<serde_json::Value>)> {
        let owned = self.owned_child(process_id)?;
        let mut owned = owned.lock().map_err(|_| lock_poisoned())?;
        write_provider_line(&mut owned, message)?;
        let deadline = Instant::now() + timeout;
        let mut notifications = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(PulseError::validation(
                    "provider_response_timeout",
                    format!("provider did not answer request {request_id:?}"),
                ));
            }
            let line = owned
                .stdout_lines
                .recv_timeout(remaining)
                .map_err(|error| {
                    PulseError::validation(
                        "provider_transport_closed",
                        format!("provider response channel closed: {error}"),
                    )
                })?;
            let value: serde_json::Value = serde_json::from_str(&line).map_err(|error| {
                PulseError::validation(
                    "provider_protocol_invalid",
                    format!("provider emitted invalid JSON: {error}"),
                )
            })?;
            if value.get("id").and_then(serde_json::Value::as_str) == Some(request_id) {
                if let Some(error) = value.get("error") {
                    return Err(PulseError::validation(
                        "provider_request_failed",
                        error.to_string(),
                    ));
                }
                return Ok((value, notifications));
            }
            notifications.push(value);
        }
    }

    pub fn drain_json(&self, process_id: &str) -> Result<Vec<serde_json::Value>> {
        let owned = {
            let children = self.children.lock().map_err(|_| lock_poisoned())?;
            children.get(process_id).cloned()
        };
        let Some(owned) = owned else {
            return Ok(Vec::new());
        };
        let owned = owned.lock().map_err(|_| lock_poisoned())?;
        let mut events = Vec::new();
        loop {
            match owned.stdout_lines.try_recv() {
                Ok(line) => {
                    let value = serde_json::from_str(&line).map_err(|error| {
                        PulseError::validation(
                            "provider_protocol_invalid",
                            format!("provider emitted invalid JSON: {error}"),
                        )
                    })?;
                    events.push(value);
                }
                Err(TryRecvError::Empty) => return Ok(events),
                Err(TryRecvError::Disconnected) => return Ok(events),
            }
        }
    }

    pub fn terminate(&self, process_id: &str) -> Result<()> {
        let owned = self
            .children
            .lock()
            .map_err(|_| lock_poisoned())?
            .get(process_id)
            .cloned();
        let Some(owned) = owned else {
            return Err(PulseError::validation(
                "managed_process_not_owned",
                "daemon does not own a live handle for the managed process",
            ));
        };
        {
            let mut child = owned.lock().map_err(|_| lock_poisoned())?;
            if child
                .child
                .try_wait()
                .map_err(|error| PulseError::io("<managed-process-wait>", error))?
                .is_none()
            {
                if !native::process_identity_matches(&child.identity, &child.executable)? {
                    let observed = native::current_process_executable(child.identity.pid)
                        .map(|path| path.to_string_lossy().into_owned())
                        .unwrap_or_else(|error| format!("<unavailable: {}>", error.code()));
                    return Err(PulseError::validation(
                        "managed_process_identity_mismatch",
                        format!(
                            "recorded PID/start/process-group/executable identity no longer matches; expected executable {}, observed {observed}; refusing cancellation",
                            child.executable.display()
                        ),
                    ));
                }
                native::terminate_process_group(&child.identity, &child.executable)?;
                let deadline = Instant::now() + Duration::from_secs(2);
                loop {
                    if child
                        .child
                        .try_wait()
                        .map_err(|error| PulseError::io("<managed-process-wait>", error))?
                        .is_some()
                    {
                        break;
                    }
                    if Instant::now() >= deadline {
                        native::force_terminate_process_group(&child.identity, &child.executable)?;
                        child
                            .child
                            .wait()
                            .map_err(|error| PulseError::io("<managed-process-wait>", error))?;
                        break;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
            }
        }
        let mut children = self.children.lock().map_err(|_| lock_poisoned())?;
        if children
            .get(process_id)
            .is_some_and(|current| Arc::ptr_eq(current, &owned))
        {
            children.remove(process_id);
        }
        Ok(())
    }

    pub fn is_alive(&self, process_id: &str) -> Result<bool> {
        let owned = self.owned_child(process_id)?;
        let mut owned = owned.lock().map_err(|_| lock_poisoned())?;
        Ok(owned
            .child
            .try_wait()
            .map_err(|error| PulseError::io("<managed-process-wait>", error))?
            .is_none())
    }

    fn owned_child(&self, process_id: &str) -> Result<Arc<Mutex<OwnedChild>>> {
        self.children
            .lock()
            .map_err(|_| lock_poisoned())?
            .get(process_id)
            .cloned()
            .ok_or_else(|| PulseError::NotFound {
                subject: format!("managed process {process_id}"),
            })
    }

    pub fn classify_recovery(&self, record: &ManagedProcessRecord) -> Result<ManagedProcessState> {
        let identity = native::NativeProcessIdentity {
            pid: record.pid,
            process_group_id: record.process_group_id,
            platform: record.platform.clone(),
            platform_start_marker: record.platform_start_marker.clone(),
            identity_status: "recorded".to_string(),
        };
        match native::process_identity_status(&identity, Path::new(&record.executable)) {
            Ok(native::ProcessIdentityStatus::Match) => Ok(ManagedProcessState::StaleNeedsOperator),
            Ok(native::ProcessIdentityStatus::Absent) => Ok(ManagedProcessState::Exited),
            Ok(native::ProcessIdentityStatus::Mismatch) | Err(_) => {
                Ok(ManagedProcessState::StaleNeedsOperator)
            }
        }
    }

    pub fn terminate_record(&self, record: &ManagedProcessRecord) -> Result<()> {
        let identity = native::NativeProcessIdentity {
            pid: record.pid,
            process_group_id: record.process_group_id,
            platform: record.platform.clone(),
            platform_start_marker: record.platform_start_marker.clone(),
            identity_status: "recorded".to_string(),
        };
        let executable = Path::new(&record.executable);
        match native::process_identity_status(&identity, executable)? {
            native::ProcessIdentityStatus::Absent => return Ok(()),
            native::ProcessIdentityStatus::Mismatch => {
                return Err(PulseError::validation(
                    "managed_process_identity_mismatch",
                    "recorded PID/start/process-group identity no longer matches; refusing cancellation",
                ))
            }
            native::ProcessIdentityStatus::Match => {}
        }
        native::terminate_process_group(&identity, executable)?;
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            match native::process_identity_status(&identity, executable)? {
                native::ProcessIdentityStatus::Absent => return Ok(()),
                native::ProcessIdentityStatus::Mismatch => {
                    return Err(PulseError::validation(
                        "managed_process_identity_mismatch",
                        "process identity changed while waiting for termination",
                    ))
                }
                native::ProcessIdentityStatus::Match => {}
            }
            thread::sleep(Duration::from_millis(20));
        }
        native::force_terminate_process_group(&identity, executable)
    }
}

fn write_provider_line(owned: &mut OwnedChild, message: &str) -> Result<()> {
    let stdin = owned.child.stdin.as_mut().ok_or_else(|| {
        PulseError::validation(
            "provider_transport_closed",
            "managed process stdin is unavailable",
        )
    })?;
    stdin
        .write_all(message.as_bytes())
        .and_then(|_| stdin.write_all(b"\n"))
        .and_then(|_| stdin.flush())
        .map_err(|error| PulseError::io("<provider-stdin>", error))
}

fn spawn_bounded_log_drain<R: Read + Send + 'static>(
    mut reader: R,
    prefix_path: PathBuf,
    tail_path: PathBuf,
    max_bytes: usize,
) -> Result<()> {
    if max_bytes == 0 {
        return Err(PulseError::validation(
            "managed_log_limit_invalid",
            "managed log limit must be positive",
        ));
    }
    thread::spawn(move || {
        let prefix_limit = max_bytes / 2;
        let tail_limit = max_bytes - prefix_limit;
        let mut prefix = Vec::with_capacity(prefix_limit);
        let mut tail = Vec::with_capacity(tail_limit);
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 8192];
        loop {
            let count = match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => count,
                Err(_) => break,
            };
            hasher.update(&buffer[..count]);
            let take = prefix_limit.saturating_sub(prefix.len()).min(count);
            prefix.extend_from_slice(&buffer[..take]);
            tail.extend_from_slice(&buffer[..count]);
            if tail.len() > tail_limit {
                tail.drain(..tail.len() - tail_limit);
            }
            let _ = crate::storage::atomic_write_private(&prefix_path, &prefix);
            let _ = crate::storage::atomic_write_private(&tail_path, &tail);
        }
        let _ = hasher.finalize();
    });
    Ok(())
}

fn spawn_provider_output_drain<R: Read + Send + 'static>(
    reader: R,
    prefix_path: PathBuf,
    tail_path: PathBuf,
    max_bytes: usize,
) -> Result<Receiver<String>> {
    if max_bytes == 0 {
        return Err(PulseError::validation(
            "managed_log_limit_invalid",
            "managed log limit must be positive",
        ));
    }
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let prefix_limit = max_bytes / 2;
        let tail_limit = max_bytes - prefix_limit;
        let mut prefix = Vec::with_capacity(prefix_limit);
        let mut tail = Vec::with_capacity(tail_limit);
        let mut reader = BufReader::new(reader);
        let mut line = Vec::new();
        loop {
            line.clear();
            let count = match reader.read_until(b'\n', &mut line) {
                Ok(0) => break,
                Ok(count) => count,
                Err(_) => break,
            };
            let take = prefix_limit.saturating_sub(prefix.len()).min(count);
            prefix.extend_from_slice(&line[..take]);
            tail.extend_from_slice(&line[..count]);
            if tail.len() > tail_limit {
                tail.drain(..tail.len() - tail_limit);
            }
            let _ = crate::storage::atomic_write_private(&prefix_path, &prefix);
            let _ = crate::storage::atomic_write_private(&tail_path, &tail);
            if let Ok(text) = std::str::from_utf8(&line) {
                let _ = sender.send(text.trim_end_matches(['\r', '\n']).to_string());
            }
        }
    });
    Ok(receiver)
}

fn lock_poisoned() -> PulseError {
    PulseError::validation(
        "daemon_process_lock_poisoned",
        "process owner lock was poisoned",
    )
}

pub fn resolve_executable(program: &str) -> Result<PathBuf> {
    let candidate = PathBuf::from(program);
    if candidate.is_absolute() {
        return candidate
            .canonicalize()
            .map_err(|error| PulseError::io(&candidate, error));
    }
    let path = std::env::var_os("PATH")
        .ok_or_else(|| PulseError::validation("provider_unavailable", "PATH is unavailable"))?;
    for directory in std::env::split_paths(&path) {
        let executable = directory.join(program);
        if executable.is_file() {
            return executable
                .canonicalize()
                .map_err(|error| PulseError::io(&executable, error));
        }
        #[cfg(windows)]
        {
            let executable = directory.join(format!("{program}.exe"));
            if executable.is_file() {
                return executable
                    .canonicalize()
                    .map_err(|error| PulseError::io(&executable, error));
            }
        }
    }
    Err(PulseError::validation(
        "provider_unavailable",
        format!("provider executable {program:?} was not found on PATH"),
    ))
}
