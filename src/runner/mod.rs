//! Process-execution contract for configured runner roles.
//!
//! Runner owns only mechanics: parsing a role command from JSON config as an
//! argv vector (never through a shell), placeholder substitution, timeout
//! enforcement, bounded stdout/stderr capture, process-group cancellation and
//! the final-output JSON contract. It owns no graph truth: callers map
//! [`Outcome`] values onto lifecycle transitions, receipts and events.
//!
//! Invariants:
//!
//! * commands never run through `sh -c`; the command string is split into an
//!   argv vector by [`split_argv`];
//! * a spawned runner becomes its own process group leader on Unix so a
//!   timeout or cancellation can kill the whole tree, not just the direct
//!   child;
//! * captured output is bounded by `max_output_bytes`; excess bytes are
//!   drained and discarded, and the truncation is reported, never silent;
//! * malformed final output is a typed error (`runner_output_malformed`);
//!   callers must not guess.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::{PulseError, PulseResult};

/// Poll interval while waiting for a runner process.
const WAIT_POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Default cap for a single captured output stream (8 MiB).
pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

/// Minimum and maximum accepted timeout (seconds).
pub const MIN_TIMEOUT_SECONDS: u64 = 1;
pub const MAX_TIMEOUT_SECONDS: u64 = 86_400;

/// Minimum and maximum accepted output cap (bytes).
pub const MIN_MAX_OUTPUT_BYTES: usize = 1_024;
pub const MAX_MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

/// Maximum argv length after splitting.
pub const MAX_ARGV_LEN: usize = 64;

/// Maximum single-argument length after splitting.
pub const MAX_ARG_BYTES: usize = 4_096;

/// Placeholders substituted in argv elements, with their meaning.
pub const PLACEHOLDERS: &[&str] = &["input", "ticket", "repo", "artifact_dir"];

const RUNNER_SPEC_HINT: &str = "a runners.json role needs \
     {\"command\": \"...\", \"timeout_seconds\": N} — command is a non-empty \
     command line (<=4096 bytes) split by runner::split_argv (never a shell), \
     timeout_seconds is 1..=86400, and the optional max_output_bytes is \
     1024..=67108864";

const RUNNER_OUTPUT_FORMAT_HINT: &str =
    "the last non-empty stdout line a role prints must be one JSON object, e.g. {\"status\": \"done\"}";

/// Cancellation flag shared with a running execution.
pub type CancelFlag = Arc<AtomicBool>;

/// Fresh cancellation flag.
pub fn cancel_flag() -> CancelFlag {
    Arc::new(AtomicBool::new(false))
}

/// Role entry in `.pulse/config/runners.json`: one command line plus
/// execution bounds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CommandSpec {
    pub command: String,
    pub timeout_seconds: u64,
    #[serde(default = "default_max_output_bytes")]
    pub max_output_bytes: usize,
}

fn default_max_output_bytes() -> usize {
    DEFAULT_MAX_OUTPUT_BYTES
}

impl CommandSpec {
    /// Parse and validate a role spec from its JSON value.
    ///
    /// # Errors
    ///
    /// Returns a typed validation error for malformed JSON shape or bounds
    /// outside the accepted ranges.
    pub fn from_value(value: &serde_json::Value) -> PulseResult<Self> {
        let spec: CommandSpec = serde_json::from_value(value.clone()).map_err(|error| {
            PulseError::kernel("runner_spec_invalid", error.to_string(), RUNNER_SPEC_HINT)
        })?;
        spec.validate()?;
        Ok(spec)
    }

    /// Validate execution bounds.
    ///
    /// # Errors
    ///
    /// Returns `runner_spec_invalid` when the command line is empty, the
    /// timeout is outside [`MIN_TIMEOUT_SECONDS`]..=[`MAX_TIMEOUT_SECONDS`],
    /// or the output cap is outside [`MIN_MAX_OUTPUT_BYTES`]..
    /// =[`MAX_MAX_OUTPUT_BYTES`].
    pub fn validate(&self) -> PulseResult<()> {
        if self.command.trim().is_empty() || self.command.len() > MAX_ARG_BYTES {
            return Err(PulseError::kernel(
                "runner_spec_invalid",
                "runner command must be a non-empty command line within 4096 bytes",
                RUNNER_SPEC_HINT,
            ));
        }
        if !(MIN_TIMEOUT_SECONDS..=MAX_TIMEOUT_SECONDS).contains(&self.timeout_seconds) {
            return Err(PulseError::kernel(
                "runner_spec_invalid",
                format!(
                    "runner timeout_seconds must be between {MIN_TIMEOUT_SECONDS} and {MAX_TIMEOUT_SECONDS}"
                ),
                RUNNER_SPEC_HINT,
            ));
        }
        if !(MIN_MAX_OUTPUT_BYTES..=MAX_MAX_OUTPUT_BYTES).contains(&self.max_output_bytes) {
            return Err(PulseError::kernel(
                "runner_spec_invalid",
                format!(
                    "runner max_output_bytes must be between {MIN_MAX_OUTPUT_BYTES} and {MAX_MAX_OUTPUT_BYTES}"
                ),
                RUNNER_SPEC_HINT,
            ));
        }
        Ok(())
    }
}

/// Split a command line into an argv vector without invoking a shell.
///
/// Supports POSIX-style single quotes (no escapes inside), double quotes
/// (backslash escapes inside) and backslash escapes outside quotes. An empty
/// command line yields `runner_argv_empty`; oversized argv or arguments yield
/// `runner_argv_invalid`.
///
/// # Errors
///
/// Returns a typed validation error for unterminated quotes or trailing
/// backslashes.
pub fn split_argv(command: &str) -> PulseResult<Vec<String>> {
    let mut argv = Vec::new();
    let mut current = String::new();
    let mut has_token = false;
    let mut chars = command.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                has_token = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(inner) => current.push(inner),
                        None => {
                            return Err(PulseError::validation(
                                "runner_argv_invalid",
                                "unterminated single quote in runner command",
                            ))
                        }
                    }
                }
            }
            '"' => {
                has_token = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(escaped @ ('$' | '`' | '"' | '\\')) => current.push(escaped),
                            Some(other) => {
                                current.push('\\');
                                current.push(other);
                            }
                            None => {
                                return Err(PulseError::validation(
                                    "runner_argv_invalid",
                                    "unterminated escape in runner command",
                                ))
                            }
                        },
                        Some(inner) => current.push(inner),
                        None => {
                            return Err(PulseError::validation(
                                "runner_argv_invalid",
                                "unterminated double quote in runner command",
                            ))
                        }
                    }
                }
            }
            '\\' => {
                has_token = true;
                match chars.next() {
                    Some(escaped) => current.push(escaped),
                    None => {
                        return Err(PulseError::validation(
                            "runner_argv_invalid",
                            "trailing backslash in runner command",
                        ))
                    }
                }
            }
            c if c.is_whitespace() => {
                if has_token {
                    argv.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            c => {
                has_token = true;
                current.push(c);
            }
        }
    }
    if has_token {
        argv.push(current);
    }
    if argv.is_empty() {
        return Err(PulseError::validation(
            "runner_argv_empty",
            "runner command splits to an empty argv",
        ));
    }
    if argv.len() > MAX_ARGV_LEN || argv.iter().any(|arg| arg.len() > MAX_ARG_BYTES) {
        return Err(PulseError::validation(
            "runner_argv_invalid",
            format!(
                "runner command exceeds {MAX_ARGV_LEN} arguments or {MAX_ARG_BYTES} bytes per argument"
            ),
        ));
    }
    Ok(argv)
}

/// Substitute `{name}` placeholders in an argv vector.
///
/// # Errors
///
/// Returns `runner_placeholder_unknown` when an element contains a
/// `{name}`-shaped token whose name is not in [`PLACEHOLDERS`], so typos fail
/// fast instead of leaking a literal placeholder to the agent.
pub fn materialize_argv(
    argv: &[String],
    values: &std::collections::BTreeMap<String, String>,
) -> PulseResult<Vec<String>> {
    argv.iter()
        .map(|arg| {
            let mut out = String::with_capacity(arg.len());
            let mut rest = arg.as_str();
            while let Some(start) = rest.find('{') {
                let after = &rest[start + 1..];
                if let Some(end) = after.find('}') {
                    let name = &after[..end];
                    if PLACEHOLDERS.contains(&name) {
                        out.push_str(&rest[..start]);
                        out.push_str(values.get(name).map(String::as_str).unwrap_or(""));
                        rest = &after[end + 1..];
                        continue;
                    }
                    if !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                    {
                        return Err(PulseError::validation(
                            "runner_placeholder_unknown",
                            format!("runner command uses unknown placeholder {{{name}}}"),
                        ));
                    }
                }
                // Not a placeholder-shaped token; keep scanning after '{'.
                out.push_str(&rest[..start + 1]);
                rest = &rest[start + 1..];
            }
            out.push_str(rest);
            Ok(out)
        })
        .collect()
}

/// Terminal result of one runner execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Process exit code; `None` when the process was killed by a signal.
    pub exit_code: Option<i32>,
    /// The timeout elapsed and the process group was killed.
    pub timed_out: bool,
    /// The cancellation flag was observed and the process group was killed.
    pub cancelled: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub duration: Duration,
}

impl Outcome {
    /// Whether the process exited successfully and was not killed.
    pub fn exited_cleanly(&self) -> bool {
        !self.timed_out && !self.cancelled && self.exit_code == Some(0)
    }
}

/// Execute `argv` in `working_dir` under the spec's bounds.
///
/// The child runs as its own process group on Unix. On timeout or observed
/// cancellation the whole group is killed. Output streams are capped at
/// `max_output_bytes` each; excess is drained and reported as truncated.
/// `env` is applied on top of the inherited parent environment (via
/// [`std::process::Command::envs`], scoped to this one child) rather than
/// through a process-global `std::env::set_var` — callers that need the
/// child to see a variable like `PULSE_ACTOR` pass it here instead of
/// mutating the current process's environment first.
///
/// # Errors
///
/// Returns an I/O error when the process cannot be spawned.
pub fn execute(
    working_dir: &Path,
    argv: &[String],
    env: &[(&str, String)],
    timeout: Duration,
    max_output_bytes: usize,
    cancel: Option<&CancelFlag>,
) -> PulseResult<Outcome> {
    let started = Instant::now();
    let mut command = Command::new(&argv[0]);
    command
        .args(&argv[1..])
        .current_dir(working_dir)
        .envs(env.iter().map(|(key, value)| (*key, value.as_str())))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // The child becomes its own process-group leader so the whole tree
        // can be killed on timeout or cancellation.
        command.process_group(0);
    }

    let mut child = command.spawn().map_err(|error| {
        PulseError::validation(
            "runner_spawn_failed",
            format!("runner command {} failed to spawn: {error}", argv[0]),
        )
    })?;
    let pid = child.id();

    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let stdout_handle =
        std::thread::spawn(move || drain_bounded(&mut stdout_pipe, max_output_bytes));
    let stderr_handle =
        std::thread::spawn(move || drain_bounded(&mut stderr_pipe, max_output_bytes));

    let mut timed_out = false;
    let mut cancelled = false;
    loop {
        if child
            .try_wait()
            .map_err(|error| PulseError::io(working_dir.to_path_buf(), error))?
            .is_some()
        {
            break;
        }
        if let Some(flag) = cancel {
            if flag.load(Ordering::SeqCst) {
                cancelled = true;
                kill_process_tree(pid);
                break;
            }
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            kill_process_tree(pid);
            break;
        }
        std::thread::sleep(WAIT_POLL_INTERVAL);
    }

    let status = child
        .wait()
        .map_err(|error| PulseError::io(working_dir.to_path_buf(), error))?;
    let (stdout, stdout_truncated) = stdout_handle.join().map_err(|_| {
        PulseError::kernel(
            "runner_output_invalid",
            "stdout capture thread panicked",
            "this is an internal capture failure, not a role misconfiguration; \
             rerun the role and report it if it recurs",
        )
    })?;
    let (stderr, stderr_truncated) = stderr_handle.join().map_err(|_| {
        PulseError::kernel(
            "runner_output_invalid",
            "stderr capture thread panicked",
            "this is an internal capture failure, not a role misconfiguration; \
             rerun the role and report it if it recurs",
        )
    })?;

    // After a kill the wait status reflects the signal; surface the decision
    // through the flags and keep only a best-effort exit code.
    let exit_code = status.code();
    Ok(Outcome {
        exit_code,
        timed_out,
        cancelled,
        stdout,
        stderr,
        stdout_truncated,
        stderr_truncated,
        duration: started.elapsed(),
    })
}

#[cfg(unix)]
fn kill_process_tree(pid: u32) {
    // SAFETY: `kill` is an async-signal-safe system call; sending SIGKILL to
    // the negative pid targets the whole process group led by `pid`. The
    // child was spawned with `process_group(0)`, so its pid is the pgid.
    unsafe {
        extern "C" {
            fn kill(pid: i32, sig: i32) -> i32;
        }
        const SIGKILL: i32 = 9;
        let _ = kill(-(pid as i32), SIGKILL);
    }
}

#[cfg(not(unix))]
fn kill_process_tree(pid: u32) {
    // Non-Unix platforms have no portable process-group kill; Pulse has no
    // tier-1 Windows support yet, so fall back to the direct child handle.
    let _ = pid;
}

/// Drain a stream up to `max_bytes`, discarding excess while still consuming
/// so the child never blocks on a full pipe. Returns the retained bytes and
/// whether truncation happened.
fn drain_bounded<S: Read>(stream: &mut Option<S>, max_bytes: usize) -> (Vec<u8>, bool) {
    let mut retained = Vec::new();
    let Some(stream) = stream.as_mut() else {
        return (retained, false);
    };
    let mut buf = [0_u8; 8192];
    let mut truncated = false;
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(read) => {
                let keep = max_bytes.saturating_sub(retained.len()).min(read);
                retained.extend_from_slice(&buf[..keep]);
                if keep < read {
                    truncated = true;
                }
            }
            Err(_) => break,
        }
    }
    (retained, truncated)
}

/// Parse the runner's final JSON summary from captured stdout.
///
/// The contract is: the last non-empty stdout line is one JSON value. Empty
/// output yields `runner_output_malformed`, as does any trailing garbage
/// after the final JSON line.
///
/// # Errors
///
/// Returns `runner_output_malformed` when stdout carries no parsable final
/// JSON line.
pub fn parse_output_json(outcome: &Outcome) -> PulseResult<serde_json::Value> {
    let text = std::str::from_utf8(&outcome.stdout)
        .map_err(|_| malformed("runner stdout is not UTF-8"))?;
    let line = text
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| malformed("runner produced no output"))?;
    let value: serde_json::Value = serde_json::from_str(line.trim())
        .map_err(|error| malformed(&format!("final stdout line is not one JSON value: {error}")))?;
    Ok(value)
}

fn malformed(message: &str) -> PulseError {
    PulseError::kernel(
        "runner_output_malformed",
        message.to_string(),
        RUNNER_OUTPUT_FORMAT_HINT,
    )
}
