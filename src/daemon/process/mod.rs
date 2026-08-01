//! Daemon-owned provider/helper process ledger and native process control.

mod native;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::{PulseError, Result};

const PROVIDER_OUTPUT_QUEUE_CAPACITY: usize = 64;
const PROVIDER_MAX_LINE_BYTES: usize = 64 * 1024;
const PROVIDER_MAX_RETURNED_NOTIFICATIONS: usize = PROVIDER_OUTPUT_QUEUE_CAPACITY;

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

const MAX_CAPTURED_LOG_FRAGMENT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CapturedProcessLogs {
    pub stdout_prefix: String,
    pub stdout_tail: String,
    pub stderr_prefix: String,
    pub stderr_tail: String,
}

/// Read only the bounded prefix/tail fragments owned by the daemon process
/// record. This is intentionally not a Core run-store or arbitrary path API.
pub fn read_captured_logs(record: &ManagedProcessRecord) -> Result<CapturedProcessLogs> {
    Ok(CapturedProcessLogs {
        stdout_prefix: read_log_fragment(Path::new(&record.stdout_prefix_path), false)?,
        stdout_tail: read_log_fragment(Path::new(&record.stdout_tail_path), true)?,
        stderr_prefix: read_log_fragment(Path::new(&record.stderr_prefix_path), false)?,
        stderr_tail: read_log_fragment(Path::new(&record.stderr_tail_path), true)?,
    })
}

fn read_log_fragment(path: &Path, tail: bool) -> Result<String> {
    let mut file = std::fs::File::open(path).map_err(|error| PulseError::io(path, error))?;
    let length = file
        .metadata()
        .map_err(|error| PulseError::io(path, error))?
        .len();
    let start = if tail {
        length.saturating_sub(MAX_CAPTURED_LOG_FRAGMENT_BYTES as u64)
    } else {
        0
    };
    file.seek(SeekFrom::Start(start))
        .map_err(|error| PulseError::io(path, error))?;
    let amount = (length - start).min(MAX_CAPTURED_LOG_FRAGMENT_BYTES as u64) as usize;
    let mut bytes = vec![0; amount];
    file.read_exact(&mut bytes)
        .map_err(|error| PulseError::io(path, error))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

struct OwnedChild {
    child: Child,
    identity: native::NativeProcessIdentity,
    executable: PathBuf,
    output: Arc<ProviderOutputDispatcher>,
    output_reader: Option<thread::JoinHandle<()>>,
}

struct ProviderOutputDispatcher {
    state: Mutex<ProviderOutputState>,
    wake: Condvar,
}

struct ProviderOutputState {
    responses: VecDeque<String>,
    active_waiters: BTreeSet<String>,
    terminal_notifications: VecDeque<String>,
    control_notifications: VecDeque<String>,
    delta_notifications: VecDeque<String>,
    server_requests: VecDeque<ProviderServerRequest>,
    dropped_notifications: u64,
    closed: bool,
}

#[derive(Debug, Clone)]
struct ProviderServerRequest {
    id: serde_json::Value,
    method: String,
}

struct ProviderOutputBatch {
    response: Option<String>,
    notifications: Vec<String>,
    server_requests: Vec<ProviderServerRequest>,
    dropped_notifications: u64,
}

struct ActiveWaiter {
    output: Arc<ProviderOutputDispatcher>,
    request_id: String,
}

impl Drop for ActiveWaiter {
    fn drop(&mut self) {
        if let Ok(mut state) = self.output.state.lock() {
            state.active_waiters.remove(&self.request_id);
            self.output.wake.notify_all();
        }
    }
}

impl ProviderOutputDispatcher {
    fn new() -> Self {
        Self {
            state: Mutex::new(ProviderOutputState {
                responses: VecDeque::new(),
                active_waiters: BTreeSet::new(),
                terminal_notifications: VecDeque::new(),
                control_notifications: VecDeque::new(),
                delta_notifications: VecDeque::new(),
                server_requests: VecDeque::new(),
                dropped_notifications: 0,
                closed: false,
            }),
            wake: Condvar::new(),
        }
    }

    fn dispatch(&self, line: String) {
        let classification = classify_provider_line(&line);
        let mut state = self.state.lock().expect("provider output dispatcher lock");
        match classification {
            ProviderLineClass::Response => {
                if !state.closed {
                    // A response without a currently waiting request cannot
                    // be correlated indefinitely. Bound this queue by
                    // discarding the oldest unmatched response so shutdown
                    // cannot strand the stdout reader behind stale replies.
                    if state.responses.len() >= PROVIDER_OUTPUT_QUEUE_CAPACITY {
                        let evict = state.responses.iter().position(|response| {
                            match response_request_id(response) {
                                Some(request_id) => !state.active_waiters.contains(&request_id),
                                None => true,
                            }
                        });
                        if let Some(index) = evict {
                            state.responses.remove(index);
                        } else {
                            return;
                        }
                    }
                    state.responses.push_back(line);
                }
            }
            ProviderLineClass::ServerRequest(request) => {
                while state.server_requests.len() >= PROVIDER_OUTPUT_QUEUE_CAPACITY && !state.closed
                {
                    state = self
                        .wake
                        .wait(state)
                        .expect("provider output dispatcher lock");
                }
                if !state.closed {
                    state.server_requests.push_back(request);
                }
            }
            ProviderLineClass::Notification {
                priority: NotificationPriority::Terminal,
            } => {
                if state.terminal_notifications.len() >= PROVIDER_OUTPUT_QUEUE_CAPACITY {
                    state.terminal_notifications.pop_front();
                    state.dropped_notifications = state.dropped_notifications.saturating_add(1);
                }
                if !state.closed {
                    state.terminal_notifications.push_back(line);
                }
            }
            ProviderLineClass::Notification {
                priority: NotificationPriority::Control,
            } => {
                if state.control_notifications.len() >= PROVIDER_OUTPUT_QUEUE_CAPACITY {
                    state.control_notifications.pop_front();
                    state.dropped_notifications = state.dropped_notifications.saturating_add(1);
                }
                if !state.closed {
                    state.control_notifications.push_back(line);
                }
            }
            ProviderLineClass::Notification {
                priority: NotificationPriority::Delta,
            } => {
                if state.delta_notifications.len() < PROVIDER_OUTPUT_QUEUE_CAPACITY {
                    state.delta_notifications.push_back(line);
                } else {
                    state.dropped_notifications = state.dropped_notifications.saturating_add(1);
                }
            }
        }
        self.wake.notify_all();
    }

    fn register_waiter(self: &Arc<Self>, request_id: &str) -> Result<ActiveWaiter> {
        let mut state = self.state.lock().map_err(|_| lock_poisoned())?;
        state.active_waiters.insert(request_id.to_string());
        Ok(ActiveWaiter {
            output: Arc::clone(self),
            request_id: request_id.to_string(),
        })
    }

    fn close(&self) {
        let mut state = self.state.lock().expect("provider output dispatcher lock");
        state.closed = true;
        self.wake.notify_all();
    }

    fn take(&self, request_id: &str, timeout: Duration) -> Result<ProviderOutputBatch> {
        let deadline = Instant::now() + timeout;
        let mut state = self.state.lock().map_err(|_| lock_poisoned())?;
        let mut pending_notifications = Vec::new();
        let mut pending_dropped_notifications: u64 = 0;
        loop {
            let response = state
                .responses
                .iter()
                .position(|line| response_matches_request(line, request_id))
                .and_then(|index| state.responses.remove(index));
            for line in state.terminal_notifications.drain(..) {
                let (accepted, evicted) =
                    append_bounded_notification_line_with_loss(&mut pending_notifications, line);
                if !accepted || evicted {
                    pending_dropped_notifications = pending_dropped_notifications.saturating_add(1);
                }
            }
            for line in state.control_notifications.drain(..) {
                let (accepted, evicted) =
                    append_bounded_notification_line_with_loss(&mut pending_notifications, line);
                if !accepted || evicted {
                    pending_dropped_notifications = pending_dropped_notifications.saturating_add(1);
                }
            }
            for line in state.delta_notifications.drain(..) {
                let (accepted, evicted) =
                    append_bounded_notification_line_with_loss(&mut pending_notifications, line);
                if !accepted || evicted {
                    pending_dropped_notifications = pending_dropped_notifications.saturating_add(1);
                }
            }
            let server_requests: Vec<_> = state.server_requests.drain(..).collect();
            pending_dropped_notifications = pending_dropped_notifications
                .saturating_add(std::mem::take(&mut state.dropped_notifications));
            if response.is_some() || !server_requests.is_empty() {
                self.wake.notify_all();
                return Ok(ProviderOutputBatch {
                    response,
                    notifications: pending_notifications,
                    server_requests,
                    dropped_notifications: pending_dropped_notifications,
                });
            }
            if state.closed && state.responses.is_empty() {
                return Err(PulseError::validation(
                    "provider_transport_closed",
                    "provider response channel closed",
                ));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(PulseError::validation(
                    "provider_response_timeout",
                    format!("provider did not answer request {request_id:?}"),
                ));
            }
            let (next_state, timed_out) = self
                .wake
                .wait_timeout(state, remaining)
                .map_err(|_| lock_poisoned())?;
            state = next_state;
            if timed_out.timed_out() {
                return Err(PulseError::validation(
                    "provider_response_timeout",
                    format!("provider did not answer request {request_id:?}"),
                ));
            }
        }
    }

    fn drain(&self) -> Result<ProviderOutputBatch> {
        let mut state = self.state.lock().map_err(|_| lock_poisoned())?;
        let mut notifications = Vec::new();
        let mut dropped_notifications: u64 = 0;
        for line in state.terminal_notifications.drain(..) {
            let (accepted, evicted) =
                append_bounded_notification_line_with_loss(&mut notifications, line);
            if !accepted || evicted {
                dropped_notifications = dropped_notifications.saturating_add(1);
            }
        }
        for line in state.control_notifications.drain(..) {
            let (accepted, evicted) =
                append_bounded_notification_line_with_loss(&mut notifications, line);
            if !accepted || evicted {
                dropped_notifications = dropped_notifications.saturating_add(1);
            }
        }
        for line in state.delta_notifications.drain(..) {
            let (accepted, evicted) =
                append_bounded_notification_line_with_loss(&mut notifications, line);
            if !accepted || evicted {
                dropped_notifications = dropped_notifications.saturating_add(1);
            }
        }
        let server_requests = state.server_requests.drain(..).collect();
        dropped_notifications =
            dropped_notifications.saturating_add(std::mem::take(&mut state.dropped_notifications));
        self.wake.notify_all();
        Ok(ProviderOutputBatch {
            response: None,
            notifications,
            server_requests,
            dropped_notifications,
        })
    }

    fn requeue_front(&self, lines: &[String]) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| lock_poisoned())?;
        for line in lines.iter().rev() {
            match classify_provider_line(line) {
                ProviderLineClass::Notification {
                    priority: NotificationPriority::Terminal,
                } => {
                    if state.terminal_notifications.len() >= PROVIDER_OUTPUT_QUEUE_CAPACITY {
                        state.terminal_notifications.pop_back();
                        state.dropped_notifications = state.dropped_notifications.saturating_add(1);
                    }
                    state.terminal_notifications.push_front(line.clone());
                }
                ProviderLineClass::Notification {
                    priority: NotificationPriority::Control,
                } => {
                    if state.control_notifications.len() >= PROVIDER_OUTPUT_QUEUE_CAPACITY {
                        state.control_notifications.pop_back();
                        state.dropped_notifications = state.dropped_notifications.saturating_add(1);
                    }
                    state.control_notifications.push_front(line.clone());
                }
                ProviderLineClass::Notification {
                    priority: NotificationPriority::Delta,
                } => {
                    if state.delta_notifications.len() >= PROVIDER_OUTPUT_QUEUE_CAPACITY {
                        state.delta_notifications.pop_back();
                        state.dropped_notifications = state.dropped_notifications.saturating_add(1);
                    }
                    state.delta_notifications.push_front(line.clone());
                }
                ProviderLineClass::Response | ProviderLineClass::ServerRequest(_) => {}
            }
        }
        self.wake.notify_all();
        Ok(())
    }
}

enum ProviderLineClass {
    Response,
    Notification { priority: NotificationPriority },
    ServerRequest(ProviderServerRequest),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotificationPriority {
    Terminal,
    Control,
    Delta,
}

fn classify_provider_line(line: &str) -> ProviderLineClass {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return ProviderLineClass::Notification {
            priority: NotificationPriority::Control,
        };
    };
    let Some(object) = value.as_object() else {
        return ProviderLineClass::Notification {
            priority: NotificationPriority::Control,
        };
    };
    let has_id = object.get("id").is_some();
    let method = object.get("method").and_then(serde_json::Value::as_str);
    match (has_id, method) {
        (true, Some(method)) => ProviderLineClass::ServerRequest(ProviderServerRequest {
            id: object.get("id").cloned().unwrap_or(serde_json::Value::Null),
            method: method.to_string(),
        }),
        (true, None) => ProviderLineClass::Response,
        (false, Some(method)) => ProviderLineClass::Notification {
            priority: notification_priority(method),
        },
        (false, None) => ProviderLineClass::Notification {
            priority: NotificationPriority::Delta,
        },
    }
}

fn response_matches_request(line: &str, request_id: &str) -> bool {
    response_request_id(line).is_some_and(|id| id == request_id)
}

fn response_request_id(line: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(line)
        .ok()
        .and_then(|value| {
            value
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
}

fn notification_priority(method: &str) -> NotificationPriority {
    if matches!(
        method,
        "turn/completed" | "turn/failed" | "turn/aborted" | "turn/cancelled" | "error"
    ) {
        NotificationPriority::Terminal
    } else if method.starts_with("approval/") || method.starts_with("thread/") {
        NotificationPriority::Control
    } else {
        NotificationPriority::Delta
    }
}

fn notification_priority_for_value(value: &serde_json::Value) -> NotificationPriority {
    value
        .get("method")
        .and_then(serde_json::Value::as_str)
        .map(notification_priority)
        .unwrap_or(NotificationPriority::Delta)
}

fn append_bounded_notification_line_with_loss(
    notifications: &mut Vec<String>,
    line: String,
) -> (bool, bool) {
    let priority = serde_json::from_str::<serde_json::Value>(&line)
        .map(|value| notification_priority_for_value(&value))
        .unwrap_or(NotificationPriority::Control);
    let capacity = PROVIDER_MAX_RETURNED_NOTIFICATIONS;
    if notifications.len() < capacity {
        notifications.push(line);
        return (true, false);
    }
    let evict = |notifications: &mut Vec<String>, priorities: &[NotificationPriority]| {
        notifications.iter().position(|existing| {
            let existing_priority = serde_json::from_str::<serde_json::Value>(existing)
                .map(|value| notification_priority_for_value(&value))
                .unwrap_or(NotificationPriority::Control);
            priorities.contains(&existing_priority)
        })
    };
    let index = match priority {
        NotificationPriority::Terminal => evict(
            notifications,
            &[NotificationPriority::Delta, NotificationPriority::Control],
        )
        .or_else(|| evict(notifications, &[NotificationPriority::Terminal])),
        NotificationPriority::Control => evict(notifications, &[NotificationPriority::Delta])
            .or_else(|| evict(notifications, &[NotificationPriority::Control])),
        NotificationPriority::Delta => None,
    };
    if let Some(index) = index {
        notifications.remove(index);
        notifications.push(line);
        (true, true)
    } else {
        (false, false)
    }
}

fn server_request_rejection(request: &ProviderServerRequest) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": request.id,
        "error": {
            "code": -32601,
            "message": format!("unsupported provider server request method {:?}", request.method),
        }
    })
    .to_string()
}

#[derive(Default, Clone)]
pub struct ProcessOwner {
    children: Arc<Mutex<BTreeMap<String, Arc<Mutex<OwnedChild>>>>>,
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
        let (output, output_reader) = spawn_provider_output_drain(
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
                output,
                output_reader: Some(output_reader),
            })),
        );
        Ok(record)
    }

    pub fn send_line(&self, process_id: &str, message: &str) -> Result<()> {
        let owned = self.owned_child_for_termination(process_id)?;
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
        let owned = self.owned_child_for_termination(process_id)?;
        let owned = owned.lock().map_err(|_| lock_poisoned())?;
        let mut child = owned;
        let _active_waiter = child.output.register_waiter(request_id)?;
        write_provider_line(&mut child, message)?;
        let deadline = Instant::now() + timeout;
        let mut notifications = Vec::new();
        let mut dropped_notifications: u64 = 0;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let batch = child.output.take(request_id, remaining)?;
            for server_request in &batch.server_requests {
                write_provider_line(&mut child, &server_request_rejection(server_request))?;
            }
            for line in batch.notifications {
                let (accepted, evicted) = append_notification_with_loss(&mut notifications, &line)?;
                if !accepted || evicted {
                    dropped_notifications = dropped_notifications.saturating_add(1);
                }
            }
            dropped_notifications =
                dropped_notifications.saturating_add(batch.dropped_notifications);
            let Some(line) = batch.response else {
                continue;
            };
            let value: serde_json::Value = serde_json::from_str(&line).map_err(|error| {
                PulseError::validation(
                    "provider_protocol_invalid_after_transport",
                    format!("provider emitted invalid JSON: {error}"),
                )
            })?;
            if response_matches_request(&line, request_id) {
                if let Some(error) = value.get("error") {
                    return Err(PulseError::validation(
                        "provider_request_failed",
                        error.to_string(),
                    ));
                }
                append_notification_loss(&mut notifications, dropped_notifications);
                return Ok((value, notifications));
            }
            let (accepted, evicted) =
                append_notification_value_with_loss(&mut notifications, value);
            if !accepted || evicted {
                dropped_notifications = dropped_notifications.saturating_add(1);
            }
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
        let mut owned = owned.lock().map_err(|_| lock_poisoned())?;
        let batch = owned.output.drain()?;
        let mut events = Vec::new();
        let mut dropped_notifications = batch.dropped_notifications;
        for server_request in &batch.server_requests {
            write_provider_line(&mut owned, &server_request_rejection(server_request))?;
        }
        for line in batch.notifications {
            if let Ok((accepted, evicted)) = append_notification_with_loss(&mut events, &line) {
                if !accepted || evicted {
                    dropped_notifications = dropped_notifications.saturating_add(1);
                }
            }
        }
        append_notification_loss(&mut events, dropped_notifications);
        Ok(events)
    }

    pub fn requeue_json(&self, process_id: &str, events: &[serde_json::Value]) -> Result<()> {
        let owned = {
            let children = self.children.lock().map_err(|_| lock_poisoned())?;
            children.get(process_id).cloned()
        };
        let Some(owned) = owned else {
            return Err(PulseError::validation(
                "managed_process_not_owned",
                format!("managed process {process_id} is not owned by this daemon"),
            ));
        };
        let lines = events
            .iter()
            .map(serde_json::to_string)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(PulseError::from)?;
        let result = owned
            .lock()
            .map_err(|_| lock_poisoned())?
            .output
            .requeue_front(&lines);
        result
    }

    pub fn terminate(&self, process_id: &str) -> Result<()> {
        let owned = self.owned_child_for_termination(process_id)?;
        self.terminate_owned(&owned)?;
        self.release_handle_for(process_id, &owned)
    }

    pub fn terminate_and_drain(&self, process_id: &str) -> Result<Vec<serde_json::Value>> {
        let owned = self.owned_child_for_termination(process_id)?;
        self.terminate_owned(&owned)?;
        let mut events = self.drain_json(process_id)?;
        self.wait_output_quiescence(&owned)?;
        events.extend(self.drain_json(process_id)?);
        Ok(events)
    }

    pub fn release_handle(&self, process_id: &str) -> Result<()> {
        let owned = self.owned_child(process_id)?;
        self.release_handle_for(process_id, &owned)
    }

    fn release_handle_for(&self, process_id: &str, owned: &Arc<Mutex<OwnedChild>>) -> Result<()> {
        let mut children = self.children.lock().map_err(|_| lock_poisoned())?;
        if children
            .get(process_id)
            .is_some_and(|current| Arc::ptr_eq(current, owned))
        {
            children.remove(process_id);
        }
        Ok(())
    }

    fn terminate_owned(&self, owned: &Arc<Mutex<OwnedChild>>) -> Result<()> {
        let mut child = owned.lock().map_err(|_| lock_poisoned())?;
        if child
            .child
            .try_wait()
            .map_err(|error| PulseError::io("<managed-process-wait>", error))?
            .is_none()
        {
            if !native::process_identity_matches(&child.identity, &child.executable)? {
                return Err(PulseError::validation(
                    "managed_process_identity_mismatch",
                    "recorded PID/start/process-group identity no longer matches; the managed \
                     process may have exited and its pid may have been reused; refusing \
                     cancellation",
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
        Ok(())
    }

    fn wait_output_quiescence(&self, owned: &Arc<Mutex<OwnedChild>>) -> Result<()> {
        let reader = owned
            .lock()
            .map_err(|_| lock_poisoned())?
            .output_reader
            .take();
        if let Some(reader) = reader {
            reader.join().map_err(|_| {
                PulseError::validation(
                    "provider_output_reader_failed",
                    "provider stdout reader did not terminate cleanly",
                )
            })?;
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

    fn owned_child_for_termination(&self, process_id: &str) -> Result<Arc<Mutex<OwnedChild>>> {
        self.children
            .lock()
            .map_err(|_| lock_poisoned())?
            .get(process_id)
            .cloned()
            .ok_or_else(|| {
                PulseError::validation(
                    "managed_process_not_owned",
                    "daemon does not own a live handle for the managed process",
                )
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
) -> Result<(Arc<ProviderOutputDispatcher>, thread::JoinHandle<()>)> {
    if max_bytes == 0 {
        return Err(PulseError::validation(
            "managed_log_limit_invalid",
            "managed log limit must be positive",
        ));
    }
    let output = Arc::new(ProviderOutputDispatcher::new());
    let drain_output = Arc::clone(&output);
    let reader_handle = thread::spawn(move || {
        let prefix_limit = max_bytes / 2;
        let tail_limit = max_bytes - prefix_limit;
        let mut prefix = Vec::with_capacity(prefix_limit);
        let mut tail = Vec::with_capacity(tail_limit);
        let mut reader = BufReader::new(reader);
        let mut line = Vec::new();
        let mut lines_since_flush = 0;
        loop {
            line.clear();
            let oversized = match read_provider_line(
                &mut reader,
                &mut line,
                &mut prefix,
                &mut tail,
                prefix_limit,
                tail_limit,
            ) {
                Ok(Some(oversized)) => oversized,
                Ok(None) => break,
                Err(_) => break,
            };
            lines_since_flush += 1;
            if lines_since_flush >= 16 {
                let _ = crate::storage::atomic_write_private(&prefix_path, &prefix);
                let _ = crate::storage::atomic_write_private(&tail_path, &tail);
                lines_since_flush = 0;
            }
            if oversized {
                drain_output.dispatch(
                    serde_json::json!({
                        "method": "pulse/provider_output_line_oversized",
                        "params": {"max_bytes": PROVIDER_MAX_LINE_BYTES}
                    })
                    .to_string(),
                );
            } else if let Ok(text) = std::str::from_utf8(&line) {
                drain_output.dispatch(text.trim_end_matches(['\r', '\n']).to_string());
            }
        }
        let _ = crate::storage::atomic_write_private(&prefix_path, &prefix);
        let _ = crate::storage::atomic_write_private(&tail_path, &tail);
        drain_output.close();
    });
    Ok((output, reader_handle))
}

fn read_provider_line<R: BufRead>(
    reader: &mut R,
    line: &mut Vec<u8>,
    prefix: &mut Vec<u8>,
    tail: &mut Vec<u8>,
    prefix_limit: usize,
    tail_limit: usize,
) -> std::io::Result<Option<bool>> {
    let mut oversized = false;
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok((!line.is_empty()).then_some(oversized));
        }
        let consumed = buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(buffer.len(), |position| position + 1);
        let chunk = &buffer[..consumed];
        let prefix_take = prefix_limit.saturating_sub(prefix.len()).min(chunk.len());
        prefix.extend_from_slice(&chunk[..prefix_take]);
        tail.extend_from_slice(chunk);
        if tail.len() > tail_limit {
            tail.drain(..tail.len() - tail_limit);
        }
        let line_take = PROVIDER_MAX_LINE_BYTES
            .saturating_sub(line.len())
            .min(chunk.len());
        line.extend_from_slice(&chunk[..line_take]);
        if line_take < chunk.len() {
            oversized = true;
        }
        let complete = chunk.last() == Some(&b'\n');
        reader.consume(consumed);
        if complete {
            return Ok(Some(oversized));
        }
    }
}

fn append_notification_with_loss(
    notifications: &mut Vec<serde_json::Value>,
    line: &str,
) -> Result<(bool, bool)> {
    let value = serde_json::from_str(line).map_err(|error| {
        PulseError::validation(
            "provider_protocol_invalid",
            format!("provider emitted invalid JSON: {error}"),
        )
    })?;
    Ok(append_notification_value_with_loss(notifications, value))
}

fn append_notification_value_with_loss(
    notifications: &mut Vec<serde_json::Value>,
    value: serde_json::Value,
) -> (bool, bool) {
    let priority = notification_priority_for_value(&value);
    let capacity = if priority == NotificationPriority::Delta {
        PROVIDER_MAX_RETURNED_NOTIFICATIONS.saturating_sub(1)
    } else {
        PROVIDER_MAX_RETURNED_NOTIFICATIONS
    };
    if notifications.len() < capacity {
        notifications.push(value);
        return (true, false);
    }
    let candidates = match priority {
        NotificationPriority::Terminal => [
            NotificationPriority::Delta,
            NotificationPriority::Control,
            NotificationPriority::Terminal,
        ],
        NotificationPriority::Control => [
            NotificationPriority::Delta,
            NotificationPriority::Control,
            NotificationPriority::Control,
        ],
        NotificationPriority::Delta => [NotificationPriority::Delta; 3],
    };
    let index = candidates.iter().find_map(|candidate| {
        notifications
            .iter()
            .position(|existing| notification_priority_for_value(existing) == *candidate)
    });
    if let Some(index) = index {
        notifications.remove(index);
        notifications.push(value);
        (true, true)
    } else {
        (false, false)
    }
}

fn append_notification_loss(notifications: &mut Vec<serde_json::Value>, dropped: u64) {
    if dropped == 0 {
        return;
    }
    if notifications.len() >= PROVIDER_MAX_RETURNED_NOTIFICATIONS {
        if let Some(index) = notifications.iter().position(|existing| {
            matches!(
                notification_priority_for_value(existing),
                NotificationPriority::Delta | NotificationPriority::Control
            )
        }) {
            notifications.remove(index);
        }
    }
    if notifications.len() < PROVIDER_MAX_RETURNED_NOTIFICATIONS {
        notifications.push(serde_json::json!({
            "method": "pulse/notification_loss",
            "params": {"dropped": dropped}
        }));
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::time::Duration;

    #[test]
    fn provider_line_cap_bounds_line_buffer_and_preserves_log_tail_cap() {
        let mut reader = BufReader::new(Cursor::new(
            [vec![b'x'; PROVIDER_MAX_LINE_BYTES + 10], vec![b'\n']].concat(),
        ));
        let mut line = Vec::new();
        let mut prefix = Vec::new();
        let mut tail = Vec::new();
        let result = read_provider_line(&mut reader, &mut line, &mut prefix, &mut tail, 32, 48)
            .expect("read capped provider line");

        assert_eq!(result, Some(true));
        assert_eq!(line.len(), PROVIDER_MAX_LINE_BYTES);
        assert_eq!(prefix.len(), 32);
        assert_eq!(tail.len(), 48);
    }

    #[test]
    fn provider_dispatcher_bounds_notifications_without_poisoning_control() {
        let output = ProviderOutputDispatcher::new();
        for index in 0..(PROVIDER_OUTPUT_QUEUE_CAPACITY * 4) {
            output.dispatch(format!("{{\"notification\":{index}}}"));
        }
        output.dispatch("{\"jsonrpc\":\"2.0\",\"id\":\"control\",\"result\":{}}".to_string());

        let batch = output
            .take("control", Duration::from_secs(2))
            .expect("control response survives notification pressure");
        assert!(batch.response.is_some());
        assert!(batch.dropped_notifications > 0);
        assert!(batch.notifications.len() <= PROVIDER_OUTPUT_QUEUE_CAPACITY);
    }

    #[test]
    fn active_waiter_response_survives_inverse_response_pressure() {
        let output = Arc::new(ProviderOutputDispatcher::new());
        let _waiter = output
            .register_waiter("awaited")
            .expect("register active response waiter");
        output.dispatch(r#"{"jsonrpc":"2.0","id":"awaited","result":{}}"#.to_string());
        for index in 0..(PROVIDER_OUTPUT_QUEUE_CAPACITY * 2) {
            output.dispatch(format!(
                r#"{{"jsonrpc":"2.0","id":"unrelated-{index}","result":{{}}}}"#
            ));
        }
        let batch = output
            .take("awaited", Duration::from_secs(2))
            .expect("active waiter response survives inverse pressure");
        assert_eq!(
            batch
                .response
                .and_then(|response| serde_json::from_str::<serde_json::Value>(&response).ok())
                .and_then(|response| {
                    response
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                })
                .unwrap(),
            "awaited"
        );
    }

    #[test]
    fn provider_dispatcher_classifies_string_and_numeric_server_requests() {
        let output = ProviderOutputDispatcher::new();
        output.dispatch(
            r#"{"jsonrpc":"2.0","id":"approval-1","method":"item/commandExecution/approval","params":{}}"#
                .to_string(),
        );
        output.dispatch(
            r#"{"jsonrpc":"2.0","id":7,"method":"item/tool/request","params":{}}"#.to_string(),
        );
        output.dispatch(r#"{"jsonrpc":"2.0","id":"turn-1","result":{"ok":true}}"#.to_string());

        let first = output
            .take("turn-1", Duration::from_secs(2))
            .expect("server requests are surfaced while awaiting response");
        assert_eq!(first.server_requests.len(), 2);
        assert_eq!(first.server_requests[0].id, serde_json::json!("approval-1"));
        assert_eq!(first.server_requests[1].id, serde_json::json!(7));
        let rejection: serde_json::Value =
            serde_json::from_str(&server_request_rejection(&first.server_requests[1]))
                .expect("valid JSON-RPC rejection");
        assert_eq!(rejection["id"], serde_json::json!(7));
        assert_eq!(rejection["error"]["code"], serde_json::json!(-32601));
        assert!(
            first.response.is_some(),
            "original response remains available"
        );
    }

    #[test]
    fn terminal_notification_survives_delta_pressure_and_records_loss() {
        let output = ProviderOutputDispatcher::new();
        for index in 0..(PROVIDER_OUTPUT_QUEUE_CAPACITY * 4) {
            output.dispatch(format!(
                r#"{{"jsonrpc":"2.0","method":"item/delta","params":{{"index":{index}}}}}"#
            ));
        }
        output.dispatch(
            r#"{"jsonrpc":"2.0","method":"turn/completed","params":{"turn":{"id":"turn-1"}}}"#
                .to_string(),
        );
        let batch = output.drain().expect("drain provider output");
        assert!(batch
            .notifications
            .iter()
            .any(|line| line.contains("turn/completed")));
        assert!(batch.dropped_notifications > 0);
    }

    #[test]
    fn newest_terminal_survives_more_than_a_queue_of_control_notifications() {
        let output = ProviderOutputDispatcher::new();
        for index in 0..(PROVIDER_OUTPUT_QUEUE_CAPACITY * 2) {
            output.dispatch(format!(
                r#"{{"jsonrpc":"2.0","method":"thread/started","params":{{"index":{index}}}}}"#
            ));
        }
        output.dispatch(
            r#"{"jsonrpc":"2.0","method":"turn/completed","params":{"turn":{"id":"turn-control-pressure"}}}"#
                .to_string(),
        );
        output.dispatch(
            r#"{"jsonrpc":"2.0","id":"delayed","result":{"turn":{"id":"turn-control-pressure"}}}"#
                .to_string(),
        );
        let batch = output
            .take("delayed", Duration::from_secs(2))
            .expect("delayed response survives control pressure");
        assert!(batch.response.is_some());
        assert!(batch
            .notifications
            .iter()
            .any(|line| line.contains("turn/completed")));
        assert!(batch.dropped_notifications > 0);
    }

    #[test]
    fn take_bounds_pending_notifications_across_wakeups_and_emits_loss_marker() {
        let output = Arc::new(ProviderOutputDispatcher::new());
        let producer = Arc::clone(&output);
        let producer_thread = thread::spawn(move || {
            for index in 0..(PROVIDER_OUTPUT_QUEUE_CAPACITY * 3) {
                producer.dispatch(format!(
                    r#"{{"jsonrpc":"2.0","method":"thread/started","params":{{"index":{index}}}}}"#
                ));
                if index % 4 == 0 {
                    thread::sleep(Duration::from_millis(1));
                }
            }
            producer.dispatch(
                r#"{"jsonrpc":"2.0","method":"turn/completed","params":{"turn":{"id":"turn-wakeup"}}}"#
                    .to_string(),
            );
            thread::sleep(Duration::from_millis(20));
            producer
                .dispatch(r#"{"jsonrpc":"2.0","id":"delayed","result":{"ok":true}}"#.to_string());
        });
        let batch = output
            .take("delayed", Duration::from_secs(2))
            .expect("delayed response arrives after repeated wakeups");
        producer_thread.join().expect("provider output producer");
        assert!(batch.notifications.len() <= PROVIDER_MAX_RETURNED_NOTIFICATIONS);
        assert!(batch
            .notifications
            .iter()
            .any(|line| line.contains("turn/completed")));
        assert!(batch.dropped_notifications > 0);

        let mut returned = Vec::new();
        for line in batch.notifications {
            let value: serde_json::Value = serde_json::from_str(&line).expect("valid notification");
            assert!(append_notification_value_with_loss(&mut returned, value).0);
        }
        append_notification_loss(&mut returned, batch.dropped_notifications);
        assert!(returned.len() <= PROVIDER_MAX_RETURNED_NOTIFICATIONS);
        assert!(returned.iter().any(|value| {
            value.get("method").and_then(serde_json::Value::as_str) == Some("turn/completed")
        }));
        assert!(returned.iter().any(|value| {
            value.get("method").and_then(serde_json::Value::as_str)
                == Some("pulse/notification_loss")
        }));
    }
}
