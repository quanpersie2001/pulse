//! Preserve-only runner profile registry loading for P2S3-I2.
//!
//! This module owns tracked runner-profile config IO and local executable
//! resolution. It deliberately does not own runtime run/attempt records,
//! process launch, Git/workspace inspection, authority, or graph semantics.

use crate::canonical_json::{hash_bytes, hash_serializable, to_canonical_bytes};
use crate::kernel::assignment_store;
use crate::run::{
    ResumeEligibilityV1, RunAttemptRecordV1, RunInputV1, RunListReportV1, RunRecordV1,
    RunRecoveryClassificationV1, RunViewV1, RunnerAdapterV1, RunnerEnvironmentSourceV1,
    RunnerEnvironmentSpecEntryV1, RunnerExecutableIdentityV1, RunnerProfileRegistryV1,
    RunnerProfileSelectionV1, WorkspaceSnapshotV1, RUN_SCHEMA_VERSION,
};
use crate::{PulseError, PulseResult};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

pub const RUNNER_PROFILE_REGISTRY_RELATIVE_PATH: &str = ".pulse/run/runner-profiles.json";

pub const RUN_RUNTIME_ROOT: &str = ".pulse/runtime/run";
pub const RUNS_DIR: &str = "runs";
pub const ATTEMPTS_DIR: &str = "attempts";
pub const CONTROL_DIR: &str = "control";
pub const INPUTS_DIR: &str = "inputs";
pub const LOGS_DIR: &str = "logs";
pub const SNAPSHOTS_DIR: &str = "snapshots";
const RECORD_EXTENSION: &str = "json";
const PROMPT_EXTENSION: &str = "md";
const LOG_EXTENSION: &str = "log";
const MAX_RECORD_BYTES: u64 = 1024 * 1024;
const MAX_PROMPT_BYTES: u64 = 2 * 1024 * 1024;
const MAX_LOG_SEGMENT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 4096;
const MAX_CLASSIFICATIONS: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStoreWriteMode {
    CreateNew,
    Replace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunStoreRecordStatus {
    Valid,
    Missing,
    Invalid,
    Ambiguous,
    OrphanControl,
    OrphanLog,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunStoreClassification {
    pub run_id: Option<String>,
    pub attempt_id: Option<String>,
    pub status: RunStoreRecordStatus,
    pub path: Option<String>,
    pub reason_codes: Vec<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SupervisorControlRecordV1 {
    pub schema_version: u32,
    pub run_id: String,
    pub attempt_id: String,
    pub nonce_hash: String,
    pub input_json_path: String,
    pub stdout_prefix_path: String,
    pub stdout_tail_path: String,
    pub stderr_prefix_path: String,
    pub stderr_tail_path: String,
    pub created_at: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SupervisorHeartbeatRecordV1 {
    pub schema_version: u32,
    pub run_id: String,
    pub attempt_id: String,
    pub observed_at: String,
    pub supervisor_pid: u64,
    pub nonce_hash: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SupervisorCancelRequestV1 {
    pub schema_version: u32,
    pub run_id: String,
    pub attempt_id: String,
    pub requested_at: String,
    pub requested_by: String,
    pub reason: String,
    pub grace_seconds: u64,
    pub force_allowed: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SupervisorExitObservationV1 {
    pub schema_version: u32,
    pub run_id: String,
    pub attempt_id: String,
    pub nonce_hash: String,
    pub exit: crate::run::RunExitResultV1,
    pub observed_at: String,
}

pub fn run_runtime_root(repo_root: &Path) -> PathBuf {
    repo_root.join(RUN_RUNTIME_ROOT)
}

pub fn runs_dir(repo_root: &Path) -> PathBuf {
    run_runtime_root(repo_root).join(RUNS_DIR)
}

pub fn attempts_dir(repo_root: &Path) -> PathBuf {
    run_runtime_root(repo_root).join(ATTEMPTS_DIR)
}

pub fn control_dir(repo_root: &Path) -> PathBuf {
    run_runtime_root(repo_root).join(CONTROL_DIR)
}

pub fn inputs_dir(repo_root: &Path) -> PathBuf {
    run_runtime_root(repo_root).join(INPUTS_DIR)
}

pub fn logs_dir(repo_root: &Path) -> PathBuf {
    run_runtime_root(repo_root).join(LOGS_DIR)
}

pub fn snapshots_dir(repo_root: &Path) -> PathBuf {
    run_runtime_root(repo_root).join(SNAPSHOTS_DIR)
}

pub fn run_record_path(repo_root: &Path, run_id: &str) -> PulseResult<PathBuf> {
    record_path(runs_dir(repo_root), "run", run_id, "run_")
}

pub fn attempt_record_path(repo_root: &Path, attempt_id: &str) -> PulseResult<PathBuf> {
    record_path(attempts_dir(repo_root), "attempt", attempt_id, "attempt_")
}

pub fn control_record_path(repo_root: &Path, run_id: &str) -> PulseResult<PathBuf> {
    record_path(control_dir(repo_root), "run", run_id, "run_")
}

pub fn cancel_request_path(repo_root: &Path, run_id: &str) -> PulseResult<PathBuf> {
    suffixed_control_path(repo_root, run_id, "cancel")
}

pub fn exit_observation_path(repo_root: &Path, run_id: &str) -> PulseResult<PathBuf> {
    suffixed_control_path(repo_root, run_id, "exit")
}

pub fn heartbeat_path(repo_root: &Path, run_id: &str) -> PulseResult<PathBuf> {
    suffixed_control_path(repo_root, run_id, "heartbeat")
}

pub fn input_json_path(repo_root: &Path, run_id: &str, attempt_id: &str) -> PulseResult<PathBuf> {
    validate_run_id(run_id)?;
    validate_attempt_id(attempt_id)?;
    Ok(inputs_dir(repo_root).join(input_json_filename(run_id, attempt_id)))
}

pub fn input_prompt_path(repo_root: &Path, run_id: &str, attempt_id: &str) -> PulseResult<PathBuf> {
    validate_run_id(run_id)?;
    validate_attempt_id(attempt_id)?;
    Ok(inputs_dir(repo_root).join(input_prompt_filename(run_id, attempt_id)))
}

pub fn snapshot_path(repo_root: &Path, attempt_id: &str, phase: &str) -> PulseResult<PathBuf> {
    validate_attempt_id(attempt_id)?;
    validate_snapshot_phase(phase)?;
    Ok(snapshots_dir(repo_root).join(snapshot_filename(attempt_id, phase)))
}

pub fn log_segment_path(
    repo_root: &Path,
    run_id: &str,
    attempt_id: &str,
    stream: &str,
    segment: &str,
) -> PulseResult<PathBuf> {
    validate_run_id(run_id)?;
    validate_attempt_id(attempt_id)?;
    validate_log_stream(stream)?;
    validate_log_segment(segment)?;
    Ok(logs_dir(repo_root)
        .join(run_id)
        .join(log_segment_filename(attempt_id, stream, segment)))
}

pub fn managed_relative_path(repo_root: &Path, path: &Path) -> PulseResult<String> {
    let relative = path
        .strip_prefix(repo_root)
        .map_err(|_| PulseError::PathEscape {
            path: path.to_path_buf(),
        })?;
    validate_managed_relative_path(relative)?;
    relative.to_str().map(str::to_string).ok_or_else(|| {
        PulseError::validation(
            "run_control_record_invalid",
            "managed runtime paths must be valid UTF-8",
        )
    })
}

pub fn validate_managed_relative_path(relative: &Path) -> PulseResult<()> {
    if relative.is_absolute() {
        return Err(PulseError::AbsolutePath {
            path: relative.to_path_buf(),
        });
    }
    let mut components = relative.components();
    match (components.next(), components.next(), components.next()) {
        (Some(Component::Normal(a)), Some(Component::Normal(b)), Some(Component::Normal(c)))
            if a == ".pulse" && b == "runtime" && c == "run" => {}
        _ => {
            return Err(PulseError::validation(
                "run_control_record_invalid",
                "path is not under .pulse/runtime/run",
            ));
        }
    }
    if relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(PulseError::PathTraversal {
            path: relative.to_path_buf(),
        });
    }
    if relative.as_os_str().to_str().is_none() {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            "managed runtime path must be valid UTF-8",
        ));
    }
    Ok(())
}

pub fn resolve_control_relative_path(repo_root: &Path, relative: &Path) -> PulseResult<PathBuf> {
    validate_managed_relative_path(relative)?;
    let mut components = relative.components();
    let prefix = PathBuf::from(".pulse")
        .join("runtime")
        .join("run")
        .join("control");
    if components.next() != Some(Component::Normal(".pulse".as_ref()))
        || components.next() != Some(Component::Normal("runtime".as_ref()))
        || components.next() != Some(Component::Normal("run".as_ref()))
        || components.next() != Some(Component::Normal("control".as_ref()))
        || relative.components().count() != prefix.components().count() + 1
    {
        return Err(PulseError::validation(
            "run_control_record_invalid",
            "supervisor control path must be a direct file under .pulse/runtime/run/control",
        ));
    }
    let absolute = repo_root.join(relative);
    ensure_managed_existing_parent(repo_root, &absolute)?;
    Ok(absolute)
}

fn record_path(dir: PathBuf, kind: &str, id: &str, expected_prefix: &str) -> PulseResult<PathBuf> {
    validate_record_id(kind, id, expected_prefix)?;
    Ok(dir.join(format!("{id}.{RECORD_EXTENSION}")))
}

fn suffixed_control_path(repo_root: &Path, run_id: &str, suffix: &str) -> PulseResult<PathBuf> {
    validate_run_id(run_id)?;
    validate_token("control suffix", suffix)?;
    Ok(control_dir(repo_root).join(format!("{run_id}.{suffix}.{RECORD_EXTENSION}")))
}

fn validate_run_id(run_id: &str) -> PulseResult<()> {
    validate_record_id("run", run_id, "run_")
}

fn validate_attempt_id(attempt_id: &str) -> PulseResult<()> {
    validate_record_id("attempt", attempt_id, "attempt_")
}

fn validate_record_id(kind: &str, id: &str, expected_prefix: &str) -> PulseResult<()> {
    if id.is_empty()
        || !id.starts_with(expected_prefix)
        || id.len() == expected_prefix.len()
        || id.contains('/')
        || id.contains('\\')
        || id.contains('.')
        || Path::new(id)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(PulseError::validation(
            "invalid_run_record_id",
            format!(
                "{kind} id {id:?} is not filesystem-safe or does not start with {expected_prefix:?}"
            ),
        ));
    }
    Ok(())
}

fn validate_token(kind: &str, value: &str) -> PulseResult<()> {
    if value.is_empty()
        || value.contains('/')
        || value.contains('\\')
        || value.contains('.')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(PulseError::validation(
            "invalid_run_record_id",
            format!("{kind} {value:?} is not filesystem-safe"),
        ));
    }
    Ok(())
}

fn validate_log_stream(stream: &str) -> PulseResult<()> {
    match stream {
        "stdout" | "stderr" => Ok(()),
        _ => Err(PulseError::validation(
            "invalid_run_record_id",
            "log stream must be stdout or stderr",
        )),
    }
}

fn validate_log_segment(segment: &str) -> PulseResult<()> {
    match segment {
        "prefix" | "tail" => Ok(()),
        _ => Err(PulseError::validation(
            "invalid_run_record_id",
            "log segment must be prefix or tail",
        )),
    }
}

fn validate_snapshot_phase(phase: &str) -> PulseResult<()> {
    match phase {
        "before" | "after" => Ok(()),
        _ => Err(PulseError::validation(
            "invalid_run_record_id",
            "snapshot phase must be before or after",
        )),
    }
}

fn input_json_filename(run_id: &str, attempt_id: &str) -> String {
    format!("{run_id}.{attempt_id}.{RECORD_EXTENSION}")
}

fn input_prompt_filename(run_id: &str, attempt_id: &str) -> String {
    format!("{run_id}.{attempt_id}.{PROMPT_EXTENSION}")
}

fn snapshot_filename(attempt_id: &str, phase: &str) -> String {
    format!("{attempt_id}.{phase}.{RECORD_EXTENSION}")
}

fn log_segment_filename(attempt_id: &str, stream: &str, segment: &str) -> String {
    format!("{attempt_id}.{stream}.{segment}.{LOG_EXTENSION}")
}

fn ensure_managed_existing_parent(repo_root: &Path, path: &Path) -> PulseResult<()> {
    let parent = path.parent().ok_or_else(|| {
        PulseError::validation("run_control_record_invalid", "managed path has no parent")
    })?;
    ensure_no_symlink_escape(repo_root, parent, false)
}

fn ensure_managed_target_for_write(repo_root: &Path, path: &Path) -> PulseResult<()> {
    let parent = path.parent().ok_or_else(|| {
        PulseError::validation("run_control_record_invalid", "managed path has no parent")
    })?;
    ensure_no_symlink_escape(repo_root, parent, true)?;
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(PulseError::validation(
                "run_control_record_invalid",
                "managed target must be a regular non-symlink file",
            ));
        }
    }
    Ok(())
}

fn ensure_no_symlink_escape(
    repo_root: &Path,
    path: &Path,
    allow_missing_final: bool,
) -> PulseResult<()> {
    let relative = path
        .strip_prefix(repo_root)
        .map_err(|_| PulseError::PathEscape {
            path: path.to_path_buf(),
        })?;
    validate_managed_relative_path(relative)?;
    let mut current = repo_root.to_path_buf();
    let mut iter = relative.components().peekable();
    while let Some(component) = iter.next() {
        match component {
            Component::Normal(part) => current.push(part),
            _ => {
                return Err(PulseError::PathTraversal {
                    path: relative.to_path_buf(),
                });
            }
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(PulseError::validation(
                    "run_control_record_invalid",
                    "managed runtime path contains a symlink component",
                ));
            }
            Ok(_) => {}
            Err(error)
                if allow_missing_final
                    && iter.peek().is_none()
                    && error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(PulseError::io(&current, error)),
        }
    }
    Ok(())
}

pub fn write_run_record(
    repo_root: &Path,
    record: &RunRecordV1,
    mode: RunStoreWriteMode,
) -> PulseResult<()> {
    assignment_store::check_enrolled(repo_root)?;
    validate_run_record(record)?;
    write_canonical_managed(
        repo_root,
        &run_record_path(repo_root, &record.run_id)?,
        record,
        mode,
    )
}

pub fn write_attempt_record(
    repo_root: &Path,
    record: &RunAttemptRecordV1,
    mode: RunStoreWriteMode,
) -> PulseResult<()> {
    assignment_store::check_enrolled(repo_root)?;
    validate_attempt_record(record)?;
    write_canonical_managed(
        repo_root,
        &attempt_record_path(repo_root, &record.attempt_id)?,
        record,
        mode,
    )
}

pub fn write_run_input(
    repo_root: &Path,
    input: &RunInputV1,
    rendered_prompt: &str,
    mode: RunStoreWriteMode,
) -> PulseResult<()> {
    assignment_store::check_enrolled(repo_root)?;
    validate_run_input(input)?;
    write_canonical_managed(
        repo_root,
        &input_json_path(repo_root, &input.run_id, &input.attempt_id)?,
        input,
        mode,
    )?;
    write_bytes_managed(
        repo_root,
        &input_prompt_path(repo_root, &input.run_id, &input.attempt_id)?,
        rendered_prompt.as_bytes(),
        mode,
    )
}

pub fn read_run_input_preserve(
    repo_root: &Path,
    run_id: &str,
    attempt_id: &str,
) -> PulseResult<Option<RunInputV1>> {
    assignment_store::check_enrolled(repo_root)?;
    let path = input_json_path(repo_root, run_id, attempt_id)?;
    let Some(input) = read_optional_canonical_managed(repo_root, &path, validate_run_input)? else {
        return Ok(None);
    };
    if input.run_id != run_id || input.attempt_id != attempt_id {
        return Err(PulseError::validation(
            "run_input_invalid",
            "run input filename does not match internal IDs",
        ));
    }
    Ok(Some(input))
}

pub fn write_workspace_snapshot(
    repo_root: &Path,
    attempt_id: &str,
    phase: &str,
    snapshot: &WorkspaceSnapshotV1,
    mode: RunStoreWriteMode,
) -> PulseResult<()> {
    assignment_store::check_enrolled(repo_root)?;
    validate_workspace_snapshot(snapshot)?;
    write_canonical_managed(
        repo_root,
        &snapshot_path(repo_root, attempt_id, phase)?,
        snapshot,
        mode,
    )
}

#[allow(dead_code)]
pub(crate) fn write_control_record(
    repo_root: &Path,
    record: &SupervisorControlRecordV1,
    mode: RunStoreWriteMode,
) -> PulseResult<()> {
    assignment_store::check_enrolled(repo_root)?;
    validate_control_record(record)?;
    write_canonical_managed(
        repo_root,
        &control_record_path(repo_root, &record.run_id)?,
        record,
        mode,
    )
}

pub fn write_log_segment(
    repo_root: &Path,
    run_id: &str,
    attempt_id: &str,
    stream: &str,
    segment: &str,
    bytes: &[u8],
    mode: RunStoreWriteMode,
) -> PulseResult<String> {
    assignment_store::check_enrolled(repo_root)?;
    let path = log_segment_path(repo_root, run_id, attempt_id, stream, segment)?;
    write_bytes_managed(repo_root, &path, bytes, mode)?;
    Ok(hash_bytes(bytes))
}

pub fn read_run_record_preserve(
    repo_root: &Path,
    run_id: &str,
) -> PulseResult<Option<RunRecordV1>> {
    assignment_store::check_enrolled(repo_root)?;
    let path = run_record_path(repo_root, run_id)?;
    let Some(record) = read_optional_canonical_managed(repo_root, &path, validate_run_record)?
    else {
        return Ok(None);
    };
    if record.run_id != run_id {
        return Err(PulseError::validation(
            "invalid_run_record",
            "run record filename does not match internal run_id",
        ));
    }
    Ok(Some(record))
}

pub fn read_attempt_record_preserve(
    repo_root: &Path,
    attempt_id: &str,
) -> PulseResult<Option<RunAttemptRecordV1>> {
    assignment_store::check_enrolled(repo_root)?;
    let path = attempt_record_path(repo_root, attempt_id)?;
    let Some(record) = read_optional_canonical_managed(repo_root, &path, validate_attempt_record)?
    else {
        return Ok(None);
    };
    if record.attempt_id != attempt_id {
        return Err(PulseError::validation(
            "invalid_run_record",
            "attempt record filename does not match internal attempt_id",
        ));
    }
    Ok(Some(record))
}

pub fn show_run_preserve(repo_root: &Path, run_id: &str) -> PulseResult<RunViewV1> {
    assignment_store::check_enrolled(repo_root)?;
    validate_run_id(run_id)?;
    let Some(run) = read_run_record_preserve(repo_root, run_id)? else {
        return Ok(RunViewV1 {
            schema_version: RUN_SCHEMA_VERSION,
            run: None,
            current_attempt: None,
            resume_eligibility: ResumeEligibilityV1::NotEvaluated,
            resume_blockers: vec!["run_not_found".to_string()],
            terminal_observation_pending: false,
            invalid_reason: None,
        });
    };
    let current_attempt = read_attempt_record_preserve(repo_root, &run.current_attempt_id)?;
    let invalid_reason = validate_run_attempt_relation(&run, current_attempt.as_ref()).err();
    Ok(RunViewV1 {
        schema_version: RUN_SCHEMA_VERSION,
        terminal_observation_pending: exit_observation_path(repo_root, run_id)?.is_file(),
        invalid_reason,
        resume_eligibility: ResumeEligibilityV1::NotEvaluated,
        resume_blockers: vec![],
        run: Some(run),
        current_attempt,
    })
}

pub fn list_runs_preserve(repo_root: &Path) -> PulseResult<RunListReportV1> {
    assignment_store::check_enrolled(repo_root)?;
    let mut runs = Vec::new();
    let mut invalid_records = Vec::new();
    let run_dir = runs_dir(repo_root);
    if !run_dir.is_dir() {
        return Ok(RunListReportV1 {
            schema_version: RUN_SCHEMA_VERSION,
            runs,
            invalid_records,
        });
    }
    for classification in classify_run_store_preserve(repo_root)? {
        match classification.status {
            RunStoreRecordStatus::Valid => {
                if let Some(run_id) = &classification.run_id {
                    runs.push(show_run_preserve(repo_root, run_id)?);
                }
            }
            RunStoreRecordStatus::Missing => {}
            _ => invalid_records.push(classification_to_recovery(classification)),
        }
    }
    runs.sort_by(|left, right| {
        left.run
            .as_ref()
            .map(|run| run.run_id.as_str())
            .cmp(&right.run.as_ref().map(|run| run.run_id.as_str()))
    });
    Ok(RunListReportV1 {
        schema_version: RUN_SCHEMA_VERSION,
        runs,
        invalid_records,
    })
}

pub fn classify_run_store_preserve(repo_root: &Path) -> PulseResult<Vec<RunStoreClassification>> {
    assignment_store::check_enrolled(repo_root)?;
    if !run_runtime_root(repo_root).exists() {
        return Ok(Vec::new());
    }
    ensure_no_symlink_escape(repo_root, &run_runtime_root(repo_root), false)?;
    let mut out = Vec::new();
    let mut run_ids = BTreeSet::new();
    let mut seen_internal_run_paths: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    let mut run_attempt_ids: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for entry in bounded_read_dir(&runs_dir(repo_root), &mut out, "runs")? {
        if !is_json_file_name(&entry.file_name) {
            continue;
        }
        let path = entry.path;
        let stem = json_stem(&entry.file_name).to_string();
        match read_required_canonical_managed::<RunRecordV1, _>(
            repo_root,
            &path,
            validate_run_record,
        ) {
            Ok(record) if record.run_id == stem => {
                run_ids.insert(record.run_id.clone());
                run_attempt_ids.insert(
                    record.run_id.clone(),
                    record.attempt_ids.iter().cloned().collect(),
                );
                seen_internal_run_paths
                    .entry(record.run_id.clone())
                    .or_default()
                    .push(path.clone());
                out.push(classification(
                    Some(record.run_id),
                    None,
                    RunStoreRecordStatus::Valid,
                    Some(&path),
                    "valid",
                ));
            }
            Ok(record) => {
                seen_internal_run_paths
                    .entry(record.run_id.clone())
                    .or_default()
                    .push(path.clone());
                out.push(classification(
                    Some(record.run_id),
                    None,
                    RunStoreRecordStatus::Invalid,
                    Some(&path),
                    "run_id_path_mismatch",
                ));
            }
            Err(error) => out.push(classification(
                Some(stem),
                None,
                RunStoreRecordStatus::Invalid,
                Some(&path),
                error.code(),
            )),
        }
        enforce_classification_limit(&mut out)?;
    }
    mark_duplicate_internal_ids(&seen_internal_run_paths, &mut out, true)?;
    classify_attempt_records(repo_root, &run_attempt_ids, &mut out)?;
    classify_orphan_controls(repo_root, &run_ids, &mut out)?;
    classify_orphan_logs(repo_root, &run_ids, &mut out)?;
    Ok(out)
}

fn write_canonical_managed<T: Serialize>(
    repo_root: &Path,
    path: &Path,
    value: &T,
    mode: RunStoreWriteMode,
) -> PulseResult<()> {
    let bytes = to_canonical_bytes(value)?;
    write_bytes_managed(repo_root, path, &bytes, mode)
}

fn write_bytes_managed(
    repo_root: &Path,
    path: &Path,
    bytes: &[u8],
    mode: RunStoreWriteMode,
) -> PulseResult<()> {
    validate_managed_absolute_path(repo_root, path)?;
    ensure_write_size_bound(path, bytes.len())?;
    if let Some(parent) = path.parent() {
        create_managed_dir_all(repo_root, parent)?;
    }
    ensure_managed_target_for_write(repo_root, path)?;
    match mode {
        RunStoreWriteMode::CreateNew => crate::storage::create_new_private(path, bytes)?,
        RunStoreWriteMode::Replace => crate::storage::atomic_write_private(path, bytes)?,
    }
    set_runtime_private_file_permissions(path)
}

fn ensure_write_size_bound(path: &Path, len: usize) -> PulseResult<()> {
    let max = managed_file_size_limit(path)?;
    if len as u64 > max {
        return Err(PulseError::validation(
            "run_record_too_large",
            "managed runtime file exceeds Slice 3 store size bound",
        ));
    }
    Ok(())
}

fn read_optional_canonical_managed<T, F>(
    repo_root: &Path,
    path: &Path,
    validate: F,
) -> PulseResult<Option<T>>
where
    T: DeserializeOwned + Serialize,
    F: Fn(&T) -> PulseResult<()>,
{
    if !path.exists() {
        return Ok(None);
    }
    read_required_canonical_managed(repo_root, path, validate).map(Some)
}

fn read_required_canonical_managed<T, F>(
    repo_root: &Path,
    path: &Path,
    validate: F,
) -> PulseResult<T>
where
    T: DeserializeOwned + Serialize,
    F: Fn(&T) -> PulseResult<()>,
{
    validate_managed_absolute_path(repo_root, path)?;
    ensure_no_symlink_escape(repo_root, path, false)?;
    let bytes = read_bounded_managed_file(path)?;
    let value: T = serde_json::from_slice(&bytes).map_err(|error| PulseError::json(path, error))?;
    let canonical = to_canonical_bytes(&value)?;
    if canonical != bytes {
        return Err(PulseError::validation(
            "invalid_run_record",
            "run store JSON is not strict canonical JSON",
        ));
    }
    validate(&value)?;
    Ok(value)
}

fn read_bounded_managed_file(path: &Path) -> PulseResult<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).map_err(|error| PulseError::io(path, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(PulseError::validation(
            "invalid_run_record",
            "managed runtime record must be a regular non-symlink file",
        ));
    }
    let max = managed_file_size_limit(path)?;
    if metadata.len() > max {
        return Err(PulseError::validation(
            "run_record_too_large",
            "managed runtime record exceeds Slice 3 read bound",
        ));
    }
    fs::read(path).map_err(|error| PulseError::io(path, error))
}

fn managed_file_size_limit(path: &Path) -> PulseResult<u64> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            PulseError::validation(
                "run_control_record_invalid",
                "managed runtime file name must be valid UTF-8",
            )
        })?;
    if name.ends_with(&format!(".{LOG_EXTENSION}")) {
        Ok(MAX_LOG_SEGMENT_BYTES)
    } else if name.ends_with(&format!(".{PROMPT_EXTENSION}")) {
        Ok(MAX_PROMPT_BYTES)
    } else if name.ends_with(&format!(".{RECORD_EXTENSION}")) {
        Ok(MAX_RECORD_BYTES)
    } else {
        Err(PulseError::validation(
            "run_control_record_invalid",
            "managed runtime file extension is not part of the run store layout",
        ))
    }
}

fn validate_managed_absolute_path(repo_root: &Path, path: &Path) -> PulseResult<()> {
    let relative = path
        .strip_prefix(repo_root)
        .map_err(|_| PulseError::PathEscape {
            path: path.to_path_buf(),
        })?;
    validate_managed_relative_path(relative)
}

fn create_managed_dir_all(repo_root: &Path, dir: &Path) -> PulseResult<()> {
    validate_managed_absolute_path(repo_root, dir)?;
    let relative = dir
        .strip_prefix(repo_root)
        .map_err(|_| PulseError::PathEscape {
            path: dir.to_path_buf(),
        })?;
    let mut current = repo_root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err(PulseError::PathTraversal {
                path: relative.to_path_buf(),
            });
        };
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(PulseError::validation(
                    "run_control_record_invalid",
                    "managed runtime path contains a symlink component",
                ));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(PulseError::validation(
                    "run_control_record_invalid",
                    "managed runtime path component is not a directory",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|error| PulseError::io(&current, error))?;
                set_runtime_private_permissions(&current)?;
            }
            Err(error) => return Err(PulseError::io(&current, error)),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn set_runtime_private_permissions(path: &Path) -> PulseResult<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| PulseError::io(path, error))
}

#[cfg(not(unix))]
fn set_runtime_private_permissions(_path: &Path) -> PulseResult<()> {
    Ok(())
}

#[cfg(unix)]
fn set_runtime_private_file_permissions(path: &Path) -> PulseResult<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| PulseError::io(path, error))
}

#[cfg(not(unix))]
fn set_runtime_private_file_permissions(_path: &Path) -> PulseResult<()> {
    Ok(())
}

fn validate_run_record(record: &RunRecordV1) -> PulseResult<()> {
    if record.schema_version != RUN_SCHEMA_VERSION {
        return Err(PulseError::validation(
            "invalid_run_record",
            "run schema_version must be 1",
        ));
    }
    validate_run_id(&record.run_id)?;
    validate_attempt_id(&record.current_attempt_id)?;
    let mut attempt_ids = BTreeSet::new();
    for attempt_id in &record.attempt_ids {
        validate_attempt_id(attempt_id)?;
        if !attempt_ids.insert(attempt_id) {
            return Err(PulseError::validation(
                "invalid_run_record",
                "run attempt_ids must not contain duplicates",
            ));
        }
    }
    if !attempt_ids.contains(&record.current_attempt_id) {
        return Err(PulseError::validation(
            "invalid_run_record",
            "current_attempt_id must be present in attempt_ids",
        ));
    }
    if record.compute_fingerprint()? != record.run_fingerprint {
        return Err(PulseError::validation(
            "invalid_run_record",
            "run_fingerprint mismatch",
        ));
    }
    Ok(())
}

fn validate_run_attempt_relation(
    run: &RunRecordV1,
    current_attempt: Option<&RunAttemptRecordV1>,
) -> std::result::Result<(), String> {
    let Some(attempt) = current_attempt else {
        return Err("current_attempt_missing".to_string());
    };
    if attempt.attempt_id != run.current_attempt_id {
        return Err("current_attempt_id_mismatch".to_string());
    }
    if attempt.run_id != run.run_id {
        return Err("current_attempt_cross_run".to_string());
    }
    if !run.attempt_ids.iter().any(|id| id == &attempt.attempt_id) {
        return Err("current_attempt_not_listed".to_string());
    }
    Ok(())
}

fn validate_attempt_record(record: &RunAttemptRecordV1) -> PulseResult<()> {
    if record.schema_version != RUN_SCHEMA_VERSION {
        return Err(PulseError::validation(
            "invalid_run_record",
            "attempt schema_version must be 1",
        ));
    }
    validate_attempt_id(&record.attempt_id)?;
    validate_run_id(&record.run_id)?;
    validate_attempt_layout(record)?;
    if record.compute_fingerprint()? != record.attempt_fingerprint {
        return Err(PulseError::validation(
            "invalid_run_record",
            "attempt_fingerprint mismatch",
        ));
    }
    Ok(())
}

fn validate_run_input(input: &RunInputV1) -> PulseResult<()> {
    if input.schema_version != RUN_SCHEMA_VERSION || input.profile != crate::run::RUN_INPUT_PROFILE
    {
        return Err(PulseError::validation(
            "run_input_invalid",
            "run input schema_version/profile mismatch",
        ));
    }
    validate_run_id(&input.run_id)?;
    validate_attempt_id(&input.attempt_id)?;
    if input.compute_fingerprint()? != input.input_fingerprint {
        return Err(PulseError::validation(
            "run_input_invalid",
            "input_fingerprint mismatch",
        ));
    }
    Ok(())
}

fn validate_attempt_layout(record: &RunAttemptRecordV1) -> PulseResult<()> {
    require_relative_path_string(
        &record.input.json_path,
        &PathBuf::from(".pulse")
            .join("runtime")
            .join("run")
            .join(INPUTS_DIR)
            .join(input_json_filename(&record.run_id, &record.attempt_id)),
        "attempt input JSON path",
    )?;
    require_relative_path_string(
        &record.input.rendered_prompt_path,
        &PathBuf::from(".pulse")
            .join("runtime")
            .join("run")
            .join(INPUTS_DIR)
            .join(input_prompt_filename(&record.run_id, &record.attempt_id)),
        "attempt prompt path",
    )?;
    validate_log_ref_layout(
        &record.run_id,
        &record.attempt_id,
        "stdout",
        &record.logs.stdout,
    )?;
    validate_log_ref_layout(
        &record.run_id,
        &record.attempt_id,
        "stderr",
        &record.logs.stderr,
    )?;
    Ok(())
}

fn validate_log_ref_layout(
    run_id: &str,
    attempt_id: &str,
    stream: &str,
    log: &crate::run::RunLogRefV1,
) -> PulseResult<()> {
    require_log_path(run_id, attempt_id, stream, &log.path)?;
    if let Some(path) = &log.retained_prefix_path {
        require_relative_path_string(
            path,
            &PathBuf::from(".pulse")
                .join("runtime")
                .join("run")
                .join(LOGS_DIR)
                .join(run_id)
                .join(log_segment_filename(attempt_id, stream, "prefix")),
            "retained prefix log path",
        )?;
    }
    if let Some(path) = &log.retained_tail_path {
        require_relative_path_string(
            path,
            &PathBuf::from(".pulse")
                .join("runtime")
                .join("run")
                .join(LOGS_DIR)
                .join(run_id)
                .join(log_segment_filename(attempt_id, stream, "tail")),
            "retained tail log path",
        )?;
    }
    Ok(())
}

fn require_log_path(run_id: &str, attempt_id: &str, stream: &str, path: &str) -> PulseResult<()> {
    let prefix = PathBuf::from(".pulse")
        .join("runtime")
        .join("run")
        .join(LOGS_DIR)
        .join(run_id)
        .join(log_segment_filename(attempt_id, stream, "prefix"));
    let tail = PathBuf::from(".pulse")
        .join("runtime")
        .join("run")
        .join(LOGS_DIR)
        .join(run_id)
        .join(log_segment_filename(attempt_id, stream, "tail"));
    let actual = safe_relative_layout_path(path)?;
    if actual != prefix && actual != tail {
        return Err(PulseError::validation(
            "invalid_run_record",
            "attempt log path does not match exact run-store log layout",
        ));
    }
    Ok(())
}

fn require_relative_path_string(path: &str, expected: &Path, label: &str) -> PulseResult<()> {
    let actual = safe_relative_layout_path(path)?;
    if actual != expected {
        return Err(PulseError::validation(
            "invalid_run_record",
            format!("{label} does not match exact run-store layout"),
        ));
    }
    Ok(())
}

fn safe_relative_layout_path(path: &str) -> PulseResult<PathBuf> {
    let relative = Path::new(path);
    validate_managed_relative_path(relative)?;
    Ok(relative.to_path_buf())
}

fn validate_workspace_snapshot(snapshot: &WorkspaceSnapshotV1) -> PulseResult<()> {
    if snapshot.schema_version != RUN_SCHEMA_VERSION {
        return Err(PulseError::validation(
            "invalid_run_record",
            "snapshot schema_version must be 1",
        ));
    }
    if snapshot.compute_identity()? != snapshot.snapshot_identity {
        return Err(PulseError::validation(
            "invalid_run_record",
            "snapshot_identity mismatch",
        ));
    }
    Ok(())
}

#[allow(dead_code)]
fn validate_control_record(record: &SupervisorControlRecordV1) -> PulseResult<()> {
    if record.schema_version != RUN_SCHEMA_VERSION {
        return Err(PulseError::validation(
            "invalid_run_record",
            "control schema_version must be 1",
        ));
    }
    validate_run_id(&record.run_id)?;
    validate_attempt_id(&record.attempt_id)?;
    require_relative_path_string(
        &record.input_json_path,
        &PathBuf::from(".pulse")
            .join("runtime")
            .join("run")
            .join(INPUTS_DIR)
            .join(input_json_filename(&record.run_id, &record.attempt_id)),
        "control input JSON path",
    )?;
    require_relative_path_string(
        &record.stdout_prefix_path,
        &PathBuf::from(".pulse")
            .join("runtime")
            .join("run")
            .join(LOGS_DIR)
            .join(&record.run_id)
            .join(log_segment_filename(&record.attempt_id, "stdout", "prefix")),
        "control stdout prefix path",
    )?;
    require_relative_path_string(
        &record.stdout_tail_path,
        &PathBuf::from(".pulse")
            .join("runtime")
            .join("run")
            .join(LOGS_DIR)
            .join(&record.run_id)
            .join(log_segment_filename(&record.attempt_id, "stdout", "tail")),
        "control stdout tail path",
    )?;
    require_relative_path_string(
        &record.stderr_prefix_path,
        &PathBuf::from(".pulse")
            .join("runtime")
            .join("run")
            .join(LOGS_DIR)
            .join(&record.run_id)
            .join(log_segment_filename(&record.attempt_id, "stderr", "prefix")),
        "control stderr prefix path",
    )?;
    require_relative_path_string(
        &record.stderr_tail_path,
        &PathBuf::from(".pulse")
            .join("runtime")
            .join("run")
            .join(LOGS_DIR)
            .join(&record.run_id)
            .join(log_segment_filename(&record.attempt_id, "stderr", "tail")),
        "control stderr tail path",
    )?;
    Ok(())
}

#[derive(Debug)]
struct ManagedDirEntry {
    path: PathBuf,
    file_name: String,
}

fn bounded_read_dir(
    dir: &Path,
    classifications: &mut Vec<RunStoreClassification>,
    label: &str,
) -> PulseResult<Vec<ManagedDirEntry>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    if !dir.is_dir() {
        let dir_path = dir.to_path_buf();
        classifications.push(classification(
            None,
            None,
            RunStoreRecordStatus::Invalid,
            Some(&dir_path),
            "run_store_path_not_directory",
        ));
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).map_err(|error| PulseError::io(dir, error))? {
        let entry = entry.map_err(|error| PulseError::io(dir, error))?;
        if entries.len() >= MAX_DIRECTORY_ENTRIES {
            let dir_path = dir.to_path_buf();
            classifications.push(classification(
                None,
                None,
                RunStoreRecordStatus::Invalid,
                Some(&dir_path),
                "run_store_directory_entry_cap_exceeded",
            ));
            break;
        }
        let file_name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(_) => {
                classifications.push(classification(
                    None,
                    None,
                    RunStoreRecordStatus::Invalid,
                    Some(&entry.path()),
                    "run_store_non_utf8_name",
                ));
                continue;
            }
        };
        if file_name.contains('/') || file_name.contains('\\') {
            classifications.push(classification(
                None,
                None,
                RunStoreRecordStatus::Invalid,
                Some(&entry.path()),
                "run_store_invalid_name",
            ));
            continue;
        }
        entries.push(ManagedDirEntry {
            path: entry.path(),
            file_name,
        });
    }
    entries.sort_by(|left, right| left.file_name.cmp(&right.file_name));
    if entries.len() >= MAX_DIRECTORY_ENTRIES {
        let dir_path = dir.to_path_buf();
        classifications.push(classification(
            None,
            None,
            RunStoreRecordStatus::Invalid,
            Some(&dir_path),
            &format!("{label}_entry_cap_reached"),
        ));
    }
    enforce_classification_limit(classifications)?;
    Ok(entries)
}

fn is_json_file_name(name: &str) -> bool {
    name.ends_with(&format!(".{RECORD_EXTENSION}"))
}

fn json_stem(name: &str) -> &str {
    name.strip_suffix(&format!(".{RECORD_EXTENSION}"))
        .unwrap_or(name)
}

fn classify_attempt_records(
    repo_root: &Path,
    run_attempt_ids: &BTreeMap<String, BTreeSet<String>>,
    out: &mut Vec<RunStoreClassification>,
) -> PulseResult<()> {
    let mut seen_internal_attempt_paths: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for entry in bounded_read_dir(&attempts_dir(repo_root), out, "attempts")? {
        if !is_json_file_name(&entry.file_name) {
            continue;
        }
        let path = entry.path;
        let stem = json_stem(&entry.file_name).to_string();
        match read_required_canonical_managed::<RunAttemptRecordV1, _>(
            repo_root,
            &path,
            validate_attempt_record,
        ) {
            Ok(record) if record.attempt_id == stem => {
                seen_internal_attempt_paths
                    .entry(record.attempt_id.clone())
                    .or_default()
                    .push(path.clone());
                let relation_ok = run_attempt_ids
                    .get(&record.run_id)
                    .is_some_and(|ids| ids.contains(&record.attempt_id));
                if !relation_ok {
                    out.push(classification(
                        Some(record.run_id),
                        Some(record.attempt_id),
                        RunStoreRecordStatus::Invalid,
                        Some(&path),
                        "attempt_run_relation_mismatch",
                    ));
                }
            }
            Ok(record) => {
                seen_internal_attempt_paths
                    .entry(record.attempt_id.clone())
                    .or_default()
                    .push(path.clone());
                out.push(classification(
                    Some(record.run_id),
                    Some(record.attempt_id),
                    RunStoreRecordStatus::Invalid,
                    Some(&path),
                    "attempt_id_path_mismatch",
                ));
            }
            Err(error) => out.push(classification(
                None,
                Some(stem),
                RunStoreRecordStatus::Invalid,
                Some(&path),
                error.code(),
            )),
        }
        enforce_classification_limit(out)?;
    }
    mark_duplicate_internal_ids(&seen_internal_attempt_paths, out, false)
}

fn classify_orphan_controls(
    repo_root: &Path,
    run_ids: &BTreeSet<String>,
    out: &mut Vec<RunStoreClassification>,
) -> PulseResult<()> {
    for entry in bounded_read_dir(&control_dir(repo_root), out, "control")? {
        if !is_json_file_name(&entry.file_name) {
            continue;
        }
        let run_id = json_stem(&entry.file_name).split('.').next().unwrap_or("");
        if !run_ids.contains(run_id) {
            out.push(classification(
                Some(run_id.to_string()),
                None,
                RunStoreRecordStatus::OrphanControl,
                Some(&entry.path),
                "orphan_control",
            ));
        }
        enforce_classification_limit(out)?;
    }
    Ok(())
}

fn classify_orphan_logs(
    repo_root: &Path,
    run_ids: &BTreeSet<String>,
    out: &mut Vec<RunStoreClassification>,
) -> PulseResult<()> {
    for entry in bounded_read_dir(&logs_dir(repo_root), out, "logs")? {
        let metadata = fs::symlink_metadata(&entry.path)
            .map_err(|error| PulseError::io(&entry.path, error))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            out.push(classification(
                Some(entry.file_name),
                None,
                RunStoreRecordStatus::Invalid,
                Some(&entry.path),
                "run_log_entry_not_directory",
            ));
            continue;
        }
        if !run_ids.contains(&entry.file_name) {
            out.push(classification(
                Some(entry.file_name),
                None,
                RunStoreRecordStatus::OrphanLog,
                Some(&entry.path),
                "orphan_log",
            ));
        }
        enforce_classification_limit(out)?;
    }
    Ok(())
}

fn mark_duplicate_internal_ids(
    seen: &BTreeMap<String, Vec<PathBuf>>,
    out: &mut Vec<RunStoreClassification>,
    is_run: bool,
) -> PulseResult<()> {
    for (id, paths) in seen {
        if paths.len() > 1 {
            for path in paths {
                out.push(classification(
                    if is_run { Some(id.clone()) } else { None },
                    if is_run { None } else { Some(id.clone()) },
                    RunStoreRecordStatus::Ambiguous,
                    Some(path),
                    if is_run {
                        "duplicate_internal_run_id"
                    } else {
                        "duplicate_internal_attempt_id"
                    },
                ));
            }
        }
        enforce_classification_limit(out)?;
    }
    Ok(())
}

fn enforce_classification_limit(out: &mut Vec<RunStoreClassification>) -> PulseResult<()> {
    if out.len() > MAX_CLASSIFICATIONS {
        out.truncate(MAX_CLASSIFICATIONS);
        out.push(RunStoreClassification {
            run_id: None,
            attempt_id: None,
            status: RunStoreRecordStatus::Invalid,
            path: None,
            reason_codes: vec!["run_store_classification_cap_exceeded".to_string()],
        });
    }
    Ok(())
}

fn classification(
    run_id: Option<String>,
    attempt_id: Option<String>,
    status: RunStoreRecordStatus,
    path: Option<&PathBuf>,
    reason: &str,
) -> RunStoreClassification {
    RunStoreClassification {
        run_id,
        attempt_id,
        status,
        path: path.and_then(|path| path.to_str().map(str::to_string)),
        reason_codes: vec![reason.to_string()],
    }
}

fn classification_to_recovery(
    classification: RunStoreClassification,
) -> RunRecoveryClassificationV1 {
    RunRecoveryClassificationV1 {
        run_id: classification.run_id,
        attempt_id: classification.attempt_id,
        classification: match classification.status {
            RunStoreRecordStatus::Valid => "live",
            RunStoreRecordStatus::Missing => "invalid",
            RunStoreRecordStatus::Invalid => "invalid",
            RunStoreRecordStatus::Ambiguous => "invalid",
            RunStoreRecordStatus::OrphanControl => "orphan_control",
            RunStoreRecordStatus::OrphanLog => "orphan_log",
        }
        .to_string(),
        mutation_available: false,
        reason_codes: classification.reason_codes,
    }
}

/// Load the tracked runner-profile registry in preserve/no-bootstrap mode.
///
/// This function validates enrollment before touching profile config and never
/// creates `.pulse/`, `.pulse/run/`, `.pulse/runtime/` or lock paths. Missing or
/// invalid profile registries fail closed because the registry is code-exec
/// configuration, not bootstrap state.
pub fn load_runner_profile_registry_preserve(
    repo_root: &Path,
) -> PulseResult<RunnerProfileRegistryV1> {
    assignment_store::check_enrolled(repo_root)?;
    let path = repo_root.join(RUNNER_PROFILE_REGISTRY_RELATIVE_PATH);
    let bytes = fs::read(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            PulseError::validation(
                "run_profile_missing",
                "tracked runner profile registry .pulse/run/runner-profiles.json is missing",
            )
        } else {
            PulseError::io(&path, error)
        }
    })?;
    let mut registry: RunnerProfileRegistryV1 =
        serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))?;
    registry.normalize();
    registry.validate()?;
    Ok(registry)
}

/// Load, validate and resolve a selected production profile without mutating
/// repository state.
pub fn select_runner_profile_preserve(
    repo_root: &Path,
    requested_profile: Option<&str>,
) -> PulseResult<RunnerProfileSelectionV1> {
    let registry = load_runner_profile_registry_preserve(repo_root)?;
    select_profile_from_registry(&registry, requested_profile)
}

pub fn select_profile_from_registry(
    registry: &RunnerProfileRegistryV1,
    requested_profile: Option<&str>,
) -> PulseResult<RunnerProfileSelectionV1> {
    registry.validate()?;
    let profile_id = requested_profile.unwrap_or(&registry.default_profile);
    let profile = registry
        .profiles
        .iter()
        .find(|profile| profile.profile_id == profile_id)
        .ok_or_else(|| PulseError::validation("run_profile_missing", "runner profile not found"))?;
    profile.validate_public()?;
    let executable = resolve_executable(&profile.executable)?;
    let profile_fingerprint = profile.fingerprint()?;
    let environment_spec_fingerprint = profile.environment_spec_fingerprint()?;
    let mut environment = profile
        .environment_allow
        .iter()
        .map(|name| RunnerEnvironmentSpecEntryV1 {
            name: name.clone(),
            source: RunnerEnvironmentSourceV1::Inherited,
        })
        .chain(
            profile
                .environment_set
                .keys()
                .map(|name| RunnerEnvironmentSpecEntryV1 {
                    name: name.clone(),
                    source: RunnerEnvironmentSourceV1::LiteralNonSecret,
                }),
        )
        .collect::<Vec<_>>();
    environment.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.source.cmp(&right.source))
    });
    environment.dedup();
    Ok(RunnerProfileSelectionV1 {
        schema_version: crate::run::RUN_SCHEMA_VERSION,
        profile_id: profile.profile_id.clone(),
        adapter: RunnerAdapterV1::CodexProcessV1,
        profile_fingerprint,
        environment_spec_fingerprint,
        executable,
        fixed_args: profile.fixed_args.clone(),
        environment,
        literal_environment_values: profile.environment_set.clone(),
    })
}

fn resolve_executable(executable: &str) -> PulseResult<RunnerExecutableIdentityV1> {
    #[cfg(not(unix))]
    {
        let _ = executable;
        return Err(PulseError::validation(
            "run_platform_unsupported",
            "runner executable resolution requires platform-specific executable permission semantics",
        ));
    }
    #[cfg(unix)]
    if executable.contains('\\') {
        return Err(PulseError::validation(
            "run_profile_invalid",
            "executable must not contain backslash",
        ));
    }
    #[cfg(unix)]
    if Path::new(executable).is_absolute() {
        return resolve_absolute_executable(Path::new(executable));
    }
    #[cfg(unix)]
    if has_path_separator(executable) || executable.chars().any(char::is_whitespace) {
        return Err(PulseError::validation(
            "run_profile_invalid",
            "executable must be an absolute path or a bare program name",
        ));
    }
    #[cfg(unix)]
    let path = env::var_os("PATH").ok_or_else(|| {
        PulseError::validation(
            "run_command_not_found",
            "PATH is not available for runner executable resolution",
        )
    })?;
    for dir in env::split_paths(&path) {
        if !is_utf8_path(&dir) || !dir.is_absolute() || path_has_symlink_component(&dir) {
            continue;
        }
        let candidate = dir.join(executable);
        match safe_executable_candidate(&candidate) {
            Ok(Some(executable)) => return Ok(executable),
            Ok(None) => continue,
            Err(error) if error.code() == "run_platform_unsupported" => return Err(error),
            Err(_) => continue,
        }
    }
    Err(PulseError::validation(
        "run_command_not_found",
        format!("runner executable {executable} was not found as a safe regular executable on inherited PATH"),
    ))
}

fn resolve_absolute_executable(path: &Path) -> PulseResult<RunnerExecutableIdentityV1> {
    match safe_executable_candidate(path)? {
        Some(executable) => Ok(executable),
        None => Err(PulseError::validation(
            "run_profile_invalid",
            "configured executable must be a normalized absolute non-symlink regular executable",
        )),
    }
}

fn safe_executable_candidate(path: &Path) -> PulseResult<Option<RunnerExecutableIdentityV1>> {
    if !is_utf8_path(path)
        || !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
        || path_has_symlink_component(path)
    {
        return Ok(None);
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return Ok(None),
    };
    if !metadata.is_file() || !has_effective_execute_permission(&metadata)? {
        return Ok(None);
    }
    let canonical = match fs::canonicalize(path) {
        Ok(canonical) => canonical,
        Err(_) => return Ok(None),
    };
    if !is_utf8_path(&canonical)
        || !canonical.is_absolute()
        || canonical
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
        || canonical != path
    {
        return Ok(None);
    }
    let identity = executable_identity_hash(&canonical, &metadata)?;
    Ok(Some(RunnerExecutableIdentityV1 {
        resolved_path: path_to_utf8(&canonical)?.to_string(),
        identity,
        identity_status: executable_identity_status(),
    }))
}

#[cfg(unix)]
fn has_effective_execute_permission(metadata: &fs::Metadata) -> PulseResult<bool> {
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
fn executable_identity_status() -> String {
    "best_effort_unix_metadata_non_symlink_effective_executable".to_string()
}

#[cfg(not(unix))]
fn has_effective_execute_permission(_metadata: &fs::Metadata) -> PulseResult<bool> {
    Err(PulseError::validation(
        "run_platform_unsupported",
        "runner executable permission semantics are not implemented on this platform",
    ))
}

#[cfg(not(unix))]
fn executable_identity_status() -> String {
    "unsupported_non_unix_permission_semantics".to_string()
}

fn executable_identity_hash(canonical: &Path, metadata: &fs::Metadata) -> PulseResult<String> {
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
    let resolved_path = path_to_utf8(canonical)?;
    let identity = PortableExecutableIdentity {
        resolved_path,
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

fn has_path_separator(value: &str) -> bool {
    value.contains('/') || value.contains('\\')
}

fn is_utf8_path(path: &Path) -> bool {
    path.as_os_str().to_str().is_some()
}

fn path_to_utf8(path: &Path) -> PulseResult<&str> {
    path.as_os_str().to_str().ok_or_else(|| {
        PulseError::validation(
            "run_profile_invalid",
            "runner executable paths must be valid UTF-8 and are never rendered lossily",
        )
    })
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

#[cfg(unix)]
fn effective_groups() -> PulseResult<Vec<u32>> {
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
fn unix_getgroups_count() -> PulseResult<usize> {
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

#[cfg(test)]
pub(crate) mod fixture_adapter {
    use super::*;
    use crate::run::RunnerProfileV1;
    use serde_json::Map;

    pub(crate) const FIXTURE_PROCESS_ADAPTER: &str = "fixture_process_v1";

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) struct FixtureRunnerProfileSelectionV1 {
        pub(crate) adapter: &'static str,
        pub(crate) executable: String,
        pub(crate) fixed_args: Vec<String>,
    }

    /// Crate-private fixture profile injection for process-store tests. This is
    /// intentionally unavailable to production JSON and integration consumers.
    pub(crate) fn fixture_registry_for_tests(
        executable: impl Into<String>,
    ) -> RunnerProfileRegistryV1 {
        RunnerProfileRegistryV1 {
            schema_version: crate::run::RUNNER_PROFILE_SCHEMA_VERSION,
            default_profile: "fixture-local".to_string(),
            profiles: vec![RunnerProfileV1 {
                profile_id: "fixture-local".to_string(),
                adapter: RunnerAdapterV1::CodexProcessV1,
                executable: executable.into(),
                fixed_args: Vec::new(),
                environment_allow: vec!["PATH".to_string()],
                environment_set: Map::new(),
                start_timeout_seconds: 1,
                run_timeout_seconds: 60,
                cancel_grace_seconds: 1,
                force_kill_after_seconds: 0,
                max_stdout_bytes: 65_536,
                max_stderr_bytes: 65_536,
            }],
        }
    }

    pub(crate) fn fixture_process_selection_for_tests(
        executable: impl Into<String>,
        fixed_args: Vec<String>,
    ) -> FixtureRunnerProfileSelectionV1 {
        FixtureRunnerProfileSelectionV1 {
            adapter: FIXTURE_PROCESS_ADAPTER,
            executable: executable.into(),
            fixed_args,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::{
        NativeResumeStatusV1, RunAssignmentV1, RunAttemptInputRefV1, RunAttemptLogsV1,
        RunAttemptProcessV1, RunAttemptRecordV1, RunAttemptStateV1, RunCancelStateV1,
        RunInputModeV1, RunInputResumeContextV1, RunInputRunnerProfileV1, RunInputV1,
        RunInstructionsV1, RunLogHashScopeV1, RunLogRefV1, RunRecordV1, RunRunnerV1, RunStateV1,
        RunSubjectV1, RunWorkspaceBindingV1, RunnerAdapterV1, RunnerProfileV1,
        WorkspaceCleanlinessV1, WorkspaceModeV1, WorkspaceOperationStateV1,
        WorkspaceSnapshotStatusV1, WorkspaceSnapshotV1, DEFAULT_LOG_REDACTION_STATUS,
        RUNNER_PROFILE_SCHEMA_VERSION, RUN_INPUT_PROFILE, RUN_KIND_SINGLE_AGENT_IMPLEMENTATION,
    };
    use serde_json::Map;
    use std::ffi::OsString;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, MutexGuard};
    use tempfile::TempDir;

    static PATH_ENV_LOCK: Mutex<()> = Mutex::new(());

    struct PathEnvGuard {
        _lock: MutexGuard<'static, ()>,
        original_path: Option<OsString>,
    }

    impl PathEnvGuard {
        fn new() -> Self {
            let lock = PATH_ENV_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let original_path = env::var_os("PATH");
            Self {
                _lock: lock,
                original_path,
            }
        }

        fn set_path<'a>(&self, entries: impl IntoIterator<Item = &'a Path>) {
            let mut paths = entries.into_iter().map(PathBuf::from).collect::<Vec<_>>();
            if let Some(original_path) = &self.original_path {
                paths.extend(env::split_paths(original_path));
            }
            env::set_var("PATH", env::join_paths(paths).unwrap());
        }
    }

    impl Drop for PathEnvGuard {
        fn drop(&mut self) {
            if let Some(original_path) = self.original_path.clone() {
                env::set_var("PATH", original_path);
            } else {
                env::remove_var("PATH");
            }
        }
    }

    fn enrolled_repo() -> TempDir {
        let repo = tempfile::tempdir().unwrap();
        fs::create_dir_all(repo.path().join(".pulse/workgraph/schemas")).unwrap();
        fs::write(repo.path().join(".pulse/workgraph/manifest.json"), b"{}\n").unwrap();
        fs::write(
            repo.path()
                .join(".pulse/workgraph/schemas/node.schema.json"),
            b"{}\n",
        )
        .unwrap();
        repo
    }

    fn registry(executable: String) -> RunnerProfileRegistryV1 {
        RunnerProfileRegistryV1 {
            schema_version: RUNNER_PROFILE_SCHEMA_VERSION,
            default_profile: "codex-local".to_string(),
            profiles: vec![RunnerProfileV1 {
                profile_id: "codex-local".to_string(),
                adapter: RunnerAdapterV1::CodexProcessV1,
                executable,
                fixed_args: vec!["exec".to_string(), "--json".to_string()],
                environment_allow: vec!["PATH".to_string(), "HOME".to_string()],
                environment_set: Map::new(),
                start_timeout_seconds: 30,
                run_timeout_seconds: 7200,
                cancel_grace_seconds: 10,
                force_kill_after_seconds: 10,
                max_stdout_bytes: 16_777_216,
                max_stderr_bytes: 16_777_216,
            }],
        }
    }

    fn write_registry(repo: &Path, registry: &RunnerProfileRegistryV1) {
        fs::create_dir_all(repo.join(".pulse/run")).unwrap();
        let bytes = crate::canonical_json::to_canonical_bytes(registry).unwrap();
        fs::write(repo.join(RUNNER_PROFILE_REGISTRY_RELATIVE_PATH), bytes).unwrap();
    }

    fn executable_file(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let mut file = fs::File::create(&path).unwrap();
        writeln!(file, "#!/bin/sh").unwrap();
        #[cfg(unix)]
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn canonical_utf8(path: &Path) -> String {
        fs::canonicalize(path)
            .unwrap()
            .to_str()
            .expect("test temp paths must canonicalize to UTF-8")
            .to_string()
    }

    const ZERO: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
    const ONE: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
    const TWO: &str = "sha256:2222222222222222222222222222222222222222222222222222222222222222";

    fn snapshot() -> WorkspaceSnapshotV1 {
        let mut value = WorkspaceSnapshotV1 {
            schema_version: RUN_SCHEMA_VERSION,
            repository_id: "repo_test".to_string(),
            workspace_id: "wt_TK-031_01JTEST".to_string(),
            workspace_mode: WorkspaceModeV1::IsolatedWorktree,
            base_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
            head_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
            diff_base_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
            operation_state: WorkspaceOperationStateV1::None,
            cleanliness: WorkspaceCleanlinessV1::Clean,
            tracked_diff_identity: ZERO.to_string(),
            untracked_manifest_identity: ONE.to_string(),
            status_identity: TWO.to_string(),
            snapshot_status: WorkspaceSnapshotStatusV1::Complete,
            captured_at: "2026-07-29T10:00:00Z".to_string(),
            snapshot_identity: String::new(),
        };
        value.snapshot_identity = value.compute_identity().unwrap();
        value
    }

    fn log_ref_for(run_id: &str, attempt_id: &str, stream: &str) -> RunLogRefV1 {
        RunLogRefV1 {
            path: format!(".pulse/runtime/run/logs/{run_id}/{attempt_id}.{stream}.prefix.log"),
            retained_prefix_path: None,
            retained_tail_path: None,
            bytes_seen: 0,
            bytes_retained: 0,
            bytes_truncated: 0,
            content_hash: ZERO.to_string(),
            hash_scope: RunLogHashScopeV1::FullUntruncatedContent,
            truncated: false,
            redaction_status: DEFAULT_LOG_REDACTION_STATUS.to_string(),
        }
    }

    fn run_record() -> RunRecordV1 {
        let mut value = RunRecordV1 {
            schema_version: RUN_SCHEMA_VERSION,
            run_id: "run_01JTEST".to_string(),
            kind: RUN_KIND_SINGLE_AGENT_IMPLEMENTATION.to_string(),
            state: RunStateV1::Running,
            subject: RunSubjectV1 {
                kind: "ticket".to_string(),
                id: "TK-031".to_string(),
                active_revision: 9,
                contract_revision: 4,
            },
            assignment: RunAssignmentV1 {
                lease_id: "lease_01JTEST".to_string(),
                prepared_assignment_id: "pa_01JTEST".to_string(),
                prepared_assignment_fingerprint: ZERO.to_string(),
                packet_fingerprint: ZERO.to_string(),
                assignee: "agent:codex-local".to_string(),
            },
            workspace: RunWorkspaceBindingV1 {
                workspace_id: "wt_TK-031_01JTEST".to_string(),
                mode: WorkspaceModeV1::IsolatedWorktree,
                path: ".pulse/runtime/workspaces/wt_TK-031_01JTEST".to_string(),
                repository_id: "repo_test".to_string(),
                base_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
            },
            runner: RunRunnerV1 {
                adapter: RunnerAdapterV1::CodexProcessV1,
                profile_id: "codex-local".to_string(),
                profile_fingerprint: ZERO.to_string(),
                resolved_executable_identity: Some("best_effort:test".to_string()),
                native_resume_status: NativeResumeStatusV1::NotInstalled,
                native_thread_id: None,
            },
            current_attempt_id: "attempt_01JTEST".to_string(),
            attempt_ids: vec!["attempt_01JTEST".to_string()],
            created_by: "human:test".to_string(),
            created_at: "2026-07-29T10:00:00Z".to_string(),
            updated_at: "2026-07-29T10:00:01Z".to_string(),
            last_heartbeat_at: Some("2026-07-29T10:00:05Z".to_string()),
            latest_exit: None,
            latest_workspace_snapshot_identity: Some(snapshot().snapshot_identity),
            reason_codes: vec![],
            run_fingerprint: String::new(),
        };
        value.run_fingerprint = value.compute_fingerprint().unwrap();
        value
    }

    fn attempt_record() -> RunAttemptRecordV1 {
        attempt_record_for("run_01JTEST", "attempt_01JTEST")
    }

    fn attempt_record_for(run_id: &str, attempt_id: &str) -> RunAttemptRecordV1 {
        let mut value = RunAttemptRecordV1 {
            schema_version: RUN_SCHEMA_VERSION,
            attempt_id: attempt_id.to_string(),
            run_id: run_id.to_string(),
            attempt_number: 1,
            state: RunAttemptStateV1::Running,
            input: RunAttemptInputRefV1 {
                run_input_identity: ZERO.to_string(),
                json_path: format!(".pulse/runtime/run/inputs/{run_id}.{attempt_id}.json"),
                rendered_prompt_identity: ONE.to_string(),
                rendered_prompt_path: format!(".pulse/runtime/run/inputs/{run_id}.{attempt_id}.md"),
            },
            process: RunAttemptProcessV1 {
                identity: None,
                started_at: Some("2026-07-29T10:00:01Z".to_string()),
                ended_at: None,
                exit: None,
            },
            workspace_before: snapshot(),
            workspace_after: Some(snapshot()),
            logs: RunAttemptLogsV1 {
                stdout: log_ref_for(run_id, attempt_id, "stdout"),
                stderr: log_ref_for(run_id, attempt_id, "stderr"),
            },
            timeout_seconds: 7200,
            cancel: RunCancelStateV1 {
                requested_at: None,
                requested_by: None,
                reason: None,
                grace_seconds: None,
                force_allowed: None,
            },
            created_at: "2026-07-29T10:00:00Z".to_string(),
            updated_at: "2026-07-29T10:00:01Z".to_string(),
            reason_codes: vec![],
            attempt_fingerprint: String::new(),
        };
        value.attempt_fingerprint = value.compute_fingerprint().unwrap();
        value
    }

    fn run_input_record() -> RunInputV1 {
        let mut value = RunInputV1 {
            schema_version: RUN_SCHEMA_VERSION,
            profile: RUN_INPUT_PROFILE.to_string(),
            run_id: "run_01JTEST".to_string(),
            attempt_id: "attempt_01JTEST".to_string(),
            attempt_number: 1,
            mode: RunInputModeV1::Start,
            prepared_assignment: minimal_prepared_assignment(),
            workspace: minimal_workspace_summary(),
            runner_profile: RunInputRunnerProfileV1 {
                profile_id: "codex-local".to_string(),
                adapter: RunnerAdapterV1::CodexProcessV1,
                profile_fingerprint: ZERO.to_string(),
            },
            instructions: RunInstructionsV1 {
                objective: "Implement run store".to_string(),
                acceptance: vec!["tests pass".to_string()],
                required_changes: vec![],
                invariants: vec!["do not close ticket".to_string()],
                hard_stops: vec![],
                expected_evidence: vec!["cargo test".to_string()],
                expected_handoff: vec![],
                authority_boundary: vec!["do_not_merge_or_deploy".to_string()],
            },
            resume: RunInputResumeContextV1 {
                previous_attempt_id: None,
                workspace_snapshot_identity: None,
                previous_exit_kind: None,
                redacted_log_tail: None,
                native_resume_status: NativeResumeStatusV1::NotInstalled,
            },
            input_fingerprint: String::new(),
        };
        value.input_fingerprint = value.compute_fingerprint().unwrap();
        value
    }

    fn minimal_workspace_summary() -> crate::assignment::AssignmentWorkspaceSummary {
        crate::assignment::AssignmentWorkspaceSummary {
            workspace_id: "wt_TK-031_01JTEST".to_string(),
            binding_status: "bound".to_string(),
            mode: "isolated_worktree".to_string(),
            path: ".pulse/runtime/workspaces/wt_TK-031_01JTEST".to_string(),
            repository_id: "repo_test".to_string(),
            base_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
            cleanliness: "clean".to_string(),
            owner_lease_id: "lease_01JTEST".to_string(),
        }
    }

    fn minimal_prepared_assignment() -> crate::assignment::PreparedAssignmentV1 {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "profile": "phase2_prepared_assignment_v1",
            "code": "prepared_assignment",
            "prepared_assignment_id": "pa_01JTEST",
            "subject": {
                "id": "TK-031",
                "kind": "ticket",
                "revision_before": 8,
                "revision_after": 9,
                "contract_revision": 4,
                "status_before": "ready",
                "status_after": "active"
            },
            "packet": {
                "schema_version": 1,
                "profile": "phase2_work_packet_preview_v1",
                "code": "reservation_candidate",
                "subject": {
                    "id": "TK-031",
                    "kind": "ticket",
                    "role": "implementation",
                    "title": "Run store",
                    "revision": 8,
                    "contract_revision": 4,
                    "status": "ready",
                    "risk": "medium",
                    "materialization": "R1",
                    "content_dir": "works/TK-031"
                },
                "snapshot": {
                    "graph_fingerprint": ZERO,
                    "readiness_profile": "phase1_contract_readiness_v1",
                    "readiness_fingerprint": ONE,
                    "readiness_status": "ready",
                    "authority_policy_revision": 1,
                    "authority_policy_fingerprint": TWO,
                    "docs_registry_revision": 1,
                    "docs_registry_fingerprint": ZERO,
                    "docs_index_fingerprint": ONE,
                    "source_commit": "0123456789abcdef0123456789abcdef01234567"
                },
                "contract": {
                    "mode": "guided",
                    "work_surface": "code",
                    "plan_policy": "worker_optional",
                    "semantic_impact": "behavior_or_public_risk_change",
                    "effort": {"multi_session": false, "multiple_dependent_decisions": false, "resume_or_audit_continuity": false},
                    "verification_profile": "service-change",
                    "brief": null,
                    "objective": "Implement run store",
                    "current_behavior": "none",
                    "target_behavior": "store",
                    "code_anchors": [],
                    "documentation_anchors": [],
                    "configuration_anchors": [],
                    "data_anchors": [],
                    "research_refs": [],
                    "required_changes": [],
                    "invariants": [],
                    "acceptance": [],
                    "scope": {"included": [], "excluded": []},
                    "implementation_freedom": [],
                    "required_decisions": [],
                    "shared_approach_refs": [],
                    "expected_evidence": [],
                    "expected_handoff": []
                },
                "context": {"parents": [], "decisions": []},
                "shaping": {
                    "status": "current",
                    "receipt_id": "rcpt_00000000000000000000000000",
                    "receipt_hash": ZERO,
                    "owning_work": {"id": "ST-001", "revision_observed": 3, "contract_revision": 2},
                    "shape_mode": "focused_branches",
                    "destination": null,
                    "map": null,
                    "critical_branches": [],
                    "bounded_fog": [],
                    "remaining_uncertainty": [],
                    "decision_frontier": {"status": "evaluated", "items": []}
                },
                "graph": {"structural_state": "executable", "hard_blockers": [], "soft_preferences": [], "supersession": null, "relations": {"outgoing": [], "incoming": []}},
                "documentation": {"applicability": {"status": "complete", "required": [], "optional": [], "write_candidates": [], "excluded": []}, "suggestion_query": {"text": "run store", "normalized_terms": []}, "suggested_sections": [], "read_budget": {"required_sections": 0, "recommended_initial_sections": 4, "max_initial_lines": 120, "suggestion_limit": 8, "snippet_max_bytes_each": 4096}, "index": {"state": "current", "fingerprint": ZERO, "mode": "lexical"}},
                "knowledge": {"status": "not_installed", "owner_phase": 4, "knowledge_fingerprint": null, "required": [], "recommended": [], "suggested": [], "excluded": []},
                "source": {"repository_id": "repo_test", "kind": "git", "commit": "0123456789abcdef0123456789abcdef01234567", "head_ref": null, "worktree_root_kind": "isolated_worktree", "cleanliness": "clean", "operation_state": "none", "currentness": "current"},
                "workspace": {"binding_status": "bound", "workspace_id": "wt_TK-031_01JTEST", "required_strategy": "isolated_worktree", "base_repository_id": "repo_test", "base_commit": "0123456789abcdef0123456789abcdef01234567", "requirements": []},
                "budget": {"profile": "phase2_packet_budget_v1", "max_canonical_json_bytes": 12000, "max_incident_relations": 40, "max_decision_frontier_items": 8, "max_suggested_sections": 8, "max_snippet_bytes_each": 4096, "recommended_initial_sections": 4, "max_initial_lines": 120, "actual_canonical_json_bytes": 0, "truncations": []},
                "dispatch": {"reservation_candidate": true, "dispatch_authorized": false, "authorization_status": "preview_only", "gate_families": [], "revalidation_preconditions": []},
                "capabilities": {"evaluation_status": "matched", "required": [], "optional": [], "missing": [], "inventory_identity": null},
                "scope": {"scope_hints": {"source_paths": [], "documentation_paths": [], "configuration_paths": [], "data_paths": [], "included": [], "excluded": []}, "implementation_freedom": [], "hard_stops": [], "enforcement": {"status": "bounded", "owner_phase": 2}},
                "assurance": {"verification_profile": "service-change", "expected_evidence": [], "expected_handoff": [], "documentation_impact": {"posture": "none", "status": "complete", "required_doc_ids": []}, "qa": {"posture": "none", "status": "not_started", "affected_case_ids": []}, "promotion_policy": {"status": "deferred", "owner_phase": 2}, "close_gate": {"status": "blocked", "owner_phase": 2}},
                "packet_fingerprint": ZERO,
                "reason_codes": []
            },
            "packet_fingerprint": ZERO,
            "revalidated_snapshot": {"graph_fingerprint": ZERO, "readiness_profile": "phase1_contract_readiness_v1", "readiness_fingerprint": ONE, "authority_policy_fingerprint": TWO, "docs_registry_fingerprint": ZERO, "docs_index_fingerprint": ONE, "source_commit": "0123456789abcdef0123456789abcdef01234567", "source_cleanliness": "clean", "repository_id": "repo_test"},
            "lease": {"lease_id": "lease_01JTEST", "state": "prepared", "assignee": "agent:codex-local", "issued_by": "human:test", "issued_at": "2026-07-29T10:00:00Z", "expires_at": "2026-07-29T11:00:00Z", "ttl_seconds": 3600, "exclusive": true},
            "workspace": minimal_workspace_summary(),
            "capability_match": {"inventory_identity": ZERO, "principal": "agent:codex-local", "status": "matched", "required": [], "matched": [], "missing": []},
            "lifecycle": {"transition": "ready_to_active", "gate_profile": "phase2_ready_to_active_v1", "gate_status": "passed", "expected_revision": 8, "new_revision": 9, "event_id": "evt_01JTEST"},
            "dispatch": {"dispatch_authorized": true, "authorization_status": "authorized", "runner_status": "not_started", "gate_families": []},
            "transaction": {"transaction_id": "txn_01JTEST", "committed_targets": [], "event_path": ".pulse/events/evt_01JTEST.json", "recovery_state": "committed"},
            "prepared_assignment_fingerprint": ZERO,
            "reason_codes": []
        })).unwrap()
    }

    #[test]
    fn run_store_preserve_missing_dir_and_create_new_collision() {
        let repo = enrolled_repo();
        let list = list_runs_preserve(repo.path()).unwrap();
        assert!(list.runs.is_empty());
        assert!(list.invalid_records.is_empty());
        assert!(!repo.path().join(".pulse/runtime").exists());

        let run = run_record();
        write_run_record(repo.path(), &run, RunStoreWriteMode::CreateNew).unwrap();
        let bytes = fs::read(run_record_path(repo.path(), &run.run_id).unwrap()).unwrap();
        assert_eq!(
            bytes,
            crate::canonical_json::to_canonical_bytes(&run).unwrap()
        );
        let duplicate =
            write_run_record(repo.path(), &run, RunStoreWriteMode::CreateNew).unwrap_err();
        assert_eq!(duplicate.code(), "io_error");
    }

    #[test]
    fn run_store_atomic_replace_and_show_projection() {
        let repo = enrolled_repo();
        let mut run = run_record();
        let attempt = attempt_record();
        write_run_record(repo.path(), &run, RunStoreWriteMode::CreateNew).unwrap();
        write_attempt_record(repo.path(), &attempt, RunStoreWriteMode::CreateNew).unwrap();

        run.state = RunStateV1::Interrupted;
        run.run_fingerprint = run.compute_fingerprint().unwrap();
        write_run_record(repo.path(), &run, RunStoreWriteMode::Replace).unwrap();

        let view = show_run_preserve(repo.path(), &run.run_id).unwrap();
        assert_eq!(view.run.unwrap().state, RunStateV1::Interrupted);
        assert_eq!(view.current_attempt.unwrap().attempt_id, attempt.attempt_id);
        assert!(!view.terminal_observation_pending);
    }

    #[test]
    fn run_store_rejects_noncanonical_unknown_and_traversal_paths() {
        let repo = enrolled_repo();
        let mut run = run_record();
        write_run_record(repo.path(), &run, RunStoreWriteMode::CreateNew).unwrap();
        let path = run_record_path(repo.path(), &run.run_id).unwrap();
        let mut value = serde_json::to_value(&run).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("native_thread".to_string(), serde_json::json!("bad"));
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let error = read_run_record_preserve(repo.path(), &run.run_id).unwrap_err();
        assert_eq!(error.code(), "json_error");

        run.run_id = "run_../escape".to_string();
        let error = write_run_record(repo.path(), &run, RunStoreWriteMode::CreateNew).unwrap_err();
        assert_eq!(error.code(), "invalid_run_record_id");
    }

    #[test]
    fn run_store_classifies_filename_id_mismatch_alias_and_broken_relation() {
        let repo = enrolled_repo();
        let run = run_record();
        let attempt = attempt_record();
        write_run_record(repo.path(), &run, RunStoreWriteMode::CreateNew).unwrap();
        write_attempt_record(repo.path(), &attempt, RunStoreWriteMode::CreateNew).unwrap();

        let alias_path = runs_dir(repo.path()).join("run_ALIAS.json");
        fs::write(
            &alias_path,
            crate::canonical_json::to_canonical_bytes(&run).unwrap(),
        )
        .unwrap();
        let classifications = classify_run_store_preserve(repo.path()).unwrap();
        assert!(classifications
            .iter()
            .any(|item| item.status == RunStoreRecordStatus::Invalid
                && item
                    .reason_codes
                    .contains(&"run_id_path_mismatch".to_string())));
        assert!(classifications
            .iter()
            .any(|item| item.status == RunStoreRecordStatus::Ambiguous
                && item
                    .reason_codes
                    .contains(&"duplicate_internal_run_id".to_string())));

        let mut cross_run_attempt = attempt;
        cross_run_attempt.run_id = "run_OTHER".to_string();
        cross_run_attempt.attempt_fingerprint = cross_run_attempt.compute_fingerprint().unwrap();
        fs::write(
            attempt_record_path(repo.path(), &cross_run_attempt.attempt_id).unwrap(),
            crate::canonical_json::to_canonical_bytes(&cross_run_attempt).unwrap(),
        )
        .unwrap();
        let classifications = classify_run_store_preserve(repo.path()).unwrap();
        assert!(classifications
            .iter()
            .any(|item| item.status == RunStoreRecordStatus::Invalid
                && item
                    .reason_codes
                    .contains(&"invalid_run_record".to_string())));
    }

    #[test]
    fn run_store_rejects_wrong_layout_paths_and_tampered_input_fingerprint() {
        let repo = enrolled_repo();
        let mut attempt = attempt_record();
        attempt.input.json_path = ".pulse/runtime/run/control/run_01JTEST.json".to_string();
        attempt.attempt_fingerprint = attempt.compute_fingerprint().unwrap();
        let error =
            write_attempt_record(repo.path(), &attempt, RunStoreWriteMode::CreateNew).unwrap_err();
        assert_eq!(error.code(), "invalid_run_record");

        let input = run_input_record();
        write_run_input(repo.path(), &input, "prompt", RunStoreWriteMode::CreateNew).unwrap();
        assert!(
            read_run_input_preserve(repo.path(), &input.run_id, &input.attempt_id)
                .unwrap()
                .is_some()
        );
        let path = input_json_path(repo.path(), &input.run_id, &input.attempt_id).unwrap();
        let mut tampered = input.clone();
        tampered.instructions.objective = "tampered".to_string();
        fs::write(
            &path,
            crate::canonical_json::to_canonical_bytes(&tampered).unwrap(),
        )
        .unwrap();
        let error =
            read_run_input_preserve(repo.path(), &input.run_id, &input.attempt_id).unwrap_err();
        assert_eq!(error.code(), "run_input_invalid");
    }

    #[test]
    fn run_store_classifies_orphan_only_tree_and_oversize_record() {
        let repo = enrolled_repo();
        fs::create_dir_all(control_dir(repo.path())).unwrap();
        fs::write(
            control_dir(repo.path()).join("run_ORPHAN.exit.json"),
            b"{}\n",
        )
        .unwrap();
        fs::create_dir_all(logs_dir(repo.path()).join("run_ORPHAN")).unwrap();
        fs::write(
            logs_dir(repo.path()).join("run_ORPHAN/attempt_X.stdout.prefix.log"),
            b"x",
        )
        .unwrap();

        let classifications = classify_run_store_preserve(repo.path()).unwrap();
        assert!(classifications
            .iter()
            .any(|item| item.status == RunStoreRecordStatus::OrphanControl));
        assert!(classifications
            .iter()
            .any(|item| item.status == RunStoreRecordStatus::OrphanLog));

        fs::create_dir_all(runs_dir(repo.path())).unwrap();
        fs::write(
            runs_dir(repo.path()).join("run_TOO_BIG.json"),
            vec![b' '; (MAX_RECORD_BYTES + 1) as usize],
        )
        .unwrap();
        let classifications = classify_run_store_preserve(repo.path()).unwrap();
        assert!(classifications.iter().any(|item| item
            .reason_codes
            .contains(&"run_record_too_large".to_string())));
    }

    #[test]
    #[cfg(unix)]
    fn run_store_reports_non_utf8_names_and_sets_file_modes() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let repo = enrolled_repo();
        fs::create_dir_all(runs_dir(repo.path())).unwrap();
        let non_utf8 = OsString::from_vec(b"run_BAD_\xFF.json".to_vec());
        if fs::write(runs_dir(repo.path()).join(non_utf8), b"{}\n").is_ok() {
            let classifications = classify_run_store_preserve(repo.path()).unwrap();
            assert!(classifications.iter().any(|item| item
                .reason_codes
                .contains(&"run_store_non_utf8_name".to_string())));
        }

        let run = run_record();
        write_run_record(repo.path(), &run, RunStoreWriteMode::CreateNew).unwrap();
        let mode = fs::metadata(run_record_path(repo.path(), &run.run_id).unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn run_store_classifies_invalid_orphan_control_and_orphan_log() {
        let repo = enrolled_repo();
        let run = run_record();
        write_run_record(repo.path(), &run, RunStoreWriteMode::CreateNew).unwrap();
        fs::write(
            run_record_path(repo.path(), &run.run_id).unwrap(),
            b"{\"bad\":true}\n",
        )
        .unwrap();
        fs::create_dir_all(control_dir(repo.path())).unwrap();
        fs::write(
            control_dir(repo.path()).join("run_ORPHAN.exit.json"),
            b"{}\n",
        )
        .unwrap();
        fs::create_dir_all(logs_dir(repo.path()).join("run_ORPHAN")).unwrap();
        fs::write(
            logs_dir(repo.path()).join("run_ORPHAN/attempt_X.stdout.prefix.log"),
            b"x",
        )
        .unwrap();

        let classifications = classify_run_store_preserve(repo.path()).unwrap();
        assert!(classifications
            .iter()
            .any(|item| item.status == RunStoreRecordStatus::Invalid));
        assert!(classifications
            .iter()
            .any(|item| item.status == RunStoreRecordStatus::OrphanControl));
        assert!(classifications
            .iter()
            .any(|item| item.status == RunStoreRecordStatus::OrphanLog));
    }

    #[test]
    #[cfg(unix)]
    fn run_store_rejects_symlink_component_escape() {
        let repo = enrolled_repo();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir_all(repo.path().join(".pulse/runtime")).unwrap();
        std::os::unix::fs::symlink(outside.path(), repo.path().join(".pulse/runtime/run")).unwrap();
        let error =
            write_run_record(repo.path(), &run_record(), RunStoreWriteMode::CreateNew).unwrap_err();
        assert_eq!(error.code(), "run_control_record_invalid");
        assert!(!outside.path().join("runs").exists());
    }

    #[test]
    fn run_store_non_enrolled_does_not_bootstrap() {
        let repo = tempfile::tempdir().unwrap();
        let error = list_runs_preserve(repo.path()).unwrap_err();
        assert_eq!(error.code(), "not_enrolled");
        assert!(!repo.path().join(".pulse").exists());
    }

    #[test]
    #[cfg(unix)]
    fn effective_execute_permission_uses_unix_access_classes() {
        assert!(effective_execute_allowed(0o100, 10, 20, 10, &[20]));
        assert!(!effective_execute_allowed(0o010, 10, 20, 10, &[20]));
        assert!(!effective_execute_allowed(0o001, 10, 20, 10, &[20]));
        assert!(effective_execute_allowed(0o010, 10, 20, 11, &[20]));
        assert!(!effective_execute_allowed(0o001, 10, 20, 11, &[20]));
        assert!(effective_execute_allowed(0o001, 10, 20, 11, &[21]));
        assert!(effective_execute_allowed(0o001, 10, 20, 0, &[]));
        assert!(!effective_execute_allowed(0o000, 10, 20, 0, &[]));
    }

    #[test]
    fn missing_registry_is_preserve_only_and_does_not_bootstrap_runtime() {
        let repo = enrolled_repo();
        let error = load_runner_profile_registry_preserve(repo.path()).unwrap_err();
        assert_eq!(error.code(), "run_profile_missing");
        assert!(!repo.path().join(".pulse/runtime").exists());
    }

    #[test]
    fn non_enrolled_registry_load_rejects_before_runtime_creation() {
        let repo = tempfile::tempdir().unwrap();
        let error = load_runner_profile_registry_preserve(repo.path()).unwrap_err();
        assert_eq!(error.code(), "not_enrolled");
        assert!(!repo.path().join(".pulse").exists());
    }

    #[test]
    fn absolute_executable_resolution_records_local_identity_outside_profile_fingerprint() {
        let repo = enrolled_repo();
        let bin_dir = tempfile::tempdir().unwrap();
        let executable = executable_file(bin_dir.path(), "codex-test");
        let mut registry = registry(canonical_utf8(&executable));
        registry.profiles[0]
            .environment_set
            .insert("PULSE_LITERAL".to_string(), serde_json::json!("non-secret"));
        write_registry(repo.path(), &registry);

        let selected = select_runner_profile_preserve(repo.path(), None).unwrap();
        assert_eq!(selected.profile_id, "codex-local");
        assert_eq!(
            selected.executable.resolved_path,
            canonical_utf8(&executable)
        );
        assert!(selected.executable.identity.starts_with("sha256:"));
        assert!(!selected
            .profile_fingerprint
            .contains(&selected.executable.resolved_path));
        let rendered = serde_json::to_string(&selected).unwrap();
        assert!(!rendered.contains("non-secret"));
        assert!(rendered.contains("PULSE_LITERAL"));
        assert!(rendered.contains("literal_non_secret"));
    }

    #[test]
    fn bare_executable_resolution_uses_inherited_path_order() {
        let path_guard = PathEnvGuard::new();
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let first_exe = executable_file(first.path(), "codex-test");
        let second_exe = executable_file(second.path(), "codex-test");
        let first_path = fs::canonicalize(first.path()).unwrap();
        let second_path = fs::canonicalize(second.path()).unwrap();
        path_guard.set_path([second_path.as_path(), first_path.as_path()]);
        let selected =
            select_profile_from_registry(&registry("codex-test".to_string()), None).unwrap();
        assert_eq!(
            selected.executable.resolved_path,
            canonical_utf8(&second_exe)
        );
        assert_ne!(
            selected.executable.resolved_path,
            canonical_utf8(&first_exe)
        );
    }

    #[test]
    fn executable_rejects_relative_separators_and_command_blobs() {
        for executable in ["./codex", "tools/codex", "codex --danger"] {
            let error =
                select_profile_from_registry(&registry(executable.to_string()), None).unwrap_err();
            assert_eq!(error.code(), "run_profile_invalid");
        }
    }

    #[test]
    fn fixture_adapter_is_crate_private_injection_only() {
        let registry = fixture_adapter::fixture_registry_for_tests("codex-test");
        assert_eq!(
            registry.profiles[0].adapter,
            RunnerAdapterV1::CodexProcessV1
        );
        assert_eq!(registry.default_profile, "fixture-local");

        let fixture_selection = fixture_adapter::fixture_process_selection_for_tests(
            "fixture-bin",
            vec!["--controlled".to_string()],
        );
        assert_eq!(
            fixture_selection.adapter,
            fixture_adapter::FIXTURE_PROCESS_ADAPTER
        );
        assert_eq!(fixture_selection.executable, "fixture-bin");
        assert_eq!(fixture_selection.fixed_args, vec!["--controlled"]);
    }
}
