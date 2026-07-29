//! Preserve-only runner profile registry loading for P2S3-I2.
//!
//! This module owns tracked runner-profile config IO and local executable
//! resolution. It deliberately does not own runtime run/attempt records,
//! process launch, Git/workspace inspection, authority, or graph semantics.

use crate::canonical_json::hash_serializable;
use crate::kernel::assignment_store;
use crate::run::{
    RunnerAdapterV1, RunnerEnvironmentSourceV1, RunnerEnvironmentSpecEntryV1,
    RunnerExecutableIdentityV1, RunnerProfileRegistryV1, RunnerProfileSelectionV1,
};
use crate::{PulseError, PulseResult};
use serde::Serialize;
use std::env;
use std::fs;
use std::path::{Component, Path};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

pub const RUNNER_PROFILE_REGISTRY_RELATIVE_PATH: &str = ".pulse/run/runner-profiles.json";

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
    })
}

fn resolve_executable(executable: &str) -> PulseResult<RunnerExecutableIdentityV1> {
    if Path::new(executable).is_absolute() {
        return resolve_absolute_executable(Path::new(executable));
    }
    if has_path_separator(executable) || executable.chars().any(char::is_whitespace) {
        return Err(PulseError::validation(
            "run_profile_invalid",
            "executable must be an absolute path or a bare program name",
        ));
    }
    let path = env::var_os("PATH").ok_or_else(|| {
        PulseError::validation(
            "run_command_not_found",
            "PATH is not available for runner executable resolution",
        )
    })?;
    for dir in env::split_paths(&path) {
        if !dir.is_absolute() {
            continue;
        }
        let candidate = dir.join(executable);
        if candidate.exists() {
            return resolve_absolute_executable(&candidate);
        }
    }
    Err(PulseError::validation(
        "run_command_not_found",
        format!("runner executable {executable} was not found on inherited PATH"),
    ))
}

fn resolve_absolute_executable(path: &Path) -> PulseResult<RunnerExecutableIdentityV1> {
    let canonical = fs::canonicalize(path).map_err(|error| PulseError::io(path, error))?;
    if !canonical.is_absolute()
        || canonical
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(PulseError::validation(
            "run_profile_invalid",
            "resolved executable path is not normalized and absolute",
        ));
    }
    let metadata = fs::metadata(&canonical).map_err(|error| PulseError::io(&canonical, error))?;
    if !metadata.is_file() {
        return Err(PulseError::validation(
            "run_profile_invalid",
            "resolved executable is not a regular file",
        ));
    }
    let identity = executable_identity_hash(&canonical, &metadata)?;
    Ok(RunnerExecutableIdentityV1 {
        resolved_path: canonical.to_string_lossy().to_string(),
        identity,
        identity_status: "best_effort_metadata".to_string(),
    })
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
    let resolved_path = canonical.to_string_lossy();
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

fn has_path_separator(value: &str) -> bool {
    value.contains('/') || value.contains('\\')
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
    use crate::run::{RunnerAdapterV1, RunnerProfileV1, RUNNER_PROFILE_SCHEMA_VERSION};
    use serde_json::Map;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

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
        path
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
        let mut registry = registry(executable.to_string_lossy().to_string());
        registry.profiles[0]
            .environment_set
            .insert("PULSE_LITERAL".to_string(), serde_json::json!("non-secret"));
        write_registry(repo.path(), &registry);

        let selected = select_runner_profile_preserve(repo.path(), None).unwrap();
        assert_eq!(selected.profile_id, "codex-local");
        assert_eq!(
            selected.executable.resolved_path,
            fs::canonicalize(executable).unwrap().to_string_lossy()
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
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let first_exe = executable_file(first.path(), "codex-test");
        let second_exe = executable_file(second.path(), "codex-test");
        let old_path = env::var_os("PATH");
        let joined = env::join_paths([second.path(), first.path()]).unwrap();
        env::set_var("PATH", joined);
        let selected =
            select_profile_from_registry(&registry("codex-test".to_string()), None).unwrap();
        if let Some(old_path) = old_path {
            env::set_var("PATH", old_path);
        } else {
            env::remove_var("PATH");
        }
        assert_eq!(
            selected.executable.resolved_path,
            fs::canonicalize(second_exe).unwrap().to_string_lossy()
        );
        assert_ne!(
            selected.executable.resolved_path,
            fs::canonicalize(first_exe).unwrap().to_string_lossy()
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
