use pulse::canonical_json::{hash_serializable, to_canonical_bytes};
use pulse::process::{
    build_descriptor_for_selection, cancel_verified_process_tree, launch_supervisor,
    run_hidden_supervisor, CancelPolicy, ControlNoncePlaintext, SupervisorCancelRequestV1,
    SupervisorControlDescriptorV1, SupervisorLaunchConfig, HIDDEN_SUPERVISOR_COMMAND,
    HIDDEN_SUPERVISOR_NONCE_ENV, PLATFORM_SUPPORT,
};
use pulse::run::{
    RunnerAdapterV1, RunnerEnvironmentSourceV1, RunnerEnvironmentSpecEntryV1,
    RunnerExecutableIdentityV1, RunnerProfileSelectionV1,
};
use serde_json::{Map, Value};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

fn pulse_bin() -> PathBuf {
    crate::common_bin::resolve_pulse_bin()
}

fn read_all_files(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).unwrap();
            if metadata.is_dir() {
                walk(&path, out);
            } else if metadata.is_file() {
                out.push((path.clone(), fs::read(&path).unwrap()));
            }
        }
    }
    let mut files = Vec::new();
    walk(root, &mut files);
    files
}

fn assert_runtime_does_not_contain(repo: &Path, marker: &str) {
    for (path, bytes) in read_all_files(&repo.join(".pulse/runtime/run")) {
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains(marker),
            "runtime file leaked marker in {}",
            path.display()
        );
    }
}

fn process_exists(pid: u64) -> bool {
    let stat = PathBuf::from(format!("/proc/{pid}/stat"));
    let Ok(text) = fs::read_to_string(stat) else {
        return false;
    };
    let Some(after_name) = text.rsplit_once(") ").map(|(_, rest)| rest) else {
        return true;
    };
    !after_name.starts_with("Z ")
}

static TEST_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

struct EnvGuard {
    old: Vec<(&'static str, Option<std::ffi::OsString>)>,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        Self::set_many(&[(key, value)])
    }

    fn set_many(values: &[(&'static str, &str)]) -> Self {
        let guard = TEST_ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut old = Vec::new();
        for (key, value) in values {
            old.push((*key, std::env::var_os(key)));
            std::env::set_var(key, value);
        }
        Self { old, _guard: guard }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, old) in self.old.iter().rev() {
            if let Some(old) = old {
                std::env::set_var(key, old);
            } else {
                std::env::remove_var(key);
            }
        }
    }
}

fn private_repo() -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".pulse/runtime/run/control")).unwrap();
    fs::create_dir_all(repo.path().join(".pulse/runtime/run/logs/run_TEST")).unwrap();
    repo
}

fn fixture_script(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("fixture.sh");
    fs::write(&path, body).unwrap();
    #[cfg(unix)]
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
fn executable_identity(path: &Path) -> String {
    #[derive(serde::Serialize)]
    struct PortableExecutableIdentity {
        resolved_path: String,
        len: u64,
        readonly: bool,
        modified_unix_seconds: Option<u64>,
        unix_dev: u64,
        unix_ino: u64,
        unix_mode: u32,
    }
    let metadata = fs::symlink_metadata(path).unwrap();
    let modified_unix_seconds = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());
    hash_serializable(&PortableExecutableIdentity {
        resolved_path: path.canonicalize().unwrap().to_str().unwrap().to_string(),
        len: metadata.len(),
        readonly: metadata.permissions().readonly(),
        modified_unix_seconds,
        unix_dev: metadata.dev(),
        unix_ino: metadata.ino(),
        unix_mode: metadata.mode(),
    })
    .unwrap()
}

fn selection(executable: &Path, args: Vec<String>) -> RunnerProfileSelectionV1 {
    let mut literal_environment_values = Map::new();
    literal_environment_values.insert(
        "PULSE_LITERAL_ENV".to_string(),
        Value::String("literal-value".to_string()),
    );
    RunnerProfileSelectionV1 {
        schema_version: 1,
        profile_id: "fixture".to_string(),
        adapter: RunnerAdapterV1::CodexProcessV1,
        profile_fingerprint: "sha256:profile".to_string(),
        environment_spec_fingerprint: "sha256:env".to_string(),
        executable: RunnerExecutableIdentityV1 {
            resolved_path: executable.to_str().unwrap().to_string(),
            identity: executable_identity(executable),
            identity_status: "verified".to_string(),
        },
        fixed_args: args,
        environment: vec![
            RunnerEnvironmentSpecEntryV1 {
                name: "PULSE_SUPERVISOR_TEST_ENV".to_string(),
                source: RunnerEnvironmentSourceV1::Inherited,
            },
            RunnerEnvironmentSpecEntryV1 {
                name: "PULSE_LITERAL_ENV".to_string(),
                source: RunnerEnvironmentSourceV1::LiteralNonSecret,
            },
        ],
        literal_environment_values,
    }
}

fn run_hidden_supervisor_with_env(
    repo: &Path,
    control: &Path,
    nonce: &ControlNoncePlaintext,
) -> pulse::Result<()> {
    run_hidden_supervisor_with_extra_env(repo, control, nonce, &[])
}

fn run_hidden_supervisor_with_extra_env(
    repo: &Path,
    control: &Path,
    nonce: &ControlNoncePlaintext,
    extra: &[(&'static str, &str)],
) -> pulse::Result<()> {
    let nonce_hex = hex::encode(nonce.plaintext_for_spawn_only());
    let literal_hex = hex::encode(b"literal-value");
    let mut env = vec![
        (HIDDEN_SUPERVISOR_NONCE_ENV, nonce_hex.as_str()),
        (
            "PULSE_RUN_SUPERVISOR_LITERAL_ENV_HEX_PULSE_LITERAL_ENV",
            literal_hex.as_str(),
        ),
    ];
    env.extend_from_slice(extra);
    let _env = EnvGuard::set_many(&env);
    run_hidden_supervisor(repo, control)
}

fn descriptor(
    repo: &Path,
    workspace: &Path,
    script: &Path,
    args: Vec<String>,
    run_timeout: u64,
    force_allowed: bool,
) -> (
    ControlNoncePlaintext,
    SupervisorControlDescriptorV1,
    PathBuf,
) {
    let nonce = ControlNoncePlaintext::generate();
    let run_id = "run_TEST";
    let attempt_id = "attempt_TEST";
    let log_root = repo.join(".pulse/runtime/run/logs/run_TEST");
    let control = PathBuf::from(".pulse/runtime/run/control/run_TEST.json");
    let desc = build_descriptor_for_selection(
        repo,
        run_id,
        attempt_id,
        &nonce,
        workspace,
        &selection(script, args),
        &repo.join(".pulse/runtime/run/inputs/run_TEST.attempt_TEST.json"),
        &log_root,
        run_timeout,
        1,
        1,
        force_allowed,
        128,
        128,
    )
    .unwrap();
    (nonce, desc, control)
}

#[test]
fn hidden_supervisor_executes_fixture_and_records_exit() {
    if !cfg!(target_os = "linux") {
        assert_eq!(PLATFORM_SUPPORT, "unsupported_macos_identity_not_proven");
        return;
    }
    let repo = private_repo();
    let workspace = tempfile::tempdir().unwrap();
    let script = fixture_script(
        workspace.path(),
        "#!/bin/sh\npwd > cwd.txt\nprintf '%s' \"$1\" > argv.txt\nprintf '%s' \"$PULSE_SUPERVISOR_TEST_ENV\" > env.txt\nprintf '%s' \"$PULSE_LITERAL_ENV\" > literal-env.txt\nprintf 'hello-stdout'\nprintf 'hello-stderr' >&2\nexit 0\n",
    );
    let (nonce, desc, control) = descriptor(
        repo.path(),
        workspace.path(),
        &script,
        vec!["arg-one".to_string()],
        60,
        true,
    );
    let control_abs = repo.path().join(&control);
    fs::write(&control_abs, to_canonical_bytes(&desc).unwrap()).unwrap();
    run_hidden_supervisor_with_extra_env(
        repo.path(),
        &control,
        &nonce,
        &[("PULSE_SUPERVISOR_TEST_ENV", "allowed-value")],
    )
    .unwrap();

    assert_eq!(
        fs::read_to_string(workspace.path().join("argv.txt")).unwrap(),
        "arg-one"
    );
    assert_eq!(
        fs::read_to_string(workspace.path().join("env.txt")).unwrap(),
        "allowed-value"
    );
    assert_eq!(
        fs::read_to_string(workspace.path().join("literal-env.txt")).unwrap(),
        "literal-value"
    );
    let descriptor_json = fs::read_to_string(&control_abs).unwrap();
    assert!(!descriptor_json.contains("literal-value"));
    assert!(!descriptor_json.contains("private_literal_environment_values"));
    assert_runtime_does_not_contain(repo.path(), "literal-value");
    let exit = fs::read_to_string(
        repo.path()
            .join(".pulse/runtime/run/control/run_TEST.exit.json"),
    )
    .unwrap();
    assert!(exit.contains("\"kind\": \"exited\""));
    assert!(exit.contains("\"identity_status\": \"verified\""));
    assert!(!exit.contains(&hex::encode(nonce.plaintext_for_spawn_only())));
    assert_eq!(
        fs::metadata(
            repo.path()
                .join(".pulse/runtime/run/control/run_TEST.exit.json")
        )
        .unwrap()
        .permissions()
        .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn self_reexec_supervisor_handshake_and_fast_exit_work() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let repo = private_repo();
    let workspace = tempfile::tempdir().unwrap();
    let script = fixture_script(workspace.path(), "#!/bin/sh\nprintf done\nexit 0\n");
    let (nonce, desc, control) =
        descriptor(repo.path(), workspace.path(), &script, vec![], 60, true);
    let report = launch_supervisor(SupervisorLaunchConfig {
        repo_root: repo.path().to_path_buf(),
        control_relative_path: control,
        descriptor: desc,
        nonce: nonce.plaintext_for_spawn_only().to_vec(),
        start_timeout: Duration::from_secs(5),
        pulse_exe: pulse_bin(),
    })
    .unwrap();
    assert_eq!(report.handshake.run_id, "run_TEST");
    assert_eq!(report.handshake.child_identity.identity_status, "verified");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !repo
        .path()
        .join(".pulse/runtime/run/control/run_TEST.exit.json")
        .exists()
    {
        assert!(Instant::now() < deadline, "exit observation not written");
        thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn large_stdout_and_stderr_are_bounded_and_truncated() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let repo = private_repo();
    let workspace = tempfile::tempdir().unwrap();
    let script = fixture_script(
        workspace.path(),
        "#!/bin/sh\ni=0; while [ $i -lt 400 ]; do printf A; printf B >&2; i=$((i+1)); done\n",
    );
    let (nonce, desc, control) =
        descriptor(repo.path(), workspace.path(), &script, vec![], 60, true);
    fs::write(
        repo.path().join(&control),
        to_canonical_bytes(&desc).unwrap(),
    )
    .unwrap();
    run_hidden_supervisor_with_env(repo.path(), &control, &nonce).unwrap();
    let exit = fs::read_to_string(
        repo.path()
            .join(".pulse/runtime/run/control/run_TEST.exit.json"),
    )
    .unwrap();
    assert!(exit.contains("\"total_bytes_seen\": 400"));
    assert!(exit.contains("\"retained_bytes\": 128"));
    assert!(exit.contains("\"truncated_bytes\": 272"));
    assert!(!exit.contains(&hex::encode(nonce.plaintext_for_spawn_only())));
}

#[test]
fn nonzero_signal_timeout_and_no_force_paths_are_observed() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let repo = private_repo();
    let workspace = tempfile::tempdir().unwrap();
    let script = fixture_script(workspace.path(), "#!/bin/sh\nexit 7\n");
    let (nonce, desc, control) =
        descriptor(repo.path(), workspace.path(), &script, vec![], 60, true);
    fs::write(
        repo.path().join(&control),
        to_canonical_bytes(&desc).unwrap(),
    )
    .unwrap();
    run_hidden_supervisor_with_env(repo.path(), &control, &nonce).unwrap();
    let exit = fs::read_to_string(
        repo.path()
            .join(".pulse/runtime/run/control/run_TEST.exit.json"),
    )
    .unwrap();
    assert!(exit.contains("\"code\": 7"));

    let repo = private_repo();
    let workspace = tempfile::tempdir().unwrap();
    let script = fixture_script(workspace.path(), "#!/bin/sh\nkill -TERM $$\n");
    let (nonce, desc, control) =
        descriptor(repo.path(), workspace.path(), &script, vec![], 60, true);
    fs::write(
        repo.path().join(&control),
        to_canonical_bytes(&desc).unwrap(),
    )
    .unwrap();
    run_hidden_supervisor_with_env(repo.path(), &control, &nonce).unwrap();
    let exit = fs::read_to_string(
        repo.path()
            .join(".pulse/runtime/run/control/run_TEST.exit.json"),
    )
    .unwrap();
    assert!(exit.contains("\"signal\": 15"));

    let repo = private_repo();
    let workspace = tempfile::tempdir().unwrap();
    let script = fixture_script(workspace.path(), "#!/bin/sh\ntrap '' TERM\nsleep 5\n");
    let (nonce, desc, control) =
        descriptor(repo.path(), workspace.path(), &script, vec![], 1, true);
    fs::write(
        repo.path().join(&control),
        to_canonical_bytes(&desc).unwrap(),
    )
    .unwrap();
    run_hidden_supervisor_with_env(repo.path(), &control, &nonce).unwrap();
    let exit = fs::read_to_string(
        repo.path()
            .join(".pulse/runtime/run/control/run_TEST.exit.json"),
    )
    .unwrap();
    assert!(exit.contains("\"timed_out\": true"));
}

#[test]
fn nonce_mismatch_and_identity_change_fail_closed() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let repo = private_repo();
    let workspace = tempfile::tempdir().unwrap();
    let script = fixture_script(workspace.path(), "#!/bin/sh\nexit 0\n");
    let (_nonce, desc, control) =
        descriptor(repo.path(), workspace.path(), &script, vec![], 60, true);
    fs::write(
        repo.path().join(&control),
        to_canonical_bytes(&desc).unwrap(),
    )
    .unwrap();
    let wrong = ControlNoncePlaintext::generate();
    let error = run_hidden_supervisor_with_env(repo.path(), &control, &wrong)
        .err()
        .unwrap();
    assert_eq!(error.code(), "run_control_record_invalid");
    assert!(!repo
        .path()
        .join(".pulse/runtime/run/control/run_TEST.exit.json")
        .exists());
    assert_ne!(wrong.record().nonce_hash, desc.nonce_hash);
}

#[test]
fn identity_mismatch_refuses_kill() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let repo = private_repo();
    let workspace = tempfile::tempdir().unwrap();
    let script = fixture_script(workspace.path(), "#!/bin/sh\ntrap '' TERM\nsleep 30\n");
    let (nonce, desc, control) =
        descriptor(repo.path(), workspace.path(), &script, vec![], 60, true);
    let report = launch_supervisor(SupervisorLaunchConfig {
        repo_root: repo.path().to_path_buf(),
        control_relative_path: control,
        descriptor: desc,
        nonce: nonce.plaintext_for_spawn_only().to_vec(),
        start_timeout: Duration::from_secs(5),
        pulse_exe: pulse_bin(),
    })
    .unwrap();
    let mut stale = report.handshake.child_identity.clone();
    stale.platform_start_marker =
        "linux_proc_stat_starttime_process_group:sha256:stale".to_string();
    let error = cancel_verified_process_tree(
        &stale,
        CancelPolicy {
            grace: Duration::from_millis(10),
            force_after: Duration::from_millis(10),
            force_allowed: true,
        },
    )
    .unwrap_err();
    assert_eq!(error.code(), "run_process_identity_mismatch");

    cancel_verified_process_tree(
        &report.handshake.child_identity,
        CancelPolicy {
            grace: Duration::from_millis(10),
            force_after: Duration::from_secs(1),
            force_allowed: true,
        },
    )
    .unwrap();
}

#[test]
fn cancel_request_stops_process_tree() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let repo = private_repo();
    let workspace = tempfile::tempdir().unwrap();
    let marker = workspace.path().join("child-alive");
    let script = fixture_script(
        workspace.path(),
        &format!(
            "#!/bin/sh\n(sh -c 'trap \"rm -f {m}; exit 0\" TERM; touch {m}; while true; do sleep 1; done') &\nwait\n",
            m = marker.display()
        ),
    );
    let (nonce, desc, control) =
        descriptor(repo.path(), workspace.path(), &script, vec![], 60, true);
    let config = SupervisorLaunchConfig {
        repo_root: repo.path().to_path_buf(),
        control_relative_path: control.clone(),
        descriptor: desc.clone(),
        nonce: nonce.plaintext_for_spawn_only().to_vec(),
        start_timeout: Duration::from_secs(5),
        pulse_exe: pulse_bin(),
    };
    launch_supervisor(config).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !marker.exists() {
        assert!(Instant::now() < deadline, "child marker not created");
        thread::sleep(Duration::from_millis(25));
    }
    let cancel = SupervisorCancelRequestV1 {
        schema_version: 1,
        run_id: "run_TEST".to_string(),
        attempt_id: "attempt_TEST".to_string(),
        nonce_hash: desc.nonce_hash.clone(),
        requested_at: "now".to_string(),
        requested_by: "test".to_string(),
        reason: "test".to_string(),
        grace_seconds: 1,
        force_allowed: true,
    };
    fs::write(
        repo.path()
            .join(".pulse/runtime/run/control/run_TEST.cancel.json"),
        to_canonical_bytes(&cancel).unwrap(),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !repo
        .path()
        .join(".pulse/runtime/run/control/run_TEST.exit.json")
        .exists()
    {
        assert!(Instant::now() < deadline, "exit observation not written");
        thread::sleep(Duration::from_millis(25));
    }
    assert!(!marker.exists(), "process tree child did not receive TERM");
}

#[test]
fn descriptor_rejects_alias_collision_and_wrong_layout_paths() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let repo = private_repo();
    fs::create_dir_all(repo.path().join(".pulse/runtime/run/inputs")).unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let script = fixture_script(workspace.path(), "#!/bin/sh\nexit 0\n");
    let (nonce, mut desc, control) =
        descriptor(repo.path(), workspace.path(), &script, vec![], 60, true);
    desc.exit_path = desc.cancel_path.clone();
    fs::write(
        repo.path().join(&control),
        to_canonical_bytes(&desc).unwrap(),
    )
    .unwrap();
    let error = run_hidden_supervisor_with_env(repo.path(), &control, &nonce).unwrap_err();
    assert_eq!(error.code(), "run_control_record_invalid");

    let (_nonce2, mut desc, control) =
        descriptor(repo.path(), workspace.path(), &script, vec![], 60, true);
    desc.input_json_path = repo
        .path()
        .join(".pulse/runtime/run/inputs/alias.json")
        .to_str()
        .unwrap()
        .to_string();
    fs::write(
        repo.path().join(&control),
        to_canonical_bytes(&desc).unwrap(),
    )
    .unwrap();
    let error = run_hidden_supervisor_with_env(repo.path(), &control, &_nonce2).unwrap_err();
    assert_eq!(error.code(), "run_control_record_invalid");
    assert!(!repo
        .path()
        .join(".pulse/runtime/run/control/run_TEST.exit.json")
        .exists());
    assert_ne!(nonce.record().nonce_hash, desc.nonce_hash);
}

#[test]
fn descriptor_rejects_symlink_component_in_managed_path() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let repo = private_repo();
    fs::create_dir_all(repo.path().join(".pulse/runtime/run/inputs-real")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        repo.path().join(".pulse/runtime/run/inputs-real"),
        repo.path().join(".pulse/runtime/run/inputs"),
    )
    .unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let script = fixture_script(workspace.path(), "#!/bin/sh\nexit 0\n");
    let (_nonce, desc, control) =
        descriptor(repo.path(), workspace.path(), &script, vec![], 60, true);
    fs::write(
        repo.path().join(&control),
        to_canonical_bytes(&desc).unwrap(),
    )
    .unwrap();
    let error = run_hidden_supervisor_with_env(repo.path(), &control, &_nonce).unwrap_err();
    assert_eq!(error.code(), "run_control_record_invalid");
}

#[test]
fn launch_supervisor_cleans_up_after_invalid_preexisting_handshake() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let repo = private_repo();
    let workspace = tempfile::tempdir().unwrap();
    let marker = workspace.path().join("alive");
    let script = fixture_script(
        workspace.path(),
        &format!(
            "#!/bin/sh\ntouch {m}\ntrap '' TERM\nwhile true; do sleep 1; done\n",
            m = marker.display()
        ),
    );
    let (nonce, desc, control) =
        descriptor(repo.path(), workspace.path(), &script, vec![], 60, true);
    let handshake = repo
        .path()
        .join(".pulse/runtime/run/control/run_TEST.handshake.json");
    fs::write(&handshake, b"{\"schema_version\":1}").unwrap();
    let error = launch_supervisor(SupervisorLaunchConfig {
        repo_root: repo.path().to_path_buf(),
        control_relative_path: control,
        descriptor: desc,
        nonce: nonce.plaintext_for_spawn_only().to_vec(),
        start_timeout: Duration::from_secs(5),
        pulse_exe: pulse_bin(),
    })
    .unwrap_err();
    assert_eq!(error.code(), "run_control_record_invalid");
    thread::sleep(Duration::from_millis(500));
    assert!(!repo
        .path()
        .join(".pulse/runtime/run/control/run_TEST.exit.json")
        .exists());
}

#[test]
fn no_force_cancel_does_not_sigkill_signal_ignoring_process() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let repo = private_repo();
    let workspace = tempfile::tempdir().unwrap();
    let marker = workspace.path().join("still-alive");
    let script = fixture_script(
        workspace.path(),
        &format!(
            "#!/bin/sh\ntouch {m}\ntrap '' TERM\nwhile true; do sleep 1; done\n",
            m = marker.display()
        ),
    );
    let (nonce, desc, control) =
        descriptor(repo.path(), workspace.path(), &script, vec![], 60, true);
    let report = launch_supervisor(SupervisorLaunchConfig {
        repo_root: repo.path().to_path_buf(),
        control_relative_path: control,
        descriptor: desc.clone(),
        nonce: nonce.plaintext_for_spawn_only().to_vec(),
        start_timeout: Duration::from_secs(5),
        pulse_exe: pulse_bin(),
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !marker.exists() {
        assert!(Instant::now() < deadline, "marker not created");
        thread::sleep(Duration::from_millis(25));
    }
    let cancel = SupervisorCancelRequestV1 {
        schema_version: 1,
        run_id: "run_TEST".to_string(),
        attempt_id: "attempt_TEST".to_string(),
        nonce_hash: desc.nonce_hash.clone(),
        requested_at: "now".to_string(),
        requested_by: "test".to_string(),
        reason: "test".to_string(),
        grace_seconds: 1,
        force_allowed: false,
    };
    fs::write(
        repo.path()
            .join(".pulse/runtime/run/control/run_TEST.cancel.json"),
        to_canonical_bytes(&cancel).unwrap(),
    )
    .unwrap();
    thread::sleep(Duration::from_secs(2));
    assert!(marker.exists(), "request force=false must not SIGKILL");
    cancel_verified_process_tree(
        &report.handshake.child_identity,
        CancelPolicy {
            grace: Duration::from_millis(10),
            force_after: Duration::from_secs(1),
            force_allowed: true,
        },
    )
    .unwrap();
}

#[test]
fn final_spawn_rejects_symlinked_executable_after_selection() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let repo = private_repo();
    let workspace = tempfile::tempdir().unwrap();
    let real = fixture_script(workspace.path(), "#!/bin/sh\nexit 0\n");
    let link = workspace.path().join("link.sh");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let (nonce, mut desc, control) =
        descriptor(repo.path(), workspace.path(), &real, vec![], 60, true);
    desc.executable_path = link.to_str().unwrap().to_string();
    fs::write(
        repo.path().join(&control),
        to_canonical_bytes(&desc).unwrap(),
    )
    .unwrap();
    let error = run_hidden_supervisor_with_env(repo.path(), &control, &nonce).unwrap_err();
    assert!(matches!(
        error.code(),
        "run_command_not_found" | "run_control_record_invalid"
    ));
    assert!(!repo
        .path()
        .join(".pulse/runtime/run/control/run_TEST.exit.json")
        .exists());
    assert_eq!(nonce.record().nonce_hash, desc.nonce_hash);
}

#[test]
fn launch_timeout_before_handshake_cleans_child_process_tree() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let _env = EnvGuard::set("PULSE_RUN_SUPERVISOR_TEST_DELAY_HANDSHAKE_MS", "1500");
    let repo = private_repo();
    let workspace = tempfile::tempdir().unwrap();
    let child_pid_file = workspace.path().join("child.pid");
    let grandchild_pid_file = workspace.path().join("grandchild.pid");
    let script = fixture_script(
        workspace.path(),
        &format!(
            "#!/bin/sh\necho $$ > {child}\n(sh -c 'echo $$ > {grand}; trap \"\" TERM; while true; do sleep 1; done') &\ntrap \"\" TERM\nwhile true; do sleep 1; done\n",
            child = child_pid_file.display(),
            grand = grandchild_pid_file.display(),
        ),
    );
    let (nonce, desc, control) =
        descriptor(repo.path(), workspace.path(), &script, vec![], 60, true);
    let launch_error = launch_supervisor(SupervisorLaunchConfig {
        repo_root: repo.path().to_path_buf(),
        control_relative_path: control,
        descriptor: desc,
        nonce: nonce.plaintext_for_spawn_only().to_vec(),
        start_timeout: Duration::from_millis(250),
        pulse_exe: pulse_bin(),
    })
    .unwrap_err();
    assert_eq!(launch_error.code(), "run_supervisor_handshake_timeout");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !child_pid_file.exists() || !grandchild_pid_file.exists() {
        assert!(
            Instant::now() < deadline,
            "child/grandchild pid markers not written"
        );
        thread::sleep(Duration::from_millis(25));
    }
    let child_pid: u64 = fs::read_to_string(&child_pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let grandchild_pid: u64 = fs::read_to_string(&grandchild_pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while process_exists(child_pid) || process_exists(grandchild_pid) {
        if Instant::now() >= deadline {
            panic!(
                "startup timeout left process tree orphaned: child={} grandchild={}",
                fs::read_to_string(format!("/proc/{child_pid}/stat"))
                    .unwrap_or_else(|_| "missing".to_string()),
                fs::read_to_string(format!("/proc/{grandchild_pid}/stat"))
                    .unwrap_or_else(|_| "missing".to_string())
            );
        }
        thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn hidden_supervisor_is_absent_from_public_help() {
    let help = Command::new(pulse_bin()).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(!String::from_utf8_lossy(&help.stdout).contains(HIDDEN_SUPERVISOR_COMMAND));
}

#[test]
fn hidden_supervisor_requires_nonce_env_on_supported_platforms() {
    let output = Command::new(pulse_bin())
        .arg(HIDDEN_SUPERVISOR_COMMAND)
        .arg("--control")
        .arg(".pulse/runtime/run/control/run_TEST.json")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    if cfg!(target_os = "linux") {
        assert!(stderr.contains("run_control_record_invalid"));
    } else {
        assert!(stderr.contains("run_platform_unsupported"));
    }

    let output = Command::new(pulse_bin())
        .arg(HIDDEN_SUPERVISOR_COMMAND)
        .arg("--control")
        .arg("../escape.json")
        .env(HIDDEN_SUPERVISOR_NONCE_ENV, "00")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    if cfg!(target_os = "linux") {
        assert!(stderr.contains("run_control_record_invalid"));
    } else {
        assert!(stderr.contains("run_platform_unsupported"));
    }
}
