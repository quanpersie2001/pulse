//! Repository enrollment (plan 0022 §14 P1.10 — full asset set for what
//! `pulse init` seeds; P1.3 shipped a minimal interim version of this same
//! function without the AGENTS block, docs map or host files).
//!
//! Creates the `.pulse/` tree, an empty `issues.jsonl`, a `runners.json`
//! seed wired to the prompt assets below (plan §8.2), a `PULSE.md` profile
//! seed (plan §8.1), the runtime/cache `.gitignore` entries (plan §3), the
//! Pulse block in `AGENTS.md` (plan §12.1), `docs/README.md` (plan §12.2)
//! and `.pulse/prompts/*.md` (plan §8.5) if any are missing. `refresh`
//! rewrites only the Pulse block region of `AGENTS.md` and overwrites the
//! prompt files (never the rest of `AGENTS.md`, and never an existing
//! `runners.json` a human may have already customized). `host` copies
//! host-specific detector files (plan §10.4) — `"claude-code"` is the only
//! one implemented. `with_qa_templates` copies `scripts/qa/{ui,api}.mjs`
//! and `scripts/qa/README.md` (plan §8.6), skipping (never overwriting) any
//! that already exist.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::Result;
use crate::storage::WriteGuard;
use crate::PulseError;

const DIRS: [&str; 7] = [
    ".pulse",
    ".pulse/receipts",
    ".pulse/evidence",
    ".pulse/events",
    ".pulse/learnings",
    ".pulse/prompts",
    ".pulse/runtime",
];

const GITIGNORE_ENTRIES: [&str; 2] = ["**/.pulse/runtime/", "**/.pulse/cache/"];

const PULSE_MD_SEED: &str = "\
# PULSE.md - seeded by `pulse init` (plan 0022 section 8.1). Human-editable.
# Each profile key below is <surface>-<risk> (a Ticket/Story's own
# `surface`/`risk` fields, e.g. a `ui` Ticket with `risk: medium` resolves
# `ui-medium`) except `decision_work`, which every `role: decision_work`
# Ticket uses regardless of its surface or risk.
fence_ignore: []
profiles:
  cli-low: {lanes: [review-correctness]}
  lib-low: {lanes: [review-correctness]}
  api-low: {lanes: [review-correctness]}
  ui-low: {lanes: [review-correctness, qa-ui]}
  api-medium: {lanes: [review-correctness, qa-api]}
  ui-medium: {lanes: [review-correctness, qa-ui]}
  api-high: {lanes: [review-correctness, review-adversarial, qa-api], human: required}
  ui-high: {lanes: [review-correctness, review-adversarial, qa-ui, qa-api], human: required}
  docs-low: {lanes: [check-docs]}
  decision_work: {lanes: []}
";

const DOCS_README_SEED: &str = "\
# Docs map

Hand-maintained index of durable docs, kept short on purpose.
`pulse docs applicable <id>` matches a Ticket's anchors/tags against
`applies_to`/`tags` frontmatter on the docs listed here; `pulse docs check`
finds broken links and stale generated sections, and confirms every path
listed below still exists.

- (add entries as `- path/to/doc.md` plus a one-line why)
- docs/operations/run.md — lane qa-* reads this file; format in
  scripts/qa/README.md
";

const RUNNERS_JSON_ROLES: &[(&str, &str, u64)] = &[
    (
        "worker",
        "claude -p --output-format text --dangerously-skip-permissions \"Read .pulse/prompts/worker.md then {input}\"",
        3600,
    ),
    (
        "worker-continue",
        "claude -p --output-format text --dangerously-skip-permissions \"Read .pulse/prompts/worker-continue.md then {input}\"",
        3600,
    ),
    (
        "review-correctness",
        "claude -p --output-format text \"Read .pulse/prompts/review-correctness.md then {input}\"",
        1800,
    ),
    (
        "review-adversarial",
        "claude -p --output-format text \"Read .pulse/prompts/review-adversarial.md then {input}\"",
        1800,
    ),
    (
        "check-docs",
        "pulse docs check --write {artifact_dir}/check-docs.json",
        120,
    ),
];

/// Only seeded when `--with-qa-templates` also copies the scripts these
/// commands point at (plan §8.6) — seeding them unconditionally would give
/// every plain `pulse init` a `runners.json` naming `scripts/qa/*.mjs`
/// files that don't exist.
const QA_RUNNERS_JSON_ROLES: &[(&str, &str, u64)] = &[
    ("qa-ui", "node scripts/qa/ui.mjs {input}", 900),
    ("qa-api", "node scripts/qa/api.mjs {input}", 900),
];

/// Plan §8.2's `runners.json` seed, built with `serde_json` rather than a
/// hand-escaped string literal (a command line already needs its own `"`
/// quoting — nesting that inside a Rust string *and* JSON by hand is a typo
/// magnet `json!` avoids). `worker`/`worker-continue`/`review-*` point at
/// the prompt files this module also seeds (plan §8.5); `check-docs` needs
/// no prompt or wrapper — `pulse docs check --write` already writes the
/// lane §8.4 shape directly (A3).
fn runners_json_seed(with_qa_templates: bool) -> serde_json::Value {
    let mut roles = serde_json::Map::new();
    for (role, command, timeout_seconds) in RUNNERS_JSON_ROLES {
        roles.insert(
            (*role).to_string(),
            json!({"command": command, "timeout_seconds": timeout_seconds}),
        );
    }
    if with_qa_templates {
        for (role, command, timeout_seconds) in QA_RUNNERS_JSON_ROLES {
            roles.insert(
                (*role).to_string(),
                json!({"command": command, "timeout_seconds": timeout_seconds}),
            );
        }
    }
    serde_json::Value::Object(roles)
}

const PROMPT_FILES: [(&str, &str); 4] = [
    ("worker.md", include_str!("../../assets/prompts/worker.md")),
    (
        "worker-continue.md",
        include_str!("../../assets/prompts/worker-continue.md"),
    ),
    (
        "review-correctness.md",
        include_str!("../../assets/prompts/review-correctness.md"),
    ),
    (
        "review-adversarial.md",
        include_str!("../../assets/prompts/review-adversarial.md"),
    ),
];

/// Writes each of [`PROMPT_FILES`] under `.pulse/prompts/` (plan §8.5):
/// missing ones are always written; existing ones only when `refresh`
/// (mirrors [`ensure_agents_block`]'s idempotent-unless-refresh contract,
/// applied per-file since a prompt file has no internal region markers to
/// preserve hand edits around).
fn ensure_prompts(repo_root: &Path, refresh: bool) -> Result<Vec<String>> {
    let dir = repo_root.join(".pulse/prompts");
    let mut created = Vec::new();
    for (name, body) in PROMPT_FILES {
        let path = dir.join(name);
        if path.exists() && !refresh {
            continue;
        }
        fs::write(&path, body).map_err(|error| PulseError::io(&path, error))?;
        created.push(format!(".pulse/prompts/{name}"));
    }
    Ok(created)
}

const AGENTS_BLOCK_BEGIN: &str = "<!-- PULSE:BEGIN -->";
const AGENTS_BLOCK_END: &str = "<!-- PULSE:END -->";

/// Plan §12.1: one paragraph on what Pulse is, the four questions before a
/// mutation, routing by shape, `done` as a gate not a claim, friction ->
/// `note --friction`, checkpoint-then-continue, a short command table. Kept
/// to what actually exists today — no skill-based routing, since
/// `pulse-shape`/`pulse-plan` aren't rebuilt until Phase 2/3.
const AGENTS_BLOCK_BODY: &str = "\
## Pulse

Pulse is the local CLI truth layer for work in this repository: one JSONL
store (`.pulse/issues.jsonl`) for Epic/Story/Ticket/Decision records,
receipts as the only proof of completion, an append-only event log, and a
friction -> learning -> check ratchet. Pulse does not run agents and has no
daemon.

Before any mutation, ask: does this id exist (`pulse work show <id>`)? what
is its current status? am I the actor allowed to change it? will this leave
`issues.jsonl` schema-valid?

Route by shape: a small, well-understood change is `pulse work new ticket
\"<title>\" --risk <low|medium|high> --surface <cli|api|ui|lib|docs>`, then
`pulse work ready <id>`, then `pulse run worker <id>` — the worker itself
reads `.pulse/prompts/worker.md` before `{input}`, so that contract is not
restated here. Anything bigger needs a Story first (`pulse work new story
...`), with an Epic above it if the work doesn't fit under an existing one.

`done` is never a claim, only a gate reading receipts: a Ticket goes
`verifying -> done` only through `pulse close`, after every lane in its
profile has a passing receipt on the handoff's commit.

Hit friction (a Pulse bug, an unclear doc, a missing check)? Record it:
`pulse note <id> \"<what happened>\" --friction` — don't work around it
silently. Learned something worth keeping from it (a failure, a
constraint, a technique)? `pulse learn add --title \"...\" --kind
<failure|constraint|technique|routing> --applies-to <glob>` records a
candidate; `pulse learn applicable <id>` shows what already applies to a
Ticket.

Need a doc before writing one? `pulse docs applicable <id>` shows which
docs match a Ticket's anchors/tags; `pulse docs check` finds broken links
and stale generated sections under `docs/`.

Context filling up mid-Ticket? `pulse checkpoint <id> --from <cp.json>`
recording what's done, what's next and any gotchas, then exit printing
exactly `{\"status\":\"continue\"}` — the runner resumes in a fresh process
with that checkpoint in the packet.

| Command | Does |
|---|---|
| `pulse work new <kind> <title>` | create a draft record |
| `pulse work show <id>` / `list` / `tree` | read records |
| `pulse work ready <id>` | run the ready gate |
| `pulse work update <id>` / `dep` / `transition` | edit a record |
| `pulse packet <id>` | the one input to read before working |
| `pulse run worker <id>` | dispatch the configured worker |
| `pulse checkpoint <id> --from <f>` | save progress mid-run |
| `pulse handoff <id> --from <f>` | hand off for review |
| `pulse run <lane> <id>` | run one review/qa lane |
| `pulse close <id>` / `close-story <id>` | the only way to `done` |
| `pulse note <id> <text> [--friction]` | append-only note |
| `pulse learn add` / `applicable <id>` | record / recall a learning |
| `pulse docs applicable <id>` / `check` | find relevant docs / doc rot |
";

const STATUSLINE_SH: &str = "#!/bin/sh\n\
# Pulse host detector for Claude Code (plan 0022 SS10.4).\n\
# Reads the statusline JSON payload on stdin; touches a marker once context\n\
# usage crosses 70% and a run is in progress, so post-tool-use.sh can tell\n\
# the agent to checkpoint and exit continue.\n\
payload=$(cat)\n\
pct=$(printf '%s' \"$payload\" | sed -n 's/.*\"used_percentage\"[: ]*\\([0-9]*\\).*/\\1/p')\n\
if [ -n \"$pct\" ] && [ \"$pct\" -ge 70 ] 2>/dev/null && [ -e .pulse/runtime/run/current ]; then\n\
  touch .pulse/runtime/context-threshold\n\
fi\n";

const POST_TOOL_USE_SH: &str = "#!/bin/sh\n\
# Pulse host detector for Claude Code (plan 0022 SS10.4).\n\
if [ -e .pulse/runtime/context-threshold ]; then\n\
  rm -f .pulse/runtime/context-threshold\n\
  printf '%s\\n' '{\"decision\":\"continue\",\"reason\":\"Context >=70%%: pulse checkpoint then exit {\\\"status\\\":\\\"continue\\\"}\"}'\n\
fi\n";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryInitStatus {
    Initialized,
    Unchanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryInitReport {
    pub schema_version: u32,
    pub status: RepositoryInitStatus,
    pub created: Vec<String>,
    /// Files `--with-qa-templates` left alone because they already existed
    /// (plan §8.6: "không ghi đè file đã có, báo skipped") — unlike the
    /// AGENTS block and prompts, a QA template is a one-time starting point
    /// a target repo is expected to edit, so it is never refreshed either.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub host_settings_snippet: Vec<String>,
}

/// # Errors
/// Propagates an I/O error creating a directory or file, or a lock timeout
/// acquiring the repository write lock.
pub fn initialize_repository(
    repo_root: &Path,
    refresh: bool,
    host: Option<&str>,
    with_qa_templates: bool,
) -> Result<RepositoryInitReport> {
    let _guard = WriteGuard::acquire(repo_root)?;
    let mut created = Vec::new();

    for dir in DIRS {
        let path = repo_root.join(dir);
        if !path.exists() {
            fs::create_dir_all(&path).map_err(|error| PulseError::io(&path, error))?;
            created.push(dir.to_string());
        }
    }

    let issues_path = crate::store::issues::issues_path(repo_root);
    if !issues_path.exists() {
        fs::write(&issues_path, b"").map_err(|error| PulseError::io(&issues_path, error))?;
        created.push(".pulse/issues.jsonl".to_string());
    }

    let runners_path = repo_root.join(".pulse/runners.json");
    if !runners_path.exists() {
        let bytes = serde_json::to_vec_pretty(&runners_json_seed(with_qa_templates))?;
        fs::write(&runners_path, bytes).map_err(|error| PulseError::io(&runners_path, error))?;
        created.push(".pulse/runners.json".to_string());
    }

    let pulse_md_path = repo_root.join("PULSE.md");
    if !pulse_md_path.exists() {
        fs::write(&pulse_md_path, PULSE_MD_SEED)
            .map_err(|error| PulseError::io(&pulse_md_path, error))?;
        created.push("PULSE.md".to_string());
    }

    let docs_readme_path = repo_root.join("docs/README.md");
    if !docs_readme_path.exists() {
        fs::create_dir_all(repo_root.join("docs"))
            .map_err(|error| PulseError::io(repo_root.join("docs"), error))?;
        fs::write(&docs_readme_path, DOCS_README_SEED)
            .map_err(|error| PulseError::io(&docs_readme_path, error))?;
        created.push("docs/README.md".to_string());
    }

    if ensure_agents_block(repo_root, refresh)? {
        created.push("AGENTS.md".to_string());
    }

    created.extend(ensure_prompts(repo_root, refresh)?);

    created.extend(ensure_gitignore_entries(repo_root)?);

    let mut skipped = Vec::new();
    if with_qa_templates {
        let (qa_created, qa_skipped) = copy_qa_templates(repo_root)?;
        created.extend(qa_created);
        skipped.extend(qa_skipped);
    }

    let mut host_settings_snippet = Vec::new();
    if let Some(host) = host {
        if host == "claude-code" {
            created.extend(write_claude_code_host_files(repo_root)?);
            host_settings_snippet = claude_code_settings_snippet();
        } else {
            return Err(PulseError::kernel(
                "host_unsupported",
                format!("--host {host} is not implemented"),
                "the only implemented host is claude-code",
            ));
        }
    }

    let status = if created.is_empty() {
        RepositoryInitStatus::Unchanged
    } else {
        RepositoryInitStatus::Initialized
    };
    Ok(RepositoryInitReport {
        schema_version: 1,
        status,
        created,
        skipped,
        host_settings_snippet,
    })
}

const QA_TEMPLATE_FILES: [(&str, &str); 3] = [
    ("ui.mjs", include_str!("../../assets/qa/ui.mjs")),
    ("api.mjs", include_str!("../../assets/qa/api.mjs")),
    ("README.md", include_str!("../../assets/qa/README.md")),
];

/// `pulse init --with-qa-templates` (plan §8.6): copies the QA lane scripts
/// into `scripts/qa/` of the target repo. Never overwrites an existing
/// file — these are starting points a repo is expected to edit once copied,
/// not a region Pulse keeps refreshing like the AGENTS block or prompts.
/// Returns `(created, skipped)`.
fn copy_qa_templates(repo_root: &Path) -> Result<(Vec<String>, Vec<String>)> {
    let dir = repo_root.join("scripts/qa");
    fs::create_dir_all(&dir).map_err(|error| PulseError::io(&dir, error))?;
    let mut created = Vec::new();
    let mut skipped = Vec::new();
    for (name, body) in QA_TEMPLATE_FILES {
        let path = dir.join(name);
        let label = format!("scripts/qa/{name}");
        if path.exists() {
            skipped.push(label);
            continue;
        }
        fs::write(&path, body).map_err(|error| PulseError::io(&path, error))?;
        created.push(label);
    }
    Ok((created, skipped))
}

/// Writes or refreshes the Pulse block in `AGENTS.md`. Returns whether it
/// wrote anything. A first write creates `AGENTS.md` with just the block;
/// an existing file without markers gets the block appended; `refresh`
/// rewrites only the region between the markers, leaving everything else
/// in the file untouched.
fn ensure_agents_block(repo_root: &Path, refresh: bool) -> Result<bool> {
    let path = repo_root.join("AGENTS.md");
    let existing = fs::read_to_string(&path).unwrap_or_default();

    let block = format!("{AGENTS_BLOCK_BEGIN}\n{AGENTS_BLOCK_BODY}{AGENTS_BLOCK_END}\n");

    let has_markers = existing.contains(AGENTS_BLOCK_BEGIN) && existing.contains(AGENTS_BLOCK_END);
    if has_markers {
        if !refresh {
            return Ok(false);
        }
        let before = existing
            .split(AGENTS_BLOCK_BEGIN)
            .next()
            .unwrap_or_default();
        let after = existing.split(AGENTS_BLOCK_END).nth(1).unwrap_or_default();
        let updated = format!("{before}{block}{after}");
        fs::write(&path, updated).map_err(|error| PulseError::io(&path, error))?;
        return Ok(true);
    }

    let updated = if existing.is_empty() {
        block
    } else {
        let mut updated = existing;
        if !updated.ends_with('\n') {
            updated.push('\n');
        }
        updated.push('\n');
        updated.push_str(&block);
        updated
    };
    fs::write(&path, updated).map_err(|error| PulseError::io(&path, error))?;
    Ok(true)
}

fn ensure_gitignore_entries(repo_root: &Path) -> Result<Vec<String>> {
    let path = repo_root.join(".gitignore");
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let missing: Vec<&str> = GITIGNORE_ENTRIES
        .into_iter()
        .filter(|entry| !existing.lines().any(|line| line.trim() == *entry))
        .collect();
    if missing.is_empty() {
        return Ok(Vec::new());
    }

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    for entry in &missing {
        updated.push_str(entry);
        updated.push('\n');
    }
    fs::write(&path, updated).map_err(|error| PulseError::io(&path, error))?;
    Ok(missing
        .into_iter()
        .map(|entry| format!(".gitignore: {entry}"))
        .collect())
}

fn write_claude_code_host_files(repo_root: &Path) -> Result<Vec<String>> {
    let dir = repo_root.join("assets/hosts/claude-code");
    fs::create_dir_all(&dir).map_err(|error| PulseError::io(&dir, error))?;
    let mut written = Vec::new();
    for (name, body) in [
        ("statusline.sh", STATUSLINE_SH),
        ("post-tool-use.sh", POST_TOOL_USE_SH),
    ] {
        let path = dir.join(name);
        if !path.exists() {
            fs::write(&path, body).map_err(|error| PulseError::io(&path, error))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o755));
            }
            written.push(format!("assets/hosts/claude-code/{name}"));
        }
    }
    Ok(written)
}

fn claude_code_settings_snippet() -> Vec<String> {
    vec![
        "statusLine: assets/hosts/claude-code/statusline.sh".to_string(),
        "hooks.PostToolUse: assets/hosts/claude-code/post-tool-use.sh".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_run_creates_everything_and_reports_initialized() {
        let repo = tempfile::tempdir().unwrap();
        let report = initialize_repository(repo.path(), false, None, false).unwrap();
        assert_eq!(report.status, RepositoryInitStatus::Initialized);
        assert!(repo.path().join(".pulse/issues.jsonl").exists());
        assert!(repo.path().join(".pulse/runners.json").exists());
        assert!(repo.path().join("PULSE.md").exists());
        assert!(repo.path().join("docs/README.md").exists());
        let agents = fs::read_to_string(repo.path().join("AGENTS.md")).unwrap();
        assert!(agents.contains(AGENTS_BLOCK_BEGIN));
        assert!(agents.contains("pulse work new"));
        let gitignore = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
        for entry in GITIGNORE_ENTRIES {
            assert!(gitignore.contains(entry));
        }
        for (name, _) in PROMPT_FILES {
            assert!(
                repo.path().join(".pulse/prompts").join(name).exists(),
                "missing prompt {name}"
            );
        }
    }

    #[test]
    fn runners_json_seed_points_every_role_at_its_prompt_or_a_wrapper_free_command() {
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, None, false).unwrap();
        let runners: serde_json::Value =
            serde_json::from_slice(&fs::read(repo.path().join(".pulse/runners.json")).unwrap())
                .unwrap();
        for (role, _, _) in RUNNERS_JSON_ROLES {
            assert!(runners.get(role).is_some(), "missing role {role}");
        }
        assert!(runners["worker"]["command"]
            .as_str()
            .unwrap()
            .contains(".pulse/prompts/worker.md"));
        assert!(runners["check-docs"]["command"]
            .as_str()
            .unwrap()
            .contains("pulse docs check --write"));
    }

    #[test]
    fn an_existing_runners_json_is_never_overwritten() {
        let repo = tempfile::tempdir().unwrap();
        fs::create_dir_all(repo.path().join(".pulse")).unwrap();
        fs::write(
            repo.path().join(".pulse/runners.json"),
            "{\"custom\": true}\n",
        )
        .unwrap();
        initialize_repository(repo.path(), true, None, false).unwrap();
        let text = fs::read_to_string(repo.path().join(".pulse/runners.json")).unwrap();
        assert!(text.contains("custom"));
    }

    #[test]
    fn prompts_are_written_once_and_left_alone_without_refresh() {
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, None, false).unwrap();
        let worker_path = repo.path().join(".pulse/prompts/worker.md");
        fs::write(&worker_path, "hand edited\n").unwrap();

        let report = initialize_repository(repo.path(), false, None, false).unwrap();
        assert_eq!(report.status, RepositoryInitStatus::Unchanged);
        assert_eq!(fs::read_to_string(&worker_path).unwrap(), "hand edited\n");
    }

    #[test]
    fn refresh_overwrites_prompt_files() {
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, None, false).unwrap();
        let worker_path = repo.path().join(".pulse/prompts/worker.md");
        fs::write(&worker_path, "stale\n").unwrap();

        let report = initialize_repository(repo.path(), true, None, false).unwrap();
        assert_eq!(report.status, RepositoryInitStatus::Initialized);
        let text = fs::read_to_string(&worker_path).unwrap();
        assert!(text.starts_with("# Pulse worker"));
        assert!(!text.contains("stale"));
    }

    #[test]
    fn second_run_is_idempotent_and_reports_unchanged() {
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, None, false).unwrap();
        let report = initialize_repository(repo.path(), false, None, false).unwrap();
        assert_eq!(report.status, RepositoryInitStatus::Unchanged);
        assert!(report.created.is_empty());
    }

    #[test]
    fn preserves_a_hand_edited_gitignore_and_only_appends_missing_entries() {
        let repo = tempfile::tempdir().unwrap();
        fs::write(repo.path().join(".gitignore"), "node_modules/\n").unwrap();
        initialize_repository(repo.path(), false, None, false).unwrap();
        let gitignore = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
        assert!(gitignore.contains("node_modules/"));
        for entry in GITIGNORE_ENTRIES {
            assert!(gitignore.contains(entry));
        }
    }

    #[test]
    fn agents_block_is_appended_after_existing_content_and_preserves_it() {
        let repo = tempfile::tempdir().unwrap();
        fs::write(
            repo.path().join("AGENTS.md"),
            "# My repo rules\n\nBe nice.\n",
        )
        .unwrap();
        initialize_repository(repo.path(), false, None, false).unwrap();
        let agents = fs::read_to_string(repo.path().join("AGENTS.md")).unwrap();
        assert!(agents.starts_with("# My repo rules\n\nBe nice.\n"));
        assert!(agents.contains(AGENTS_BLOCK_BEGIN));
    }

    #[test]
    fn without_refresh_an_existing_block_is_left_untouched() {
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, None, false).unwrap();
        let path = repo.path().join("AGENTS.md");
        let mut hand_edited = fs::read_to_string(&path).unwrap();
        hand_edited = hand_edited.replace("Pulse is the local CLI", "HAND EDITED TEXT");
        fs::write(&path, &hand_edited).unwrap();

        let report = initialize_repository(repo.path(), false, None, false).unwrap();
        assert_eq!(report.status, RepositoryInitStatus::Unchanged);
        let after = fs::read_to_string(&path).unwrap();
        assert!(after.contains("HAND EDITED TEXT"));
    }

    #[test]
    fn refresh_rewrites_only_the_block_region() {
        let repo = tempfile::tempdir().unwrap();
        fs::write(
            repo.path().join("AGENTS.md"),
            "# My repo rules\n\nBe nice.\n",
        )
        .unwrap();
        initialize_repository(repo.path(), false, None, false).unwrap();
        let path = repo.path().join("AGENTS.md");
        let mut hand_edited = fs::read_to_string(&path).unwrap();
        hand_edited = hand_edited.replace("Pulse is the local CLI", "STALE TEXT");
        fs::write(&path, &hand_edited).unwrap();

        let report = initialize_repository(repo.path(), true, None, false).unwrap();
        assert_eq!(report.status, RepositoryInitStatus::Initialized);
        let after = fs::read_to_string(&path).unwrap();
        assert!(after.starts_with("# My repo rules\n\nBe nice.\n"));
        assert!(!after.contains("STALE TEXT"));
        assert!(after.contains("Pulse is the local CLI"));
    }

    #[test]
    fn host_claude_code_writes_detector_files_and_a_settings_snippet() {
        let repo = tempfile::tempdir().unwrap();
        let report = initialize_repository(repo.path(), false, Some("claude-code"), false).unwrap();
        assert!(repo
            .path()
            .join("assets/hosts/claude-code/statusline.sh")
            .exists());
        assert!(repo
            .path()
            .join("assets/hosts/claude-code/post-tool-use.sh")
            .exists());
        assert!(!report.host_settings_snippet.is_empty());
    }

    #[test]
    fn unsupported_host_is_refused() {
        let repo = tempfile::tempdir().unwrap();
        let err = initialize_repository(repo.path(), false, Some("cursor"), false).unwrap_err();
        assert_eq!(err.code(), "host_unsupported");
    }

    #[test]
    fn with_qa_templates_copies_the_scripts_and_seeds_their_roles() {
        let repo = tempfile::tempdir().unwrap();
        let report = initialize_repository(repo.path(), false, None, true).unwrap();
        for (name, _) in QA_TEMPLATE_FILES {
            assert!(
                repo.path().join("scripts/qa").join(name).exists(),
                "missing scripts/qa/{name}"
            );
            assert!(report.created.contains(&format!("scripts/qa/{name}")));
        }
        let runners: serde_json::Value =
            serde_json::from_slice(&fs::read(repo.path().join(".pulse/runners.json")).unwrap())
                .unwrap();
        assert_eq!(
            runners["qa-ui"]["command"],
            serde_json::json!("node scripts/qa/ui.mjs {input}")
        );
        assert!(runners.get("qa-api").is_some());
    }

    #[test]
    fn without_with_qa_templates_neither_scripts_nor_roles_are_seeded() {
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, None, false).unwrap();
        assert!(!repo.path().join("scripts/qa").exists());
        let runners: serde_json::Value =
            serde_json::from_slice(&fs::read(repo.path().join(".pulse/runners.json")).unwrap())
                .unwrap();
        assert!(runners.get("qa-ui").is_none());
    }

    #[test]
    fn an_existing_qa_template_file_is_skipped_not_overwritten() {
        let repo = tempfile::tempdir().unwrap();
        fs::create_dir_all(repo.path().join("scripts/qa")).unwrap();
        fs::write(repo.path().join("scripts/qa/ui.mjs"), "// hand written\n").unwrap();

        let report = initialize_repository(repo.path(), true, None, true).unwrap();
        assert_eq!(
            fs::read_to_string(repo.path().join("scripts/qa/ui.mjs")).unwrap(),
            "// hand written\n"
        );
        assert!(report.skipped.contains(&"scripts/qa/ui.mjs".to_string()));
        assert!(report.created.contains(&"scripts/qa/api.mjs".to_string()));
    }

    /// Plan §8.6's own test requirement: `node --check` on both QA scripts
    /// (skipped, with a stated reason, when `node` is not on PATH — this
    /// crate's own test suite must not depend on a Node toolchain).
    #[test]
    fn qa_templates_are_syntactically_valid_node() {
        let node = std::process::Command::new("node").arg("--version").output();
        if node.is_err() {
            eprintln!("skipping qa_templates_are_syntactically_valid_node: `node` is not on PATH");
            return;
        }
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, None, true).unwrap();
        for name in ["ui.mjs", "api.mjs"] {
            let path = repo.path().join("scripts/qa").join(name);
            let status = std::process::Command::new("node")
                .arg("--check")
                .arg(&path)
                .status()
                .unwrap();
            assert!(status.success(), "node --check failed for {name}");
        }
    }
}
