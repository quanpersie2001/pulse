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
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

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
    let metadata = fs::metadata(path).unwrap();
    hash_serializable(&(
        path.canonicalize().unwrap().to_str().unwrap().to_string(),
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.mtime(),
        metadata.permissions().mode() & 0o777,
    ))
    .unwrap()
}

fn selection(executable: &Path, args: Vec<String>) -> RunnerProfileSelectionV1 {
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
        environment: vec![RunnerEnvironmentSpecEntryV1 {
            name: "PULSE_SUPERVISOR_TEST_ENV".to_string(),
            source: RunnerEnvironmentSourceV1::Inherited,
        }],
    }
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
    let mut desc = build_descriptor_for_selection(
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
    desc.heartbeat_path = repo
        .join(".pulse/runtime/run/control/run_TEST.heartbeat.json")
        .to_str()
        .unwrap()
        .to_string();
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
        "#!/bin/sh\npwd > cwd.txt\nprintf '%s' \"$1\" > argv.txt\nprintf '%s' \"$PULSE_SUPERVISOR_TEST_ENV\" > env.txt\nprintf 'hello-stdout'\nprintf 'hello-stderr' >&2\nexit 0\n",
    );
    std::env::set_var("PULSE_SUPERVISOR_TEST_ENV", "allowed-value");
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
    run_hidden_supervisor(repo.path(), &control).unwrap();

    assert_eq!(
        fs::read_to_string(workspace.path().join("argv.txt")).unwrap(),
        "arg-one"
    );
    assert_eq!(
        fs::read_to_string(workspace.path().join("env.txt")).unwrap(),
        "allowed-value"
    );
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
        pulse_exe: PathBuf::from(crate::common_bin::bin()),
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
    run_hidden_supervisor(repo.path(), &control).unwrap();
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
    let (_nonce, desc, control) =
        descriptor(repo.path(), workspace.path(), &script, vec![], 60, true);
    fs::write(
        repo.path().join(&control),
        to_canonical_bytes(&desc).unwrap(),
    )
    .unwrap();
    run_hidden_supervisor(repo.path(), &control).unwrap();
    let exit = fs::read_to_string(
        repo.path()
            .join(".pulse/runtime/run/control/run_TEST.exit.json"),
    )
    .unwrap();
    assert!(exit.contains("\"code\": 7"));

    let repo = private_repo();
    let workspace = tempfile::tempdir().unwrap();
    let script = fixture_script(workspace.path(), "#!/bin/sh\nkill -TERM $$\n");
    let (_nonce, desc, control) =
        descriptor(repo.path(), workspace.path(), &script, vec![], 60, true);
    fs::write(
        repo.path().join(&control),
        to_canonical_bytes(&desc).unwrap(),
    )
    .unwrap();
    run_hidden_supervisor(repo.path(), &control).unwrap();
    let exit = fs::read_to_string(
        repo.path()
            .join(".pulse/runtime/run/control/run_TEST.exit.json"),
    )
    .unwrap();
    assert!(exit.contains("\"signal\": 15"));

    let repo = private_repo();
    let workspace = tempfile::tempdir().unwrap();
    let script = fixture_script(workspace.path(), "#!/bin/sh\ntrap '' TERM\nsleep 5\n");
    let (_nonce, desc, control) =
        descriptor(repo.path(), workspace.path(), &script, vec![], 1, true);
    fs::write(
        repo.path().join(&control),
        to_canonical_bytes(&desc).unwrap(),
    )
    .unwrap();
    run_hidden_supervisor(repo.path(), &control).unwrap();
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
    let error = run_hidden_supervisor(repo.path(), &control).err().unwrap();
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
        pulse_exe: PathBuf::from(crate::common_bin::bin()),
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
        pulse_exe: PathBuf::from(crate::common_bin::bin()),
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
fn hidden_supervisor_is_absent_from_public_help() {
    let help = Command::new(crate::common_bin::bin())
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(!String::from_utf8_lossy(&help.stdout).contains(HIDDEN_SUPERVISOR_COMMAND));
}

#[test]
fn hidden_supervisor_requires_nonce_env_on_supported_platforms() {
    let output = Command::new(crate::common_bin::bin())
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

    let output = Command::new(crate::common_bin::bin())
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
