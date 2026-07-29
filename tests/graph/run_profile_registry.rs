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
use std::ffi::OsString;
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
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
        env::set_var("PATH", path_with_original(entries, &self.original_path));
    }
}

impl Drop for PathEnvGuard {
    fn drop(&mut self) {
        restore_path(self.original_path.clone());
    }
}

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
fn canonical_utf8(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap()
        .to_str()
        .expect("test temp paths must canonicalize to UTF-8")
        .to_string()
}

#[cfg(unix)]
fn canonical_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap()
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
    let mut registry = registry(canonical_utf8(&executable));
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
        canonical_utf8(&executable)
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
        select_profile_from_registry(&registry(canonical_utf8(&executable)), None).unwrap_err();
    assert_eq!(error.code(), "run_profile_invalid");
}

#[test]
#[cfg(unix)]
fn executable_resolution_rejects_absolute_directory_and_symlink() {
    let bin_dir = tempfile::tempdir().unwrap();
    let directory = bin_dir.path().join("codex-dir");
    fs::create_dir(&directory).unwrap();
    let error =
        select_profile_from_registry(&registry(canonical_utf8(&directory)), None).unwrap_err();
    assert_eq!(error.code(), "run_profile_invalid");

    let real = executable_file(bin_dir.path(), "codex-real");
    let symlink = bin_dir.path().join("codex-link");
    std::os::unix::fs::symlink(&real, &symlink).unwrap();
    let error =
        select_profile_from_registry(&registry(symlink.to_str().unwrap()), None).unwrap_err();
    assert_eq!(error.code(), "run_profile_invalid");
}

#[test]
#[cfg(unix)]
fn bare_name_resolution_skips_unsafe_path_candidates_and_keeps_order() {
    let path_guard = PathEnvGuard::new();
    let unsafe_first = tempfile::tempdir().unwrap();
    let unsafe_second = tempfile::tempdir().unwrap();
    let safe_third = tempfile::tempdir().unwrap();
    non_executable_file(unsafe_first.path(), "codex-test");
    fs::create_dir(unsafe_second.path().join("codex-test")).unwrap();
    let safe_exe = executable_file(safe_third.path(), "codex-test");

    let unsafe_first_path = canonical_path(unsafe_first.path());
    let unsafe_second_path = canonical_path(unsafe_second.path());
    let safe_third_path = canonical_path(safe_third.path());
    path_guard.set_path([
        unsafe_first_path.as_path(),
        unsafe_second_path.as_path(),
        safe_third_path.as_path(),
    ]);
    let selected = select_profile_from_registry(&registry("codex-test"), None).unwrap();
    assert_eq!(selected.executable.resolved_path, canonical_utf8(&safe_exe));
}

#[test]
#[cfg(unix)]
fn bare_name_resolution_skips_symlink_to_executable() {
    let path_guard = PathEnvGuard::new();
    let symlink_dir = tempfile::tempdir().unwrap();
    let safe_dir = tempfile::tempdir().unwrap();
    let real = executable_file(safe_dir.path(), "codex-real");
    std::os::unix::fs::symlink(&real, symlink_dir.path().join("codex-test")).unwrap();
    let safe_exe = executable_file(safe_dir.path(), "codex-test");

    let symlink_dir_path = canonical_path(symlink_dir.path());
    let safe_dir_path = canonical_path(safe_dir.path());
    path_guard.set_path([symlink_dir_path.as_path(), safe_dir_path.as_path()]);
    let selected = select_profile_from_registry(&registry("codex-test"), None).unwrap();
    assert_eq!(selected.executable.resolved_path, canonical_utf8(&safe_exe));
}

#[test]
#[cfg(unix)]
fn bare_name_resolution_reports_not_found_when_only_unsafe_candidates_exist() {
    let path_guard = PathEnvGuard::new();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    non_executable_file(first.path(), "codex-test");
    fs::create_dir(second.path().join("codex-test")).unwrap();

    let first_path = canonical_path(first.path());
    let second_path = canonical_path(second.path());
    path_guard.set_path([first_path.as_path(), second_path.as_path()]);
    let error = select_profile_from_registry(&registry("codex-test"), None).unwrap_err();
    assert_eq!(error.code(), "run_command_not_found");
}

#[test]
#[cfg(unix)]
fn bare_name_resolution_uses_first_safe_executable_in_path_order() {
    let path_guard = PathEnvGuard::new();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let first_exe = executable_file(first.path(), "codex-test");
    let second_exe = executable_file(second.path(), "codex-test");
    let first_path = canonical_path(first.path());
    let second_path = canonical_path(second.path());
    path_guard.set_path([first_path.as_path(), second_path.as_path()]);
    let selected = select_profile_from_registry(&registry("codex-test"), None).unwrap();
    assert_eq!(
        selected.executable.resolved_path,
        canonical_utf8(&first_exe)
    );
    assert_ne!(
        selected.executable.resolved_path,
        canonical_utf8(&second_exe)
    );
}

fn path_with_original<'a>(
    entries: impl IntoIterator<Item = &'a Path>,
    old_path: &Option<OsString>,
) -> OsString {
    let mut paths = entries.into_iter().map(PathBuf::from).collect::<Vec<_>>();
    if let Some(old_path) = old_path {
        paths.extend(env::split_paths(old_path));
    }
    env::join_paths(paths).unwrap()
}

fn restore_path(old_path: Option<OsString>) {
    if let Some(old_path) = old_path {
        env::set_var("PATH", old_path);
    } else {
        env::remove_var("PATH");
    }
}

#[test]
fn executable_backslash_is_rejected_by_schema_and_model_even_when_absolute_like() {
    let mut value = serde_json::to_value(registry("/tmp\\codex")).unwrap();
    assert!(JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .compile(&serde_json::from_str(RUNNER_PROFILES_SCHEMA).unwrap())
        .unwrap()
        .validate(&value)
        .is_err());
    assert!(
        serde_json::from_value::<RunnerProfileRegistryV1>(value.clone())
            .unwrap()
            .validate()
            .is_err()
    );

    value["profiles"][0]["executable"] = json!("C:\\codex");
    assert!(serde_json::from_value::<RunnerProfileRegistryV1>(value)
        .unwrap()
        .validate()
        .is_err());
}

#[test]
#[cfg(unix)]
fn absolute_executable_rejects_symlink_directory_component() {
    let base = tempfile::tempdir().unwrap();
    let real_dir = base.path().join("real-bin");
    fs::create_dir(&real_dir).unwrap();
    let executable = executable_file(&real_dir, "codex-test");
    let link_dir = base.path().join("link-bin");
    std::os::unix::fs::symlink(&real_dir, &link_dir).unwrap();
    let via_link = link_dir.join("codex-test");

    assert!(select_profile_from_registry(&registry(canonical_utf8(&executable)), None).is_ok());
    let error =
        select_profile_from_registry(&registry(via_link.to_str().unwrap()), None).unwrap_err();
    assert_eq!(error.code(), "run_profile_invalid");
}

#[test]
#[cfg(unix)]
fn path_resolution_rejects_symlink_path_directory_component() {
    let path_guard = PathEnvGuard::new();
    let base = tempfile::tempdir().unwrap();
    let real_dir = base.path().join("real-bin");
    fs::create_dir(&real_dir).unwrap();
    executable_file(&real_dir, "codex-test");
    let link_dir = base.path().join("link-bin");
    std::os::unix::fs::symlink(&real_dir, &link_dir).unwrap();
    let safe_dir = tempfile::tempdir().unwrap();
    let safe_exe = executable_file(safe_dir.path(), "codex-test");

    let safe_dir_path = canonical_path(safe_dir.path());
    path_guard.set_path([link_dir.as_path(), safe_dir_path.as_path()]);
    let selected = select_profile_from_registry(&registry("codex-test"), None).unwrap();
    assert_eq!(selected.executable.resolved_path, canonical_utf8(&safe_exe));
}

#[test]
#[cfg(unix)]
fn path_resolution_rejects_non_utf8_path_entries_and_candidate_identity() {
    let path_guard = PathEnvGuard::new();
    let base = tempfile::tempdir().unwrap();
    let non_utf8_dir = base
        .path()
        .join(std::ffi::OsString::from_vec(b"bin-\xFF".to_vec()));
    if fs::create_dir(&non_utf8_dir).is_ok() {
        let _ = executable_file(&non_utf8_dir, "codex-test");
    }
    let safe_dir = tempfile::tempdir().unwrap();
    let safe_exe = executable_file(safe_dir.path(), "codex-test");

    let safe_dir_path = canonical_path(safe_dir.path());
    path_guard.set_path([non_utf8_dir.as_path(), safe_dir_path.as_path()]);
    let selected = select_profile_from_registry(&registry("codex-test"), None).unwrap();
    assert_eq!(selected.executable.resolved_path, canonical_utf8(&safe_exe));

    let lossy_configured_executable = base.path().join("bin-�").join("codex-test");
    let error = select_profile_from_registry(
        &registry(lossy_configured_executable.to_string_lossy()),
        None,
    )
    .unwrap_err();
    assert_eq!(error.code(), "run_profile_invalid");
}

#[test]
#[cfg(unix)]
fn effective_permission_rejects_owner_without_owner_execute_even_when_other_execute_is_set() {
    let bin_dir = tempfile::tempdir().unwrap();
    let executable = file_with_mode(bin_dir.path(), "codex-test", 0o001);
    if executable.metadata().unwrap().uid() == unsafe { geteuid() } {
        let error =
            select_profile_from_registry(&registry(canonical_utf8(&executable)), None).unwrap_err();
        assert_eq!(error.code(), "run_profile_invalid");
    }
}

#[test]
#[cfg(unix)]
fn effective_permission_accepts_owner_execute_and_profile_fingerprint_ignores_local_identity() {
    let path_guard = PathEnvGuard::new();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let _first_exe = executable_file(first.path(), "codex-test");
    let _second_exe = executable_file(second.path(), "codex-test");
    let first_registry = registry("codex-test");
    let second_registry = registry("codex-test");

    assert_eq!(
        first_registry.profiles[0].fingerprint().unwrap(),
        second_registry.profiles[0].fingerprint().unwrap()
    );
    let first_path = canonical_path(first.path());
    path_guard.set_path([first_path.as_path()]);
    let first_selected = select_profile_from_registry(&first_registry, None).unwrap();
    let second_path = canonical_path(second.path());
    path_guard.set_path([second_path.as_path()]);
    let second_selected = select_profile_from_registry(&second_registry, None).unwrap();
    assert_ne!(
        first_selected.executable.identity,
        second_selected.executable.identity
    );
}

#[cfg(unix)]
extern "C" {
    fn geteuid() -> u32;
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
