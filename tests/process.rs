//! Process / crash-recovery / concurrency integration tests.
//!
//! This crate isolates the subprocess-spawning, timing-sensitive suites that
//! exercise cross-process CAS, failpoint crash recovery and supersession
//! process recovery. Each test spawns real `pulse` child processes (often under
//! a `--test-failpoint` with `PULSE_FAILPOINT_SLEEP_MS`) against an isolated
//! `tempfile::tempdir()` target repository and synchronizes kills on the
//! failpoint-specific durable state, so the suite is deterministic under the
//! default parallelism of `cargo test --all-targets`.
//!
//! Separating these from the pure-logic `graph` crate keeps per-crate
//! parallelism bounded and gives the process/recovery surface a focused
//! runnable target (`cargo test --test process`).
//!
//! The shared CLI binary resolver lives in `tests/common` and is wired below.

#[path = "common/bin.rs"]
mod common_bin;

#[path = "process/crash_recovery_process.rs"]
mod crash_recovery_process;
#[path = "process/process_concurrency.rs"]
mod process_concurrency;
#[path = "process/process_lifecycle_concurrency.rs"]
mod process_lifecycle_concurrency;
#[path = "process/supersession_process_recovery.rs"]
mod supersession_process_recovery;
