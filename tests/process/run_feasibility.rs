use pulse::canonical_json::hash_bytes;
use pulse::process::{
    current_process_identity, drain_to_bounded_logs, process_identity_matches, spawn_process_group,
    terminate_process_group, wait_status_code, write_nonce_record_without_plaintext,
    ControlNoncePlaintext, HIDDEN_SUPERVISOR_COMMAND, HIDDEN_SUPERVISOR_NONCE_ENV,
    PLATFORM_SUPPORT,
};
use std::io::{ErrorKind, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn i0_code_keeps_rust_178_msrv_syntax_contract() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cargo_toml = std::fs::read_to_string(manifest_dir.join("Cargo.toml")).unwrap();
    assert!(cargo_toml.contains("rust-version = \"1.78\""));

    let process_source = std::fs::read_to_string(manifest_dir.join("src/process.rs")).unwrap();
    assert!(!process_source.contains("unsafe extern"));
}

#[test]
fn platform_support_is_explicit_not_generic_unix() {
    if cfg!(target_os = "linux") {
        assert_eq!(PLATFORM_SUPPORT, "linux_proc_stat_starttime_process_group");
    } else if cfg!(target_os = "macos") {
        assert_eq!(PLATFORM_SUPPORT, "unsupported_macos_identity_not_proven");
        assert_eq!(
            current_process_identity(std::process::id())
                .unwrap_err()
                .code(),
            "run_platform_unsupported"
        );
    } else {
        assert_eq!(PLATFORM_SUPPORT, "unsupported");
    }
}

#[test]
fn process_identity_uses_start_marker_and_process_group_before_cancellation() {
    if !cfg!(target_os = "linux") {
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
    if !cfg!(target_os = "linux") {
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

#[test]
fn hidden_supervisor_is_parsed_but_absent_from_public_help() {
    let help = Command::new(crate::common_bin::bin())
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    let help_text = String::from_utf8_lossy(&help.stdout);
    assert!(!help_text.contains(HIDDEN_SUPERVISOR_COMMAND));

    let nonce = ControlNoncePlaintext::generate();
    let output = Command::new(crate::common_bin::bin())
        .arg(HIDDEN_SUPERVISOR_COMMAND)
        .arg("--control")
        .arg(".pulse/runtime/run/control/probe.json")
        .arg("--probe")
        .env(
            HIDDEN_SUPERVISOR_NONCE_ENV,
            hex::encode(nonce.plaintext_for_spawn_only()),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "hidden supervisor probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("self_reexec_available_hidden_dispatch_required"));
    assert!(stdout.contains(HIDDEN_SUPERVISOR_COMMAND));
}

#[test]
fn hidden_supervisor_rejects_control_path_escape() {
    let nonce = ControlNoncePlaintext::generate();
    let output = Command::new(crate::common_bin::bin())
        .arg(HIDDEN_SUPERVISOR_COMMAND)
        .arg("--control")
        .arg(".pulse/runtime/run/../escape.json")
        .arg("--probe")
        .env(
            HIDDEN_SUPERVISOR_NONCE_ENV,
            hex::encode(nonce.plaintext_for_spawn_only()),
        )
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("run_control_record_invalid"));
}

struct LargePatternReader {
    remaining: usize,
    position: usize,
    chunk_limit: usize,
}

impl Read for LargePatternReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }
        let n = self.remaining.min(buf.len()).min(self.chunk_limit);
        for (offset, slot) in buf[..n].iter_mut().enumerate() {
            *slot = ((self.position + offset) % 251) as u8;
        }
        self.position += n;
        self.remaining -= n;
        Ok(n)
    }
}

struct FailingAfterReader {
    emitted: bool,
}

impl Read for FailingAfterReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if !self.emitted {
            self.emitted = true;
            buf[..4].copy_from_slice(b"abcd");
            Ok(4)
        } else {
            Err(ErrorKind::Other.into())
        }
    }
}

#[test]
fn bounded_log_hashes_incrementally_without_retaining_full_stream_on_error() {
    let tmp = tempfile::tempdir().unwrap();
    let handle = drain_to_bounded_logs(
        FailingAfterReader { emitted: false },
        tmp.path().join("stdout.prefix.log"),
        tmp.path().join("stdout.tail.log"),
        8,
    );
    let error = handle.join().unwrap().unwrap_err();
    assert_eq!(error.code(), "io_error");
    assert!(!tmp.path().join("stdout.prefix.log").exists());
    assert!(!tmp.path().join("stdout.tail.log").exists());
}

#[test]
fn bounded_log_large_stream_keeps_bounded_files_and_full_hash_semantics() {
    let tmp = tempfile::tempdir().unwrap();
    let total = 2 * 1024 * 1024 + 17;
    let mut expected = Vec::with_capacity(total);
    for index in 0..total {
        expected.push((index % 251) as u8);
    }
    let handle = drain_to_bounded_logs(
        LargePatternReader {
            remaining: total,
            position: 0,
            chunk_limit: 4096,
        },
        tmp.path().join("stdout.prefix.log"),
        tmp.path().join("stdout.tail.log"),
        128,
    );
    let record = handle.join().unwrap().unwrap();
    assert_eq!(record.total_bytes_seen, total as u64);
    assert_eq!(record.retained_bytes, 128);
    assert_eq!(record.content_hash, hash_bytes(&expected));
    assert_eq!(
        std::fs::metadata(tmp.path().join("stdout.prefix.log"))
            .unwrap()
            .len(),
        64
    );
    assert_eq!(
        std::fs::metadata(tmp.path().join("stdout.tail.log"))
            .unwrap()
            .len(),
        64
    );
}
