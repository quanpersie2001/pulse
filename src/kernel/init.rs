//! Repository enrollment (plan 0022 §14 P1.10 — full asset set for what
//! `pulse init` seeds; P1.3 shipped a minimal interim version of this same
//! function without the AGENTS block, docs map or host files).
//!
//! Creates the `.pulse/` tree, an empty `issues.jsonl`, an empty
//! `runners.json`, a `PULSE.md` profile seed (plan §8.1), the runtime/cache
//! `.gitignore` entries (plan §3), the Pulse block in `AGENTS.md` (plan
//! §12.1) and `docs/README.md` (plan §12.2) if either is missing. `refresh`
//! rewrites only the Pulse block region of `AGENTS.md`, never the rest of
//! the file. `host` copies host-specific detector files (plan §10.4) —
//! `"claude-code"` is the only one implemented.
//!
//! Not yet implemented: `--with-qa-templates` (assets/qa/{ui,api}.mjs) and
//! the review/worker prompt assets (plan §8.5/§8.6) — no lane or worker
//! runs against a real target exist yet to consume them (that starts in
//! Phase 2).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::storage::WriteGuard;
use crate::PulseError;

const DIRS: [&str; 6] = [
    ".pulse",
    ".pulse/receipts",
    ".pulse/evidence",
    ".pulse/events",
    ".pulse/learnings",
    ".pulse/runtime",
];

const GITIGNORE_ENTRIES: [&str; 2] = ["**/.pulse/runtime/", "**/.pulse/cache/"];

const PULSE_MD_SEED: &str = "\
# PULSE.md - seeded by `pulse init` (plan 0022 section 8.1). Human-editable.
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
`pulse docs applicable <id>` (plan 0022 P2.1) will match a Ticket's
anchors/tags against `applies_to`/`tags` frontmatter on the docs listed
here; `pulse docs check` will find broken links and stale generated
sections.

- (add entries as `- path/to/doc.md` plus a one-line why)
";

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
`pulse work ready <id>`, then `pulse run worker <id>`. Anything bigger needs
a Story first (`pulse work new story ...`), with an Epic above it if the
work doesn't fit under an existing one.

`done` is never a claim, only a gate reading receipts: a Ticket goes
`verifying -> done` only through `pulse close`, after every lane in its
profile has a passing receipt on the handoff's commit.

Hit friction (a Pulse bug, an unclear doc, a missing check)? Record it:
`pulse note <id> \"<what happened>\" --friction` — don't work around it
silently.

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
        fs::write(&runners_path, b"{}\n").map_err(|error| PulseError::io(&runners_path, error))?;
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

    created.extend(ensure_gitignore_entries(repo_root)?);

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
        host_settings_snippet,
    })
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
        let report = initialize_repository(repo.path(), false, None).unwrap();
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
    }

    #[test]
    fn second_run_is_idempotent_and_reports_unchanged() {
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, None).unwrap();
        let report = initialize_repository(repo.path(), false, None).unwrap();
        assert_eq!(report.status, RepositoryInitStatus::Unchanged);
        assert!(report.created.is_empty());
    }

    #[test]
    fn preserves_a_hand_edited_gitignore_and_only_appends_missing_entries() {
        let repo = tempfile::tempdir().unwrap();
        fs::write(repo.path().join(".gitignore"), "node_modules/\n").unwrap();
        initialize_repository(repo.path(), false, None).unwrap();
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
        initialize_repository(repo.path(), false, None).unwrap();
        let agents = fs::read_to_string(repo.path().join("AGENTS.md")).unwrap();
        assert!(agents.starts_with("# My repo rules\n\nBe nice.\n"));
        assert!(agents.contains(AGENTS_BLOCK_BEGIN));
    }

    #[test]
    fn without_refresh_an_existing_block_is_left_untouched() {
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, None).unwrap();
        let path = repo.path().join("AGENTS.md");
        let mut hand_edited = fs::read_to_string(&path).unwrap();
        hand_edited = hand_edited.replace("Pulse is the local CLI", "HAND EDITED TEXT");
        fs::write(&path, &hand_edited).unwrap();

        let report = initialize_repository(repo.path(), false, None).unwrap();
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
        initialize_repository(repo.path(), false, None).unwrap();
        let path = repo.path().join("AGENTS.md");
        let mut hand_edited = fs::read_to_string(&path).unwrap();
        hand_edited = hand_edited.replace("Pulse is the local CLI", "STALE TEXT");
        fs::write(&path, &hand_edited).unwrap();

        let report = initialize_repository(repo.path(), true, None).unwrap();
        assert_eq!(report.status, RepositoryInitStatus::Initialized);
        let after = fs::read_to_string(&path).unwrap();
        assert!(after.starts_with("# My repo rules\n\nBe nice.\n"));
        assert!(!after.contains("STALE TEXT"));
        assert!(after.contains("Pulse is the local CLI"));
    }

    #[test]
    fn host_claude_code_writes_detector_files_and_a_settings_snippet() {
        let repo = tempfile::tempdir().unwrap();
        let report = initialize_repository(repo.path(), false, Some("claude-code")).unwrap();
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
        let err = initialize_repository(repo.path(), false, Some("cursor")).unwrap_err();
        assert_eq!(err.code(), "host_unsupported");
    }
}
