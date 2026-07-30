//! Low-level process supervisor primitives for P2S3.
//!
//! These APIs deliberately stop at platform/process mechanics. They do not know
//! about Pulse Tickets, assignments, graph state, authority, or public `run`
//! commands.

use crate::canonical_json::{hash_bytes, hash_serializable, to_canonical_bytes, SHA256_PREFIX};
use crate::run::{
    ProcessIdentityV1 as RunProcessIdentityV1, RunExitKindV1, RunExitResultV1,
    RunnerEnvironmentSourceV1, RunnerProfileSelectionV1,
};
use crate::{PulseError, Result};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
#[cfg(unix)]
use std::os::unix::net::UnixStream;

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

#[cfg(target_os = "linux")]
pub const PLATFORM_SUPPORT: &str = "linux_proc_stat_starttime_process_group";
#[cfg(target_os = "macos")]
pub const PLATFORM_SUPPORT: &str = "unsupported_macos_identity_not_proven";
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub const PLATFORM_SUPPORT: &str = "unsupported";

const PREFIX_BUDGET_DIVISOR: usize = 2;
const SUPERVISOR_HEARTBEAT_INTERVAL: Duration = Duration::from_millis(100);
const SUPERVISOR_POLL_INTERVAL: Duration = Duration::from_millis(25);
const MANAGED_CONTROL_PREFIX: &str = ".pulse/runtime/run/control";
#[cfg(target_os = "linux")]
const F_GETFD: i32 = 1;
#[cfg(target_os = "linux")]
const F_SETFD: i32 = 2;
#[cfg(target_os = "linux")]
const FD_CLOEXEC: i32 = 1;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupervisorControlDescriptorV1 {
    pub schema_version: u32,
    pub run_id: String,
    pub attempt_id: String,
    pub nonce_hash: String,
    pub workspace_path: String,
    pub executable_path: String,
    pub executable_identity: String,
    pub argv: Vec<String>,
    pub argv_hash: String,
    pub environment: Vec<SupervisorEnvironmentEntryV1>,
    pub environment_spec_fingerprint: String,
    #[serde(default, skip_serializing)]
    pub private_literal_environment_values: BTreeMap<String, String>,
    pub input_json_path: String,
    pub stdout_prefix_path: String,
    pub stdout_tail_path: String,
    pub stderr_prefix_path: String,
    pub stderr_tail_path: String,
    pub heartbeat_path: String,
    pub cancel_path: String,
    pub exit_path: String,
    pub max_stdout_bytes: u64,
    pub max_stderr_bytes: u64,
    pub run_timeout_seconds: u64,
    pub cancel_grace_seconds: u64,
    pub force_kill_after_seconds: u64,
    pub force_allowed: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupervisorEnvironmentEntryV1 {
    pub name: String,
    pub source: RunnerEnvironmentSourceV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupervisorHeartbeatRecordV1 {
    pub schema_version: u32,
    pub run_id: String,
    pub attempt_id: String,
    pub observed_at: String,
    pub supervisor_pid: u64,
    pub child_pid: Option<u64>,
    pub nonce_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupervisorCancelRequestV1 {
    pub schema_version: u32,
    pub run_id: String,
    pub attempt_id: String,
    pub nonce_hash: String,
    pub requested_at: String,
    pub requested_by: String,
    pub reason: String,
    pub grace_seconds: u64,
    pub force_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupervisorExitObservationV1 {
    pub schema_version: u32,
    pub run_id: String,
    pub attempt_id: String,
    pub nonce_hash: String,
    pub process_identity: RunProcessIdentityV1,
    pub stdout: BoundedLogRefV1,
    pub stderr: BoundedLogRefV1,
    pub exit: RunExitResultV1,
    pub observed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupervisorStartupHandshakeV1 {
    pub schema_version: u32,
    pub run_id: String,
    pub attempt_id: String,
    pub nonce_hash: String,
    pub supervisor_pid: u64,
    pub child_identity: RunProcessIdentityV1,
    pub started_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorLaunchConfig {
    pub repo_root: PathBuf,
    pub control_relative_path: PathBuf,
    pub descriptor: SupervisorControlDescriptorV1,
    pub nonce: Vec<u8>,
    pub start_timeout: Duration,
    pub pulse_exe: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorLaunchReport {
    pub handshake: SupervisorStartupHandshakeV1,
    pub supervisor_identity: ProcessIdentityV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancelPolicy {
    pub grace: Duration,
    pub force_after: Duration,
    pub force_allowed: bool,
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
pub const HIDDEN_SUPERVISOR_HANDSHAKE_ENV: &str = "PULSE_RUN_SUPERVISOR_HANDSHAKE_PATH";
pub const HIDDEN_SUPERVISOR_PARENT_FD_ENV: &str = "PULSE_RUN_SUPERVISOR_PARENT_FD";
pub const HIDDEN_SUPERVISOR_LITERAL_ENV_PREFIX: &str = "PULSE_RUN_SUPERVISOR_LITERAL_ENV_HEX_";
const HIDDEN_SUPERVISOR_TEST_DELAY_HANDSHAKE_MS_ENV: &str =
    "PULSE_RUN_SUPERVISOR_TEST_DELAY_HANDSHAKE_MS";

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
    if control
        .components()
        .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
        || !control.starts_with(MANAGED_CONTROL_PREFIX)
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
    std::env::remove_var(HIDDEN_SUPERVISOR_NONCE_ENV);
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

pub fn run_hidden_supervisor(repo_root: &Path, control: &Path) -> Result<()> {
    ensure_supported_platform()?;
    validate_hidden_supervisor_control_path(control)?;
    let nonce = read_nonce_from_environment()?;
    let parent_lifetime = take_parent_lifetime_pipe()?;
    let control_path = repo_root.join(control);
    validate_managed_control_file(repo_root, &control_path)?;
    let mut descriptor: SupervisorControlDescriptorV1 = read_canonical_json(&control_path)?;
    descriptor.private_literal_environment_values =
        read_private_literal_environment_values(&descriptor)?;
    validate_descriptor(repo_root, control, &descriptor, &nonce)?;
    let result = supervisor_main(repo_root, control, descriptor, nonce, parent_lifetime);
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            eprintln!(
                "{{\"class\":\"run_supervisor_error\",\"code\":\"{}\"}}",
                error.code()
            );
            Err(error)
        }
    }
}

#[cfg(unix)]
fn parent_lifetime_pipe() -> Result<(UnixStream, UnixStream)> {
    UnixStream::pair().map_err(|error| PulseError::io("<supervisor-parent-pipe>", error))
}

#[cfg(unix)]
fn spawn_supervisor_with_inherited_fd(mut command: Command, keep_fd: RawFd) -> Result<Child> {
    unsafe {
        command.pre_exec(move || {
            clear_cloexec(keep_fd);
            Ok(())
        });
    }
    command
        .spawn()
        .map_err(|error| PulseError::io("<supervisor-spawn>", error))
}

#[cfg(unix)]
fn take_parent_lifetime_pipe() -> Result<Option<UnixStream>> {
    let Some(fd_raw) = std::env::var_os(HIDDEN_SUPERVISOR_PARENT_FD_ENV) else {
        return Ok(None);
    };
    std::env::remove_var(HIDDEN_SUPERVISOR_PARENT_FD_ENV);
    let fd_text = fd_raw.to_str().ok_or_else(|| {
        PulseError::validation(
            "run_control_record_invalid",
            "supervisor parent pipe fd was not UTF-8",
        )
    })?;
    let fd: RawFd = fd_text.parse().map_err(|_| {
        PulseError::validation(
            "run_control_record_invalid",
            "supervisor parent pipe fd was not an integer",
        )
    })?;
    if fd < 0 {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            "supervisor parent pipe fd was invalid",
        ));
    }
    let stream = unsafe { UnixStream::from_raw_fd(fd) };
    set_cloexec(stream.as_raw_fd());
    stream
        .set_nonblocking(true)
        .map_err(|error| PulseError::io("<supervisor-parent-pipe>", error))?;
    Ok(Some(stream))
}

#[cfg(unix)]
fn add_private_literal_environment(
    command: &mut Command,
    descriptor: &SupervisorControlDescriptorV1,
) -> Result<()> {
    for (name, value) in &descriptor.private_literal_environment_values {
        command.env(
            literal_transport_env_name(name)?,
            hex::encode(value.as_bytes()),
        );
    }
    Ok(())
}

#[cfg(unix)]
fn read_private_literal_environment_values(
    descriptor: &SupervisorControlDescriptorV1,
) -> Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    for entry in &descriptor.environment {
        if entry.source != RunnerEnvironmentSourceV1::LiteralNonSecret {
            continue;
        }
        let transport_name = literal_transport_env_name(&entry.name)?;
        let encoded = std::env::var(&transport_name).map_err(|_| {
            PulseError::validation(
                "run_control_record_invalid",
                "literal environment private transport was missing",
            )
        })?;
        std::env::remove_var(&transport_name);
        let bytes = hex::decode(encoded).map_err(|_| {
            PulseError::validation(
                "run_control_record_invalid",
                "literal environment private transport was not valid hex",
            )
        })?;
        let value = String::from_utf8(bytes).map_err(|_| {
            PulseError::validation(
                "run_control_record_invalid",
                "literal environment private transport was not UTF-8",
            )
        })?;
        values.insert(entry.name.clone(), value);
    }
    Ok(values)
}

fn literal_transport_env_name(name: &str) -> Result<String> {
    if !valid_env_name(name) {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            "literal environment name is invalid",
        ));
    }
    Ok(format!("{HIDDEN_SUPERVISOR_LITERAL_ENV_PREFIX}{name}"))
}

fn valid_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first == '_' || first.is_ascii_uppercase() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_ascii_uppercase() || c.is_ascii_digit())
}

pub fn launch_supervisor(mut config: SupervisorLaunchConfig) -> Result<SupervisorLaunchReport> {
    ensure_supported_platform()?;
    validate_hidden_supervisor_control_path(&config.control_relative_path)?;
    let private_literal_environment_values =
        std::mem::take(&mut config.descriptor.private_literal_environment_values);
    validate_descriptor(
        &config.repo_root,
        &config.control_relative_path,
        &config.descriptor,
        &config.nonce,
    )?;
    let control_path = config.repo_root.join(&config.control_relative_path);
    write_json_create_new_private(&control_path, &config.descriptor)?;
    config.descriptor.private_literal_environment_values = private_literal_environment_values;
    let handshake_path = control_path.with_extension("handshake.json");
    if handshake_path.exists() {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            "startup handshake path already exists",
        ));
    }
    let (parent_pipe, child_pipe) = parent_lifetime_pipe()?;
    let mut child = Command::new(&config.pulse_exe);
    child
        .arg("--repo-root")
        .arg(&config.repo_root)
        .arg(HIDDEN_SUPERVISOR_COMMAND)
        .arg("--control")
        .arg(&config.control_relative_path)
        .env(HIDDEN_SUPERVISOR_NONCE_ENV, hex::encode(&config.nonce))
        .env(HIDDEN_SUPERVISOR_HANDSHAKE_ENV, &handshake_path)
        .env(
            HIDDEN_SUPERVISOR_PARENT_FD_ENV,
            child_pipe.as_raw_fd().to_string(),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    add_private_literal_environment(&mut child, &config.descriptor)?;
    let mut supervisor = spawn_supervisor_with_inherited_fd(child, child_pipe.as_raw_fd())?;
    drop(child_pipe);
    let supervisor_identity =
        current_process_identity(supervisor.id()).unwrap_or_else(|_| ProcessIdentityV1 {
            pid: supervisor.id(),
            process_group_id: None,
            platform: PLATFORM_SUPPORT.to_string(),
            platform_start_marker: "supervisor_fast_exit_identity_unavailable".to_string(),
            identity_status: "startup_observation_only".to_string(),
        });
    let deadline = Instant::now() + config.start_timeout;
    loop {
        if handshake_path.exists() {
            let handshake_result = (|| -> Result<SupervisorStartupHandshakeV1> {
                let handshake: SupervisorStartupHandshakeV1 = read_canonical_json(&handshake_path)?;
                validate_handshake(&config.descriptor, &handshake)?;
                Ok(handshake)
            })();
            match handshake_result {
                Ok(handshake) => {
                    return Ok(SupervisorLaunchReport {
                        handshake,
                        supervisor_identity,
                    });
                }
                Err(error) => {
                    drop(parent_pipe);
                    cleanup_supervisor_after_start_failure(&mut supervisor, &handshake_path);
                    return Err(error);
                }
            }
        }
        if let Some(status) = supervisor
            .try_wait()
            .map_err(|error| PulseError::io("<supervisor-wait>", error))?
        {
            return Err(PulseError::validation(
                "run_supervisor_spawn_failed",
                format!("supervisor exited before handshake: {status}"),
            ));
        }
        if Instant::now() >= deadline {
            drop(parent_pipe);
            cleanup_supervisor_after_start_failure(&mut supervisor, &handshake_path);
            return Err(PulseError::validation(
                "run_supervisor_handshake_timeout",
                "supervisor did not publish verified startup handshake before timeout",
            ));
        }
        thread::sleep(SUPERVISOR_POLL_INTERVAL);
    }
}

fn cleanup_supervisor_after_start_failure(supervisor: &mut Child, handshake_path: &Path) {
    if let Ok(handshake) = read_canonical_json::<SupervisorStartupHandshakeV1>(handshake_path) {
        let low = low_level_identity_from_run(&handshake.child_identity).ok();
        if let Some(identity) = low.as_ref() {
            if process_identity_matches(identity).unwrap_or(false) {
                let _ = signal_process_group(identity, 15);
                thread::sleep(Duration::from_millis(100));
                if process_identity_matches(identity).unwrap_or(false) {
                    let _ = signal_process_group(identity, 9);
                }
            }
        }
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if supervisor.try_wait().ok().flatten().is_some() {
            return;
        }
        thread::sleep(SUPERVISOR_POLL_INTERVAL);
    }
    let _ = supervisor.kill();
    let _ = supervisor.wait();
}

#[allow(clippy::too_many_arguments)]
pub fn build_descriptor_for_selection(
    repo_root: &Path,
    run_id: &str,
    attempt_id: &str,
    nonce: &ControlNoncePlaintext,
    workspace_path: &Path,
    selection: &RunnerProfileSelectionV1,
    input_json_path: &Path,
    log_root: &Path,
    timeout_seconds: u64,
    cancel_grace_seconds: u64,
    force_kill_after_seconds: u64,
    force_allowed: bool,
    max_stdout_bytes: u64,
    max_stderr_bytes: u64,
) -> Result<SupervisorControlDescriptorV1> {
    let argv = selection.fixed_args.clone();
    let argv_hash = hash_argv(&selection.executable.resolved_path, &argv)?;
    let env_entries = build_environment_spec(selection)?;
    let control_root = repo_root.join(MANAGED_CONTROL_PREFIX);
    Ok(SupervisorControlDescriptorV1 {
        schema_version: 1,
        run_id: run_id.to_string(),
        attempt_id: attempt_id.to_string(),
        nonce_hash: nonce.record().nonce_hash.clone(),
        workspace_path: path_to_string(workspace_path)?,
        executable_path: selection.executable.resolved_path.clone(),
        executable_identity: selection.executable.identity.clone(),
        argv,
        argv_hash,
        environment: env_entries,
        environment_spec_fingerprint: selection.environment_spec_fingerprint.clone(),
        private_literal_environment_values: literal_environment_values(selection)?,
        input_json_path: path_to_string(input_json_path)?,
        stdout_prefix_path: path_to_string(
            &log_root.join(format!("{attempt_id}.stdout.prefix.log")),
        )?,
        stdout_tail_path: path_to_string(&log_root.join(format!("{attempt_id}.stdout.tail.log")))?,
        stderr_prefix_path: path_to_string(
            &log_root.join(format!("{attempt_id}.stderr.prefix.log")),
        )?,
        stderr_tail_path: path_to_string(&log_root.join(format!("{attempt_id}.stderr.tail.log")))?,
        heartbeat_path: path_to_string(&control_root.join(format!("{run_id}.heartbeat.json")))?,
        cancel_path: path_to_string(&control_root.join(format!("{run_id}.cancel.json")))?,
        exit_path: path_to_string(&control_root.join(format!("{run_id}.exit.json")))?,
        max_stdout_bytes,
        max_stderr_bytes,
        run_timeout_seconds: timeout_seconds,
        cancel_grace_seconds,
        force_kill_after_seconds,
        force_allowed,
        created_at: now_rfc3339(),
    })
}

pub fn spawn_process_group(command: &mut Command) -> Result<Child> {
    ensure_supported_platform()?;
    #[cfg(target_os = "linux")]
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
    signal_process_group(identity, 15)
}

pub fn signal_process_group(identity: &ProcessIdentityV1, signal: i32) -> Result<()> {
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
    #[cfg(target_os = "linux")]
    {
        if kill_process_group(pgid, signal) != 0 {
            return Err(PulseError::validation(
                "run_cancel_signal_failed",
                std::io::Error::last_os_error().to_string(),
            ));
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (pgid, signal);
    }
    Ok(())
}

pub fn cancel_verified_process_tree(
    identity: &RunProcessIdentityV1,
    policy: CancelPolicy,
) -> Result<()> {
    let low = low_level_identity_from_run(identity)?;
    signal_process_group(&low, 15)?;
    let grace_deadline = Instant::now() + policy.grace;
    while Instant::now() < grace_deadline {
        if !process_identity_matches(&low).unwrap_or(false) {
            return Ok(());
        }
        thread::sleep(SUPERVISOR_POLL_INTERVAL);
    }
    if !policy.force_allowed {
        return Err(PulseError::validation(
            "run_force_kill_disallowed",
            "process did not stop during grace and force was disabled",
        ));
    }
    if !process_identity_matches(&low)? {
        return Ok(());
    }
    signal_process_group(&low, 9)?;
    let force_deadline = Instant::now() + policy.force_after;
    while Instant::now() < force_deadline {
        if !process_identity_matches(&low).unwrap_or(false) {
            return Ok(());
        }
        thread::sleep(SUPERVISOR_POLL_INTERVAL);
    }
    Err(PulseError::validation(
        "run_cancel_timeout",
        "verified process tree did not stop within policy bounds",
    ))
}

pub fn wait_status_code(status: ExitStatus) -> (Option<i32>, Option<i32>) {
    #[cfg(unix)]
    {
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
    write_private_create_new(prefix_path, &prefix)?;
    write_private_create_new(tail_path, &tail)?;
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
    write_private_create_new(path, &bytes)
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

fn supervisor_main(
    repo_root: &Path,
    control_relative: &Path,
    descriptor: SupervisorControlDescriptorV1,
    nonce: Vec<u8>,
    parent_lifetime: Option<UnixStream>,
) -> Result<()> {
    let mut command = build_child_command(&descriptor)?;
    command
        .current_dir(&descriptor.workspace_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let parent_closed = Arc::new(AtomicBool::new(false));
    let parent_watch_active = Arc::new(AtomicBool::new(true));
    let parent_watcher = parent_lifetime.map(|stream| {
        watch_parent_lifetime(
            stream,
            Arc::clone(&parent_closed),
            Arc::clone(&parent_watch_active),
        )
    });
    if parent_closed.load(Ordering::SeqCst) {
        parent_watch_active.store(false, Ordering::SeqCst);
        drop(parent_closed);
        join_parent_watcher(parent_watcher);
        return Err(PulseError::validation(
            "run_supervisor_parent_closed",
            "launching parent closed startup channel before child spawn",
        ));
    }
    let mut child = match spawn_process_group(&mut command) {
        Ok(child) => child,
        Err(error) => {
            write_failed_start_observation(repo_root, &descriptor, &nonce, error.code())?;
            parent_watch_active.store(false, Ordering::SeqCst);
            drop(parent_closed);
            join_parent_watcher(parent_watcher);
            return Err(error);
        }
    };
    let child_identity_low = match current_process_identity(child.id()) {
        Ok(identity) => identity,
        Err(error) => {
            cleanup_spawned_child(&mut child, None, None);
            return Err(error);
        }
    };
    let started_at = now_rfc3339();
    let child_identity = run_identity_from_low_level(
        &descriptor,
        std::process::id() as u64,
        &child_identity_low,
        &started_at,
    );

    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            cleanup_spawned_child(&mut child, Some(&child_identity_low), None);
            return Err(PulseError::validation(
                "run_log_open_failed",
                "child stdout pipe was unavailable",
            ));
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            cleanup_spawned_child(&mut child, Some(&child_identity_low), None);
            return Err(PulseError::validation(
                "run_log_open_failed",
                "child stderr pipe was unavailable",
            ));
        }
    };
    let stdout_log = drain_to_bounded_logs(
        stdout,
        PathBuf::from(&descriptor.stdout_prefix_path),
        PathBuf::from(&descriptor.stdout_tail_path),
        descriptor.max_stdout_bytes as usize,
    );
    let stderr_log = drain_to_bounded_logs(
        stderr,
        PathBuf::from(&descriptor.stderr_prefix_path),
        PathBuf::from(&descriptor.stderr_tail_path),
        descriptor.max_stderr_bytes as usize,
    );

    maybe_delay_startup_handshake_for_tests(&parent_closed);
    if parent_closed.load(Ordering::SeqCst) {
        cleanup_spawned_child(
            &mut child,
            Some(&child_identity_low),
            Some((stdout_log, stderr_log)),
        );
        parent_watch_active.store(false, Ordering::SeqCst);
        drop(parent_closed);
        join_parent_watcher(parent_watcher);
        return Err(PulseError::validation(
            "run_supervisor_parent_closed",
            "launching parent closed startup channel before handshake",
        ));
    }

    if let Err(error) = write_startup_handshake(&descriptor, &child_identity) {
        cleanup_spawned_child(
            &mut child,
            Some(&child_identity_low),
            Some((stdout_log, stderr_log)),
        );
        parent_watch_active.store(false, Ordering::SeqCst);
        drop(parent_closed);
        join_parent_watcher(parent_watcher);
        return Err(error);
    }
    if let Err(error) = write_heartbeat(&descriptor, Some(child.id() as u64)) {
        cleanup_spawned_child(
            &mut child,
            Some(&child_identity_low),
            Some((stdout_log, stderr_log)),
        );
        parent_watch_active.store(false, Ordering::SeqCst);
        drop(parent_closed);
        join_parent_watcher(parent_watcher);
        return Err(error);
    }

    let started = Instant::now();
    let mut cancel_requested = false;
    let mut timed_out = false;
    let exit_status = loop {
        if let Some(exit) = child
            .try_wait()
            .map_err(|error| PulseError::io("<child-wait>", error))?
        {
            break exit;
        }
        if parent_closed.load(Ordering::SeqCst) && !cancel_requested {
            cancel_requested = true;
            request_signal_policy(&child_identity, descriptor_timeout_policy(&descriptor))?;
        }
        if !cancel_requested
            && started.elapsed() >= Duration::from_secs(descriptor.run_timeout_seconds)
        {
            timed_out = true;
            cancel_requested = true;
            let policy = descriptor_timeout_policy(&descriptor);
            request_signal_policy(&child_identity, policy)?;
        }
        if !cancel_requested && Path::new(&descriptor.cancel_path).exists() {
            let request: SupervisorCancelRequestV1 =
                read_canonical_json(Path::new(&descriptor.cancel_path))?;
            validate_cancel_request(&descriptor, &request)?;
            cancel_requested = true;
            let policy = effective_cancel_policy(&descriptor, &request);
            request_signal_policy(&child_identity, policy)?;
        }
        if cancel_requested {
            let policy = if Path::new(&descriptor.cancel_path).exists() {
                let request: SupervisorCancelRequestV1 =
                    read_canonical_json(Path::new(&descriptor.cancel_path))?;
                validate_cancel_request(&descriptor, &request)?;
                effective_cancel_policy(&descriptor, &request)
            } else {
                descriptor_timeout_policy(&descriptor)
            };
            if let Some(exit) = wait_for_cancelled(&mut child, &child_identity, policy)? {
                break exit;
            }
        }
        write_heartbeat(&descriptor, Some(child.id() as u64))?;
        thread::sleep(SUPERVISOR_HEARTBEAT_INTERVAL);
    };
    let stdout = join_log(stdout_log)?;
    let stderr = join_log(stderr_log)?;
    let exit = run_exit_result(exit_status, timed_out, cancel_requested);
    let observation = SupervisorExitObservationV1 {
        schema_version: 1,
        run_id: descriptor.run_id.clone(),
        attempt_id: descriptor.attempt_id.clone(),
        nonce_hash: hash_bytes(&nonce),
        process_identity: child_identity,
        stdout,
        stderr,
        observed_at: now_rfc3339(),
        exit,
    };
    write_json_create_new_private(Path::new(&descriptor.exit_path), &observation)?;
    let _ = (repo_root, control_relative);
    Ok(())
}

#[cfg(unix)]
fn watch_parent_lifetime(
    stream: UnixStream,
    closed: Arc<AtomicBool>,
    active: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut byte = [0_u8; 1];
        while active.load(Ordering::SeqCst) {
            match (&stream).read(&mut byte) {
                Ok(0) => {
                    closed.store(true, Ordering::SeqCst);
                    break;
                }
                Ok(_) => continue,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(SUPERVISOR_POLL_INTERVAL);
                }
                Err(_) => {
                    closed.store(true, Ordering::SeqCst);
                    break;
                }
            }
        }
    })
}

#[cfg(unix)]
fn join_parent_watcher(handle: Option<thread::JoinHandle<()>>) {
    if let Some(handle) = handle {
        let _ = handle.join();
    }
}

fn maybe_delay_startup_handshake_for_tests(parent_closed: &AtomicBool) {
    if let Ok(value) = std::env::var(HIDDEN_SUPERVISOR_TEST_DELAY_HANDSHAKE_MS_ENV) {
        if let Ok(ms) = value.parse::<u64>() {
            let deadline = Instant::now() + Duration::from_millis(ms);
            while Instant::now() < deadline && !parent_closed.load(Ordering::SeqCst) {
                thread::sleep(SUPERVISOR_POLL_INTERVAL);
            }
        }
    }
}

fn build_child_command(descriptor: &SupervisorControlDescriptorV1) -> Result<Command> {
    let executable = PathBuf::from(&descriptor.executable_path);
    revalidate_executable_identity(&executable, &descriptor.executable_identity)?;
    let actual_argv_hash = hash_argv(&descriptor.executable_path, &descriptor.argv)?;
    if actual_argv_hash != descriptor.argv_hash {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            "descriptor argv hash mismatch",
        ));
    }
    let mut command = Command::new(&executable);
    command.args(&descriptor.argv);
    command.env_clear();
    for entry in &descriptor.environment {
        match entry.source {
            RunnerEnvironmentSourceV1::Inherited => {
                if let Some(value) = std::env::var_os(&entry.name) {
                    command.env(&entry.name, value);
                }
            }
            RunnerEnvironmentSourceV1::LiteralNonSecret => {
                let value = descriptor
                    .private_literal_environment_values
                    .get(&entry.name)
                    .ok_or_else(|| {
                        PulseError::validation(
                            "run_control_record_invalid",
                            "literal environment entry missing private transport value",
                        )
                    })?;
                command.env(&entry.name, value);
            }
        }
    }
    Ok(command)
}

fn request_signal_policy(identity: &RunProcessIdentityV1, _policy: CancelPolicy) -> Result<()> {
    let low = low_level_identity_from_run(identity)?;
    signal_process_group(&low, 15)
}

fn descriptor_timeout_policy(descriptor: &SupervisorControlDescriptorV1) -> CancelPolicy {
    CancelPolicy {
        grace: Duration::from_secs(descriptor.cancel_grace_seconds),
        force_after: Duration::from_secs(descriptor.force_kill_after_seconds),
        force_allowed: descriptor.force_allowed,
    }
}

fn effective_cancel_policy(
    descriptor: &SupervisorControlDescriptorV1,
    request: &SupervisorCancelRequestV1,
) -> CancelPolicy {
    let descriptor_grace = Duration::from_secs(descriptor.cancel_grace_seconds);
    let request_grace = Duration::from_secs(request.grace_seconds);
    CancelPolicy {
        grace: std::cmp::min(descriptor_grace, request_grace),
        force_after: Duration::from_secs(descriptor.force_kill_after_seconds),
        force_allowed: descriptor.force_allowed && request.force_allowed,
    }
}

fn cleanup_spawned_child(
    child: &mut Child,
    identity: Option<&ProcessIdentityV1>,
    logs: Option<(LogDrainHandle, LogDrainHandle)>,
) {
    if let Some(identity) = identity {
        if process_identity_matches(identity).unwrap_or(false) {
            let _ = signal_process_group(identity, 15);
            let deadline = Instant::now() + Duration::from_millis(500);
            let mut child_exited = false;
            while Instant::now() < deadline {
                if child.try_wait().ok().flatten().is_some() {
                    child_exited = true;
                    break;
                }
                thread::sleep(SUPERVISOR_POLL_INTERVAL);
            }
            if let Some(_pgid) = identity.process_group_id {
                #[cfg(target_os = "linux")]
                {
                    let _ = kill_process_group(_pgid, 9);
                }
            } else if process_identity_matches(identity).unwrap_or(false) {
                let _ = signal_process_group(identity, 9);
            }
            if child_exited {
                // The direct child may have exited on TERM while grandchildren in
                // its process group ignored TERM. Keep falling through to join
                // log drainers after the best-effort group KILL above.
            }
        }
    } else {
        let _ = child.kill();
    }
    let _ = child.wait();
    if let Some((stdout, stderr)) = logs {
        let _ = join_log(stdout);
        let _ = join_log(stderr);
    }
}

fn wait_for_cancelled(
    child: &mut Child,
    identity: &RunProcessIdentityV1,
    policy: CancelPolicy,
) -> Result<Option<ExitStatus>> {
    if let Some(status) = child
        .try_wait()
        .map_err(|error| PulseError::io("<child-wait>", error))?
    {
        return Ok(Some(status));
    }
    let low = low_level_identity_from_run(identity)?;
    let grace_deadline = Instant::now() + policy.grace;
    while Instant::now() < grace_deadline {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| PulseError::io("<child-wait>", error))?
        {
            return Ok(Some(status));
        }
        thread::sleep(SUPERVISOR_POLL_INTERVAL);
    }
    if !policy.force_allowed {
        return Ok(None);
    }
    if process_identity_matches(&low)? {
        signal_process_group(&low, 9)?;
    }
    let deadline = Instant::now() + policy.force_after;
    while Instant::now() < deadline {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| PulseError::io("<child-wait>", error))?
        {
            return Ok(Some(status));
        }
        thread::sleep(SUPERVISOR_POLL_INTERVAL);
    }
    Ok(None)
}

fn write_failed_start_observation(
    _repo_root: &Path,
    descriptor: &SupervisorControlDescriptorV1,
    nonce: &[u8],
    cause: &str,
) -> Result<()> {
    let now = now_rfc3339();
    let identity = RunProcessIdentityV1 {
        supervisor_pid: std::process::id() as u64,
        child_pid: 0,
        process_group_id: None,
        supervisor_nonce_hash: hash_bytes(nonce),
        started_at: now.clone(),
        platform_start_marker: "unavailable_before_spawn".to_string(),
        argv_hash: descriptor.argv_hash.clone(),
        executable_identity: descriptor.executable_identity.clone(),
        identity_status: "failed_to_start".to_string(),
    };
    let observation = SupervisorExitObservationV1 {
        schema_version: 1,
        run_id: descriptor.run_id.clone(),
        attempt_id: descriptor.attempt_id.clone(),
        nonce_hash: hash_bytes(nonce),
        process_identity: identity,
        stdout: empty_log_ref(&descriptor.stdout_prefix_path, &descriptor.stdout_tail_path),
        stderr: empty_log_ref(&descriptor.stderr_prefix_path, &descriptor.stderr_tail_path),
        exit: RunExitResultV1 {
            kind: RunExitKindV1::FailedToStart,
            code: None,
            signal: None,
            timed_out: false,
            cancelled: false,
            observed_at: now.clone(),
        },
        observed_at: format!("{now}:cause_code={cause}"),
    };
    write_json_create_new_private(Path::new(&descriptor.exit_path), &observation)
}

fn write_startup_handshake(
    descriptor: &SupervisorControlDescriptorV1,
    identity: &RunProcessIdentityV1,
) -> Result<()> {
    let path = std::env::var_os(HIDDEN_SUPERVISOR_HANDSHAKE_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(&descriptor.heartbeat_path).with_extension("handshake.json")
        });
    let handshake = SupervisorStartupHandshakeV1 {
        schema_version: 1,
        run_id: descriptor.run_id.clone(),
        attempt_id: descriptor.attempt_id.clone(),
        nonce_hash: descriptor.nonce_hash.clone(),
        supervisor_pid: std::process::id() as u64,
        child_identity: identity.clone(),
        started_at: identity.started_at.clone(),
    };
    write_json_create_new_private(&path, &handshake)
}

fn write_heartbeat(
    descriptor: &SupervisorControlDescriptorV1,
    child_pid: Option<u64>,
) -> Result<()> {
    let heartbeat = SupervisorHeartbeatRecordV1 {
        schema_version: 1,
        run_id: descriptor.run_id.clone(),
        attempt_id: descriptor.attempt_id.clone(),
        observed_at: now_rfc3339(),
        supervisor_pid: std::process::id() as u64,
        child_pid,
        nonce_hash: descriptor.nonce_hash.clone(),
    };
    write_json_replace_private(Path::new(&descriptor.heartbeat_path), &heartbeat)
}

fn validate_descriptor(
    repo_root: &Path,
    control_relative: &Path,
    descriptor: &SupervisorControlDescriptorV1,
    nonce: &[u8],
) -> Result<()> {
    if descriptor.schema_version != 1 {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            "supervisor descriptor schema_version must be 1",
        ));
    }
    if descriptor.nonce_hash != hash_bytes(nonce) {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            "supervisor nonce hash mismatch",
        ));
    }
    validate_id("run", &descriptor.run_id, "run_")?;
    validate_id("attempt", &descriptor.attempt_id, "attempt_")?;
    let expected_control =
        PathBuf::from(MANAGED_CONTROL_PREFIX).join(format!("{}.json", descriptor.run_id));
    if control_relative != expected_control {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            "control path must be exact .pulse/runtime/run/control/<run_id>.json",
        ));
    }
    validate_descriptor_paths(repo_root, descriptor)?;
    if descriptor.max_stdout_bytes == 0 || descriptor.max_stderr_bytes == 0 {
        return Err(PulseError::validation(
            "run_profile_invalid",
            "log byte limits must be positive",
        ));
    }
    let workspace = Path::new(&descriptor.workspace_path);
    if !workspace.is_absolute() || !workspace.is_dir() {
        return Err(PulseError::validation(
            "run_workspace_not_found",
            "supervisor workspace must be an existing absolute directory",
        ));
    }
    let executable = Path::new(&descriptor.executable_path);
    revalidate_executable_identity(executable, &descriptor.executable_identity)?;
    let argv_hash = hash_argv(&descriptor.executable_path, &descriptor.argv)?;
    if argv_hash != descriptor.argv_hash {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            "argv hash does not match descriptor argv",
        ));
    }
    for arg in &descriptor.argv {
        if arg.contains('\0') {
            return Err(PulseError::validation(
                "run_control_record_invalid",
                "argv contains NUL",
            ));
        }
    }
    Ok(())
}

fn validate_descriptor_paths(
    repo_root: &Path,
    descriptor: &SupervisorControlDescriptorV1,
) -> Result<()> {
    let expected = BTreeMap::from([
        (
            "input_json_path",
            repo_root.join(format!(
                ".pulse/runtime/run/inputs/{}.{}.json",
                descriptor.run_id, descriptor.attempt_id
            )),
        ),
        (
            "stdout_prefix_path",
            repo_root.join(format!(
                ".pulse/runtime/run/logs/{}/{}.stdout.prefix.log",
                descriptor.run_id, descriptor.attempt_id
            )),
        ),
        (
            "stdout_tail_path",
            repo_root.join(format!(
                ".pulse/runtime/run/logs/{}/{}.stdout.tail.log",
                descriptor.run_id, descriptor.attempt_id
            )),
        ),
        (
            "stderr_prefix_path",
            repo_root.join(format!(
                ".pulse/runtime/run/logs/{}/{}.stderr.prefix.log",
                descriptor.run_id, descriptor.attempt_id
            )),
        ),
        (
            "stderr_tail_path",
            repo_root.join(format!(
                ".pulse/runtime/run/logs/{}/{}.stderr.tail.log",
                descriptor.run_id, descriptor.attempt_id
            )),
        ),
        (
            "heartbeat_path",
            repo_root.join(format!(
                ".pulse/runtime/run/control/{}.heartbeat.json",
                descriptor.run_id
            )),
        ),
        (
            "cancel_path",
            repo_root.join(format!(
                ".pulse/runtime/run/control/{}.cancel.json",
                descriptor.run_id
            )),
        ),
        (
            "exit_path",
            repo_root.join(format!(
                ".pulse/runtime/run/control/{}.exit.json",
                descriptor.run_id
            )),
        ),
    ]);
    let actual = BTreeMap::from([
        ("input_json_path", Path::new(&descriptor.input_json_path)),
        (
            "stdout_prefix_path",
            Path::new(&descriptor.stdout_prefix_path),
        ),
        ("stdout_tail_path", Path::new(&descriptor.stdout_tail_path)),
        (
            "stderr_prefix_path",
            Path::new(&descriptor.stderr_prefix_path),
        ),
        ("stderr_tail_path", Path::new(&descriptor.stderr_tail_path)),
        ("heartbeat_path", Path::new(&descriptor.heartbeat_path)),
        ("cancel_path", Path::new(&descriptor.cancel_path)),
        ("exit_path", Path::new(&descriptor.exit_path)),
    ]);
    let mut seen = std::collections::BTreeSet::new();
    for (field, expected_path) in expected {
        let path = actual.get(field).expect("descriptor path field listed");
        validate_managed_absolute_path(repo_root, path)?;
        if *path != expected_path.as_path() {
            return Err(PulseError::validation(
                "run_control_record_invalid",
                format!("descriptor {field} does not match exact managed layout"),
            ));
        }
        if !seen.insert((*path).to_path_buf()) {
            return Err(PulseError::validation(
                "run_control_record_invalid",
                "descriptor managed paths collide",
            ));
        }
    }
    Ok(())
}

fn validate_handshake(
    descriptor: &SupervisorControlDescriptorV1,
    handshake: &SupervisorStartupHandshakeV1,
) -> Result<()> {
    if handshake.schema_version != 1
        || handshake.run_id != descriptor.run_id
        || handshake.attempt_id != descriptor.attempt_id
        || handshake.nonce_hash != descriptor.nonce_hash
        || handshake.child_identity.supervisor_nonce_hash != descriptor.nonce_hash
        || handshake.child_identity.argv_hash != descriptor.argv_hash
        || handshake.child_identity.executable_identity != descriptor.executable_identity
        || handshake.child_identity.identity_status != "verified"
    {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            "startup handshake does not match supervisor descriptor",
        ));
    }
    let current = low_level_identity_from_run(&handshake.child_identity)?;
    match process_identity_matches(&current) {
        Ok(true) => Ok(()),
        Ok(false) => Err(PulseError::validation(
            "run_process_identity_mismatch",
            "startup child identity is no longer current",
        )),
        Err(error) => {
            if fast_exit_observation_matches_handshake(descriptor, handshake).unwrap_or(false) {
                Ok(())
            } else {
                Err(error)
            }
        }
    }
}

fn fast_exit_observation_matches_handshake(
    descriptor: &SupervisorControlDescriptorV1,
    handshake: &SupervisorStartupHandshakeV1,
) -> Result<bool> {
    let exit_path = Path::new(&descriptor.exit_path);
    if !exit_path.exists() {
        return Ok(false);
    }
    let observation: SupervisorExitObservationV1 = read_canonical_json(exit_path)?;
    Ok(observation.schema_version == 1
        && observation.run_id == descriptor.run_id
        && observation.attempt_id == descriptor.attempt_id
        && observation.nonce_hash == descriptor.nonce_hash
        && observation.process_identity == handshake.child_identity)
}

fn validate_cancel_request(
    descriptor: &SupervisorControlDescriptorV1,
    request: &SupervisorCancelRequestV1,
) -> Result<()> {
    if request.schema_version != 1
        || request.run_id != descriptor.run_id
        || request.attempt_id != descriptor.attempt_id
        || request.nonce_hash != descriptor.nonce_hash
    {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            "cancel request does not match supervisor descriptor",
        ));
    }
    Ok(())
}

fn read_nonce_from_environment() -> Result<Vec<u8>> {
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
    Ok(nonce)
}

fn read_canonical_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).map_err(|error| PulseError::io(path, error))?;
    serde_json::from_slice(&bytes).map_err(|error| PulseError::json(path, error))
}

fn join_log(handle: LogDrainHandle) -> Result<BoundedLogRefV1> {
    match handle.join() {
        Ok(result) => result,
        Err(_) => Err(PulseError::validation(
            "run_log_open_failed",
            "log drain thread panicked",
        )),
    }
}

fn run_exit_result(status: ExitStatus, timed_out: bool, cancelled: bool) -> RunExitResultV1 {
    let (code, signal) = wait_status_code(status);
    let kind = if timed_out {
        RunExitKindV1::TimedOut
    } else if cancelled {
        RunExitKindV1::Cancelled
    } else {
        RunExitKindV1::Exited
    };
    RunExitResultV1 {
        kind,
        code,
        signal,
        timed_out,
        cancelled,
        observed_at: now_rfc3339(),
    }
}

fn run_identity_from_low_level(
    descriptor: &SupervisorControlDescriptorV1,
    supervisor_pid: u64,
    low: &ProcessIdentityV1,
    started_at: &str,
) -> RunProcessIdentityV1 {
    RunProcessIdentityV1 {
        supervisor_pid,
        child_pid: low.pid as u64,
        process_group_id: low.process_group_id.map(|value| value as u64),
        supervisor_nonce_hash: descriptor.nonce_hash.clone(),
        started_at: started_at.to_string(),
        platform_start_marker: format!("{}:{}", low.platform, low.platform_start_marker),
        argv_hash: descriptor.argv_hash.clone(),
        executable_identity: descriptor.executable_identity.clone(),
        identity_status: low.identity_status.clone(),
    }
}

fn low_level_identity_from_run(identity: &RunProcessIdentityV1) -> Result<ProcessIdentityV1> {
    let Some((platform, marker)) = identity.platform_start_marker.split_once(':') else {
        return Err(PulseError::validation(
            "run_process_identity_unavailable",
            "run process identity marker is malformed",
        ));
    };
    Ok(ProcessIdentityV1 {
        pid: u32::try_from(identity.child_pid).map_err(|_| {
            PulseError::validation("run_process_identity_unavailable", "pid exceeds u32 range")
        })?,
        process_group_id: identity.process_group_id.map(|value| value as i64),
        platform: platform.to_string(),
        platform_start_marker: marker.to_string(),
        identity_status: identity.identity_status.clone(),
    })
}

fn empty_log_ref(prefix: &str, tail: &str) -> BoundedLogRefV1 {
    BoundedLogRefV1 {
        prefix_path: prefix.to_string(),
        tail_path: tail.to_string(),
        total_bytes_seen: 0,
        retained_bytes: 0,
        truncated_bytes: 0,
        content_hash: hash_bytes(&[]),
        content_hash_semantics: "sha256_full_stream_even_when_retention_truncated".to_string(),
        redaction_status: "not_applied_runtime_private".to_string(),
    }
}

fn build_environment_spec(
    selection: &RunnerProfileSelectionV1,
) -> Result<Vec<SupervisorEnvironmentEntryV1>> {
    let literal_values = literal_environment_values(selection)?;
    let mut entries = Vec::new();
    for env in &selection.environment {
        if env.source == RunnerEnvironmentSourceV1::LiteralNonSecret
            && !literal_values.contains_key(&env.name)
        {
            return Err(PulseError::validation(
                "run_profile_invalid",
                "literal environment spec is missing its private value",
            ));
        }
        entries.push(SupervisorEnvironmentEntryV1 {
            name: env.name.clone(),
            source: env.source.clone(),
        });
    }
    Ok(entries)
}

fn literal_environment_values(
    selection: &RunnerProfileSelectionV1,
) -> Result<BTreeMap<String, String>> {
    let mut literal_values = selection.literal_environment_values.clone();
    let mut values = BTreeMap::new();
    for env in &selection.environment {
        if env.source != RunnerEnvironmentSourceV1::LiteralNonSecret {
            continue;
        }
        let value = literal_values.remove(&env.name).ok_or_else(|| {
            PulseError::validation(
                "run_profile_invalid",
                "literal environment spec is missing its private value",
            )
        })?;
        let Some(value) = value.as_str() else {
            return Err(PulseError::validation(
                "run_profile_invalid",
                "literal environment values must be strings",
            ));
        };
        values.insert(env.name.clone(), value.to_string());
    }
    if !literal_values.is_empty() {
        return Err(PulseError::validation(
            "run_profile_invalid",
            "literal environment value without matching environment spec",
        ));
    }
    Ok(values)
}

fn hash_argv(executable: &str, argv: &[String]) -> Result<String> {
    hash_serializable(&(executable, argv))
}

fn revalidate_executable_identity(path: &Path, expected_identity: &str) -> Result<()> {
    if !path_is_safe_utf8_absolute_normalized(path) || path_has_symlink_component(path) {
        return Err(PulseError::validation(
            "run_command_not_found",
            "resolved executable path is not a normalized absolute non-symlink UTF-8 path",
        ));
    }
    let metadata = fs::symlink_metadata(path).map_err(|error| PulseError::io(path, error))?;
    if !metadata.is_file() {
        return Err(PulseError::validation(
            "run_command_not_found",
            "resolved executable is not a regular file",
        ));
    }
    if !has_effective_execute_permission(&metadata)? {
        return Err(PulseError::validation(
            "run_command_not_found",
            "resolved executable is not executable by the effective user",
        ));
    }
    let canonical = fs::canonicalize(path).map_err(|error| PulseError::io(path, error))?;
    if canonical != path || !path_is_safe_utf8_absolute_normalized(&canonical) {
        return Err(PulseError::validation(
            "run_command_not_found",
            "resolved executable canonical path changed before spawn",
        ));
    }
    let identity = executable_identity_hash(&canonical, &metadata)?;
    if identity != expected_identity {
        return Err(PulseError::validation(
            "run_process_identity_mismatch",
            "resolved executable identity changed before spawn",
        ));
    }
    Ok(())
}

fn executable_identity_hash(canonical: &Path, metadata: &fs::Metadata) -> Result<String> {
    #[derive(Serialize)]
    struct PortableExecutableIdentity<'a> {
        resolved_path: &'a str,
        len: u64,
        readonly: bool,
        modified_unix_seconds: Option<u64>,
        #[cfg(unix)]
        unix_dev: u64,
        #[cfg(unix)]
        unix_ino: u64,
        #[cfg(unix)]
        unix_mode: u32,
    }
    let modified_unix_seconds = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());
    let resolved_path = path_to_string(canonical)?;
    let identity = PortableExecutableIdentity {
        resolved_path: &resolved_path,
        len: metadata.len(),
        readonly: metadata.permissions().readonly(),
        modified_unix_seconds,
        #[cfg(unix)]
        unix_dev: metadata.dev(),
        #[cfg(unix)]
        unix_ino: metadata.ino(),
        #[cfg(unix)]
        unix_mode: metadata.mode(),
    };
    hash_serializable(&identity)
}

fn validate_managed_control_file(repo_root: &Path, path: &Path) -> Result<()> {
    validate_managed_absolute_path(repo_root, path)?;
    if path.extension().and_then(OsStr::to_str) != Some("json") {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            "managed control file must be json",
        ));
    }
    Ok(())
}

fn validate_managed_absolute_path(repo_root: &Path, path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            "managed runtime path must be absolute",
        ));
    }
    if !path_is_safe_utf8_absolute_normalized(path) {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            "managed runtime path must be normalized UTF-8 without traversal or backslash",
        ));
    }
    if !path.starts_with(repo_root.join(".pulse/runtime/run")) {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            "managed runtime path escapes .pulse/runtime/run",
        ));
    }
    ensure_no_symlink_component_except_missing_final(repo_root, path)?;
    Ok(())
}

fn path_is_safe_utf8_absolute_normalized(path: &Path) -> bool {
    path.is_absolute()
        && path
            .as_os_str()
            .to_str()
            .map(|value| !value.contains('\\'))
            .unwrap_or(false)
        && path
            .components()
            .all(|component| matches!(component, Component::RootDir | Component::Normal(_)))
}

fn path_has_symlink_component(path: &Path) -> bool {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir => current.push(component.as_os_str()),
            Component::Normal(part) => {
                current.push(part);
                match fs::symlink_metadata(&current) {
                    Ok(metadata) if metadata.file_type().is_symlink() => return true,
                    Ok(_) => {}
                    Err(_) => return true,
                }
            }
            Component::CurDir | Component::ParentDir | Component::Prefix(_) => return true,
        }
    }
    false
}

fn ensure_no_symlink_component_except_missing_final(repo_root: &Path, path: &Path) -> Result<()> {
    let managed_root = repo_root.join(".pulse/runtime/run");
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir => current.push(component.as_os_str()),
            Component::Normal(part) => {
                current.push(part);
                match fs::symlink_metadata(&current) {
                    Ok(metadata) if metadata.file_type().is_symlink() => {
                        return Err(PulseError::validation(
                            "run_control_record_invalid",
                            "managed runtime path contains symlink component",
                        ));
                    }
                    Ok(metadata) if metadata.is_dir() || metadata.is_file() => {}
                    Ok(_) => {
                        return Err(PulseError::validation(
                            "run_control_record_invalid",
                            "managed runtime path contains unsupported file type",
                        ));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        if current == path {
                            break;
                        }
                        if current.starts_with(&managed_root) {
                            fs::create_dir_all(&current)
                                .map_err(|error| PulseError::io(&current, error))?;
                            set_private_dir_permissions(&current)?;
                        } else {
                            return Err(PulseError::validation(
                                "run_control_record_invalid",
                                "managed runtime parent is missing outside runtime root",
                            ));
                        }
                    }
                    Err(error) => return Err(PulseError::io(&current, error)),
                }
            }
            Component::CurDir | Component::ParentDir | Component::Prefix(_) => {
                return Err(PulseError::validation(
                    "run_control_record_invalid",
                    "managed runtime path contains traversal",
                ));
            }
        }
    }
    Ok(())
}

fn validate_id(kind: &'static str, value: &str, prefix: &str) -> Result<()> {
    if !value.starts_with(prefix)
        || value.len() <= prefix.len()
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
    {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            format!("invalid {kind} id"),
        ));
    }
    Ok(())
}

fn path_to_string(path: &Path) -> Result<String> {
    path.to_str()
        .map(ToString::to_string)
        .ok_or_else(|| PulseError::validation("run_control_record_invalid", "path is not UTF-8"))
}

fn write_json_create_new_private<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = to_canonical_bytes(value)?;
    write_private_create_new(path, &bytes)
}

fn write_json_replace_private<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = to_canonical_bytes(value)?;
    write_private_replace(path, &bytes)
}

fn write_private_create_new(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
        set_private_dir_permissions(parent)?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(path)
        .map_err(|error| PulseError::io(path, error))?;
    let result = (|| -> Result<()> {
        file.write_all(bytes)
            .map_err(|error| PulseError::io(path, error))?;
        file.flush().map_err(|error| PulseError::io(path, error))?;
        file.sync_all()
            .map_err(|error| PulseError::io(path, error))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}

fn write_private_replace(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
        set_private_dir_permissions(parent)?;
    }
    let temp = unique_private_temp(path);
    write_private_create_new(&temp, bytes)?;
    if let Err(error) = fs::rename(&temp, path).map_err(|error| PulseError::io(path, error)) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    Ok(())
}

fn unique_private_temp(path: &Path) -> PathBuf {
    let mut random = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut random);
    let file = path.file_name().and_then(OsStr::to_str).unwrap_or("record");
    path.with_file_name(format!(".{file}.pulse-tmp-{}", hex::encode(random)))
}

fn set_private_dir_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| PulseError::io(path, error))?;
    }
    Ok(())
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
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
fn has_effective_execute_permission(metadata: &fs::Metadata) -> Result<bool> {
    Ok(effective_execute_allowed(
        metadata.permissions().mode(),
        metadata.uid(),
        metadata.gid(),
        unix_geteuid(),
        &effective_groups()?,
    ))
}

#[cfg(unix)]
fn effective_execute_allowed(
    mode: u32,
    file_uid: u32,
    file_gid: u32,
    effective_uid: u32,
    effective_groups: &[u32],
) -> bool {
    if effective_uid == 0 {
        return mode & 0o111 != 0;
    }
    if effective_uid == file_uid {
        return mode & 0o100 != 0;
    }
    if effective_groups.contains(&file_gid) {
        return mode & 0o010 != 0;
    }
    mode & 0o001 != 0
}

#[cfg(unix)]
fn effective_groups() -> Result<Vec<u32>> {
    let primary_gid = unix_getegid();
    let count = unix_getgroups_count()?;
    let mut groups = vec![0_u32; count];
    if count > 0 {
        let read = unsafe { getgroups(count as i32, groups.as_mut_ptr()) };
        if read < 0 {
            return Err(PulseError::validation(
                "run_platform_unsupported",
                "could not inspect effective Unix groups for executable permission validation",
            ));
        }
        groups.truncate(read as usize);
    }
    if !groups.contains(&primary_gid) {
        groups.push(primary_gid);
    }
    Ok(groups)
}

#[cfg(unix)]
fn unix_getgroups_count() -> Result<usize> {
    let count = unsafe { getgroups(0, std::ptr::null_mut()) };
    if count < 0 {
        return Err(PulseError::validation(
            "run_platform_unsupported",
            "could not inspect Unix group count for executable permission validation",
        ));
    }
    Ok(count as usize)
}

#[cfg(unix)]
fn unix_geteuid() -> u32 {
    unsafe { geteuid() }
}

#[cfg(unix)]
fn unix_getegid() -> u32 {
    unsafe { getegid() }
}

#[cfg(unix)]
extern "C" {
    fn geteuid() -> u32;
    fn getegid() -> u32;
    fn getgroups(size: i32, list: *mut u32) -> i32;
}

#[cfg(target_os = "linux")]
fn clear_cloexec(fd: RawFd) {
    update_cloexec(fd, false);
}

#[cfg(target_os = "linux")]
fn set_cloexec(fd: RawFd) {
    update_cloexec(fd, true);
}

#[cfg(target_os = "linux")]
fn update_cloexec(fd: RawFd, enabled: bool) {
    unsafe {
        let flags = fcntl(fd, F_GETFD);
        if flags >= 0 {
            let updated = if enabled {
                flags | FD_CLOEXEC
            } else {
                flags & !FD_CLOEXEC
            };
            let _ = fcntl(fd, F_SETFD, updated);
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn clear_cloexec(_fd: RawFd) {}

#[cfg(not(target_os = "linux"))]
fn set_cloexec(_fd: RawFd) {}

#[cfg(target_os = "linux")]
fn setpgid_zero() -> i32 {
    unsafe { setpgid(0, 0) }
}

#[cfg(target_os = "linux")]
fn kill_process_group(pgid: i64, signal: i32) -> i32 {
    unsafe { kill(-(pgid as i32), signal) }
}

#[cfg(target_os = "linux")]
extern "C" {
    fn setpgid(pid: i32, pgid: i32) -> i32;
    fn kill(pid: i32, sig: i32) -> i32;
    fn fcntl(fd: i32, cmd: i32, ...) -> i32;
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
