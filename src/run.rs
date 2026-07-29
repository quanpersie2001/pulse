//! Narrow Slice 3 runner feasibility contracts.
//!
//! This module intentionally does **not** implement public `pulse run` behavior.
//! It records the prerequisite feasibility decisions for P2S3-I0 in executable
//! value contracts that later implementation slices can consume.

use crate::canonical_json::{hash_bytes, hash_serializable};
use crate::{PulseError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub const RUNNER_PROFILE_SCHEMA_VERSION: u64 = 1;
pub const PUBLIC_CODEX_ADAPTER: &str = "codex_process_v1";
pub const NATIVE_RESUME_STATUS: &str = "not_installed";
pub const DEFAULT_LOG_REDACTION_STATUS: &str = "not_applied_runtime_private";
pub const RUN_INPUT_CONFIDENTIALITY: &str = "runtime_private_repository_sensitive";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunnerProfileRegistryV1 {
    pub schema_version: u64,
    pub default_profile: String,
    pub profiles: Vec<RunnerProfileV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunnerProfileV1 {
    pub profile_id: String,
    pub adapter: String,
    pub executable: String,
    #[serde(default)]
    pub fixed_args: Vec<String>,
    #[serde(default)]
    pub environment_allow: Vec<String>,
    #[serde(default)]
    pub environment_set: serde_json::Map<String, serde_json::Value>,
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
    pub schema_version: u64,
    pub public_adapter: String,
    pub executable_resolution: String,
    pub shell_invocation: String,
    pub inherited_environment_values_recorded: bool,
    pub environment_fingerprint_semantics: String,
    pub raw_prompt_storage: String,
    pub raw_log_storage: String,
    pub default_log_redaction_status: String,
    pub native_resume_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResolvedExecutableIdentityV1 {
    pub executable: String,
    pub resolved_path: String,
    pub metadata_identity: String,
}

impl RunnerProfileRegistryV1 {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != RUNNER_PROFILE_SCHEMA_VERSION {
            return Err(PulseError::validation(
                "run_profile_invalid",
                "runner profile registry schema_version must be 1",
            ));
        }
        if self.profiles.is_empty() || self.profiles.len() > 32 {
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
    pub fn validate_public(&self) -> Result<()> {
        validate_profile_id(&self.profile_id)?;
        if self.adapter != PUBLIC_CODEX_ADAPTER {
            return Err(PulseError::validation(
                "run_profile_invalid",
                "public runner profiles may only use codex_process_v1",
            ));
        }
        validate_executable(&self.executable)?;
        if self.fixed_args.len() > 64 || self.fixed_args.iter().any(|arg| arg.len() > 4096) {
            return Err(PulseError::validation(
                "run_profile_invalid",
                "fixed_args exceeds Slice 3 bounds",
            ));
        }
        for name in &self.environment_allow {
            validate_env_name(name)?;
        }
        for (name, value) in &self.environment_set {
            validate_env_name(name)?;
            if !value.is_string() {
                return Err(PulseError::validation(
                    "run_profile_invalid",
                    "environment_set values must be literal strings",
                ));
            }
        }
        validate_range(self.start_timeout_seconds, 1, 300, "start_timeout_seconds")?;
        validate_range(self.run_timeout_seconds, 60, 86_400, "run_timeout_seconds")?;
        validate_range(self.cancel_grace_seconds, 1, 300, "cancel_grace_seconds")?;
        validate_range(
            self.force_kill_after_seconds,
            0,
            300,
            "force_kill_after_seconds",
        )?;
        validate_range(
            self.max_stdout_bytes,
            65_536,
            67_108_864,
            "max_stdout_bytes",
        )?;
        validate_range(
            self.max_stderr_bytes,
            65_536,
            67_108_864,
            "max_stderr_bytes",
        )?;
        Ok(())
    }

    pub fn fingerprint(&self) -> Result<String> {
        self.validate_public()?;
        hash_serializable(self)
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
        let mut literal_non_secret = self
            .environment_set
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        literal_non_secret.sort_unstable();
        hash_serializable(&EnvSpec {
            inherited,
            literal_non_secret,
        })
    }
}

pub fn runner_profile_threat_model() -> RunnerProfileThreatModelV1 {
    RunnerProfileThreatModelV1 {
        schema_version: 1,
        public_adapter: PUBLIC_CODEX_ADAPTER.to_string(),
        executable_resolution:
            "absolute_normalized_regular_file_or_bare_path_lookup_no_repository_relative_paths"
                .to_string(),
        shell_invocation: "never".to_string(),
        inherited_environment_values_recorded: false,
        environment_fingerprint_semantics: "names_and_source_classes_only_no_inherited_values"
            .to_string(),
        raw_prompt_storage: RUN_INPUT_CONFIDENTIALITY.to_string(),
        raw_log_storage: "runtime_private_gitignored_bounded_prefix_tail".to_string(),
        default_log_redaction_status: DEFAULT_LOG_REDACTION_STATUS.to_string(),
        native_resume_status: NATIVE_RESUME_STATUS.to_string(),
    }
}

pub fn resolve_executable_identity(executable: &str) -> Result<ResolvedExecutableIdentityV1> {
    validate_executable(executable)?;
    let path = Path::new(executable);
    let resolved = if path.is_absolute() {
        let canonical = fs::canonicalize(path).map_err(|error| PulseError::io(path, error))?;
        let metadata =
            fs::metadata(&canonical).map_err(|error| PulseError::io(&canonical, error))?;
        if !metadata.is_file() {
            return Err(PulseError::validation(
                "run_command_not_found",
                "configured executable is not a regular file",
            ));
        }
        canonical
    } else {
        resolve_bare_executable(executable)?
    };
    let metadata = fs::metadata(&resolved).map_err(|error| PulseError::io(&resolved, error))?;
    let metadata_identity = hash_bytes(
        format!(
            "path={}\nlen={}\nreadonly={}\n",
            resolved.display(),
            metadata.len(),
            metadata.permissions().readonly()
        )
        .as_bytes(),
    );
    Ok(ResolvedExecutableIdentityV1 {
        executable: executable.to_string(),
        resolved_path: resolved.to_string_lossy().to_string(),
        metadata_identity,
    })
}

fn resolve_bare_executable(executable: &str) -> Result<PathBuf> {
    let path_var = env::var_os("PATH").ok_or_else(|| {
        PulseError::validation(
            "run_command_not_found",
            "PATH is unavailable for executable lookup",
        )
    })?;
    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(executable);
        if let Ok(metadata) = fs::metadata(&candidate) {
            if metadata.is_file() {
                return fs::canonicalize(&candidate)
                    .map_err(|error| PulseError::io(candidate, error));
            }
        }
    }
    Err(PulseError::validation(
        "run_command_not_found",
        "configured executable was not found on PATH",
    ))
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
    if value.is_empty() || value.len() > 4096 || value.contains('\0') {
        return Err(PulseError::validation(
            "run_profile_invalid",
            "executable must be non-empty, bounded, and contain no NUL",
        ));
    }
    let path = Path::new(value);
    if path.is_relative() && path.components().count() != 1 {
        return Err(PulseError::validation(
            "run_profile_invalid",
            "relative executable paths with separators are deferred in Slice 3",
        ));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_profile() -> RunnerProfileV1 {
        RunnerProfileV1 {
            profile_id: "codex-local".to_string(),
            adapter: PUBLIC_CODEX_ADAPTER.to_string(),
            executable: "codex".to_string(),
            fixed_args: vec!["exec".to_string(), "--json".to_string()],
            environment_allow: vec![
                "PATH".to_string(),
                "HOME".to_string(),
                "CODEX_HOME".to_string(),
            ],
            environment_set: serde_json::Map::new(),
            start_timeout_seconds: 30,
            run_timeout_seconds: 7200,
            cancel_grace_seconds: 10,
            force_kill_after_seconds: 10,
            max_stdout_bytes: 16_777_216,
            max_stderr_bytes: 16_777_216,
        }
    }

    #[test]
    fn profile_validation_rejects_shell_and_test_adapter_shapes() {
        let mut profile = valid_profile();
        profile.adapter = "fixture_process_v1".to_string();
        assert_eq!(
            profile.validate_public().unwrap_err().code(),
            "run_profile_invalid"
        );

        let mut profile = valid_profile();
        profile.executable = "tools/codex".to_string();
        assert_eq!(
            profile.validate_public().unwrap_err().code(),
            "run_profile_invalid"
        );
    }

    #[test]
    fn environment_fingerprint_excludes_values() {
        let mut profile = valid_profile();
        profile.environment_set.insert(
            "TOKEN".to_string(),
            serde_json::Value::String("secret-one".to_string()),
        );
        let first = profile.environment_spec_fingerprint().unwrap();
        profile.environment_set.insert(
            "TOKEN".to_string(),
            serde_json::Value::String("secret-two".to_string()),
        );
        let second = profile.environment_spec_fingerprint().unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn threat_model_records_input_and_log_confidentiality() {
        let model = runner_profile_threat_model();
        assert_eq!(model.shell_invocation, "never");
        assert!(!model.inherited_environment_values_recorded);
        assert_eq!(model.raw_prompt_storage, RUN_INPUT_CONFIDENTIALITY);
        assert_eq!(
            model.default_log_redaction_status,
            DEFAULT_LOG_REDACTION_STATUS
        );
    }
}
