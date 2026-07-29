//! Low-level process feasibility primitives for P2S3-I0.
//!
//! These APIs deliberately stop at platform/process mechanics. They do not know
//! about Pulse Tickets, assignments, graph state, authority, or public `run`
//! commands.

#[cfg(target_os = "linux")]
use crate::canonical_json::hash_serializable;
use crate::canonical_json::{hash_bytes, SHA256_PREFIX};
use crate::{PulseError, Result};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[cfg(target_os = "linux")]
pub const PLATFORM_SUPPORT: &str = "linux_proc_stat_starttime_process_group";
#[cfg(target_os = "macos")]
pub const PLATFORM_SUPPORT: &str = "unsupported_macos_identity_not_proven";
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub const PLATFORM_SUPPORT: &str = "unsupported";

const PREFIX_BUDGET_DIVISOR: usize = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProcessIdentityV1 {
    pub pid: u32,
    pub process_group_id: Option<i64>,
    pub platform: String,
    pub platform_start_marker: String,
    pub identity_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SupervisorPackagingProbeV1 {
    pub schema_version: u64,
    pub current_exe: String,
    pub hidden_command: String,
    pub packaging_status: String,
    pub fallback_error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ControlNonceV1 {
    pub nonce_hash: String,
    pub transport: String,
    pub plaintext_persisted: bool,
    pub threat_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BoundedLogRefV1 {
    pub prefix_path: String,
    pub tail_path: String,
    pub total_bytes_seen: u64,
    pub retained_bytes: u64,
    pub truncated_bytes: u64,
    pub content_hash: String,
    pub content_hash_semantics: String,
    pub redaction_status: String,
}

#[derive(Debug, Clone)]
pub struct ControlNoncePlaintext {
    plaintext: Vec<u8>,
    record: ControlNonceV1,
}

impl ControlNoncePlaintext {
    pub fn generate() -> Self {
        let mut plaintext = vec![0_u8; 32];
        rand::thread_rng().fill_bytes(&mut plaintext);
        let record = ControlNonceV1 {
            nonce_hash: hash_bytes(&plaintext),
            transport: nonce_transport().to_string(),
            plaintext_persisted: false,
            threat_model: "same-user environment fallback can be inspected by same uid; descriptor transport preferred when installed".to_string(),
        };
        Self { plaintext, record }
    }

    pub fn record(&self) -> &ControlNonceV1 {
        &self.record
    }

    pub fn plaintext_for_spawn_only(&self) -> &[u8] {
        &self.plaintext
    }
}

impl Drop for ControlNoncePlaintext {
    fn drop(&mut self) {
        self.plaintext.fill(0);
    }
}

pub fn ensure_supported_platform() -> Result<()> {
    if cfg!(target_os = "linux") {
        Ok(())
    } else if cfg!(target_os = "macos") {
        Err(PulseError::validation(
            "run_platform_unsupported",
            "Slice 3 macOS cancellation is unsupported until a Rust 1.78-compatible kernel process creation marker is proven",
        ))
    } else {
        Err(PulseError::validation(
            "run_platform_unsupported",
            "Slice 3 process supervision is only proven for Linux",
        ))
    }
}

pub const HIDDEN_SUPERVISOR_COMMAND: &str = "__run-supervisor";
pub const HIDDEN_SUPERVISOR_NONCE_ENV: &str = "PULSE_RUN_SUPERVISOR_NONCE_HEX";

pub fn supervisor_packaging_probe() -> Result<SupervisorPackagingProbeV1> {
    let current_exe =
        std::env::current_exe().map_err(|error| PulseError::io("<current_exe>", error))?;
    supervisor_packaging_probe_for_exe(&current_exe)
}

pub fn supervisor_packaging_probe_for_exe(
    current_exe: &Path,
) -> Result<SupervisorPackagingProbeV1> {
    let metadata = fs::metadata(current_exe).map_err(|error| PulseError::io(current_exe, error))?;
    let packaging_status = if metadata.is_file() {
        "self_reexec_available_hidden_dispatch_required"
    } else {
        "self_reexec_unavailable"
    };
    Ok(SupervisorPackagingProbeV1 {
        schema_version: 1,
        current_exe: current_exe.to_string_lossy().to_string(),
        hidden_command: HIDDEN_SUPERVISOR_COMMAND.to_string(),
        packaging_status: packaging_status.to_string(),
        fallback_error: "run_supervisor_spawn_failed".to_string(),
    })
}

pub fn validate_hidden_supervisor_control_path(control: &Path) -> Result<()> {
    if control.is_absolute() {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            "hidden supervisor control path must be repository-relative",
        ));
    }
    let normalized = control.components().collect::<Vec<_>>();
    if normalized
        .iter()
        .any(|component| matches!(component, std::path::Component::ParentDir))
        || !control.starts_with(".pulse/runtime/run/control")
        || control.extension().and_then(|value| value.to_str()) != Some("json")
    {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            "hidden supervisor control path must stay under .pulse/runtime/run/control/*.json",
        ));
    }
    Ok(())
}

pub fn hidden_supervisor_probe_dispatch(control: &Path) -> Result<SupervisorPackagingProbeV1> {
    validate_hidden_supervisor_control_path(control)?;
    let nonce_hex = std::env::var(HIDDEN_SUPERVISOR_NONCE_ENV).map_err(|_| {
        PulseError::validation(
            "run_control_record_invalid",
            "hidden supervisor nonce environment was not provided",
        )
    })?;
    let nonce = hex::decode(nonce_hex).map_err(|_| {
        PulseError::validation(
            "run_control_record_invalid",
            "hidden supervisor nonce environment was not valid hex",
        )
    })?;
    if nonce.len() < 32 {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            "hidden supervisor nonce is below Slice 3 entropy floor",
        ));
    }
    supervisor_packaging_probe()
}

pub fn spawn_process_group(command: &mut Command) -> Result<Child> {
    ensure_supported_platform()?;
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            let rc = setpgid_zero();
            if rc == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        });
    }
    command
        .spawn()
        .map_err(|error| PulseError::io("<process-spawn>", error))
}

pub fn current_process_identity(pid: u32) -> Result<ProcessIdentityV1> {
    ensure_supported_platform()?;
    platform_process_identity(pid)
}

pub fn process_identity_matches(expected: &ProcessIdentityV1) -> Result<bool> {
    let current = current_process_identity(expected.pid)?;
    Ok(current.platform == expected.platform
        && current.platform_start_marker == expected.platform_start_marker
        && current.process_group_id == expected.process_group_id)
}

pub fn terminate_process_group(identity: &ProcessIdentityV1) -> Result<()> {
    if !process_identity_matches(identity)? {
        return Err(PulseError::validation(
            "run_process_identity_mismatch",
            "current process identity does not match recorded start marker",
        ));
    }
    let Some(pgid) = identity.process_group_id else {
        return Err(PulseError::validation(
            "run_process_identity_unavailable",
            "process group id is unavailable",
        ));
    };
    #[cfg(unix)]
    unsafe {
        if kill_process_group(pgid, 15) != 0 {
            return Err(PulseError::validation(
                "run_cancel_signal_failed",
                std::io::Error::last_os_error().to_string(),
            ));
        }
    }
    Ok(())
}

pub fn wait_status_code(status: ExitStatus) -> (Option<i32>, Option<i32>) {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        (status.code(), status.signal())
    }
    #[cfg(not(unix))]
    {
        (status.code(), None)
    }
}

pub fn drain_to_bounded_logs<R: Read + Send + 'static>(
    mut reader: R,
    prefix_path: PathBuf,
    tail_path: PathBuf,
    max_bytes: usize,
) -> thread::JoinHandle<Result<BoundedLogRefV1>> {
    thread::spawn(move || drain_reader(&mut reader, &prefix_path, &tail_path, max_bytes))
}

fn drain_reader(
    reader: &mut dyn Read,
    prefix_path: &Path,
    tail_path: &Path,
    max_bytes: usize,
) -> Result<BoundedLogRefV1> {
    if max_bytes == 0 {
        return Err(PulseError::validation(
            "run_profile_invalid",
            "log byte limit must be positive",
        ));
    }
    let prefix_limit = max_bytes / PREFIX_BUDGET_DIVISOR;
    let tail_limit = max_bytes.saturating_sub(prefix_limit);
    let mut prefix = Vec::with_capacity(prefix_limit);
    let mut tail = Vec::with_capacity(tail_limit);
    let mut total = 0_u64;
    let mut full_hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| PulseError::io(prefix_path, error))?;
        if read == 0 {
            break;
        }
        total += read as u64;
        full_hasher.update(&buffer[..read]);
        append_prefix_tail(
            &buffer[..read],
            prefix_limit,
            tail_limit,
            &mut prefix,
            &mut tail,
        );
    }
    write_create_new(prefix_path, &prefix)?;
    write_create_new(tail_path, &tail)?;
    let retained = (prefix.len() + tail.len()) as u64;
    Ok(BoundedLogRefV1 {
        prefix_path: prefix_path.to_string_lossy().to_string(),
        tail_path: tail_path.to_string_lossy().to_string(),
        total_bytes_seen: total,
        retained_bytes: retained,
        truncated_bytes: total.saturating_sub(retained),
        content_hash: format!("{SHA256_PREFIX}{}", hex::encode(full_hasher.finalize())),
        content_hash_semantics: "sha256_full_stream_even_when_retention_truncated".to_string(),
        redaction_status: "not_applied_runtime_private".to_string(),
    })
}

fn append_prefix_tail(
    chunk: &[u8],
    prefix_limit: usize,
    tail_limit: usize,
    prefix: &mut Vec<u8>,
    tail: &mut Vec<u8>,
) {
    let prefix_remaining = prefix_limit.saturating_sub(prefix.len());
    let prefix_take = prefix_remaining.min(chunk.len());
    prefix.extend_from_slice(&chunk[..prefix_take]);
    if tail_limit == 0 {
        return;
    }
    tail.extend_from_slice(chunk);
    if tail.len() > tail_limit {
        let overflow = tail.len() - tail_limit;
        tail.drain(..overflow);
    }
}

fn write_create_new(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| PulseError::io(path, error))?;
    file.write_all(bytes)
        .map_err(|error| PulseError::io(path, error))
}

#[cfg(target_os = "linux")]
fn platform_process_identity(pid: u32) -> Result<ProcessIdentityV1> {
    let stat_path = PathBuf::from(format!("/proc/{pid}/stat"));
    let stat = fs::read_to_string(&stat_path).map_err(|error| PulseError::io(&stat_path, error))?;
    let close = stat.rfind(')').ok_or_else(|| {
        PulseError::validation("run_process_identity_unavailable", "malformed /proc stat")
    })?;
    let fields = stat[close + 2..].split_whitespace().collect::<Vec<_>>();
    if fields.len() <= 19 {
        return Err(PulseError::validation(
            "run_process_identity_unavailable",
            "missing /proc stat starttime field",
        ));
    }
    let pgrp = fields
        .get(2)
        .ok_or_else(|| PulseError::validation("run_process_identity_unavailable", "missing pgrp"))?
        .parse::<i64>()
        .map_err(|_| PulseError::validation("run_process_identity_unavailable", "invalid pgrp"))?;
    let start_ticks = fields[19];
    let boot_id = fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap_or_default();
    Ok(ProcessIdentityV1 {
        pid,
        process_group_id: Some(pgrp),
        platform: PLATFORM_SUPPORT.to_string(),
        platform_start_marker: hash_serializable(&(boot_id.trim(), start_ticks))?,
        identity_status: "verified".to_string(),
    })
}

#[cfg(target_os = "macos")]
fn platform_process_identity(_pid: u32) -> Result<ProcessIdentityV1> {
    Err(PulseError::validation(
        "run_platform_unsupported",
        "macOS process identity is not proven by a Rust 1.78-compatible kernel creation marker",
    ))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn platform_process_identity(_pid: u32) -> Result<ProcessIdentityV1> {
    Err(PulseError::validation(
        "run_platform_unsupported",
        "process identity marker is not implemented on this platform",
    ))
}

fn nonce_transport() -> &'static str {
    "protected_environment_fallback_descriptor_preferred"
}

#[cfg(unix)]
unsafe fn setpgid_zero() -> i32 {
    extern "C" {
        fn setpgid(pid: i32, pgid: i32) -> i32;
    }
    unsafe { setpgid(0, 0) }
}

#[cfg(unix)]
unsafe fn kill_process_group(pgid: i64, signal: i32) -> i32 {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    unsafe { kill(-(pgid as i32), signal) }
}

pub type LogDrainHandle = thread::JoinHandle<Result<BoundedLogRefV1>>;

pub struct SpawnedWithDrainedLogs {
    pub child: Child,
    pub stdout_log: LogDrainHandle,
    pub stderr_log: LogDrainHandle,
}

pub fn spawn_with_drained_logs(
    command: &mut Command,
    stdout_prefix: PathBuf,
    stdout_tail: PathBuf,
    stderr_prefix: PathBuf,
    stderr_tail: PathBuf,
    max_bytes_per_stream: usize,
) -> Result<SpawnedWithDrainedLogs> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = spawn_process_group(command)?;
    let stdout = child.stdout.take().ok_or_else(|| {
        PulseError::validation("run_log_open_failed", "child stdout pipe was unavailable")
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        PulseError::validation("run_log_open_failed", "child stderr pipe was unavailable")
    })?;
    let stdout_log =
        drain_to_bounded_logs(stdout, stdout_prefix, stdout_tail, max_bytes_per_stream);
    let stderr_log =
        drain_to_bounded_logs(stderr, stderr_prefix, stderr_tail, max_bytes_per_stream);
    Ok(SpawnedWithDrainedLogs {
        child,
        stdout_log,
        stderr_log,
    })
}

pub fn write_nonce_record_without_plaintext(
    path: &Path,
    nonce: &ControlNoncePlaintext,
) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(nonce.record()).map_err(crate::PulseError::from)?;
    let text = String::from_utf8_lossy(&bytes);
    let plaintext_hex = hex::encode(nonce.plaintext_for_spawn_only());
    if text.contains(&plaintext_hex) {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            "nonce plaintext would be persisted",
        ));
    }
    write_create_new(path, &bytes)
}

pub fn shared_vec_reader(bytes: Vec<u8>) -> impl Read + Send + 'static {
    struct Reader {
        inner: Arc<Mutex<std::io::Cursor<Vec<u8>>>>,
    }
    impl Read for Reader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.inner.lock().expect("reader mutex poisoned").read(buf)
        }
    }
    Reader {
        inner: Arc::new(Mutex::new(std::io::Cursor::new(bytes))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonce_record_never_contains_plaintext() {
        let tmp = tempfile::tempdir().unwrap();
        let nonce = ControlNoncePlaintext::generate();
        let path = tmp.path().join("control.json");
        write_nonce_record_without_plaintext(&path, &nonce).unwrap();
        let body = fs::read_to_string(path).unwrap();
        assert!(body.contains("nonce_hash"));
        assert!(!body.contains(&hex::encode(nonce.plaintext_for_spawn_only())));
        assert!(!nonce.record().plaintext_persisted);
    }

    #[test]
    fn bounded_log_retention_keeps_prefix_and_tail_only() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = (0..200_u16).map(|value| value as u8).collect::<Vec<_>>();
        let handle = drain_to_bounded_logs(
            shared_vec_reader(bytes.clone()),
            tmp.path().join("stdout.prefix.log"),
            tmp.path().join("stdout.tail.log"),
            40,
        );
        let record = handle.join().unwrap().unwrap();
        assert_eq!(record.total_bytes_seen, 200);
        assert_eq!(record.retained_bytes, 40);
        assert_eq!(record.truncated_bytes, 160);
        assert_eq!(record.content_hash, hash_bytes(&bytes));
        assert_eq!(
            fs::read(tmp.path().join("stdout.prefix.log")).unwrap(),
            bytes[..20]
        );
        assert_eq!(
            fs::read(tmp.path().join("stdout.tail.log")).unwrap(),
            bytes[180..]
        );
        assert_eq!(
            record.content_hash_semantics,
            "sha256_full_stream_even_when_retention_truncated"
        );
    }

    #[test]
    fn packaging_probe_uses_current_exe_and_hidden_command() {
        let probe = supervisor_packaging_probe().unwrap();
        assert_eq!(probe.hidden_command, "__run-supervisor");
        assert!(probe.current_exe.contains("pulse") || probe.current_exe.contains("process"));
        assert_eq!(probe.fallback_error, "run_supervisor_spawn_failed");
    }
}
