use serde::{Deserialize, Serialize};
#[cfg(target_os = "linux")]
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use crate::canonical_json::hash_serializable;
use crate::{PulseError, Result};

#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "linux")]
const PLATFORM: &str = "linux_proc_starttime_process_group";
#[cfg(target_os = "macos")]
const PLATFORM: &str = "macos_libproc_starttime_process_group";
#[cfg(target_os = "windows")]
const PLATFORM: &str = "windows_job_object_creation_time";
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const PLATFORM: &str = "unsupported";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NativeProcessIdentity {
    pub pid: u32,
    pub process_group_id: Option<i64>,
    pub platform: String,
    pub platform_start_marker: String,
    pub identity_status: String,
}

pub fn ensure_supported_platform() -> Result<()> {
    if PLATFORM == "unsupported" {
        Err(PulseError::validation(
            "managed_process_platform_unsupported",
            "daemon ProcessOwner supports Linux, macOS and Windows",
        ))
    } else {
        Ok(())
    }
}

pub fn spawn_process_group(command: &mut Command) -> Result<Child> {
    ensure_supported_platform()?;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        });
    }
    #[cfg(windows)]
    command.creation_flags(
        windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP
            | windows_sys::Win32::System::Threading::CREATE_SUSPENDED,
    );
    let child = command
        .spawn()
        .map_err(|error| PulseError::io("<managed-process-spawn>", error))?;
    #[cfg(windows)]
    let child = {
        let mut child = child;
        if let Err(error) =
            assign_windows_job(&child).and_then(|_| resume_windows_child(child.id()))
        {
            finish_windows_job(child.id());
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        child
    };
    Ok(child)
}

pub fn current_process_identity(pid: u32) -> Result<NativeProcessIdentity> {
    platform_process_identity(pid)
}

pub fn current_process_executable(pid: u32) -> Result<PathBuf> {
    process_executable(pid)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessIdentityStatus {
    Match,
    Absent,
    Mismatch,
}

/// Decide whether the process currently owning `expected.pid` is the same
/// incarnation we recorded, or whether the pid is now absent / a different
/// (reused) incarnation.
///
/// Identity is authoritative on `(pid, platform, start-marker,
/// process-group)`: the high-resolution process start timestamp uniquely
/// identifies a process incarnation per boot, so a reused pid always carries a
/// different start-marker and is rejected. `pid_reuse_identity_mismatch_refuses_cancellation`
/// pins that contract.
///
/// The executable path is deliberately NOT part of this decision. On macOS
/// `proc_pidpath` is observably non-deterministic for a process launched as
/// `/bin/sh` (which is GNU bash 3.2 in sh mode): the same live process can
/// report `/bin/sh` at one instant and `/bin/bash` at another, so an exact
/// executable-path comparison produces false mismatches that would refuse to
/// cancel a process we own (a correctness bug). The start-marker already
/// subsumes the anti-PID-reuse role the executable check played, so dropping it
/// is strictly more reliable. `expected_executable` is retained only for caller
/// symmetry.
pub fn process_identity_status(
    expected: &NativeProcessIdentity,
    _expected_executable: &Path,
) -> Result<ProcessIdentityStatus> {
    let current = match current_process_identity(expected.pid) {
        Ok(current) => current,
        Err(error) if error.code() == "managed_process_not_found" => {
            return Ok(ProcessIdentityStatus::Absent)
        }
        Err(error) => return Err(error),
    };
    if current.platform == expected.platform
        && current.platform_start_marker == expected.platform_start_marker
        && current.process_group_id == expected.process_group_id
    {
        Ok(ProcessIdentityStatus::Match)
    } else {
        Ok(ProcessIdentityStatus::Mismatch)
    }
}

pub fn process_identity_matches(
    expected: &NativeProcessIdentity,
    expected_executable: &Path,
) -> Result<bool> {
    Ok(process_identity_status(expected, expected_executable)? == ProcessIdentityStatus::Match)
}

pub fn terminate_process_group(identity: &NativeProcessIdentity, executable: &Path) -> Result<()> {
    if !process_identity_matches(identity, executable)? {
        return Err(PulseError::validation(
            "managed_process_identity_mismatch",
            "PID/platform/start-marker/process-group identity does not match the managed process",
        ));
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let pgid = identity.process_group_id.ok_or_else(|| {
            PulseError::validation(
                "managed_process_identity_unavailable",
                "managed process group ID is unavailable",
            )
        })?;
        let result = unsafe { libc::kill(-(pgid as libc::pid_t), libc::SIGTERM) };
        if result != 0 {
            return Err(PulseError::validation(
                "managed_process_interrupt_failed",
                std::io::Error::last_os_error().to_string(),
            ));
        }
    }
    #[cfg(windows)]
    terminate_windows_job(identity.pid)?;
    Ok(())
}

pub fn force_terminate_process_group(
    identity: &NativeProcessIdentity,
    executable: &Path,
) -> Result<()> {
    if !process_identity_matches(identity, executable)? {
        return Ok(());
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let pgid = identity.process_group_id.ok_or_else(|| {
            PulseError::validation(
                "managed_process_identity_unavailable",
                "managed process group ID is unavailable",
            )
        })?;
        if unsafe { libc::kill(-(pgid as libc::pid_t), libc::SIGKILL) } != 0 {
            return Err(PulseError::validation(
                "managed_process_force_cancel_failed",
                std::io::Error::last_os_error().to_string(),
            ));
        }
    }
    #[cfg(windows)]
    terminate_windows_job(identity.pid)?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn platform_process_identity(pid: u32) -> Result<NativeProcessIdentity> {
    let stat_path = PathBuf::from(format!("/proc/{pid}/stat"));
    let stat = fs::read_to_string(&stat_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            PulseError::validation(
                "managed_process_not_found",
                format!("process {pid} no longer exists"),
            )
        } else {
            PulseError::io(&stat_path, error)
        }
    })?;
    let close = stat.rfind(')').ok_or_else(|| {
        PulseError::validation(
            "managed_process_identity_unavailable",
            "malformed /proc stat",
        )
    })?;
    let fields = stat[close + 2..].split_whitespace().collect::<Vec<_>>();
    let pgrp = fields
        .get(2)
        .ok_or_else(|| {
            PulseError::validation(
                "managed_process_identity_unavailable",
                "missing process group",
            )
        })?
        .parse::<i64>()
        .map_err(|_| {
            PulseError::validation(
                "managed_process_identity_unavailable",
                "invalid process group",
            )
        })?;
    let start_ticks = fields.get(19).ok_or_else(|| {
        PulseError::validation(
            "managed_process_identity_unavailable",
            "missing process start marker",
        )
    })?;
    let boot_id = fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap_or_default();
    Ok(NativeProcessIdentity {
        pid,
        process_group_id: Some(pgrp),
        platform: PLATFORM.to_string(),
        platform_start_marker: hash_serializable(&(boot_id.trim(), start_ticks))?,
        identity_status: "verified".to_string(),
    })
}

#[cfg(target_os = "linux")]
fn process_executable(pid: u32) -> Result<PathBuf> {
    fs::read_link(format!("/proc/{pid}/exe")).map_err(|error| {
        PulseError::validation(
            "managed_process_identity_unavailable",
            format!("cannot inspect process executable: {error}"),
        )
    })
}

#[cfg(target_os = "macos")]
fn platform_process_identity(pid: u32) -> Result<NativeProcessIdentity> {
    let info = macos_bsd_info(pid)?;
    Ok(NativeProcessIdentity {
        pid,
        process_group_id: Some(i64::from(info.pbi_pgid)),
        platform: PLATFORM.to_string(),
        platform_start_marker: hash_serializable(&(info.pbi_start_tvsec, info.pbi_start_tvusec))?,
        identity_status: "verified".to_string(),
    })
}

#[cfg(target_os = "macos")]
fn macos_bsd_info(pid: u32) -> Result<libc::proc_bsdinfo> {
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = std::mem::size_of::<libc::proc_bsdinfo>();
    let read = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size as libc::c_int,
        )
    };
    if read != size as libc::c_int {
        let error = std::io::Error::last_os_error();
        let code = if error.raw_os_error() == Some(libc::ESRCH) {
            "managed_process_not_found"
        } else {
            "managed_process_identity_unavailable"
        };
        return Err(PulseError::validation(code, error.to_string()));
    }
    let info = unsafe { info.assume_init() };
    if info.pbi_pid != pid || info.pbi_pgid == 0 {
        return Err(PulseError::validation(
            "managed_process_identity_unavailable",
            "libproc returned an inconsistent process identity",
        ));
    }
    Ok(info)
}

#[cfg(target_os = "macos")]
fn process_executable(pid: u32) -> Result<PathBuf> {
    let mut buffer = vec![0_u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let read = unsafe {
        libc::proc_pidpath(
            pid as libc::c_int,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
        )
    };
    if read <= 0 {
        return Err(PulseError::validation(
            "managed_process_identity_unavailable",
            "libproc could not inspect process executable",
        ));
    }
    buffer.truncate(read as usize);
    let path = std::str::from_utf8(&buffer)
        .map_err(|_| {
            PulseError::validation(
                "managed_process_identity_unavailable",
                "process executable path is not UTF-8",
            )
        })?
        .trim_end_matches('\0');
    PathBuf::from(path)
        .canonicalize()
        .map_err(|error| PulseError::io(path, error))
}

#[cfg(windows)]
static WINDOWS_JOBS: once_cell::sync::Lazy<
    std::sync::Mutex<std::collections::BTreeMap<u32, isize>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(std::collections::BTreeMap::new()));

#[cfg(windows)]
fn assign_windows_job(child: &Child) -> Result<()> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job == 0 {
        return Err(last_windows_error("CreateJobObjectW"));
    }
    let mut limits = unsafe { std::mem::zeroed::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let configured = unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    let assigned = if configured != 0 {
        unsafe { AssignProcessToJobObject(job, child.as_raw_handle() as isize) }
    } else {
        0
    };
    if configured == 0 || assigned == 0 {
        unsafe { CloseHandle(job) };
        return Err(last_windows_error("AssignProcessToJobObject"));
    }
    WINDOWS_JOBS
        .lock()
        .map_err(|_| lock_error())?
        .insert(child.id(), job);
    Ok(())
}

#[cfg(windows)]
fn resume_windows_child(pid: u32) -> Result<()> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
    };
    use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(last_windows_error("CreateToolhelp32Snapshot"));
    }
    let mut entry = unsafe { std::mem::zeroed::<THREADENTRY32>() };
    entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
    let mut available = unsafe { Thread32First(snapshot, &mut entry) } != 0;
    let mut resumed = false;
    while available {
        if entry.th32OwnerProcessID == pid {
            let handle = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if handle != 0 {
                resumed = unsafe { ResumeThread(handle) } != u32::MAX;
                unsafe { CloseHandle(handle) };
                if resumed {
                    break;
                }
            }
        }
        available = unsafe { Thread32Next(snapshot, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snapshot) };
    if resumed {
        Ok(())
    } else {
        Err(PulseError::validation(
            "managed_process_identity_unavailable",
            "could not resume managed Windows process",
        ))
    }
}

#[cfg(windows)]
fn terminate_windows_job(pid: u32) -> Result<()> {
    use windows_sys::Win32::System::JobObjects::TerminateJobObject;
    let jobs = WINDOWS_JOBS.lock().map_err(|_| lock_error())?;
    let job = jobs.get(&pid).copied().ok_or_else(|| {
        PulseError::validation(
            "managed_process_identity_unavailable",
            "owned Windows Job Object is unavailable",
        )
    })?;
    if unsafe { TerminateJobObject(job, 1) } == 0 {
        return Err(last_windows_error("TerminateJobObject"));
    }
    Ok(())
}

#[cfg(windows)]
fn finish_windows_job(pid: u32) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::JobObjects::TerminateJobObject;
    if let Ok(mut jobs) = WINDOWS_JOBS.lock() {
        if let Some(job) = jobs.remove(&pid) {
            unsafe {
                let _ = TerminateJobObject(job, 1);
                CloseHandle(job);
            }
        }
    }
}

#[cfg(windows)]
fn platform_process_identity(pid: u32) -> Result<NativeProcessIdentity> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        SYNCHRONIZE,
    };
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, 0, pid) };
    if process == 0 {
        return Err(PulseError::validation(
            "managed_process_not_found",
            format!("process {pid} no longer exists"),
        ));
    }
    let mut creation = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut exit = creation;
    let mut kernel = creation;
    let mut user = creation;
    let mut code = 0_u32;
    let times =
        unsafe { GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user) };
    let active = unsafe { GetExitCodeProcess(process, &mut code) };
    unsafe { CloseHandle(process) };
    if times == 0 || active == 0 || code != STILL_ACTIVE {
        return Err(PulseError::validation(
            "managed_process_not_found",
            format!("process {pid} is not active"),
        ));
    }
    Ok(NativeProcessIdentity {
        pid,
        process_group_id: Some(i64::from(pid)),
        platform: PLATFORM.to_string(),
        platform_start_marker: hash_serializable(&(
            creation.dwHighDateTime,
            creation.dwLowDateTime,
        ))?,
        identity_status: "verified".to_string(),
    })
}

#[cfg(windows)]
fn process_executable(pid: u32) -> Result<PathBuf> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process == 0 {
        return Err(last_windows_error("OpenProcess"));
    }
    let mut buffer = vec![0_u16; 32768];
    let mut size = buffer.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut size)
    };
    unsafe { CloseHandle(process) };
    if result == 0 {
        return Err(last_windows_error("QueryFullProcessImageNameW"));
    }
    buffer.truncate(size as usize);
    PathBuf::from(String::from_utf16_lossy(&buffer))
        .canonicalize()
        .map_err(|error| PulseError::io("<managed-process-executable>", error))
}

#[cfg(windows)]
fn last_windows_error(operation: &str) -> PulseError {
    PulseError::validation(
        "managed_process_identity_unavailable",
        format!("{operation} failed: {}", std::io::Error::last_os_error()),
    )
}

#[cfg(windows)]
fn lock_error() -> PulseError {
    PulseError::validation(
        "managed_process_identity_unavailable",
        "Windows Job Object registry lock was poisoned",
    )
}
