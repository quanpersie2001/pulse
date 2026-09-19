//! CLI adapter for the pre-edit gate (plan 0025 G1).
//!
//! Exit codes are the contract with the host, not a style choice:
//!
//! * allow → exit `0`, nothing printed — the host lets the edit through;
//! * deny → exit `2`, the reason on STDERR — Claude Code feeds a
//!   PreToolUse hook's stderr back to the editing agent on exit 2, which
//!   is exactly the loop a reservation wants (`pulse reserve` or stop);
//! * internal failure (torn store, broken `PULSE.md`, malformed stdin
//!   JSON) → exit `1` through the ordinary error path — a torn store must
//!   never lock every edit of a working repository, and `pulse doctor` is
//!   the place a torn store is reported. Only a `Deny` is a deliberate
//!   decision.
//!
//! The snippet subcommand only PRINTS host configuration to stdout. Pulse
//! never writes into `~/.claude`, `~/.codex` or any host settings file —
//! pasting it is the user's informed act. Only hosts whose hook shape and
//! payload keys were verified against `references/repo-harness` are
//! offered; anything else is [`PulseError::kernel`] `hook_host_unknown`
//! rather than an invented configuration.

use std::io::Read;
use std::path::Path;

use clap::Subcommand;
use serde_json::Value;

use crate::error::{PulseError, Result};
use crate::identity::actor::{parse_actor, ActorRef};
use crate::kernel::hook::{self, EditDecision};

#[derive(Subcommand)]
pub(crate) enum HookCommand {
    /// Decide whether one file edit may proceed. Allow exits 0 silently;
    /// a denial exits 2 with the reason on stderr.
    PreEdit {
        /// File about to be edited, repo-relative or absolute. Repeatable.
        #[arg(long)]
        path: Vec<String>,
        /// Read ONE JSON object from stdin and take the edit path(s) from
        /// the keys a host hook really sends (`tool_input.file_path`,
        /// Claude Code's PreToolUse payload; `*** Add/Update/Delete File:`
        /// lines in `tool_input.command`, Codex's apply-patch form).
        /// No path in the payload — the tool is not a file edit — allows.
        #[arg(long)]
        stdin_json: bool,
        /// Self-declared caller (kind:id). Falls back to `PULSE_ACTOR`,
        /// never to `git config user.name`: a hook that guessed "human"
        /// would make the actor rules meaningless.
        #[arg(long)]
        actor: Option<String>,
    },
    /// Print the hook configuration for `<host>` to stdout, for pasting
    /// into the host's settings. Never writes any file.
    Snippet {
        /// Which host's configuration to print (`claude`).
        host: String,
    },
}

pub(crate) fn handle(repo_root: &Path, command: HookCommand) -> Result<()> {
    match command {
        HookCommand::PreEdit {
            path,
            stdin_json,
            actor,
        } => handle_pre_edit(repo_root, path, stdin_json, actor),
        HookCommand::Snippet { host } => handle_snippet(&host),
    }
}

fn handle_pre_edit(
    repo_root: &Path,
    paths: Vec<String>,
    stdin_json: bool,
    actor: Option<String>,
) -> Result<()> {
    let actor = resolve_hook_actor(actor)?;
    let mut paths = paths;
    if stdin_json {
        paths.extend(payload_paths(&read_stdin_json()?)?);
    }
    // A payload with no file path is not an edit (a Bash command, a web
    // search): allow without printing anything.
    if paths.is_empty() {
        return Ok(());
    }
    for path in &paths {
        match hook::pre_edit(repo_root, path, actor.as_ref())? {
            EditDecision::Allow { .. } => {}
            EditDecision::Deny { message } => {
                // The deny contract is a distinct exit code (see the
                // module `//!`), so the host shows the reason to the agent.
                eprintln!("{message}");
                std::process::exit(2);
            }
        }
    }
    Ok(())
}

/// `--actor` wins, then `PULSE_ACTOR`, then nothing. Deliberately NOT
/// `identity::actor::resolve_actor`: its last fallback is
/// `git config user.name`, and a hook that promoted a repo setting to
/// "the human is editing" would make every actor rule in the gate mean
/// nothing (plan 0025 G1).
fn resolve_hook_actor(explicit: Option<String>) -> Result<Option<ActorRef>> {
    if let Some(raw) = explicit {
        return Ok(Some(parse_actor(&raw)?));
    }
    if let Ok(raw) = std::env::var("PULSE_ACTOR") {
        if !raw.trim().is_empty() {
            return Ok(Some(parse_actor(raw.trim())?));
        }
    }
    Ok(None)
}

fn read_stdin_json() -> Result<Value> {
    let mut raw = String::new();
    std::io::stdin()
        .read_to_string(&mut raw)
        .map_err(|error| PulseError::validation("json_error", error.to_string()))?;
    serde_json::from_str(&raw)
        .map_err(|error| PulseError::validation("json_error", error.to_string()))
}

/// The path keys host hooks really send, verified against
/// `references/repo-harness/src/cli/hook/hook-input.ts` (plan 0025 G1):
/// Claude Code puts the file in `tool_input.file_path` (Edit/Write/
/// MultiEdit) or `tool_input.notebook_path` (NotebookEdit); Codex puts
/// apply-patch `*** Add/Update/Delete File:` (and `*** Move to:`) lines in
/// `tool_input.command`. The bare top-level spellings are the reference
/// harness's own fallbacks. A relative path resolves against the payload's
/// `cwd` (the session directory hosts report), then the process cwd.
fn payload_paths(payload: &Value) -> Result<Vec<String>> {
    let mut paths = Vec::new();
    let mut push = |value: Option<&str>| {
        if let Some(path) = value.map(str::trim).filter(|path| !path.is_empty()) {
            if !paths.iter().any(|seen| seen == path) {
                paths.push(path.to_string());
            }
        }
    };
    push(
        payload
            .pointer("/tool_input/file_path")
            .and_then(Value::as_str),
    );
    push(payload.pointer("/tool_input/path").and_then(Value::as_str));
    push(
        payload
            .pointer("/tool_input/notebook_path")
            .and_then(Value::as_str),
    );
    push(payload.get("file_path").and_then(Value::as_str));
    push(payload.get("notebook_path").and_then(Value::as_str));
    let cwd = payload
        .get("cwd")
        .and_then(Value::as_str)
        .map(std::path::PathBuf::from);
    if let Some(command) = payload
        .pointer("/tool_input/command")
        .and_then(Value::as_str)
    {
        for line in command.lines() {
            for marker in ["*** Add File: ", "*** Update File: ", "*** Delete File: "] {
                if let Some(path) = line.strip_prefix(marker) {
                    push(Some(path.trim()));
                }
            }
            if let Some(path) = line.strip_prefix("*** Move to: ") {
                push(Some(path.trim()));
            }
        }
    }
    if paths.iter().any(|path| !Path::new(path).is_absolute()) {
        // Relative spellings only make sense against a directory; the
        // payload's `cwd` is the host's answer, the process cwd the
        // fallback.
        let base = cwd.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        paths = paths
            .into_iter()
            .map(|path| {
                if Path::new(&path).is_absolute() {
                    path
                } else {
                    base.join(&path).to_string_lossy().into_owned()
                }
            })
            .collect();
    }
    Ok(paths)
}

/// The Claude Code PreToolUse entry, byte-for-byte the shape the reference
/// harness installs into `~/.claude/settings.json` /
/// `.claude/settings.json` (`{hooks: {PreToolUse: [{matcher, hooks:
/// [{type, command, timeout}]}]}}`). `pulse` must be on PATH: a missing
/// binary exits 127, which Claude Code treats as a non-blocking hook error
/// — the gate fails open, the doctor-of-last-resort stays
/// `handoff_unreserved_changes`.
const CLAUDE_SNIPPET: &str = r#"{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Edit|Write|MultiEdit|NotebookEdit",
        "hooks": [
          {
            "type": "command",
            "command": "pulse hook pre-edit --stdin-json",
            "timeout": 10
          }
        ]
      }
    ]
  }
}"#;

fn handle_snippet(host: &str) -> Result<()> {
    match host {
        "claude" => println!("{CLAUDE_SNIPPET}"),
        other => {
            return Err(PulseError::kernel(
                "hook_host_unknown",
                format!("no verified hook configuration for host \"{other}\""),
                "`pulse hook snippet claude` prints the PreToolUse JSON to paste into \
                 .claude/settings.json; other hosts are not verified yet, and Pulse \
                 does not invent configurations",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn payload_paths_reads_the_claude_code_keys() {
        let payload = json!({
            "session_id": "s", "tool_name": "Edit",
            "tool_input": {"file_path": "/repo/src/lib.rs", "old_string": "a"},
            "cwd": "/repo",
        });
        assert_eq!(payload_paths(&payload).unwrap(), vec!["/repo/src/lib.rs"]);
    }

    #[test]
    fn payload_paths_reads_the_codex_apply_patch_lines() {
        let payload = json!({
            "tool_name": "apply_patch",
            "tool_input": {"command": "*** Begin Patch\n*** Add File: src/new.rs\n+line\n*** Update File: src/old.rs\n*** Move to: src/renamed.rs\n*** End Patch"},
            "cwd": "/repo",
        });
        let paths = payload_paths(&payload).unwrap();
        // `base.join` spells paths with the platform separator; the parsing
        // is what this test pins, so compare in a separator-neutral form.
        let normalized: Vec<String> = paths.iter().map(|path| path.replace('\\', "/")).collect();
        assert_eq!(
            normalized,
            vec![
                "/repo/src/new.rs",
                "/repo/src/old.rs",
                "/repo/src/renamed.rs"
            ]
        );
    }

    #[test]
    fn a_payload_without_a_file_path_yields_nothing() {
        let payload = json!({
            "tool_name": "Bash",
            "tool_input": {"command": "cargo test"},
        });
        assert!(payload_paths(&payload).unwrap().is_empty());
    }

    #[test]
    fn a_relative_payload_path_resolves_against_the_payload_cwd() {
        let payload = json!({
            "tool_input": {"file_path": "src/lib.rs"},
            "cwd": "/repo/sub",
        });
        let paths = payload_paths(&payload).unwrap();
        let normalized: Vec<String> = paths.iter().map(|path| path.replace('\\', "/")).collect();
        assert_eq!(normalized, vec!["/repo/sub/src/lib.rs"]);
    }

    #[test]
    fn malformed_stdin_json_is_an_error_not_a_decision() {
        let err = serde_json::from_str::<Value>("{nope").unwrap_err();
        assert!(PulseError::validation("json_error", err.to_string()).code() == "json_error");
    }

    #[test]
    fn the_claude_snippet_is_valid_json_with_the_real_command() {
        let parsed: Value = serde_json::from_str(CLAUDE_SNIPPET).expect("snippet is valid JSON");
        let entry = &parsed["hooks"]["PreToolUse"][0];
        assert_eq!(entry["matcher"], "Edit|Write|MultiEdit|NotebookEdit");
        assert_eq!(entry["hooks"][0]["type"], "command");
        assert_eq!(
            entry["hooks"][0]["command"],
            "pulse hook pre-edit --stdin-json"
        );
    }
}
