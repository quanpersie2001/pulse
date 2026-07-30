//! Pure Slice 3 run value contracts.
//!
//! This module owns only public DTOs, enums, deterministic normalization and
//! fingerprints for the P2S3 single-agent runner contract. It intentionally has
//! no graph-store, filesystem, process, Git, workspace, CLI or policy imports.

use crate::assignment::{AssignmentWorkspaceSummary, PreparedAssignmentV1};
use crate::canonical_json::{self, hash_bytes, hash_serializable};
use crate::{PulseError, PulseResult, Result};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use std::collections::HashSet;

pub const RUN_SCHEMA_VERSION: u32 = 1;
pub const RUN_KIND_SINGLE_AGENT_IMPLEMENTATION: &str = "single_agent_implementation";
pub const RUN_INPUT_PROFILE: &str = "phase2_single_agent_run_input_v1";
pub const RUNNER_PROFILE_SCHEMA_VERSION: u32 = 1;
pub const PUBLIC_CODEX_ADAPTER: &str = "codex_process_v1";
pub const RUNNER_PROFILE_MAX_PROFILES: usize = 32;
pub const RUNNER_PROFILE_MAX_ID_BYTES: usize = 128;
pub const RUNNER_PROFILE_MAX_EXECUTABLE_BYTES: usize = 4096;
pub const RUNNER_PROFILE_MAX_FIXED_ARGS: usize = 64;
pub const RUNNER_PROFILE_MAX_ARG_BYTES: usize = 4096;
pub const RUNNER_PROFILE_MAX_ENVIRONMENT_ALLOW: usize = 128;
pub const RUNNER_PROFILE_MAX_ENVIRONMENT_SET: usize = 128;
pub const RUNNER_PROFILE_MAX_ENVIRONMENT_VALUE_BYTES: usize = 4096;
pub const RUNNER_PROFILE_MIN_START_TIMEOUT_SECONDS: u64 = 1;
pub const RUNNER_PROFILE_MAX_START_TIMEOUT_SECONDS: u64 = 300;
pub const RUNNER_PROFILE_MIN_RUN_TIMEOUT_SECONDS: u64 = 60;
pub const RUNNER_PROFILE_MAX_RUN_TIMEOUT_SECONDS: u64 = 86_400;
pub const RUNNER_PROFILE_MIN_CANCEL_GRACE_SECONDS: u64 = 1;
pub const RUNNER_PROFILE_MAX_CANCEL_GRACE_SECONDS: u64 = 300;
pub const RUNNER_PROFILE_MIN_FORCE_KILL_AFTER_SECONDS: u64 = 0;
pub const RUNNER_PROFILE_MAX_FORCE_KILL_AFTER_SECONDS: u64 = 300;
pub const RUNNER_PROFILE_MIN_LOG_BYTES: u64 = 65_536;
pub const RUNNER_PROFILE_MAX_LOG_BYTES: u64 = 67_108_864;
pub const NATIVE_RESUME_STATUS: &str = "not_installed";
pub const DEFAULT_LOG_REDACTION_STATUS: &str = "not_applied_runtime_private";
pub const RUN_INPUT_CONFIDENTIALITY: &str = "runtime_private_repository_sensitive";

pub const RUN_SCHEMA: &str = include_str!("schema/run/run.schema.json");
pub const RUN_ATTEMPT_SCHEMA: &str = include_str!("schema/run/run-attempt.schema.json");
pub const RUN_INPUT_SCHEMA: &str = include_str!("schema/run/run-input.schema.json");
pub const WORKSPACE_SNAPSHOT_SCHEMA: &str =
    include_str!("schema/run/workspace-snapshot.schema.json");
pub const RUNNER_PROFILES_SCHEMA: &str = include_str!("schema/run/runner-profiles.schema.json");
pub const RUN_START_REPORT_SCHEMA: &str = include_str!("schema/run/run-start-report.schema.json");
pub const RUN_CANCEL_REPORT_SCHEMA: &str = include_str!("schema/run/run-cancel-report.schema.json");
pub const RUN_RECOVERY_REPORT_SCHEMA: &str =
    include_str!("schema/run/run-recovery-report.schema.json");
pub const RUN_VIEW_SCHEMA: &str = include_str!("schema/run/run-view.schema.json");
pub const RUN_LIST_REPORT_SCHEMA: &str = include_str!("schema/run/run-list-report.schema.json");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStateV1 {
    Starting,
    Running,
    CancelRequested,
    Interrupted,
    Exited,
    Cancelled,
    FailedToStart,
    StaleNeedsOperator,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunAttemptStateV1 {
    Starting,
    Running,
    CancelRequested,
    Exited,
    Cancelled,
    Interrupted,
    FailedToStart,
    StaleNeedsOperator,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunnerAdapterV1 {
    CodexProcessV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NativeResumeStatusV1 {
    NotInstalled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RunnerEnvironmentSourceV1 {
    Inherited,
    LiteralNonSecret,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunnerEnvironmentSpecEntryV1 {
    pub name: String,
    pub source: RunnerEnvironmentSourceV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunnerExecutableIdentityV1 {
    pub resolved_path: String,
    pub identity: String,
    pub identity_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunnerProfileSelectionV1 {
    pub schema_version: u32,
    pub profile_id: String,
    pub adapter: RunnerAdapterV1,
    pub profile_fingerprint: String,
    pub environment_spec_fingerprint: String,
    pub executable: RunnerExecutableIdentityV1,
    pub fixed_args: Vec<String>,
    pub environment: Vec<RunnerEnvironmentSpecEntryV1>,
    #[serde(default, skip_serializing)]
    pub literal_environment_values: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceModeV1 {
    InPlace,
    IsolatedWorktree,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceOperationStateV1 {
    None,
    Merge,
    Rebase,
    CherryPick,
    Revert,
    Bisect,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceCleanlinessV1 {
    Clean,
    Dirty,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceSnapshotStatusV1 {
    Complete,
    Unsupported,
    BoundedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunInputModeV1 {
    Start,
    Resume,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResumeEligibilityV1 {
    Available,
    NotAvailable,
    Blocked,
    NotEvaluated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunExitKindV1 {
    Exited,
    Cancelled,
    TimedOut,
    Interrupted,
    FailedToStart,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunLogHashScopeV1 {
    FullUntruncatedContent,
    RetainedBytesOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunRecordV1 {
    pub schema_version: u32,
    pub run_id: String,
    pub kind: String,
    pub state: RunStateV1,
    pub subject: RunSubjectV1,
    pub assignment: RunAssignmentV1,
    pub workspace: RunWorkspaceBindingV1,
    pub runner: RunRunnerV1,
    pub current_attempt_id: String,
    pub attempt_ids: Vec<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(deserialize_with = "required_option")]
    pub last_heartbeat_at: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub latest_exit: Option<RunExitResultV1>,
    #[serde(deserialize_with = "required_option")]
    pub latest_workspace_snapshot_identity: Option<String>,
    pub reason_codes: Vec<String>,
    pub run_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunSubjectV1 {
    pub kind: String,
    pub id: String,
    pub active_revision: u64,
    pub contract_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunAssignmentV1 {
    pub lease_id: String,
    pub prepared_assignment_id: String,
    pub prepared_assignment_fingerprint: String,
    pub packet_fingerprint: String,
    pub assignee: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunWorkspaceBindingV1 {
    pub workspace_id: String,
    pub mode: WorkspaceModeV1,
    pub path: String,
    pub repository_id: String,
    pub base_commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunRunnerV1 {
    pub adapter: RunnerAdapterV1,
    pub profile_id: String,
    pub profile_fingerprint: String,
    #[serde(deserialize_with = "required_option")]
    pub resolved_executable_identity: Option<String>,
    pub native_resume_status: NativeResumeStatusV1,
    #[serde(deserialize_with = "required_option")]
    pub native_thread_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunAttemptRecordV1 {
    pub schema_version: u32,
    pub attempt_id: String,
    pub run_id: String,
    pub attempt_number: u64,
    pub state: RunAttemptStateV1,
    pub input: RunAttemptInputRefV1,
    pub process: RunAttemptProcessV1,
    pub workspace_before: WorkspaceSnapshotV1,
    #[serde(deserialize_with = "required_option")]
    pub workspace_after: Option<WorkspaceSnapshotV1>,
    pub logs: RunAttemptLogsV1,
    pub timeout_seconds: u64,
    pub cancel: RunCancelStateV1,
    pub created_at: String,
    pub updated_at: String,
    pub reason_codes: Vec<String>,
    pub attempt_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunAttemptInputRefV1 {
    pub run_input_identity: String,
    pub json_path: String,
    pub rendered_prompt_identity: String,
    pub rendered_prompt_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunAttemptProcessV1 {
    #[serde(deserialize_with = "required_option")]
    pub identity: Option<ProcessIdentityV1>,
    #[serde(deserialize_with = "required_option")]
    pub started_at: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub ended_at: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub exit: Option<RunExitResultV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProcessIdentityV1 {
    pub supervisor_pid: u64,
    pub child_pid: u64,
    #[serde(deserialize_with = "required_option")]
    pub process_group_id: Option<u64>,
    pub supervisor_nonce_hash: String,
    pub started_at: String,
    pub platform_start_marker: String,
    pub argv_hash: String,
    pub executable_identity: String,
    pub identity_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunExitResultV1 {
    pub kind: RunExitKindV1,
    #[serde(deserialize_with = "required_option")]
    pub code: Option<i32>,
    #[serde(deserialize_with = "required_option")]
    pub signal: Option<i32>,
    pub timed_out: bool,
    pub cancelled: bool,
    pub observed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunAttemptLogsV1 {
    pub stdout: RunLogRefV1,
    pub stderr: RunLogRefV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunLogRefV1 {
    pub path: String,
    #[serde(deserialize_with = "required_option")]
    pub retained_prefix_path: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub retained_tail_path: Option<String>,
    pub bytes_seen: u64,
    pub bytes_retained: u64,
    pub bytes_truncated: u64,
    pub content_hash: String,
    pub hash_scope: RunLogHashScopeV1,
    pub truncated: bool,
    pub redaction_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunCancelStateV1 {
    #[serde(deserialize_with = "required_option")]
    pub requested_at: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub requested_by: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub reason: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub grace_seconds: Option<u64>,
    #[serde(deserialize_with = "required_option")]
    pub force_allowed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceSnapshotV1 {
    pub schema_version: u32,
    pub repository_id: String,
    pub workspace_id: String,
    pub workspace_mode: WorkspaceModeV1,
    pub base_commit: String,
    pub head_commit: String,
    pub diff_base_commit: String,
    pub operation_state: WorkspaceOperationStateV1,
    pub cleanliness: WorkspaceCleanlinessV1,
    pub tracked_diff_identity: String,
    pub untracked_manifest_identity: String,
    pub status_identity: String,
    pub snapshot_status: WorkspaceSnapshotStatusV1,
    pub captured_at: String,
    pub snapshot_identity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunInputV1 {
    pub schema_version: u32,
    pub profile: String,
    pub run_id: String,
    pub attempt_id: String,
    pub attempt_number: u64,
    pub mode: RunInputModeV1,
    pub prepared_assignment: PreparedAssignmentV1,
    pub workspace: AssignmentWorkspaceSummary,
    pub runner_profile: RunInputRunnerProfileV1,
    pub instructions: RunInstructionsV1,
    pub resume: RunInputResumeContextV1,
    pub input_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunInputRunnerProfileV1 {
    pub profile_id: String,
    pub adapter: RunnerAdapterV1,
    pub profile_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunInstructionsV1 {
    pub objective: String,
    pub acceptance: Vec<String>,
    pub required_changes: Vec<String>,
    pub invariants: Vec<String>,
    pub hard_stops: Vec<String>,
    pub expected_evidence: Vec<String>,
    pub expected_handoff: Vec<String>,
    pub authority_boundary: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunInputResumeContextV1 {
    #[serde(deserialize_with = "required_option")]
    pub previous_attempt_id: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub workspace_snapshot_identity: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub previous_exit_kind: Option<RunExitKindV1>,
    #[serde(deserialize_with = "required_option")]
    pub redacted_log_tail: Option<String>,
    pub native_resume_status: NativeResumeStatusV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunnerProfileRegistryV1 {
    pub schema_version: u32,
    pub default_profile: String,
    pub profiles: Vec<RunnerProfileV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunnerProfileV1 {
    pub profile_id: String,
    pub adapter: RunnerAdapterV1,
    pub executable: String,
    pub fixed_args: Vec<String>,
    pub environment_allow: Vec<String>,
    pub environment_set: Map<String, Value>,
    pub start_timeout_seconds: u64,
    pub run_timeout_seconds: u64,
    pub cancel_grace_seconds: u64,
    pub force_kill_after_seconds: u64,
    pub max_stdout_bytes: u64,
    pub max_stderr_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunnerProfileThreatModelV1 {
    pub schema_version: u32,
    pub public_adapter: RunnerAdapterV1,
    pub executable_resolution: String,
    pub shell_invocation: String,
    pub inherited_environment_values_recorded: bool,
    pub environment_fingerprint_semantics: String,
    pub raw_prompt_storage: String,
    pub raw_log_storage: String,
    pub default_log_redaction_status: String,
    pub native_resume_status: NativeResumeStatusV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunStartReportV1 {
    pub schema_version: u32,
    pub run: RunRecordV1,
    pub attempt: RunAttemptRecordV1,
    pub terminal_observation_pending: bool,
    pub handoff_status: String,
    pub verification_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunCancelReportV1 {
    pub schema_version: u32,
    pub run_id: String,
    pub attempt_id: String,
    pub state: RunStateV1,
    pub already_terminal: bool,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunRecoveryReportV1 {
    pub schema_version: u32,
    pub classifications: Vec<RunRecoveryClassificationV1>,
    pub mutations_applied: Vec<String>,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunRecoveryClassificationV1 {
    #[serde(deserialize_with = "required_option")]
    pub run_id: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub attempt_id: Option<String>,
    pub classification: String,
    pub mutation_available: bool,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunViewV1 {
    pub schema_version: u32,
    #[serde(deserialize_with = "required_option")]
    pub run: Option<RunRecordV1>,
    #[serde(deserialize_with = "required_option")]
    pub current_attempt: Option<RunAttemptRecordV1>,
    pub resume_eligibility: ResumeEligibilityV1,
    pub resume_blockers: Vec<String>,
    pub terminal_observation_pending: bool,
    #[serde(deserialize_with = "required_option")]
    pub invalid_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunListReportV1 {
    pub schema_version: u32,
    pub runs: Vec<RunViewV1>,
    pub invalid_records: Vec<RunRecoveryClassificationV1>,
}

impl RunRecordV1 {
    pub fn normalize(&mut self) {
        self.reason_codes.sort();
        self.reason_codes.dedup();
    }

    pub fn compute_fingerprint(&self) -> PulseResult<String> {
        fingerprint_without_fields(self, &["run_fingerprint", "last_heartbeat_at"])
    }
}

impl RunAttemptRecordV1 {
    pub fn normalize(&mut self) {
        self.reason_codes.sort();
        self.reason_codes.dedup();
    }

    pub fn compute_fingerprint(&self) -> PulseResult<String> {
        fingerprint_without_fields(self, &["attempt_fingerprint"])
    }
}

impl WorkspaceSnapshotV1 {
    pub fn compute_identity(&self) -> PulseResult<String> {
        fingerprint_without_fields(self, &["snapshot_identity", "captured_at"])
    }
}

impl RunInputV1 {
    pub fn normalize(&mut self) {
        normalize_strings(&mut self.instructions.acceptance);
        normalize_strings(&mut self.instructions.required_changes);
        normalize_strings(&mut self.instructions.invariants);
        normalize_strings(&mut self.instructions.hard_stops);
        normalize_strings(&mut self.instructions.expected_evidence);
        normalize_strings(&mut self.instructions.expected_handoff);
        normalize_strings(&mut self.instructions.authority_boundary);
        self.prepared_assignment.normalize();
    }

    pub fn compute_fingerprint(&self) -> PulseResult<String> {
        fingerprint_without_fields(self, &["input_fingerprint"])
    }
}

impl RunnerProfileRegistryV1 {
    pub fn normalize(&mut self) {
        for profile in &mut self.profiles {
            profile.normalize();
        }
        self.profiles
            .sort_by(|left, right| left.profile_id.cmp(&right.profile_id));
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != RUNNER_PROFILE_SCHEMA_VERSION {
            return Err(PulseError::validation(
                "run_profile_invalid",
                "runner profile registry schema_version must be 1",
            ));
        }
        if self.profiles.is_empty() || self.profiles.len() > RUNNER_PROFILE_MAX_PROFILES {
            return Err(PulseError::validation(
                "run_profile_invalid",
                "runner profile registry must contain 1..32 profiles",
            ));
        }
        validate_profile_id(&self.default_profile)?;
        let mut ids = HashSet::new();
        let mut has_default = false;
        for profile in &self.profiles {
            profile.validate_public()?;
            if !ids.insert(profile.profile_id.clone()) {
                return Err(PulseError::validation(
                    "run_profile_invalid",
                    format!("duplicate runner profile id {}", profile.profile_id),
                ));
            }
            if profile.profile_id == self.default_profile {
                has_default = true;
            }
        }
        if !has_default {
            return Err(PulseError::validation(
                "run_profile_missing",
                "default runner profile is not present in registry",
            ));
        }
        Ok(())
    }

    pub fn profile_fingerprint(&self, profile_id: &str) -> Result<String> {
        let profile = self
            .profiles
            .iter()
            .find(|profile| profile.profile_id == profile_id)
            .ok_or_else(|| {
                PulseError::validation("run_profile_missing", "runner profile not found")
            })?;
        profile.fingerprint()
    }
}

impl RunnerProfileV1 {
    pub fn normalize(&mut self) {
        // fixed_args is an ordered argv segment: order and duplicates are
        // semantic for both execution and profile fingerprints.
        normalize_strings(&mut self.environment_allow);
    }

    pub fn validate_public(&self) -> Result<()> {
        validate_profile_id(&self.profile_id)?;
        if self.adapter != RunnerAdapterV1::CodexProcessV1 {
            return Err(PulseError::validation(
                "run_profile_invalid",
                "public runner profiles may only use codex_process_v1",
            ));
        }
        validate_executable(&self.executable)?;
        if self.fixed_args.len() > RUNNER_PROFILE_MAX_FIXED_ARGS
            || self
                .fixed_args
                .iter()
                .any(|arg| arg.len() > RUNNER_PROFILE_MAX_ARG_BYTES || arg.contains('\0'))
        {
            return Err(PulseError::validation(
                "run_profile_invalid",
                "fixed_args exceeds Slice 3 bounds",
            ));
        }
        if self.environment_allow.len() > RUNNER_PROFILE_MAX_ENVIRONMENT_ALLOW {
            return Err(PulseError::validation(
                "run_profile_invalid",
                "environment_allow exceeds Slice 3 bounds",
            ));
        }
        for name in &self.environment_allow {
            validate_env_name(name)?;
        }
        if self.environment_set.len() > RUNNER_PROFILE_MAX_ENVIRONMENT_SET {
            return Err(PulseError::validation(
                "run_profile_invalid",
                "environment_set exceeds Slice 3 bounds",
            ));
        }
        for (name, value) in &self.environment_set {
            validate_env_name(name)?;
            let Some(value) = value.as_str() else {
                return Err(PulseError::validation(
                    "run_profile_invalid",
                    "environment_set values must be literal strings",
                ));
            };
            if value.is_empty()
                || value.len() > RUNNER_PROFILE_MAX_ENVIRONMENT_VALUE_BYTES
                || value.contains('\0')
                || looks_like_tracked_secret(name, value)
            {
                return Err(PulseError::validation(
                    "run_profile_invalid",
                    "tracked literal secret-bearing environment values are disallowed",
                ));
            }
        }
        validate_range(
            self.start_timeout_seconds,
            RUNNER_PROFILE_MIN_START_TIMEOUT_SECONDS,
            RUNNER_PROFILE_MAX_START_TIMEOUT_SECONDS,
            "start_timeout_seconds",
        )?;
        validate_range(
            self.run_timeout_seconds,
            RUNNER_PROFILE_MIN_RUN_TIMEOUT_SECONDS,
            RUNNER_PROFILE_MAX_RUN_TIMEOUT_SECONDS,
            "run_timeout_seconds",
        )?;
        validate_range(
            self.cancel_grace_seconds,
            RUNNER_PROFILE_MIN_CANCEL_GRACE_SECONDS,
            RUNNER_PROFILE_MAX_CANCEL_GRACE_SECONDS,
            "cancel_grace_seconds",
        )?;
        validate_range(
            self.force_kill_after_seconds,
            RUNNER_PROFILE_MIN_FORCE_KILL_AFTER_SECONDS,
            RUNNER_PROFILE_MAX_FORCE_KILL_AFTER_SECONDS,
            "force_kill_after_seconds",
        )?;
        validate_range(
            self.max_stdout_bytes,
            RUNNER_PROFILE_MIN_LOG_BYTES,
            RUNNER_PROFILE_MAX_LOG_BYTES,
            "max_stdout_bytes",
        )?;
        validate_range(
            self.max_stderr_bytes,
            RUNNER_PROFILE_MIN_LOG_BYTES,
            RUNNER_PROFILE_MAX_LOG_BYTES,
            "max_stderr_bytes",
        )?;
        Ok(())
    }

    pub fn fingerprint(&self) -> Result<String> {
        self.validate_public()?;
        let mut owned = self.clone();
        owned.normalize();
        hash_serializable(&owned)
    }

    pub fn environment_spec_fingerprint(&self) -> Result<String> {
        self.validate_public()?;
        #[derive(Serialize)]
        struct EnvSpec<'a> {
            inherited: Vec<&'a str>,
            literal_non_secret: Vec<&'a str>,
        }
        let mut inherited = self
            .environment_allow
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        inherited.sort_unstable();
        inherited.dedup();
        let mut literal_non_secret = self
            .environment_set
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        literal_non_secret.sort_unstable();
        literal_non_secret.dedup();
        hash_serializable(&EnvSpec {
            inherited,
            literal_non_secret,
        })
    }
}

pub fn runner_profile_threat_model() -> RunnerProfileThreatModelV1 {
    RunnerProfileThreatModelV1 {
        schema_version: 1,
        public_adapter: RunnerAdapterV1::CodexProcessV1,
        executable_resolution:
            "absolute_normalized_non_symlink_regular_executable_or_bare_path_first_safe_executable_no_repository_relative_paths_unix_execute_bits_non_unix_unsupported"
                .to_string(),
        shell_invocation: "never".to_string(),
        inherited_environment_values_recorded: false,
        environment_fingerprint_semantics: "names_and_source_classes_only_no_inherited_values"
            .to_string(),
        raw_prompt_storage: RUN_INPUT_CONFIDENTIALITY.to_string(),
        raw_log_storage: "runtime_private_gitignored_bounded_prefix_tail".to_string(),
        default_log_redaction_status: DEFAULT_LOG_REDACTION_STATUS.to_string(),
        native_resume_status: NativeResumeStatusV1::NotInstalled,
    }
}

fn required_option<'de, D, T>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

fn fingerprint_without_fields<T: Serialize>(value: &T, excluded: &[&str]) -> PulseResult<String> {
    let value = serde_json::to_value(value)?;
    let projection = strip_fields(&value, excluded);
    let canonical = canonical_json::to_canonical_value(&projection)?;
    let bytes = canonical_json::canonical_value_bytes(&canonical)?;
    Ok(hash_bytes(&bytes))
}

fn strip_fields(value: &Value, excluded: &[&str]) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, child) in map {
                if excluded.iter().any(|excluded_key| excluded_key == key) {
                    continue;
                }
                out.insert(key.clone(), strip_fields(child, excluded));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| strip_fields(item, excluded))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn normalize_strings(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}

fn validate_profile_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(PulseError::validation(
            "run_profile_invalid",
            "profile_id must be filesystem-safe and 1..128 bytes",
        ));
    }
    Ok(())
}

fn validate_executable(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > RUNNER_PROFILE_MAX_EXECUTABLE_BYTES || value.contains('\0')
    {
        return Err(PulseError::validation(
            "run_profile_invalid",
            "executable must be non-empty, bounded, and contain no NUL",
        ));
    }
    if value.chars().any(char::is_whitespace) {
        return Err(PulseError::validation(
            "run_profile_invalid",
            "executable must be a single program path or bare name, not a command blob",
        ));
    }
    if value.contains('\\') {
        return Err(PulseError::validation(
            "run_profile_invalid",
            "runner executable paths must use platform path separators and must not contain backslash",
        ));
    }
    if value.contains('/') && !value.starts_with('/') {
        return Err(PulseError::validation(
            "run_profile_invalid",
            "relative executable paths with separators are deferred in Slice 3",
        ));
    }
    Ok(())
}

fn looks_like_tracked_secret(name: &str, value: &str) -> bool {
    let upper_name = name.to_ascii_uppercase();
    if upper_name.contains("SECRET")
        || upper_name.contains("TOKEN")
        || upper_name.contains("PASSWORD")
        || upper_name.contains("PRIVATE_KEY")
        || upper_name.ends_with("_KEY")
    {
        return true;
    }
    let upper_value = value.to_ascii_uppercase();
    upper_value.contains("-----BEGIN ")
        || upper_value.contains("SECRET=")
        || upper_value.contains("TOKEN=")
        || upper_value.contains("PASSWORD=")
}

fn validate_env_name(value: &str) -> Result<()> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(PulseError::validation(
            "run_profile_invalid",
            "empty environment name",
        ));
    };
    if !(first.is_ascii_uppercase() || first == '_')
        || !chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(PulseError::validation(
            "run_profile_invalid",
            "environment names must match [A-Z_][A-Z0-9_]*",
        ));
    }
    Ok(())
}

fn validate_range(value: u64, min: u64, max: u64, field: &str) -> Result<()> {
    if !(min..=max).contains(&value) {
        return Err(PulseError::validation(
            "run_profile_invalid",
            format!("{field} is outside {min}..={max}"),
        ));
    }
    Ok(())
}
