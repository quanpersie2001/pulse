use pulse::process::{
    current_process_identity, process_identity_matches, spawn_process_group,
    terminate_process_group, wait_status_code, write_nonce_record_without_plaintext,
    ControlNoncePlaintext, PLATFORM_SUPPORT,
};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn platform_support_is_explicit_not_generic_unix() {
    if cfg!(target_os = "linux") {
        assert_eq!(PLATFORM_SUPPORT, "linux_proc_stat_starttime_process_group");
    } else if cfg!(target_os = "macos") {
        assert_eq!(PLATFORM_SUPPORT, "macos_kinfo_proc_starttime_process_group");
    } else {
        assert_eq!(PLATFORM_SUPPORT, "unsupported");
    }
}

#[test]
fn process_identity_uses_start_marker_and_process_group_before_cancellation() {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return;
    }

    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg("trap 'exit 0' TERM; sleep 30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = spawn_process_group(&mut command).unwrap();
    let identity = current_process_identity(child.id()).unwrap();
    assert_eq!(identity.identity_status, "verified");
    assert!(identity.platform_start_marker.starts_with("sha256:"));
    assert_eq!(identity.process_group_id, Some(child.id() as i64));
    assert!(process_identity_matches(&identity).unwrap());

    terminate_process_group(&identity).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            let (_code, signal) = wait_status_code(status);
            assert!(status.success() || signal == Some(15));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "child did not terminate after process-group signal"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn stale_process_identity_is_not_signalled() {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return;
    }

    let mut command = Command::new("sh");
    command.arg("-c").arg("sleep 30");
    let mut child = spawn_process_group(&mut command).unwrap();
    let mut identity = current_process_identity(child.id()).unwrap();
    identity.platform_start_marker = "sha256:stale".to_string();
    let error = terminate_process_group(&identity).unwrap_err();
    assert_eq!(error.code(), "run_process_identity_mismatch");
    child.kill().unwrap();
    let _ = child.wait();
}

#[test]
fn nonce_plaintext_stays_out_of_control_record() {
    let tmp = tempfile::tempdir().unwrap();
    let nonce = ControlNoncePlaintext::generate();
    let plaintext_hex = hex::encode(nonce.plaintext_for_spawn_only());
    let path = tmp.path().join("control.json");
    write_nonce_record_without_plaintext(&path, &nonce).unwrap();
    let body = std::fs::read_to_string(path).unwrap();
    assert!(body.contains("nonce_hash"));
    assert!(!body.contains(&plaintext_hex));
    assert!(!nonce.record().plaintext_persisted);
}
