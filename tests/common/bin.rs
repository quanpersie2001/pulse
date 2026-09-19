//! Shared resolver for the Pulse binary under test.
//!
//! Used across integration-test crates so each crate can resolve the CLI binary
//! without accidentally selecting a stale repository-local `target/debug/pulse`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

static BUILD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Resolve the Pulse CLI binary path.
///
/// Prefers Cargo's `CARGO_BIN_EXE_pulse`. When Cargo does not inject that path
/// for the current integration test invocation, resolves the sibling binary in
/// the active target directory and builds it under a process-local lock if it is
/// missing. The fallback intentionally derives from the test executable path or
/// `CARGO_TARGET_DIR`; it never falls back to a preexisting repository
/// `target/debug/pulse` that may belong to a different host/toolchain.
pub fn bin() -> String {
    resolve_pulse_bin().to_string_lossy().into_owned()
}

pub fn resolve_pulse_bin() -> PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_pulse").map(PathBuf::from) {
        return path;
    }
    let candidate = target_dir_from_current_test().join(binary_name("pulse"));
    if candidate.is_file() {
        return candidate;
    }
    let _guard = BUILD_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if candidate.is_file() {
        return candidate;
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut command = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    command
        .arg("build")
        .arg("--locked")
        .arg("--bin")
        .arg("pulse")
        .current_dir(&manifest_dir)
        .stdin(Stdio::null());
    if let Some(target_dir) = std::env::var_os("CARGO_TARGET_DIR") {
        command.env("CARGO_TARGET_DIR", target_dir);
    }
    let status = command.status().expect("build pulse test binary");
    assert!(status.success(), "cargo build --locked --bin pulse failed");
    assert!(
        candidate.is_file(),
        "pulse binary was not produced at {}",
        candidate.display()
    );
    candidate
}

fn target_dir_from_current_test() -> PathBuf {
    if let Some(target_dir) = std::env::var_os("CARGO_TARGET_DIR") {
        return PathBuf::from(target_dir).join("debug");
    }
    let current = std::env::current_exe().expect("current test executable path");
    current
        .parent()
        .and_then(Path::parent)
        .expect("test executable under target/debug/deps")
        .to_path_buf()
}

fn binary_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}
