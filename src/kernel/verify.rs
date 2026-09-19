//! Observed verification: `pulse verify <id>` runs the argv a record
//! declares (decision 0026).
//!
//! Purpose: turn a Ticket's `verify[]` from something the worker *says* it
//! ran into something Pulse *observed*. Nothing here chooses a command —
//! the argv comes from the record, and editing a record is
//! `Action::MutateGraph`, which no agent holds.
//!
//! State touched: `.pulse/runtime/verify/` (scratch captures, deleted as soon
//! as they are bounded), `.pulse/evidence/<id>/verify/<name>.log` (one
//! bounded, redacted log per declared command), `.pulse/receipts/*.jsonl`
//! (one `verify` receipt) and the event log (`verify.recorded`). Never a path
//! outside `.pulse/`.
//!
//! Invariants:
//!
//! * only argv a record declares is run. No shell, ever — `sh -c` appears in
//!   this file's tests to *produce* output, never as the runner's mechanism;
//! * commands run one at a time, in record order, each bounded by the
//!   timeout, and the call always terminates;
//! * a command that cannot be spawned is an *observation*, not an error: one
//!   broken declaration must not hide the other results;
//! * a log is the bounded tail of the merged stdout+stderr ([`LOG_TAIL_BYTES`],
//!   the same bound decision 0024 §3 chose) and passes the tracked-plane
//!   redaction boundary before it is written.
//!
//! A killed command's *grandchildren* can outlive it: `kill()` signals the
//! child we spawned, not a process group, so a child that forked and exited
//! leaves its own children running. Deliberate for now — process groups and
//! reaping are a host concern, not a truth layer's (decision 0026, risks 1).
//!
//! Allowed dependencies: `kernel::{issues, profile, roles}`, `store::issues`,
//! `storage`, `evidence::{receipt, redaction}`, `event`, `identity::actor`.
//! Never the CLI layer above — `tests/architecture_guards.rs::
//! kernel_does_not_depend_on_cli` scans for that dependency by name.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde_json::{json, Value};

use crate::error::{PulseError, Result};
use crate::event::emit_event;
use crate::evidence::receipt::{record_receipt, NewReceipt, ReceiptSource, ReceiptSubject};
use crate::evidence::redaction::clean_text;
use crate::identity::actor::ActorRef;
use crate::kernel::issues::require;
use crate::kernel::profile;
use crate::kernel::roles::{authorize, Action};
use crate::storage;
use crate::store::issues;

/// The tail of a command's output that is kept. Decision 0024 §3's bound:
/// "the interesting end of a broken run is its last lines". Plan 0025 D1
/// sketched 64 KiB; the existing bound wins so the harness has one number.
const LOG_TAIL_BYTES: usize = 32 * 1024;

/// Where a capture lives while it is being collected. Under
/// `.pulse/runtime/` — ignored state — so a capture in flight never reads as
/// tracked evidence, and (being under `.pulse/`) it can never move a fence.
const SCRATCH_DIR: &str = ".pulse/runtime/verify";

/// How often a running command is asked whether it has finished. Small
/// enough that the timeout is honoured to within a few milliseconds, large
/// enough not to spin a core.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// What running one declared argv looked like from outside the process.
#[derive(Debug, Clone)]
pub(crate) struct Observed {
    /// The exit code, or `None` when the command was killed (timeout,
    /// signal) or never started — the log says which for the spawn case.
    pub exit: Option<i64>,
    pub timed_out: bool,
    pub duration_ms: u64,
    /// The bounded tail of stdout+stderr merged, already redacted.
    pub log: String,
}

fn invalid(message: impl Into<String>) -> PulseError {
    PulseError::kernel(
        "verify_argv_invalid",
        message,
        "a verify[] entry is {name, argv, cwd?}: argv is a non-empty list, cwd names a \
         directory under the repository root, and name is a plain file name ([A-Za-z0-9._-]) \
         unique within verify[]",
    )
}

/// Run one argv and report what Pulse observed.
///
/// Never runs a shell, and never returns `Err` for the outcome of the
/// command itself: a declaration that cannot even spawn comes back as an
/// [`Observed`] with `exit: None`, so one broken entry cannot hide the
/// others. `Err` means the *declaration* is unusable (empty argv, a `cwd`
/// that does not name a directory in the repo) — surfacing that loudly is
/// the point.
///
/// # Errors
/// `verify_argv_invalid` if `argv` is empty, or `cwd` is not a
/// repository-relative path naming an existing directory.
pub(crate) fn run_argv(
    repo_root: &Path,
    argv: &[String],
    cwd: Option<&str>,
    timeout: Duration,
) -> Result<Observed> {
    let Some((program, args)) = argv.split_first() else {
        return Err(invalid("verify entry declares an empty argv"));
    };
    let working_dir = match cwd {
        Some(cwd) => resolve_cwd(repo_root, cwd)?,
        None => repo_root.to_path_buf(),
    };

    let scratch_dir = repo_root.join(SCRATCH_DIR);
    std::fs::create_dir_all(&scratch_dir).map_err(|error| PulseError::io(&scratch_dir, error))?;
    let scratch = scratch_dir.join(format!("{}.log", ulid::Ulid::new()));
    let capture = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&scratch)
        .map_err(|error| PulseError::io(&scratch, error))?;
    let capture_err = capture
        .try_clone()
        .map_err(|error| PulseError::io(&scratch, error))?;

    let started = Instant::now();
    // Both streams go to one file rather than two pipes. A pipe the parent
    // is not draining deadlocks a child that writes more than the pipe
    // buffer (>64 KiB); a file has no such edge, so no reader thread is
    // needed and a surviving grandchild holding the handle cannot wedge us.
    let spawned = Command::new(program)
        .args(args)
        .current_dir(&working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(capture))
        .stderr(Stdio::from(capture_err))
        .spawn();

    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => {
            let _ = std::fs::remove_file(&scratch);
            return Ok(Observed {
                exit: None,
                timed_out: false,
                duration_ms: elapsed_ms(started),
                log: format!("<spawn failed: {error}>"),
            });
        }
    };

    // No timeout crate: poll, then kill. `wait()` after `kill()` reaps, and
    // cannot block on a pipe because there is no pipe.
    let deadline = started + timeout;
    let mut timed_out = false;
    let exit = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code().map(i64::from),
            Ok(None) => {
                if Instant::now() >= deadline {
                    timed_out = true;
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            // The child is gone but its status is unreadable; treat it as an
            // observation with no exit code rather than failing the run.
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let duration_ms = elapsed_ms(started);

    let captured = read_tail(&scratch).unwrap_or_default();
    let _ = std::fs::remove_file(&scratch);

    Ok(Observed {
        exit,
        timed_out,
        duration_ms,
        log: redact(repo_root, &captured),
    })
}

/// Persist one observed run's bounded, redacted log to `path`, creating its
/// directory. Shared by `pulse verify` and `pulse lane reconcile` so both
/// write logs through one rule (decision 0027 C3) — a reconcile log is the
/// same kind of artifact as a verify log, bounded and redacted the same way.
///
/// # Errors
/// Propagates directory-creation and atomic-write I/O errors.
pub(crate) fn write_observed_log(path: &Path, observed: &Observed) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
    }
    storage::atomic_write(path, observed.log.as_bytes())
}

/// Turn a declared `cwd` into an absolute path, refusing anything that
/// escapes the repository or does not name a directory.
fn resolve_cwd(repo_root: &Path, cwd: &str) -> Result<PathBuf> {
    let relative = storage::safe_repo_relative(cwd).map_err(|_| {
        invalid(format!(
            "cwd {cwd:?} is not a repository-relative path inside the repo"
        ))
    })?;
    let resolved = repo_root.join(&relative);
    if !resolved.is_dir() {
        return Err(invalid(format!("cwd {cwd:?} is not a directory")));
    }
    Ok(resolved)
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The last [`LOG_TAIL_BYTES`] of `path`, converted lossily to text and moved
/// forward onto a char boundary so the result is always valid UTF-8 and never
/// longer than the bound.
fn read_tail(path: &Path) -> Result<String> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(path).map_err(|error| PulseError::io(path, error))?;
    let len = file
        .metadata()
        .map_err(|error| PulseError::io(path, error))?
        .len();
    if len > LOG_TAIL_BYTES as u64 {
        file.seek(SeekFrom::Start(len - LOG_TAIL_BYTES as u64))
            .map_err(|error| PulseError::io(path, error))?;
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| PulseError::io(path, error))?;
    Ok(tail_chars(&String::from_utf8_lossy(&bytes), LOG_TAIL_BYTES))
}

/// The last `max` bytes of `text`, advanced to the next char boundary when
/// that would otherwise split one.
fn tail_chars(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_string()
}

/// Apply the tracked-plane boundary (decision 0012 §4) to a captured log.
///
/// A refusal is not a failed run: the exit code is evidence too, so the log
/// is replaced by one sentence saying it was withheld, and the run's result
/// survives. (Note the boundary's own edge: it treats a *whole value*
/// starting with an absolute path as a path, so a log whose first character
/// is `/` is withheld rather than rewritten — conservative by design, and
/// cheaper to explain than a per-line scanner.)
fn redact(repo_root: &Path, log: &str) -> String {
    match clean_text(repo_root, "verify log", log) {
        Ok(cleaned) => cleaned,
        Err(_) => "<log withheld: redaction refused it>".to_string(),
    }
}

/// One declared command, lifted out of the record or out of an active
/// learning's check (plan 0025 E2).
#[derive(Debug, Clone)]
struct DeclaredCommand {
    name: String,
    argv: Vec<String>,
    cwd: Option<String>,
    /// Where the declaration came from: `"ticket"` (`verify[]`) or
    /// `"learning"` (an active learning's `check_argv`). Carried into the
    /// receipt's results so the evidence says who declared what.
    source: &'static str,
}

/// Read `verify[]` off a record. A record with no `verify[]` yields an empty
/// list — the caller decides whether that is an error.
fn declared_commands(ticket: &Value) -> Vec<DeclaredCommand> {
    ticket
        .get("verify")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .map(|entry| DeclaredCommand {
                    name: entry
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    argv: entry
                        .get("argv")
                        .and_then(Value::as_array)
                        .map(|list| {
                            list.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                    cwd: entry.get("cwd").and_then(Value::as_str).map(str::to_string),
                    source: "ticket",
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Every active learning whose `applies_to`/`tags` match the record and that
/// carries a `check_argv` (plan 0025 E2). Matching is deliberately NOT
/// capped by `recall::RECALL_LIMIT` — five is the bound for what fits in a
/// worker's packet, not for what counts as evidence; every matching check
/// runs. Only `active` learnings are returned: a candidate has never been
/// blessed by a human, and running its argv would let an agent introduce
/// commands Pulse executes — the exact boundary decision 0026 draws ("argv
/// nằm trong record mà chỉ `human:` sửa được").
fn learning_commands(repo_root: &Path, ticket: &Value) -> Result<Vec<DeclaredCommand>> {
    let anchors: Vec<String> = ticket
        .pointer("/context/anchors")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let tags: Vec<String> = ticket
        .get("tags")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let anchor_refs: Vec<&str> = anchors.iter().map(String::as_str).collect();
    let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();
    Ok(
        crate::learn::recall::matching_for(repo_root, &anchor_refs, &tag_refs, false)?
            .into_iter()
            .filter(|learning| !learning.frontmatter.check_argv.is_empty())
            .map(|learning| DeclaredCommand {
                // A dot, not a colon: `validate` only allows [A-Za-z0-9._-] in
                // a name (it is a log-file stem).
                name: format!("learning.{}", learning.frontmatter.id),
                argv: learning.frontmatter.check_argv.clone(),
                cwd: learning.frontmatter.check_cwd.clone(),
                source: "learning",
            })
            .collect(),
    )
}

/// The names a `pulse verify` receipt must carry for this record to count as
/// verified: every `verify[]` name plus every enforceable `learning.<id>`
/// (plan 0025 E2). One function so the handoff gate and the lane seal rule
/// (decision 0026 D2) agree on what "the ticket declares verify" means.
///
/// # Errors
/// Propagates the learnings/event-log read failures behind learning match.
pub(crate) fn required_names(repo_root: &Path, ticket: &Value) -> Result<Vec<String>> {
    let mut names: Vec<String> = declared_commands(ticket)
        .into_iter()
        .map(|command| command.name)
        .collect();
    names.extend(
        learning_commands(repo_root, ticket)?
            .into_iter()
            .map(|command| command.name),
    );
    Ok(names)
}

/// Whether `name` can be used as the stem of a log file name: non-empty and
/// drawn only from `[A-Za-z0-9._-]`. Without a separator in the set, a name
/// cannot traverse out of the verify directory.
fn safe_log_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Check every declaration before running any of them. A malformed record
/// must fail loudly and leave no half-written evidence behind.
fn validate(repo_root: &Path, declared: &[DeclaredCommand]) -> Result<()> {
    let mut seen: Vec<&str> = Vec::with_capacity(declared.len());
    for command in declared {
        if !safe_log_name(&command.name) {
            return Err(invalid(format!(
                "verify name {:?} is not a plain file name ([A-Za-z0-9._-])",
                command.name
            )));
        }
        // Two entries writing one log would leave the first result pointing
        // at the second command's output — a receipt that lies. Uniqueness is
        // part of the name being usable as a file name here.
        if seen.contains(&command.name.as_str()) {
            return Err(invalid(format!(
                "verify name {:?} is declared more than once",
                command.name
            )));
        }
        seen.push(&command.name);
        if command.argv.is_empty() {
            return Err(invalid(format!(
                "verify {} declares an empty argv",
                command.name
            )));
        }
        if let Some(cwd) = &command.cwd {
            resolve_cwd(repo_root, cwd)?;
        }
    }
    Ok(())
}

/// Run every command the Ticket declares, in order, and seal one `verify`
/// receipt describing what was observed (decision 0026).
///
/// The receipt is sealed whether or not the commands passed: a failed verify
/// is evidence, and the caller decides how to report it. `passed` in the
/// returned value is the caller's exit-code signal.
///
/// # Errors
/// `role_forbidden` for a `system` actor; `verify_not_runnable` when the
/// Ticket is neither `active` nor `verifying`; `verify_nothing_declared`
/// when `verify[]` is empty; `verify_argv_invalid` for a malformed entry.
pub fn verify(repo_root: &Path, actor: &ActorRef, id: &str, timeout: Duration) -> Result<Value> {
    authorize(actor, Action::Verify)?;

    let records = issues::read_all(repo_root)?;
    let ticket = require(&records, id)?;

    let status = ticket.get("status").and_then(Value::as_str).unwrap_or("");
    if status != "active" && status != "verifying" {
        return Err(PulseError::kernel(
            "verify_not_runnable",
            format!("ticket is {status}, not active or verifying"),
            "claim it first; verify runs against work in progress or under review",
        ));
    }

    // Plan 0025 E2: two declaration sources — the ticket's `verify[]` and
    // every active matching learning's `check_argv`.
    let mut declared = declared_commands(ticket);
    declared.extend(learning_commands(repo_root, ticket)?);
    if declared.is_empty() {
        return Err(PulseError::kernel(
            "verify_nothing_declared",
            format!("{id} declares no verify[] command and no learning check applies to it"),
            "add verify[] with `pulse work update`, or hand off without a verify receipt",
        ));
    }
    validate(repo_root, &declared)?;

    let log_dir = repo_root.join(".pulse/evidence").join(id).join("verify");
    std::fs::create_dir_all(&log_dir).map_err(|error| PulseError::io(&log_dir, error))?;

    // Sequential, in record order, and never stopping at the first failure:
    // a gate reports everything it saw (the shape `kernel::ready` and the
    // close gate use). No write lock is held here — a declared command may
    // run for minutes and another worker needs the store's lock meanwhile.
    let mut results = Vec::with_capacity(declared.len());
    let mut passed = true;
    for command in &declared {
        let observed = run_argv(repo_root, &command.argv, command.cwd.as_deref(), timeout)?;
        // Rewritten on every run by design: an earlier receipt keeps the
        // sha256 of the bytes *it* hashed, so it still describes exactly what
        // it saw. The file on disk is the most recent observation.
        write_observed_log(&log_dir.join(format!("{}.log", command.name)), &observed)?;
        if observed.exit != Some(0) {
            passed = false;
        }
        results.push(json!({
            "name": command.name,
            "argv": command.argv,
            "cwd": command.cwd,
            "exit": observed.exit,
            "timed_out": observed.timed_out,
            "duration_ms": observed.duration_ms,
            "log": format!(".pulse/evidence/{id}/verify/{}.log", command.name),
            "source": command.source,
        }));
    }

    // The fence is taken AFTER the commands ran (decision 0026, risk 3): the
    // receipt says "on THIS tree these commands gave THESE results". A verify
    // that rewrites a file therefore matches the tree it produced — intended,
    // and why `handoff_verify_stale` cannot catch a tree-writing verify on
    // its own (`handoff_unreserved_changes` is what catches it).
    let fence = profile::fence_for(repo_root, ticket)?;
    let revision = ticket.get("revision").and_then(Value::as_u64);
    let artifact_paths: Vec<String> = results
        .iter()
        .filter_map(|result| {
            result
                .get("log")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();
    let receipt = record_receipt(
        repo_root,
        None,
        NewReceipt {
            kind: "verify".to_string(),
            subject: ReceiptSubject {
                id: id.to_string(),
                revision,
            },
            actor: actor.as_kind_id(),
            source: ReceiptSource {
                commit: fence.commit,
                dirty_hash: fence.dirty_hash,
            },
            run_id: None,
            payload: json!({"results": results, "passed": passed}),
            artifact_paths,
        },
    )?;

    emit_event(
        repo_root,
        "verify.recorded",
        actor.as_kind_id(),
        id,
        json!({"passed": passed, "receipt": receipt.id}),
        Utc::now(),
    )?;

    Ok(json!({
        "receipt": receipt.id,
        "passed": passed,
        "results": results,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical_json::hash_bytes;
    use crate::identity::actor::ActorKind;

    fn agent(id: &str) -> ActorRef {
        ActorRef {
            kind: ActorKind::Agent,
            id: id.to_string(),
        }
    }

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| (*part).to_string()).collect()
    }

    /// A git repository holding one Ticket, so a fence can be taken. Mirrors
    /// `kernel::checkpoint`'s fixture.
    fn repo_with_ticket(status: &str, verify: Value, touches: Value) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(std::process::Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .status()
                .unwrap()
                .success());
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "test"]);
        std::fs::write(dir.path().join("README.md"), "x\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "init"]);
        crate::store::issues::mutate(dir.path(), |mut records| {
            let mut record = json!({
                "schema": 3, "id": "TK-a3f9", "kind": "ticket", "title": "t",
                "status": status, "revision": 1,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "role": "implementation",
            });
            if !verify.is_null() {
                record["verify"] = verify;
            }
            if !touches.is_null() {
                record["touches"] = touches;
            }
            records.push(record);
            Ok(records)
        })
        .unwrap();
        dir
    }

    fn verify_receipt(repo: &Path) -> crate::evidence::receipt::ReceiptEnvelope {
        crate::evidence::receipt::list_receipts(repo)
            .unwrap()
            .receipts
            .into_iter()
            .find(|receipt| receipt.kind == "verify")
            .expect("a verify receipt was sealed")
    }

    #[test]
    fn a_command_that_exits_zero_is_observed_as_zero() {
        let repo = tempfile::tempdir().unwrap();
        let observed =
            run_argv(repo.path(), &argv(&["true"]), None, Duration::from_secs(30)).unwrap();
        assert_eq!(observed.exit, Some(0));
        assert!(!observed.timed_out);
        assert!(observed.log.is_empty(), "{:?}", observed.log);
    }

    #[test]
    fn a_command_that_exits_nonzero_reports_its_code() {
        let repo = tempfile::tempdir().unwrap();
        let observed = run_argv(
            repo.path(),
            &argv(&["false"]),
            None,
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(observed.exit, Some(1));
        assert!(!observed.timed_out);
    }

    #[test]
    fn a_command_that_cannot_spawn_does_not_break_the_next_one() {
        let repo = tempfile::tempdir().unwrap();
        let missing = run_argv(
            repo.path(),
            &argv(&["pulse-verify-nonexistent-program"]),
            None,
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(missing.exit, None);
        assert!(!missing.timed_out, "a spawn failure is not a timeout");
        assert!(missing.log.contains("spawn failed"), "{}", missing.log);

        let after = run_argv(repo.path(), &argv(&["true"]), None, Duration::from_secs(30)).unwrap();
        assert_eq!(after.exit, Some(0));
    }

    #[test]
    fn a_command_that_outlives_the_timeout_is_killed() {
        let repo = tempfile::tempdir().unwrap();
        let started = Instant::now();
        let observed = run_argv(
            repo.path(),
            &argv(&["sleep", "5"]),
            None,
            Duration::from_secs(1),
        )
        .unwrap();
        assert!(observed.timed_out, "{observed:?}");
        assert_eq!(observed.exit, None);
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "kill must not wait out the command: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn an_empty_argv_is_rejected() {
        let repo = tempfile::tempdir().unwrap();
        let error = run_argv(repo.path(), &[], None, Duration::from_secs(30)).unwrap_err();
        assert_eq!(error.code(), "verify_argv_invalid");
    }

    #[test]
    fn a_cwd_outside_the_repository_is_rejected() {
        let repo = tempfile::tempdir().unwrap();
        let error = run_argv(
            repo.path(),
            &argv(&["true"]),
            Some("../.."),
            Duration::from_secs(30),
        )
        .unwrap_err();
        assert_eq!(error.code(), "verify_argv_invalid");
    }

    #[test]
    fn a_cwd_that_is_not_a_directory_is_rejected() {
        let repo = tempfile::tempdir().unwrap();
        let error = run_argv(
            repo.path(),
            &argv(&["true"]),
            Some("does/not/exist"),
            Duration::from_secs(30),
        )
        .unwrap_err();
        assert_eq!(error.code(), "verify_argv_invalid");
    }

    #[test]
    fn a_large_output_does_not_deadlock_and_the_log_keeps_the_tail() {
        // >64 KiB is past a pipe buffer: with pipes this would deadlock
        // before the parent ever called `wait`.
        let repo = tempfile::tempdir().unwrap();
        let observed = run_argv(
            repo.path(),
            &argv(&[
                "sh",
                "-c",
                "printf 'START'; head -c 204800 /dev/zero | tr '\\0' 'x'; printf 'END'",
            ]),
            None,
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(observed.exit, Some(0));
        assert!(observed.log.ends_with("END"), "the tail must survive");
        assert!(!observed.log.contains("START"), "the head must be dropped");
        assert!(
            observed.log.len() <= LOG_TAIL_BYTES,
            "the log is bounded: {} bytes",
            observed.log.len()
        );
    }

    #[test]
    fn a_log_that_starts_with_an_absolute_path_is_withheld() {
        // The tracked-plane boundary treats a value that begins with `/` as a
        // path and refuses it when the whole value does not canonicalize as
        // one — and a log trailing a newline never does. So a command that
        // logs absolute paths (a plain `pwd`, a file listing) is withheld
        // rather than rewritten. Exit codes survive, which is what the gate
        // reads; the log is the casualty. Pinned here so a change to that
        // trade-off is deliberate.
        let repo = tempfile::tempdir().unwrap();
        let observed = run_argv(
            repo.path(),
            &argv(&["sh", "-c", "echo /etc/hosts"]),
            None,
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(observed.exit, Some(0));
        assert_eq!(observed.log, "<log withheld: redaction refused it>");
    }

    #[test]
    fn a_log_that_trips_redaction_is_withheld_and_the_exit_code_survives() {
        // The secret is assembled by the shell so the *argv* stays clean —
        // an argv carrying a secret would be refused when the receipt is
        // sealed, which is a different rule.
        let repo = tempfile::tempdir().unwrap();
        let observed = run_argv(
            repo.path(),
            &argv(&["sh", "-c", "printf 'AKIA'; printf 'IOSFODNN7EXAMPLE'"]),
            None,
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(observed.exit, Some(0), "redaction must not eat the result");
        assert_eq!(observed.log, "<log withheld: redaction refused it>");
    }

    #[test]
    fn verify_runs_every_declared_command_and_seals_a_receipt() {
        let repo = repo_with_ticket(
            "active",
            json!([
                {"name": "one", "argv": ["sh", "-c", "echo first"]},
                {"name": "two", "argv": ["true"]},
            ]),
            Value::Null,
        );
        let report = verify(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(report["passed"], true, "{report}");
        assert_eq!(report["results"].as_array().unwrap().len(), 2);

        let receipt = verify_receipt(repo.path());
        assert_eq!(receipt.subject.id, "TK-a3f9");
        assert_eq!(receipt.payload["passed"], true);
        assert_eq!(receipt.artifacts.len(), 2);
    }

    #[test]
    fn a_failing_command_is_reported_but_the_receipt_is_still_sealed() {
        let repo = repo_with_ticket(
            "active",
            json!([
                {"name": "bad", "argv": ["false"]},
                {"name": "good", "argv": ["true"]},
            ]),
            Value::Null,
        );
        let report = verify(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(report["passed"], false, "{report}");
        // "Report everything": the second command still ran.
        assert_eq!(report["results"].as_array().unwrap().len(), 2);
        assert_eq!(report["results"][1]["exit"], 0);
        // A failed verify is evidence, not something swallowed.
        assert_eq!(verify_receipt(repo.path()).payload["passed"], false);
    }

    #[test]
    fn a_declared_command_that_cannot_spawn_does_not_hide_the_next_one() {
        let repo = repo_with_ticket(
            "active",
            json!([
                {"name": "missing", "argv": ["pulse-verify-nonexistent-program"]},
                {"name": "ok", "argv": ["true"]},
            ]),
            Value::Null,
        );
        let report = verify(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(report["passed"], false);
        assert_eq!(report["results"][0]["exit"], Value::Null);
        assert_eq!(report["results"][1]["exit"], 0);
        let log = std::fs::read_to_string(
            repo.path()
                .join(".pulse/evidence/TK-a3f9/verify/missing.log"),
        )
        .unwrap();
        assert!(log.contains("spawn failed"), "{log}");
    }

    #[test]
    fn the_receipt_hashes_the_log_file_it_wrote() {
        let repo = repo_with_ticket(
            "active",
            json!([{"name": "ok", "argv": ["sh", "-c", "echo hello-log"]}]),
            Value::Null,
        );
        verify(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            Duration::from_secs(30),
        )
        .unwrap();
        let receipt = verify_receipt(repo.path());
        assert_eq!(receipt.artifacts.len(), 1);
        let artifact = &receipt.artifacts[0];
        assert_eq!(artifact.path, ".pulse/evidence/TK-a3f9/verify/ok.log");
        let bytes = std::fs::read(repo.path().join(&artifact.path)).unwrap();
        assert_eq!(
            artifact.sha256,
            hash_bytes(&bytes).trim_start_matches("sha256:")
        );
        assert!(String::from_utf8_lossy(&bytes).contains("hello-log"));
    }

    #[test]
    fn the_receipt_fence_is_the_scope_hash_when_the_ticket_has_touches() {
        let repo = repo_with_ticket(
            "active",
            json!([{"name": "ok", "argv": ["true"]}]),
            json!(["README.md"]),
        );
        verify(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            Duration::from_secs(30),
        )
        .unwrap();
        let receipt = verify_receipt(repo.path());
        assert!(
            receipt.source.dirty_hash.starts_with("scope:sha256:"),
            "{:?}",
            receipt.source
        );
    }

    #[test]
    fn a_ticket_with_no_declared_verify_is_refused() {
        let repo = repo_with_ticket("active", Value::Null, Value::Null);
        let error = verify(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            Duration::from_secs(30),
        )
        .unwrap_err();
        assert_eq!(error.code(), "verify_nothing_declared");
        assert!(error.hint().is_some());
    }

    #[test]
    fn a_ticket_that_is_not_active_or_verifying_is_refused() {
        let repo = repo_with_ticket(
            "ready",
            json!([{"name": "ok", "argv": ["true"]}]),
            Value::Null,
        );
        let error = verify(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            Duration::from_secs(30),
        )
        .unwrap_err();
        assert_eq!(error.code(), "verify_not_runnable");
        assert!(error.hint().is_some());
    }

    #[test]
    fn a_verifying_ticket_is_still_runnable() {
        let repo = repo_with_ticket(
            "verifying",
            json!([{"name": "ok", "argv": ["true"]}]),
            Value::Null,
        );
        let report = verify(
            repo.path(),
            &agent("review-correctness"),
            "TK-a3f9",
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(report["passed"], true);
    }

    #[test]
    fn an_unsafe_log_name_is_refused_before_anything_runs() {
        let repo = repo_with_ticket(
            "active",
            json!([{"name": "../escape", "argv": ["sh", "-c", "echo ran > marker"]}]),
            Value::Null,
        );
        let error = verify(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            Duration::from_secs(30),
        )
        .unwrap_err();
        assert_eq!(error.code(), "verify_argv_invalid");
        // Nothing ran, so nothing was written.
        assert!(!repo.path().join("marker").exists());
        assert!(!repo.path().join(".pulse/evidence/TK-a3f9/verify").exists());
    }

    #[test]
    fn two_entries_may_not_share_a_log_name() {
        let repo = repo_with_ticket(
            "active",
            json!([
                {"name": "same", "argv": ["true"]},
                {"name": "same", "argv": ["false"]},
            ]),
            Value::Null,
        );
        let error = verify(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            Duration::from_secs(30),
        )
        .unwrap_err();
        assert_eq!(error.code(), "verify_argv_invalid");
    }

    #[test]
    fn every_verified_result_names_a_log_path_and_the_event_recorded_it() {
        let repo = repo_with_ticket(
            "active",
            json!([{"name": "ok", "argv": ["true"]}]),
            Value::Null,
        );
        let report = verify(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(report["receipt"], verify_receipt(repo.path()).id);
        let events = crate::event::read_events(repo.path()).unwrap();
        let recorded = events
            .iter()
            .find(|event| event.event_type == "verify.recorded")
            .expect("verify.recorded was emitted");
        assert_eq!(recorded.subject.id, "TK-a3f9");
        assert_eq!(recorded.payload["passed"], true);
    }

    // --- Plan 0025 E2: active learning checks are enforced through verify ---

    /// Seed the ticket with a tag and write one learning carrying that tag.
    fn with_learning(
        repo: &tempfile::TempDir,
        learning_id: &str,
        status: &str,
        check_argv: &[&str],
    ) {
        crate::store::issues::mutate(repo.path(), |mut records| {
            let ticket = records
                .iter_mut()
                .find(|record| record["id"] == "TK-a3f9")
                .unwrap();
            ticket["tags"] = json!(["auth"]);
            Ok(records)
        })
        .unwrap();
        crate::learn::store::write(
            repo.path(),
            &crate::learn::store::Learning {
                frontmatter: crate::learn::store::Frontmatter {
                    id: learning_id.to_string(),
                    status: status.to_string(),
                    kind: "failure".to_string(),
                    applies_to: vec![],
                    tags: vec!["auth".to_string()],
                    from: vec![],
                    expected_signal: String::new(),
                    usage: crate::learn::store::UsageCounts::default(),
                    check_argv: check_argv.iter().map(|s| s.to_string()).collect(),
                    check_cwd: None,
                    cites: vec![],
                },
                body: "## Summary\ns\n".to_string(),
            },
        )
        .unwrap();
    }

    #[test]
    fn an_active_learning_check_runs_under_the_learning_name() {
        let repo = repo_with_ticket("active", Value::Null, Value::Null);
        with_learning(&repo, "LRN-1111", "active", &["false"]);
        let report = verify(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(report["passed"], false, "{report}");
        let results = report["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["name"], "learning.LRN-1111");
        assert_eq!(results[0]["source"], "learning");
        assert_eq!(results[0]["exit"], 1);
        // The receipt names it too, so the handoff gate can say which check.
        assert_eq!(
            verify_receipt(repo.path()).payload["results"][0]["name"],
            "learning.LRN-1111"
        );
    }

    #[test]
    fn a_candidate_learning_check_never_runs() {
        // Decision 0026's boundary: only argv a human put in place runs. A
        // candidate has never been through `pulse learn activate`.
        let repo = repo_with_ticket("active", Value::Null, Value::Null);
        with_learning(&repo, "LRN-2222", "candidate", &["false"]);
        let error = verify(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            Duration::from_secs(30),
        )
        .unwrap_err();
        assert_eq!(error.code(), "verify_nothing_declared");
    }

    #[test]
    fn a_passing_learning_check_counts_as_passed_evidence() {
        let repo = repo_with_ticket("active", Value::Null, Value::Null);
        with_learning(&repo, "LRN-3333", "active", &["true"]);
        let report = verify(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(report["passed"], true);
        assert_eq!(report["results"][0]["source"], "learning");
    }

    #[test]
    fn a_learning_that_matches_nothing_does_not_run() {
        let repo = repo_with_ticket("active", Value::Null, Value::Null);
        with_learning(&repo, "LRN-4444", "active", &["false"]);
        // Strip the tag again: no match, nothing to run.
        crate::store::issues::mutate(repo.path(), |mut records| {
            let ticket = records
                .iter_mut()
                .find(|record| record["id"] == "TK-a3f9")
                .unwrap();
            ticket["tags"] = json!([]);
            Ok(records)
        })
        .unwrap();
        let error = verify(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            Duration::from_secs(30),
        )
        .unwrap_err();
        assert_eq!(error.code(), "verify_nothing_declared");
    }

    #[test]
    fn more_than_five_matching_learning_checks_all_run() {
        // RECALL_LIMIT bounds the packet, not enforcement (plan 0025 E2):
        // every matching check is evidence, so none may be silently dropped.
        let repo = repo_with_ticket("active", Value::Null, Value::Null);
        with_learning(&repo, "LRN-0001", "active", &["true"]);
        for index in 2..=6 {
            let id = format!("LRN-{index:04}");
            with_learning(&repo, &id, "active", &["true"]);
        }
        let report = verify(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            Duration::from_secs(30),
        )
        .unwrap();
        let results = report["results"].as_array().unwrap();
        assert_eq!(results.len(), 6, "all six run, not just the packet's five");
        assert!(results.iter().all(|r| r["source"] == "learning"));
    }

    #[test]
    fn a_ticket_name_may_not_collide_with_a_learning_name() {
        let repo = repo_with_ticket(
            "active",
            json!([{"name": "learning.LRN-1111", "argv": ["true"]}]),
            Value::Null,
        );
        with_learning(&repo, "LRN-1111", "active", &["true"]);
        let error = verify(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            Duration::from_secs(30),
        )
        .unwrap_err();
        assert_eq!(error.code(), "verify_argv_invalid");
    }

    #[test]
    fn required_names_unions_ticket_and_learning_names() {
        let repo = repo_with_ticket(
            "active",
            json!([{"name": "unit", "argv": ["true"]}]),
            Value::Null,
        );
        with_learning(&repo, "LRN-1111", "active", &["true"]);
        let records = crate::store::issues::read_all(repo.path()).unwrap();
        let ticket = records
            .iter()
            .find(|record| record["id"] == "TK-a3f9")
            .unwrap();
        let mut names = required_names(repo.path(), ticket).unwrap();
        names.sort();
        assert_eq!(names, vec!["learning.LRN-1111", "unit"]);
    }
}
