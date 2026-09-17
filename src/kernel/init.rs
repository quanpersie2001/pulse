//! Repository enrollment (plan 0022 §14 P1.10 — full asset set for what
//! `pulse init` seeds; P1.3 shipped a minimal interim version of this same
//! function without the AGENTS block, docs map or host files).
//!
//! Creates the `.pulse/` tree, an empty `issues.jsonl`, a `runners.json`
//! seed wired to the prompt templates below (plan §8.2), a `PULSE.md`
//! profile seed (plan §8.1), the runtime/cache `.gitignore` entries (plan
//! §3), the Pulse block in `AGENTS.md` (plan §12.1), `docs/README.md` and
//! `docs/operations/run.md` (plan §12.2, §8.6) and `.pulse/prompts/*.md`
//! (plan §8.5) if any are missing — so a fresh `pulse docs check` passes
//! rather than immediately reporting `docs/README.md`'s own
//! `docs/operations/run.md` reference as missing. `refresh`
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

const PULSE_MD_SEED: &str = include_str!("../../templates/seeds/PULSE.md");

const DOCS_README_SEED: &str = include_str!("../../templates/seeds/docs-README.md");

/// Referenced by [`DOCS_README_SEED`] since P1.10/A6, but never itself
/// created until A8.2 — a fresh `pulse init` (with or without
/// `--with-qa-templates`) left `docs check` reporting the path as missing
/// immediately, on every repo. Placeholder `docker compose` argv
/// (syntactically valid, not meant to run as-is) for both blocks — `pulse
/// init` has no way to know this repo's actual stack; a human edits
/// `start`/`ready_url`/`stop`/`log` for how it really runs, per
/// `scripts/qa/README.md`.
const RUN_MD_SEED: &str = include_str!("../../templates/seeds/run.md");

/// Plan §8.2's `runners.json` seed, now a real file at
/// `templates/seeds/runners.json` (`include_str!`'d — a JSON file is
/// readable, lintable and diffable outside Rust in a way a code-built table
/// never was). The `qa-ui`/`qa-api` roles are only seeded when
/// `--with-qa-templates` also copies the scripts these commands point at
/// (plan §8.6) — seeding them unconditionally would give every plain
/// `pulse init` a `runners.json` naming `scripts/qa/*.mjs` files that don't
/// exist, so the plain run strips every `qa-*` role from the seed.
fn runners_json_seed(with_qa_templates: bool) -> serde_json::Value {
    let mut seed: serde_json::Value =
        serde_json::from_str(include_str!("../../templates/seeds/runners.json"))
            .expect("templates/seeds/runners.json must be valid JSON");
    if !with_qa_templates {
        seed.as_object_mut()
            .expect("runners.json seed must be an object")
            .retain(|role, _| !role.starts_with("qa-"));
    }
    seed
}

const PROMPT_FILES: [(&str, &str); 4] = [
    (
        "worker.md",
        include_str!("../../templates/prompts/worker.md"),
    ),
    (
        "worker-continue.md",
        include_str!("../../templates/prompts/worker-continue.md"),
    ),
    (
        "review-correctness.md",
        include_str!("../../templates/prompts/review-correctness.md"),
    ),
    (
        "review-adversarial.md",
        include_str!("../../templates/prompts/review-adversarial.md"),
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
/// `pulse-shape`/`pulse-plan` aren't rebuilt until Phase 2/3. Body lives in
/// `templates/seeds/agents-block.md`, `include_str!`'d here.
const AGENTS_BLOCK_BODY: &str = include_str!("../../templates/seeds/agents-block.md");

const STATUSLINE_SH: &str = include_str!("../../templates/hosts/claude-code/statusline.sh");

const POST_TOOL_USE_SH: &str = include_str!("../../templates/hosts/claude-code/post-tool-use.sh");

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

    let run_md_path = repo_root.join("docs/operations/run.md");
    if !run_md_path.exists() {
        fs::create_dir_all(repo_root.join("docs/operations"))
            .map_err(|error| PulseError::io(repo_root.join("docs/operations"), error))?;
        fs::write(&run_md_path, RUN_MD_SEED)
            .map_err(|error| PulseError::io(&run_md_path, error))?;
        created.push("docs/operations/run.md".to_string());
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
    ("ui.mjs", include_str!("../../templates/qa/ui.mjs")),
    ("api.mjs", include_str!("../../templates/qa/api.mjs")),
    ("README.md", include_str!("../../templates/qa/README.md")),
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

/// Writes into `.pulse/hosts/claude-code/` (never `assets/hosts/`) — that
/// path is Pulse's own repository layout, not a convention a target repo
/// should adopt; `.pulse/` is where Pulse-owned generated files belong, and
/// unlike `.pulse/runtime/`/`.pulse/cache/` it is tracked, not gitignored
/// (plan §3, §10.4).
fn write_claude_code_host_files(repo_root: &Path) -> Result<Vec<String>> {
    let dir = repo_root.join(".pulse/hosts/claude-code");
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
            written.push(format!(".pulse/hosts/claude-code/{name}"));
        }
    }
    Ok(written)
}

fn claude_code_settings_snippet() -> Vec<String> {
    vec![
        "statusLine: .pulse/hosts/claude-code/statusline.sh".to_string(),
        "hooks.PostToolUse: .pulse/hosts/claude-code/post-tool-use.sh".to_string(),
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
        assert!(repo.path().join("docs/operations/run.md").exists());
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
        // Every non-qa role in the seed file must survive the plain init's
        // qa-* strip and appear in the written runners.json.
        let seed: serde_json::Value =
            serde_json::from_str(include_str!("../../templates/seeds/runners.json")).unwrap();
        let base_roles: Vec<&str> = seed
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .filter(|role| !role.starts_with("qa-"))
            .collect();
        assert!(!base_roles.is_empty(), "seed lost its base roles");
        for role in base_roles {
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
            .join(".pulse/hosts/claude-code/statusline.sh")
            .exists());
        assert!(repo
            .path()
            .join(".pulse/hosts/claude-code/post-tool-use.sh")
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

    /// The A8.1 regression, exercised for real: `start` is a long-running
    /// server (`python3 -m http.server`, which never exits), so the old
    /// awaited-execFile code blocked forever before ever polling
    /// `ready_url`. With `await_exit: false` (this block's explicit
    /// opt-out) the script must still finish quickly (bounded at 30s so a
    /// regression fails instead of hanging the suite — an unconditional
    /// await burns the script's 120s cap and dwarfs the bound), report
    /// `inconclusive` (the case carries no `check`), record the detached
    /// `start` truthfully in `commands_run[]`, leave the HTTP transcript,
    /// and leave no server listening. Skipped, with a stated reason, when
    /// `node` or `python3` is not on PATH — the suite must not depend on
    /// either toolchain. The block sets `await_exit: false` explicitly: a
    /// never-exiting start cannot be awaited (dogfood ST-1 F2), which is
    /// exactly the opt-out this test pins.
    #[test]
    fn qa_api_script_survives_a_long_running_start_and_stops_the_server() {
        if std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!(
                "skipping qa_api_script_survives_a_long_running_start_and_stops_the_server: \
                 `node` is not on PATH"
            );
            return;
        }
        if std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!(
                "skipping qa_api_script_survives_a_long_running_start_and_stops_the_server: \
                 `python3` is not on PATH"
            );
            return;
        }
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/qa/api.mjs");
        let repo = tempfile::tempdir().unwrap();
        fs::create_dir_all(repo.path().join("docs/operations")).unwrap();
        fs::write(
            repo.path().join("docs/operations/run.md"),
            "# Run\n\n```pulse-run\nid: api\nstart: [\"python3\", \"-m\", \"http.server\", \"18080\"]\n\
             ready_url: \"http://127.0.0.1:18080/\"\nstop: [\"pkill\", \"-f\", \"http.server 18080\"]\n\
             log: \".pulse/runtime/logs/api.log\"\nawait_exit: false\n```\n",
        )
        .unwrap();
        fs::write(
            repo.path().join("input.json"),
            r#"{"qa_cases":[{"id":"QA-001","steps":["GET /"]}],"evidence_dir":".pulse/evidence/TK-qa","handoff_commit":""}"#,
        )
        .unwrap();

        let mut child = std::process::Command::new("node")
            .arg(&script)
            .arg("input.json")
            .current_dir(repo.path())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let started = std::time::Instant::now();
        let status = loop {
            match child.try_wait().expect("node child should be pollable") {
                Some(status) => break status,
                None if started.elapsed() < std::time::Duration::from_secs(30) => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                None => {
                    let _ = child.kill();
                    panic!(
                        "api.mjs was still running after 30s — with `await_exit: false` it must \
                         not await the start child"
                    );
                }
            }
        };
        assert!(status.success(), "api.mjs exited {status}");

        let evidence = repo.path().join(".pulse/evidence/TK-qa");
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(evidence.join("qa-api.json")).unwrap()).unwrap();
        assert_eq!(report["verdict"], "inconclusive");
        assert_eq!(
            report["commands_run"][0]["argv"],
            serde_json::json!(["python3", "-m", "http.server", "18080"])
        );
        assert_eq!(report["commands_run"][0]["exit"], serde_json::Value::Null);
        assert_eq!(report["commands_run"][0]["detached"], true);
        assert_eq!(report["commands_run"][1]["exit"], 0); // pkill stopped it
        let http = fs::read_to_string(evidence.join("logs/QA-001.http.txt")).unwrap();
        assert!(
            http.contains("< 200"),
            "expected the GET / transcript to record a 200: {http}"
        );

        let mut stopped = false;
        for _ in 0..40 {
            if std::net::TcpStream::connect("127.0.0.1:18080").is_err() {
                stopped = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(
            stopped,
            "http.server on 127.0.0.1:18080 still listens after api.mjs exited"
        );
    }

    /// The dogfood ST-1 F2 regression: with the default `await_exit: true`,
    /// the script must wait for the detached `start` to exit before trusting
    /// `ready_url`, even when `ready_url` already answers — a pre-existing
    /// instance answering early is exactly the stale-instance race. Here the
    /// start is `sleep 3` (exits after 3s, starts nothing) and an unrelated
    /// server already answers `ready_url` from t=0: the script may only
    /// finish after the start child has exited, i.e. no earlier than ~3s.
    #[test]
    fn qa_api_script_awaits_start_exit_before_trusting_ready_url() {
        if std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!(
                "skipping qa_api_script_awaits_start_exit_before_trusting_ready_url: \
                 `node` is not on PATH"
            );
            return;
        }
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/qa/api.mjs");
        let repo = tempfile::tempdir().unwrap();
        fs::create_dir_all(repo.path().join("docs/operations")).unwrap();
        fs::write(
            repo.path().join("docs/operations/run.md"),
            "# Run\n\n```pulse-run\nid: api\nstart: [\"sh\", \"-c\", \"sleep 3\"]\n\
             ready_url: \"http://127.0.0.1:18081/\"\nstop: [\"true\"]\n\
             log: \".pulse/runtime/logs/api.log\"\nawait_exit: true\n```\n",
        )
        .unwrap();
        fs::write(
            repo.path().join("input.json"),
            r#"{"qa_cases":[{"id":"QA-001","steps":["GET /"]}],"evidence_dir":".pulse/evidence/TK-qa","handoff_commit":""}"#,
        )
        .unwrap();

        // An unrelated server already answers `ready_url` from t=0.
        let mut server = std::process::Command::new("node")
            .arg("-e")
            .arg("require('http').createServer((q,s)=>s.end('ok')).listen(18081)")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut up = false;
        for _ in 0..50 {
            if std::net::TcpStream::connect("127.0.0.1:18081").is_ok() {
                up = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(up, "node http server on 18081 never came up");

        let started = std::time::Instant::now();
        let output = std::process::Command::new("node")
            .arg(&script)
            .arg("input.json")
            .current_dir(repo.path())
            .output()
            .expect("node child should run");
        let elapsed = started.elapsed();
        let _ = server.kill();
        let _ = server.wait();

        assert!(
            output.status.success(),
            "api.mjs exited {:?}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            elapsed >= std::time::Duration::from_secs(2),
            "api.mjs finished in {elapsed:?} — it trusted `ready_url` while the \
             start child was still running (the F2 stale-instance race)"
        );
        let evidence = repo.path().join(".pulse/evidence/TK-qa");
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(evidence.join("qa-api.json")).unwrap()).unwrap();
        assert_eq!(report["verdict"], "inconclusive");
        let http = fs::read_to_string(evidence.join("logs/QA-001.http.txt")).unwrap();
        assert!(
            http.contains("< 200"),
            "expected the GET / transcript to record a 200: {http}"
        );
    }

    /// The block's optional `migrate: [argv]` (dogfood ST-2 F26/F29) must
    /// run after `start` has settled and BEFORE any QA traffic: the case
    /// step here fetches `/migrate-marker`, a file only the migrate command
    /// creates — a 200 in the transcript proves the migrate ran before the
    /// cases, a 404 would mean the lane graded a database the migrate never
    /// touched. Also pins the truthful `commands_run[]` (start detached,
    /// migrate with its real exit, stop) and the migrate tail kept in
    /// `logs/migrate.txt`. A block WITHOUT `migrate` is covered by the two
    /// tests above: absent key = skipped, behavior unchanged.
    #[test]
    fn qa_api_script_runs_the_optional_migrate_argv_before_any_qa_traffic() {
        if std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!(
                "skipping qa_api_script_runs_the_optional_migrate_argv_before_any_qa_traffic: \
                 `node` is not on PATH"
            );
            return;
        }
        if std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!(
                "skipping qa_api_script_runs_the_optional_migrate_argv_before_any_qa_traffic: \
                 `python3` is not on PATH"
            );
            return;
        }
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/qa/api.mjs");
        let repo = tempfile::tempdir().unwrap();
        fs::create_dir_all(repo.path().join("docs/operations")).unwrap();
        fs::write(
            repo.path().join("docs/operations/run.md"),
            "# Run\n\n```pulse-run\nid: api\nstart: [\"python3\", \"-m\", \"http.server\", \"18083\"]\n\
             migrate: [\"sh\", \"-c\", \"echo migrated > migrate-marker && echo migrate-ok-line\"]\n\
             ready_url: \"http://127.0.0.1:18083/\"\nstop: [\"pkill\", \"-f\", \"http.server 18083\"]\n\
             log: \".pulse/runtime/logs/api.log\"\nawait_exit: false\n```\n",
        )
        .unwrap();
        fs::write(
            repo.path().join("input.json"),
            r#"{"qa_cases":[{"id":"QA-001","steps":["GET /migrate-marker"]}],"evidence_dir":".pulse/evidence/TK-qa","handoff_commit":""}"#,
        )
        .unwrap();

        let output = std::process::Command::new("node")
            .arg(&script)
            .arg("input.json")
            .current_dir(repo.path())
            .output()
            .expect("node child should run");
        assert!(
            output.status.success(),
            "api.mjs exited {:?}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(
            repo.path().join("migrate-marker").exists(),
            "the migrate argv never ran"
        );
        let evidence = repo.path().join(".pulse/evidence/TK-qa");
        let migrate_log = fs::read_to_string(evidence.join("logs/migrate.txt")).unwrap();
        assert!(
            migrate_log.contains("migrate-ok-line"),
            "migrate stdout must be kept in logs/migrate.txt: {migrate_log}"
        );
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(evidence.join("qa-api.json")).unwrap()).unwrap();
        assert_eq!(report["commands_run"].as_array().unwrap().len(), 3);
        assert_eq!(
            report["commands_run"][1]["argv"],
            serde_json::json!([
                "sh",
                "-c",
                "echo migrated > migrate-marker && echo migrate-ok-line"
            ])
        );
        assert_eq!(report["commands_run"][1]["exit"], 0);
        let http = fs::read_to_string(evidence.join("logs/QA-001.http.txt")).unwrap();
        assert!(
            http.contains("> GET /migrate-marker") && http.contains("< 200"),
            "the QA case must see the migrate's file (200), not a 404 — \
             migrate ran after the cases: {http}"
        );
    }
}
