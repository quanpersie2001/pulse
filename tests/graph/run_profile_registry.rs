use jsonschema::JSONSchema;
use pulse::canonical_json::to_canonical_bytes;
use pulse::kernel::run_store::{
    load_runner_profile_registry_preserve, select_profile_from_registry,
    select_runner_profile_preserve, RUNNER_PROFILE_REGISTRY_RELATIVE_PATH,
};
use pulse::run::{
    RunnerAdapterV1, RunnerProfileRegistryV1, RunnerProfileV1, RUNNER_PROFILES_SCHEMA,
    RUNNER_PROFILE_SCHEMA_VERSION,
};
use serde_json::{json, Map, Value};
use std::env;
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tempfile::TempDir;

static PATH_ENV_LOCK: Mutex<()> = Mutex::new(());

fn schema_validate(value: &Value) {
    let schema_json: Value = serde_json::from_str(RUNNER_PROFILES_SCHEMA).unwrap();
    let compiled = JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .compile(&schema_json)
        .unwrap();
    if !compiled.is_valid(value) {
        let errors = compiled
            .validate(value)
            .unwrap_err()
            .map(|error| error.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        panic!("runner profile schema validation failed: {errors}");
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

fn registry(executable: impl Into<String>) -> RunnerProfileRegistryV1 {
    RunnerProfileRegistryV1 {
        schema_version: RUNNER_PROFILE_SCHEMA_VERSION,
        default_profile: "codex-local".to_string(),
        profiles: vec![RunnerProfileV1 {
            profile_id: "codex-local".to_string(),
            adapter: RunnerAdapterV1::CodexProcessV1,
            executable: executable.into(),
            fixed_args: vec!["exec".to_string(), "--json".to_string()],
            environment_allow: vec![
                "PATH".to_string(),
                "HOME".to_string(),
                "CODEX_HOME".to_string(),
            ],
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
    fs::write(
        repo.join(RUNNER_PROFILE_REGISTRY_RELATIVE_PATH),
        to_canonical_bytes(registry).unwrap(),
    )
    .unwrap();
}

fn file_with_mode(dir: &Path, name: &str, mode: u32) -> PathBuf {
    let path = dir.join(name);
    let mut file = fs::File::create(&path).unwrap();
    writeln!(file, "#!/bin/sh").unwrap();
    #[cfg(unix)]
    fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
    let _ = mode;
    path
}

fn executable_file(dir: &Path, name: &str) -> PathBuf {
    file_with_mode(dir, name, 0o755)
}

#[cfg(unix)]
fn non_executable_file(dir: &Path, name: &str) -> PathBuf {
    file_with_mode(dir, name, 0o644)
}

#[test]
fn preserve_load_rejects_non_enrolled_and_missing_without_runtime_bootstrap() {
    let non_enrolled = tempfile::tempdir().unwrap();
    let err = load_runner_profile_registry_preserve(non_enrolled.path()).unwrap_err();
    assert_eq!(err.code(), "not_enrolled");
    assert!(!non_enrolled.path().join(".pulse").exists());

    let enrolled = enrolled_repo();
    let err = load_runner_profile_registry_preserve(enrolled.path()).unwrap_err();
    assert_eq!(err.code(), "run_profile_missing");
    assert!(!enrolled.path().join(".pulse/runtime").exists());
}

#[test]
fn production_json_accepts_only_codex_process_and_rejects_fixture_adapter() {
    let mut value = serde_json::to_value(registry("codex")).unwrap();
    schema_validate(&value);
    let decoded: RunnerProfileRegistryV1 = serde_json::from_value(value.clone()).unwrap();
    decoded.validate().unwrap();

    value["profiles"][0]["adapter"] = json!("fixture_process_v1");
    assert!(JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .compile(&serde_json::from_str(RUNNER_PROFILES_SCHEMA).unwrap())
        .unwrap()
        .validate(&value)
        .is_err());
    assert!(serde_json::from_value::<RunnerProfileRegistryV1>(value).is_err());
}

#[test]
fn path_env_timeout_log_and_secret_bounds_fail_closed() {
    let cases = [
        ("executable", json!("tools/codex")),
        ("start_timeout_seconds", json!(0)),
        ("run_timeout_seconds", json!(59)),
        ("cancel_grace_seconds", json!(301)),
        ("force_kill_after_seconds", json!(301)),
        ("max_stdout_bytes", json!(65_535)),
        ("max_stderr_bytes", json!(67_108_865)),
    ];
    for (field, bad) in cases {
        let mut value = serde_json::to_value(registry("codex")).unwrap();
        value["profiles"][0][field] = bad;
        let model_rejects = match serde_json::from_value::<RunnerProfileRegistryV1>(value.clone()) {
            Ok(registry) => registry.validate().is_err(),
            Err(_) => true,
        };
        assert!(model_rejects, "serde/model should reject {field}");
        assert!(
            JSONSchema::options()
                .with_draft(jsonschema::Draft::Draft202012)
                .compile(&serde_json::from_str(RUNNER_PROFILES_SCHEMA).unwrap())
                .unwrap()
                .validate(&value)
                .is_err(),
            "schema should reject {field}"
        );
    }

    let mut value = serde_json::to_value(registry("codex")).unwrap();
    value["profiles"][0]["environment_allow"] = json!(["lowercase"]);
    assert!(
        serde_json::from_value::<RunnerProfileRegistryV1>(value.clone())
            .unwrap()
            .validate()
            .is_err()
    );
    assert!(JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .compile(&serde_json::from_str(RUNNER_PROFILES_SCHEMA).unwrap())
        .unwrap()
        .validate(&value)
        .is_err());

    let mut registry = registry("codex");
    registry.profiles[0]
        .environment_set
        .insert("API_TOKEN".to_string(), json!("literal-secret"));
    assert_eq!(
        registry.validate().unwrap_err().code(),
        "run_profile_invalid"
    );
}

#[test]
fn selected_profile_reports_no_environment_values_and_keeps_profile_fingerprint_local_free() {
    let repo = enrolled_repo();
    let bin_dir = tempfile::tempdir().unwrap();
    let executable = executable_file(bin_dir.path(), "codex-test");
    let mut registry = registry(executable.to_string_lossy().to_string());
    registry.profiles[0]
        .environment_set
        .insert("PULSE_LITERAL".to_string(), json!("non-secret-literal"));
    let original_fingerprint = registry.profiles[0].fingerprint().unwrap();
    write_registry(repo.path(), &registry);

    let selected = select_runner_profile_preserve(repo.path(), None).unwrap();
    assert_eq!(selected.profile_fingerprint, original_fingerprint);
    assert!(selected.executable.identity.starts_with("sha256:"));
    assert_eq!(
        selected.executable.resolved_path,
        fs::canonicalize(&executable).unwrap().to_string_lossy()
    );
    let rendered = serde_json::to_string(&selected).unwrap();
    assert!(!rendered.contains("non-secret-literal"));
    assert!(rendered.contains("PULSE_LITERAL"));
    assert!(rendered.contains("literal_non_secret"));
}

#[test]
#[cfg(unix)]
fn executable_resolution_rejects_absolute_non_executable_file() {
    let bin_dir = tempfile::tempdir().unwrap();
    let executable = non_executable_file(bin_dir.path(), "codex-test");
    let error =
        select_profile_from_registry(&registry(executable.to_string_lossy()), None).unwrap_err();
    assert_eq!(error.code(), "run_profile_invalid");
}

#[test]
#[cfg(unix)]
fn executable_resolution_rejects_absolute_directory_and_symlink() {
    let bin_dir = tempfile::tempdir().unwrap();
    let directory = bin_dir.path().join("codex-dir");
    fs::create_dir(&directory).unwrap();
    let error =
        select_profile_from_registry(&registry(directory.to_string_lossy()), None).unwrap_err();
    assert_eq!(error.code(), "run_profile_invalid");

    let real = executable_file(bin_dir.path(), "codex-real");
    let symlink = bin_dir.path().join("codex-link");
    std::os::unix::fs::symlink(&real, &symlink).unwrap();
    let error =
        select_profile_from_registry(&registry(symlink.to_string_lossy()), None).unwrap_err();
    assert_eq!(error.code(), "run_profile_invalid");
}

#[test]
#[cfg(unix)]
fn bare_name_resolution_skips_unsafe_path_candidates_and_keeps_order() {
    let _guard = PATH_ENV_LOCK.lock().unwrap();
    let unsafe_first = tempfile::tempdir().unwrap();
    let unsafe_second = tempfile::tempdir().unwrap();
    let safe_third = tempfile::tempdir().unwrap();
    non_executable_file(unsafe_first.path(), "codex-test");
    fs::create_dir(unsafe_second.path().join("codex-test")).unwrap();
    let safe_exe = executable_file(safe_third.path(), "codex-test");

    let old_path = env::var_os("PATH");
    let joined =
        env::join_paths([unsafe_first.path(), unsafe_second.path(), safe_third.path()]).unwrap();
    env::set_var("PATH", joined);
    let selected = select_profile_from_registry(&registry("codex-test"), None).unwrap();
    restore_path(old_path);
    assert_eq!(
        selected.executable.resolved_path,
        fs::canonicalize(safe_exe).unwrap().to_string_lossy()
    );
}

#[test]
#[cfg(unix)]
fn bare_name_resolution_skips_symlink_to_executable() {
    let _guard = PATH_ENV_LOCK.lock().unwrap();
    let symlink_dir = tempfile::tempdir().unwrap();
    let safe_dir = tempfile::tempdir().unwrap();
    let real = executable_file(safe_dir.path(), "codex-real");
    std::os::unix::fs::symlink(&real, symlink_dir.path().join("codex-test")).unwrap();
    let safe_exe = executable_file(safe_dir.path(), "codex-test");

    let old_path = env::var_os("PATH");
    let joined = env::join_paths([symlink_dir.path(), safe_dir.path()]).unwrap();
    env::set_var("PATH", joined);
    let selected = select_profile_from_registry(&registry("codex-test"), None).unwrap();
    restore_path(old_path);
    assert_eq!(
        selected.executable.resolved_path,
        fs::canonicalize(safe_exe).unwrap().to_string_lossy()
    );
}

#[test]
#[cfg(unix)]
fn bare_name_resolution_reports_not_found_when_only_unsafe_candidates_exist() {
    let _guard = PATH_ENV_LOCK.lock().unwrap();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    non_executable_file(first.path(), "codex-test");
    fs::create_dir(second.path().join("codex-test")).unwrap();

    let old_path = env::var_os("PATH");
    let joined = env::join_paths([first.path(), second.path()]).unwrap();
    env::set_var("PATH", joined);
    let error = select_profile_from_registry(&registry("codex-test"), None).unwrap_err();
    restore_path(old_path);
    assert_eq!(error.code(), "run_command_not_found");
}

#[test]
#[cfg(unix)]
fn bare_name_resolution_uses_first_safe_executable_in_path_order() {
    let _guard = PATH_ENV_LOCK.lock().unwrap();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let first_exe = executable_file(first.path(), "codex-test");
    let second_exe = executable_file(second.path(), "codex-test");
    let old_path = env::var_os("PATH");
    let joined = env::join_paths([first.path(), second.path()]).unwrap();
    env::set_var("PATH", joined);
    let selected = select_profile_from_registry(&registry("codex-test"), None).unwrap();
    restore_path(old_path);
    assert_eq!(
        selected.executable.resolved_path,
        fs::canonicalize(first_exe).unwrap().to_string_lossy()
    );
    assert_ne!(
        selected.executable.resolved_path,
        fs::canonicalize(second_exe).unwrap().to_string_lossy()
    );
}

fn restore_path(old_path: Option<std::ffi::OsString>) {
    if let Some(old_path) = old_path {
        env::set_var("PATH", old_path);
    } else {
        env::remove_var("PATH");
    }
}

#[test]
fn fixed_args_order_and_duplicates_remain_profile_semantics() {
    let mut base = registry("codex").profiles.remove(0);
    let mut reordered = base.clone();
    reordered.fixed_args = vec!["--json".to_string(), "exec".to_string()];
    let mut duplicated = base.clone();
    duplicated.fixed_args = vec![
        "exec".to_string(),
        "--json".to_string(),
        "--json".to_string(),
    ];
    assert_ne!(
        base.fingerprint().unwrap(),
        reordered.fingerprint().unwrap()
    );
    assert_ne!(
        base.fingerprint().unwrap(),
        duplicated.fingerprint().unwrap()
    );

    base.environment_allow = vec!["PATH".to_string(), "HOME".to_string()];
    reordered.environment_allow = vec!["HOME".to_string(), "PATH".to_string()];
    assert_eq!(
        base.environment_spec_fingerprint().unwrap(),
        reordered.environment_spec_fingerprint().unwrap()
    );
}
