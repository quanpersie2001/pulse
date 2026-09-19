//! Repository enrollment (plan 0022 §14 P1.10 — full asset set for what
//! `pulse init` seeds; P1.3 shipped a minimal interim version of this same
//! function without the AGENTS block, docs map or host files).
//!
//! Creates the `.pulse/` tree, an empty `issues.jsonl`, a `PULSE.md`
//! profile seed (plan §8.1), the runtime/cache `.gitignore` entries (plan
//! §3), the Pulse block in `AGENTS.md` (plan §12.1), `docs/README.md` and
//! `docs/operations/run.md` (plan §12.2, §8.6) and `.pulse/prompts/*.md`
//! (plan §8.5) if any are missing — so a fresh `pulse docs check` passes
//! rather than immediately reporting `docs/README.md`'s own
//! `docs/operations/run.md` reference as missing.
//!
//! `refresh` re-renders the Pulse block region of `AGENTS.md` and the
//! prompt files — never the rest of `AGENTS.md` — through a three-way
//! merge (plan 0025 G2): every template-written file keeps its as-shipped
//! copy in `.pulse/base/` (durable, committed state — never gitignored),
//! and `git merge-file` folds the template's new side into a file the
//! user has edited. A clean merge lands; a conflict leaves the user's
//! file untouched, saves the marker version under `.pulse/runtime/refresh/`
//! and says so — a living prompt is never handed conflict markers. A file
//! with no base (enrolled by an older Pulse) is kept as-is with the new
//! template beside it until the user resolves it with `--take-new` or
//! `--keep-mine`.
//!
//! Nothing here writes a dispatch table. The prompts describe what a worker
//! or a lane must do and which `pulse` commands record it; how that agent
//! gets started is the host's business, so there is no runner config to
//! seed and no host detector to install.
//!
//! `with_qa_templates` copies `scripts/qa/{ui,api}.mjs` and
//! `scripts/qa/README.md` (plan §8.6), skipping (never overwriting) any
//! that already exist.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::storage::WriteGuard;
use crate::PulseError;

const DIRS: [&str; 8] = [
    ".pulse",
    ".pulse/receipts",
    ".pulse/evidence",
    ".pulse/events",
    ".pulse/learnings",
    ".pulse/prompts",
    ".pulse/base",
    ".pulse/runtime",
];

const GITIGNORE_ENTRIES: [&str; 2] = ["**/.pulse/runtime/", "**/.pulse/cache/"];

/// Where the as-shipped copies of template-written files live (plan 0025
/// G2). Durable, committed state — deliberately NOT in
/// [`GITIGNORE_ENTRIES`]: without a base, a later refresh cannot tell the
/// user's edit from the template's and must fall back to keep-and-show.
const BASE_DIR: &str = ".pulse/base";

/// Where refresh keeps its bounded artifacts: temp merge inputs, and the
/// `.conflict`/`.new` files a conflicted or base-less file points at.
const REFRESH_DIR: &str = ".pulse/runtime/refresh";

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

/// The starting shape of `docs/architecture/overview.md`: headed sections
/// a planner fills in, so the doc `docs/README.md` maps exists from the
/// first `pulse init` and says what belongs in it.
const ARCHITECTURE_MD_SEED: &str = include_str!("../../templates/seeds/architecture-overview.md");

const DOC_SEEDS: [(&str, &str); 3] = [
    ("docs/README.md", DOCS_README_SEED),
    ("docs/operations/run.md", RUN_MD_SEED),
    ("docs/architecture/overview.md", ARCHITECTURE_MD_SEED),
];

const PROMPT_FILES: [(&str, &str); 5] = [
    ("host.md", include_str!("../../templates/prompts/host.md")),
    (
        "worker.md",
        include_str!("../../templates/prompts/worker.md"),
    ),
    (
        "review-correctness.md",
        include_str!("../../templates/prompts/review-correctness.md"),
    ),
    (
        "review-adversarial.md",
        include_str!("../../templates/prompts/review-adversarial.md"),
    ),
    (
        "reconcile.md",
        include_str!("../../templates/prompts/reconcile.md"),
    ),
];

/// One refreshable template-written unit (plan 0025 G2): a prompt file,
/// or the AGENTS block region treated as one text file. `label` names the
/// file in reports and in `--take-new`/`--keep-mine`; `artifact` is the
/// flat name for `.pulse/runtime/refresh/` artifacts; `base_rel` is where
/// the as-shipped copy lives.
struct RefreshUnit {
    label: String,
    artifact: String,
    base_rel: String,
    new_body: String,
}

fn refresh_units() -> Vec<RefreshUnit> {
    let mut units: Vec<RefreshUnit> = PROMPT_FILES
        .iter()
        .map(|(name, body)| RefreshUnit {
            label: format!("prompts/{name}"),
            artifact: format!("prompts-{name}"),
            base_rel: format!("{BASE_DIR}/prompts/{name}"),
            new_body: (*body).to_string(),
        })
        .collect();
    units.push(RefreshUnit {
        label: "agents-block".to_string(),
        artifact: "agents-block".to_string(),
        base_rel: format!("{BASE_DIR}/agents-block.md"),
        new_body: AGENTS_BLOCK_BODY.to_string(),
    });
    units
}

/// What the user chose for a file a refresh could not merge cleanly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshTake {
    /// Write the template over the file and rebase `.pulse/base` on it.
    New,
    /// Keep the user's file; advance `.pulse/base` so the next refresh
    /// merges for real instead of falling back to keep-and-show again.
    Mine,
}

/// How a refreshable unit came out (plan 0025 G2). `created` is a fresh
/// write; `unchanged` nothing to do; `updated` a file the user never
/// touched, moved to the new template; `merged` a clean three-way merge of
/// the user's edits and the template's; `conflict` both sides changed the
/// same lines — the user's file is untouched, markers saved; `kept` a file
/// with no base at all — the user's file stays, the new template is filed
/// next to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshAction {
    Created,
    Unchanged,
    Updated,
    Merged,
    Conflict,
    Kept,
}

/// One refreshable unit's outcome, for the report and the CLI line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshedFile {
    pub file: String,
    pub action: RefreshAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl RefreshedFile {
    fn new(file: &str, action: RefreshAction) -> Self {
        Self {
            file: file.to_string(),
            action,
            note: None,
        }
    }

    fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
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
    /// Per refreshable file outcome, populated only when `--refresh` (or a
    /// `--take-new`/`--keep-mine` resolution) ran — plan 0025 G2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refreshed: Vec<RefreshedFile>,
}

/// # Errors
/// Propagates an I/O error creating a directory or file, or a lock timeout
/// acquiring the repository write lock.
pub fn initialize_repository(
    repo_root: &Path,
    refresh: bool,
    with_qa_templates: bool,
) -> Result<RepositoryInitReport> {
    initialize_repository_ext(repo_root, refresh, with_qa_templates, None)
}

/// [`initialize_repository`] with an optional resolution for one file a
/// previous refresh filed as `kept` (no base) — see [`RefreshTake`].
///
/// # Errors
/// `init_refresh_unknown_file` if `resolve` names a file outside the
/// refreshable set (the prompt files and `agents-block`). Propagates the
/// same I/O and lock errors as [`initialize_repository`].
pub fn initialize_repository_ext(
    repo_root: &Path,
    refresh: bool,
    with_qa_templates: bool,
    resolve: Option<(&str, RefreshTake)>,
) -> Result<RepositoryInitReport> {
    if let Some((name, _)) = resolve {
        if !refresh_units().iter().any(|unit| unit.label == name) {
            return Err(PulseError::kernel(
                "init_refresh_unknown_file",
                format!("\"{name}\" is not a file `pulse init --refresh` manages"),
                "refreshable files are .pulse/prompts/<name>.md and `agents-block`",
            ));
        }
    }
    let _guard = WriteGuard::acquire(repo_root)?;
    let mut created = Vec::new();
    let mut refreshed = Vec::new();
    // Dogfood 0025, F2: a block write inside an existing AGENTS.md is not a
    // file creation. Whether the file pre-existed decides how the block's
    // `wrote_live` is reported below.
    let agents_md_existed = repo_root.join("AGENTS.md").exists();

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

    let pulse_md_path = repo_root.join("PULSE.md");
    if !pulse_md_path.exists() {
        fs::write(&pulse_md_path, PULSE_MD_SEED)
            .map_err(|error| PulseError::io(&pulse_md_path, error))?;
        created.push("PULSE.md".to_string());
    }

    // Write-once doc seeds: the repo owns them from here on, so an
    // existing file is never touched and `--refresh` does not reach them.
    for (rel, seed) in DOC_SEEDS {
        let path = repo_root.join(rel);
        if path.exists() {
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
        }
        fs::write(&path, seed).map_err(|error| PulseError::io(&path, error))?;
        created.push(rel.to_string());
    }

    let block = ensure_agents_block(repo_root, refresh, resolve)?;
    // Only a file that did not exist before this run lands in `created` —
    // `--take-new agents-block` on a living AGENTS.md used to print
    // "created AGENTS.md" and panic operators whose file was fine
    // (dogfood 0025, F2); the refreshed[] line tells that story instead.
    let block_wrote_existing_file = block.wrote_live && agents_md_existed;
    if block.wrote_live && !agents_md_existed {
        created.push("AGENTS.md".to_string());
    }
    refreshed.push(block.report);

    refreshed.extend(ensure_prompts(repo_root, refresh, resolve)?);

    created.extend(ensure_gitignore_entries(repo_root)?);

    let mut skipped = Vec::new();
    if with_qa_templates {
        let (qa_created, qa_skipped) = copy_qa_templates(repo_root)?;
        created.extend(qa_created);
        skipped.extend(qa_skipped);
    }

    // A conflict or a kept file is a handled outcome, not a failure — the
    // CLI exits 0 and points at the artifacts. Only actual writes make the
    // run "Initialized".
    let mutated = block_wrote_existing_file
        || !created.is_empty()
        || refreshed
            .iter()
            .any(|file| matches!(file.action, RefreshAction::Updated | RefreshAction::Merged));
    let status = if mutated {
        RepositoryInitStatus::Initialized
    } else {
        RepositoryInitStatus::Unchanged
    };
    // Without --refresh the per-file array stays empty: the created/skipped
    // lists above are the whole story of a plain init — except a Pulse block
    // appended into an existing AGENTS.md, which would otherwise be an
    // invisible write (the file is not `created`, nothing else changed).
    let refreshed = if refresh || resolve.is_some() || block_wrote_existing_file {
        refreshed
    } else {
        Vec::new()
    };
    Ok(RepositoryInitReport {
        schema_version: 1,
        status,
        created,
        skipped,
        refreshed,
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

/// The live location of a unit's file: a prompt path, or `AGENTS.md`
/// itself for the block unit.
fn live_path(repo_root: &Path, unit: &RefreshUnit) -> std::path::PathBuf {
    if unit.label == "agents-block" {
        repo_root.join("AGENTS.md")
    } else {
        repo_root.join(".pulse").join(&unit.label)
    }
}

fn read_base(repo_root: &Path, unit: &RefreshUnit) -> Option<String> {
    fs::read_to_string(repo_root.join(&unit.base_rel)).ok()
}

fn write_base(repo_root: &Path, unit: &RefreshUnit, body: &str) -> Result<()> {
    let path = repo_root.join(&unit.base_rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
    }
    fs::write(&path, body).map_err(|error| PulseError::io(&path, error))
}

fn runtime_artifact(repo_root: &Path, unit: &RefreshUnit, suffix: &str) -> std::path::PathBuf {
    repo_root
        .join(REFRESH_DIR)
        .join(format!("{}{suffix}", unit.artifact))
}

/// The outcome of one unit's refresh pass.
struct UnitOutcome {
    report: RefreshedFile,
    wrote_live: bool,
}

/// Run the plan 0025 G2 refresh algorithm for one unit. `local` is the
/// file's current content (`None` = the file does not exist yet — it is
/// created, which is the only path that reports `created`).
fn refresh_unit(
    repo_root: &Path,
    unit: &RefreshUnit,
    local: Option<String>,
    resolve: Option<RefreshTake>,
) -> Result<UnitOutcome> {
    let new = &unit.new_body;

    // A user resolution (`--take-new`/`--keep-mine`) overrides the
    // algorithm for this unit only.
    if let Some(take) = resolve {
        return Ok(match take {
            RefreshTake::New => {
                write_unit(repo_root, unit, new)?;
                write_base(repo_root, unit, new)?;
                UnitOutcome {
                    report: RefreshedFile::new(&unit.label, RefreshAction::Updated).with_note(
                        "resolved with --take-new: the template is now the file, and \
                         the base is rebased on it",
                    ),
                    wrote_live: true,
                }
            }
            RefreshTake::Mine => {
                // The live file is untouched; the base moves to the new
                // template so the NEXT refresh merges for real.
                write_base(repo_root, unit, new)?;
                UnitOutcome {
                    report: RefreshedFile::new(&unit.label, RefreshAction::Kept).with_note(
                        "resolved with --keep-mine: your file stays; the base moved \
                         to the current template, so the next refresh merges",
                    ),
                    wrote_live: false,
                }
            }
        });
    }

    let Some(local_body) = local else {
        write_unit(repo_root, unit, new)?;
        write_base(repo_root, unit, new)?;
        return Ok(UnitOutcome {
            report: RefreshedFile::new(&unit.label, RefreshAction::Created),
            wrote_live: true,
        });
    };

    if local_body == *new {
        // Still in sync — keep the base honest so a future user edit is
        // recognized as the only difference.
        write_base(repo_root, unit, new)?;
        return Ok(UnitOutcome {
            report: RefreshedFile::new(&unit.label, RefreshAction::Unchanged),
            wrote_live: false,
        });
    }

    match read_base(repo_root, unit) {
        None => {
            // No base (enrolled by an older Pulse): do not guess. Keep the
            // user's file, file the new template beside it, and leave the
            // base absent until the user resolves.
            let artifact = runtime_artifact(repo_root, unit, ".new");
            if let Some(parent) = artifact.parent() {
                fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
            }
            fs::write(&artifact, new).map_err(|error| PulseError::io(&artifact, error))?;
            Ok(UnitOutcome {
                report: RefreshedFile::new(&unit.label, RefreshAction::Kept).with_note(format!(
                    "no base to merge against — compare with {} and resolve with \
                         `pulse init --refresh --take-new {}` or `--keep-mine {}`",
                    artifact
                        .strip_prefix(repo_root)
                        .unwrap_or(&artifact)
                        .display(),
                    unit.label,
                    unit.label
                )),
                wrote_live: false,
            })
        }
        Some(base) if base == local_body => {
            // The user never touched it: move straight to the new template.
            write_unit(repo_root, unit, new)?;
            write_base(repo_root, unit, new)?;
            Ok(UnitOutcome {
                report: RefreshedFile::new(&unit.label, RefreshAction::Updated),
                wrote_live: true,
            })
        }
        Some(base) => {
            // Both sides may have moved: three-way merge. A clean merge
            // lands; a conflict never touches the user's file — the marker
            // version is filed under .pulse/runtime/refresh/ instead, so a
            // living prompt is never handed markers.
            match merge_three_way(repo_root, unit, &local_body, &base, new) {
                MergeOutcome::Merged(merged) => {
                    write_unit(repo_root, unit, &merged)?;
                    write_base(repo_root, unit, new)?;
                    Ok(UnitOutcome {
                        report: RefreshedFile::new(&unit.label, RefreshAction::Merged),
                        wrote_live: true,
                    })
                }
                MergeOutcome::Conflict(markers) => {
                    let mut report = RefreshedFile::new(&unit.label, RefreshAction::Conflict);
                    if let Some(markers) = markers {
                        let artifact = runtime_artifact(repo_root, unit, ".conflict");
                        if let Some(parent) = artifact.parent() {
                            fs::create_dir_all(parent)
                                .map_err(|error| PulseError::io(parent, error))?;
                        }
                        fs::write(&artifact, &markers)
                            .map_err(|error| PulseError::io(&artifact, error))?;
                        report = report.with_note(format!(
                            "your file is untouched; the merged-with-markers version is \
                             {} — resolve with `pulse init --refresh --take-new {}` \
                             or `--keep-mine {}`",
                            artifact
                                .strip_prefix(repo_root)
                                .unwrap_or(&artifact)
                                .display(),
                            unit.label,
                            unit.label
                        ));
                    } else {
                        report = report
                            .with_note("git merge-file could not run; your file is untouched");
                    }
                    Ok(UnitOutcome {
                        report,
                        wrote_live: false,
                    })
                }
            }
        }
    }
}

fn write_unit(repo_root: &Path, unit: &RefreshUnit, body: &str) -> Result<()> {
    if unit.label == "agents-block" {
        write_block_region(repo_root, body)
    } else {
        let path = live_path(repo_root, unit);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
        }
        fs::write(&path, body).map_err(|error| PulseError::io(&path, error))
    }
}

enum MergeOutcome {
    Merged(String),
    /// `None` when git could not run at all — handled like a conflict
    /// (report and stop) but with no marker file to point at.
    Conflict(Option<String>),
}

/// `git merge-file -p <local> <base> <new>` on temp files under
/// `.pulse/runtime/refresh/`. Exit 0 = clean merge on stdout; exit
/// 1..=127 = that many conflicts, markers on stdout; anything else (git
/// missing, killed) = [`MergeOutcome::Conflict`] with no output.
fn merge_three_way(
    repo_root: &Path,
    unit: &RefreshUnit,
    local: &str,
    base: &str,
    new: &str,
) -> MergeOutcome {
    let dir = repo_root.join(REFRESH_DIR);
    if fs::create_dir_all(&dir).is_err() {
        return MergeOutcome::Conflict(None);
    }
    let write_temp = |suffix: &str, body: &str| -> Option<std::path::PathBuf> {
        let path = dir.join(format!("{}.{}", unit.artifact, suffix));
        fs::write(&path, body).ok()?;
        Some(path)
    };
    let (Some(local_path), Some(base_path), Some(new_path)) = (
        write_temp("local", local),
        write_temp("base", base),
        write_temp("new", new),
    ) else {
        return MergeOutcome::Conflict(None);
    };
    let output = std::process::Command::new("git")
        .current_dir(repo_root)
        .args([
            "merge-file",
            "-p",
            "-L",
            "yours",
            "-L",
            "base",
            "-L",
            "template",
            &local_path.to_string_lossy(),
            &base_path.to_string_lossy(),
            &new_path.to_string_lossy(),
        ])
        .output();
    // The temps are disposable; the conflict artifact is what survives.
    for temp in [&local_path, &base_path, &new_path] {
        let _ = fs::remove_file(temp);
    }
    let Ok(output) = output else {
        return MergeOutcome::Conflict(None);
    };
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    match output.status.code() {
        Some(0) => MergeOutcome::Merged(stdout),
        Some(code) if (1..=127).contains(&code) => MergeOutcome::Conflict(Some(stdout)),
        _ => MergeOutcome::Conflict(None),
    }
}

/// Create/refresh every refreshable unit (plan 0025 G2). Without
/// `refresh`, a missing file is created (and its base snapshotted); an
/// existing one is left alone. With `refresh`, every existing file goes
/// through [`refresh_unit`].
fn ensure_prompts(
    repo_root: &Path,
    refresh: bool,
    resolve: Option<(&str, RefreshTake)>,
) -> Result<Vec<RefreshedFile>> {
    let mut reports = Vec::new();
    for unit in refresh_units() {
        if unit.label == "agents-block" {
            continue;
        }
        let local = fs::read_to_string(live_path(repo_root, &unit)).ok();
        let resolution = resolve.and_then(|(name, take)| (name == unit.label).then_some(take));
        // Refresh or a named resolution touches every unit; a plain init
        // only creates what is missing and leaves the rest unreported.
        if !(refresh || resolution.is_some() || local.is_none()) {
            continue;
        }
        let outcome = refresh_unit(repo_root, &unit, local, resolution)?;
        reports.push(outcome.report);
    }
    Ok(reports)
}

/// The Pulse block's current region content, without the markers (plan
/// 0025 G2 treats the region as one refreshable text file). `None` when
/// the markers are missing.
fn block_region(existing: &str) -> Option<&str> {
    let start = existing.find(AGENTS_BLOCK_BEGIN)? + AGENTS_BLOCK_BEGIN.len();
    existing.find(AGENTS_BLOCK_END)?;
    let body = &existing[start..];
    let end = body.find(AGENTS_BLOCK_END)?;
    let region = &body[..end];
    Some(region.strip_prefix('\n').unwrap_or(region))
}

/// Splice a refreshed region back between the markers; everything outside
/// the block is byte-for-byte untouched.
fn write_block_region(repo_root: &Path, region: &str) -> Result<()> {
    let path = repo_root.join("AGENTS.md");
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let before = existing
        .split(AGENTS_BLOCK_BEGIN)
        .next()
        .unwrap_or_default();
    let after = existing.split(AGENTS_BLOCK_END).nth(1).unwrap_or_default();
    let updated = format!("{before}{AGENTS_BLOCK_BEGIN}\n{region}{AGENTS_BLOCK_END}{after}");
    fs::write(&path, updated).map_err(|error| PulseError::io(&path, error))
}

/// Create the Pulse block in `AGENTS.md`, or — with `refresh`/a named
/// resolution — refresh its region through the plan 0025 G2 algorithm. A
/// first write creates `AGENTS.md` with just the block; an existing file
/// without markers gets the block appended (and its base snapshotted);
/// an existing block region merges like any other refreshable file.
fn ensure_agents_block(
    repo_root: &Path,
    refresh: bool,
    resolve: Option<(&str, RefreshTake)>,
) -> Result<UnitOutcome> {
    let unit = refresh_units()
        .into_iter()
        .find(|unit| unit.label == "agents-block")
        .expect("agents-block is a built-in refresh unit");
    let existing = fs::read_to_string(repo_root.join("AGENTS.md")).unwrap_or_default();
    let has_markers = existing.contains(AGENTS_BLOCK_BEGIN) && existing.contains(AGENTS_BLOCK_END);
    let block_resolution = resolve.and_then(|(name, take)| (name == unit.label).then_some(take));

    if has_markers && (refresh || block_resolution.is_some()) {
        let local = block_region(&existing).map(str::to_string);
        return refresh_unit(repo_root, &unit, local, block_resolution);
    }
    if has_markers {
        // Plain init leaves an existing block untouched.
        return Ok(UnitOutcome {
            report: RefreshedFile::new(&unit.label, RefreshAction::Unchanged),
            wrote_live: false,
        });
    }

    // No block yet: create or append it, and snapshot the base.
    let block = format!("{AGENTS_BLOCK_BEGIN}\n{AGENTS_BLOCK_BODY}{AGENTS_BLOCK_END}\n");
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
    let path = repo_root.join("AGENTS.md");
    fs::write(&path, updated).map_err(|error| PulseError::io(&path, error))?;
    write_base(repo_root, &unit, AGENTS_BLOCK_BODY)?;
    Ok(UnitOutcome {
        report: RefreshedFile::new(&unit.label, RefreshAction::Created),
        wrote_live: true,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_run_creates_everything_and_reports_initialized() {
        let repo = tempfile::tempdir().unwrap();
        let report = initialize_repository(repo.path(), false, false).unwrap();
        assert_eq!(report.status, RepositoryInitStatus::Initialized);
        assert!(repo.path().join(".pulse/issues.jsonl").exists());
        assert!(repo.path().join("PULSE.md").exists());
        assert!(repo.path().join("docs/README.md").exists());
        assert!(repo.path().join("docs/operations/run.md").exists());
        assert!(repo.path().join("docs/architecture/overview.md").exists());
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
    fn prompts_are_written_once_and_left_alone_without_refresh() {
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, false).unwrap();
        let worker_path = repo.path().join(".pulse/prompts/worker.md");
        fs::write(&worker_path, "hand edited\n").unwrap();

        let report = initialize_repository(repo.path(), false, false).unwrap();
        assert_eq!(report.status, RepositoryInitStatus::Unchanged);
        assert_eq!(fs::read_to_string(&worker_path).unwrap(), "hand edited\n");
    }

    #[test]
    fn refresh_moves_an_unmodified_prompt_to_the_new_template() {
        // Plan 0025 G2 rewrote the old overwrite semantics: an unmodified
        // file (local == base) is recognized as the user never touching it
        // and moves to the new template; a hand edit is merged, never
        // clobbered. The template "evolution" is simulated by aging the
        // base snapshot AND the on-disk file — the embedded template
        // constant is fixed, so the pair stands for what an older Pulse
        // shipped.
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, false).unwrap();
        let base_path = repo.path().join(".pulse/base/prompts/worker.md");
        let shipped = fs::read_to_string(&base_path).unwrap();
        let aged = shipped.replace("# Pulse worker", "# Pulse worker (aged)");
        fs::write(&base_path, &aged).unwrap();
        fs::write(repo.path().join(".pulse/prompts/worker.md"), &aged).unwrap();

        let report = initialize_repository(repo.path(), true, false).unwrap();
        let worker = report
            .refreshed
            .iter()
            .find(|file| file.file == "prompts/worker.md")
            .unwrap();
        assert_eq!(worker.action, RefreshAction::Updated);
        let text = fs::read_to_string(repo.path().join(".pulse/prompts/worker.md")).unwrap();
        assert!(text.starts_with("# Pulse worker\n"));
        assert_eq!(
            fs::read_to_string(&base_path).unwrap(),
            text,
            "base moved with it"
        );
    }

    #[test]
    fn refresh_merges_a_user_edit_that_the_template_change_does_not_touch() {
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, false).unwrap();
        let base_path = repo.path().join(".pulse/base/prompts/worker.md");
        let shipped = fs::read_to_string(&base_path).unwrap();
        let aged = shipped.replace("# Pulse worker", "# Pulse worker (aged)");
        fs::write(&base_path, &aged).unwrap();
        // The user appended their own line and kept the aged heading — the
        // template change (the heading) does not touch the user's line.
        let local = format!("{aged}\nUSER NOTE LINE\n");
        fs::write(repo.path().join(".pulse/prompts/worker.md"), &local).unwrap();

        let report = initialize_repository(repo.path(), true, false).unwrap();
        let worker = report
            .refreshed
            .iter()
            .find(|file| file.file == "prompts/worker.md")
            .unwrap();
        assert_eq!(worker.action, RefreshAction::Merged, "{worker:?}");
        let text = fs::read_to_string(repo.path().join(".pulse/prompts/worker.md")).unwrap();
        assert!(text.contains("USER NOTE LINE"), "{text}");
        assert!(
            text.starts_with("# Pulse worker\n"),
            "template side landed: {text}"
        );
    }

    #[test]
    fn refresh_leaves_a_conflicted_file_untouched_and_files_the_markers() {
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, false).unwrap();
        let base_path = repo.path().join(".pulse/base/prompts/worker.md");
        let shipped = fs::read_to_string(&base_path).unwrap();
        let aged = shipped.replace("# Pulse worker", "# Pulse worker (aged)");
        fs::write(&base_path, &aged).unwrap();
        // The user edited the SAME line the template changed, differently:
        // a merge would lose one side, so it must conflict.
        fs::write(
            repo.path().join(".pulse/prompts/worker.md"),
            aged.replace("# Pulse worker (aged)", "# My own worker prompt"),
        )
        .unwrap();

        let report = initialize_repository(repo.path(), true, false).unwrap();
        let worker = report
            .refreshed
            .iter()
            .find(|file| file.file == "prompts/worker.md")
            .unwrap();
        assert_eq!(worker.action, RefreshAction::Conflict);
        // The living prompt is never handed markers.
        let text = fs::read_to_string(repo.path().join(".pulse/prompts/worker.md")).unwrap();
        assert!(text.contains("# My own worker prompt"), "{text}");
        assert!(!text.contains("<<<<<<<"), "{text}");
        // The marker version is filed, and the base is unmoved.
        let conflict = fs::read_to_string(
            repo.path()
                .join(".pulse/runtime/refresh/prompts-worker.md.conflict"),
        )
        .unwrap();
        assert!(conflict.contains("<<<<<<<"));
        assert_eq!(fs::read_to_string(&base_path).unwrap(), aged);
    }

    #[test]
    fn refresh_keeps_a_baseless_file_and_files_the_new_template() {
        // A repo enrolled before .pulse/base existed: no way to tell the
        // user's edit from the template's, so nothing is guessed — the
        // file stays, the new template is filed beside it, and the base
        // stays absent until the user resolves.
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, false).unwrap();
        let worker_path = repo.path().join(".pulse/prompts/worker.md");
        fs::write(&worker_path, "sentinel-do-not-keep\n").unwrap();
        fs::remove_file(repo.path().join(".pulse/base/prompts/worker.md")).unwrap();

        let report = initialize_repository(repo.path(), true, false).unwrap();
        let worker = report
            .refreshed
            .iter()
            .find(|file| file.file == "prompts/worker.md")
            .unwrap();
        assert_eq!(worker.action, RefreshAction::Kept);
        assert!(
            worker
                .note
                .as_deref()
                .is_some_and(|note| note.contains(".new")),
            "{worker:?}"
        );
        assert_eq!(
            fs::read_to_string(&worker_path).unwrap(),
            "sentinel-do-not-keep\n"
        );
        assert!(repo
            .path()
            .join(".pulse/runtime/refresh/prompts-worker.md.new")
            .exists());
        assert!(!repo.path().join(".pulse/base/prompts/worker.md").exists());
    }

    #[test]
    fn take_new_writes_the_template_and_rebases_and_keep_mine_advances_the_base() {
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, false).unwrap();
        let worker_path = repo.path().join(".pulse/prompts/worker.md");
        fs::write(&worker_path, "sentinel-do-not-keep\n").unwrap();
        fs::remove_file(repo.path().join(".pulse/base/prompts/worker.md")).unwrap();

        // --keep-mine: the file stays, the base moves to the template so
        // the next refresh merges for real.
        let report = initialize_repository_ext(
            repo.path(),
            true,
            false,
            Some(("prompts/worker.md", RefreshTake::Mine)),
        )
        .unwrap();
        let worker = report
            .refreshed
            .iter()
            .find(|file| file.file == "prompts/worker.md")
            .unwrap();
        assert_eq!(worker.action, RefreshAction::Kept);
        assert_eq!(
            fs::read_to_string(&worker_path).unwrap(),
            "sentinel-do-not-keep\n"
        );
        let base = fs::read_to_string(repo.path().join(".pulse/base/prompts/worker.md")).unwrap();
        assert!(base.starts_with("# Pulse worker\n"), "{base}");

        // The next refresh now merges (theirs == base, so the user's text
        // wins every hunk) instead of falling back to keep-and-show.
        let report = initialize_repository(repo.path(), true, false).unwrap();
        let worker = report
            .refreshed
            .iter()
            .find(|file| file.file == "prompts/worker.md")
            .unwrap();
        assert_eq!(worker.action, RefreshAction::Merged, "{worker:?}");

        // --take-new: the template becomes the file and the base rebases.
        fs::write(&worker_path, "sentinel-replaced-by-take-new\n").unwrap();
        let report = initialize_repository_ext(
            repo.path(),
            true,
            false,
            Some(("prompts/worker.md", RefreshTake::New)),
        )
        .unwrap();
        let worker = report
            .refreshed
            .iter()
            .find(|file| file.file == "prompts/worker.md")
            .unwrap();
        assert_eq!(worker.action, RefreshAction::Updated);
        let text = fs::read_to_string(&worker_path).unwrap();
        assert!(text.starts_with("# Pulse worker\n"), "{text}");
        assert_eq!(
            fs::read_to_string(repo.path().join(".pulse/base/prompts/worker.md")).unwrap(),
            text
        );
    }

    #[test]
    fn take_new_on_the_agents_block_never_reports_the_file_as_created() {
        // Dogfood 0025, F2: `--take-new agents-block` printed "created
        // AGENTS.md" while the file existed — only the block region inside
        // it had been rewritten. The file lands in `created` when it is
        // genuinely new; otherwise the refreshed[] line carries the story.
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, false).unwrap();
        assert!(repo.path().join("AGENTS.md").exists());
        // Age the block: no base, so the block resolves as kept until the
        // user takes new.
        fs::remove_file(repo.path().join(".pulse/base/agents-block.md")).unwrap();
        fs::write(
            repo.path().join("AGENTS.md"),
            "<!-- PULSE:BEGIN -->\nsentinel\n<!-- PULSE:END -->\n",
        )
        .unwrap();

        let report = initialize_repository_ext(
            repo.path(),
            true,
            false,
            Some(("agents-block", RefreshTake::New)),
        )
        .unwrap();
        assert!(
            !report
                .created
                .iter()
                .any(|entry| entry.contains("AGENTS.md")),
            "an existing AGENTS.md is not created: {:?}",
            report.created
        );
        let block = report
            .refreshed
            .iter()
            .find(|file| file.file == "agents-block")
            .unwrap();
        assert_eq!(block.action, RefreshAction::Updated);
        assert_eq!(report.status, RepositoryInitStatus::Initialized);
        let agents = fs::read_to_string(repo.path().join("AGENTS.md")).unwrap();
        assert!(agents.starts_with("<!-- PULSE:BEGIN -->"), "{agents}");
        assert!(!agents.contains("sentinel"), "{agents}");
    }

    #[test]
    fn a_resolution_naming_an_unmanaged_file_is_init_refresh_unknown_file() {
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, false).unwrap();
        let err = initialize_repository_ext(
            repo.path(),
            true,
            false,
            Some(("docs/README.md", RefreshTake::New)),
        )
        .unwrap_err();
        assert_eq!(err.code(), "init_refresh_unknown_file");
        assert!(err.hint().is_some());
    }

    #[test]
    fn the_pulse_base_state_is_never_gitignored() {
        // .pulse/base is the durable side of the three-way merge: gitignored
        // bases would silently downgrade every later refresh to keep-and-show.
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, false).unwrap();
        let gitignore = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
        assert!(!gitignore.contains(".pulse/base"), "{gitignore}");
        let report = initialize_repository(repo.path(), true, false).unwrap();
        assert!(!report
            .created
            .iter()
            .any(|entry| entry.contains(".gitignore: **/.pulse/base")));
    }

    #[test]
    fn refreshing_twice_in_a_row_reports_all_unchanged_the_second_time() {
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, false).unwrap();
        let first = initialize_repository(repo.path(), true, false).unwrap();
        assert_eq!(first.status, RepositoryInitStatus::Unchanged);
        let second = initialize_repository(repo.path(), true, false).unwrap();
        assert_eq!(second.status, RepositoryInitStatus::Unchanged);
        for file in &second.refreshed {
            assert_eq!(file.action, RefreshAction::Unchanged, "{file:?}");
        }
        assert!(second.created.is_empty());
    }

    #[test]
    fn second_run_is_idempotent_and_reports_unchanged() {
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, false).unwrap();
        let report = initialize_repository(repo.path(), false, false).unwrap();
        assert_eq!(report.status, RepositoryInitStatus::Unchanged);
        assert!(report.created.is_empty());
    }

    #[test]
    fn preserves_a_hand_edited_gitignore_and_only_appends_missing_entries() {
        let repo = tempfile::tempdir().unwrap();
        fs::write(repo.path().join(".gitignore"), "node_modules/\n").unwrap();
        initialize_repository(repo.path(), false, false).unwrap();
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
        initialize_repository(repo.path(), false, false).unwrap();
        let agents = fs::read_to_string(repo.path().join("AGENTS.md")).unwrap();
        assert!(agents.starts_with("# My repo rules\n\nBe nice.\n"));
        assert!(agents.contains(AGENTS_BLOCK_BEGIN));
    }

    #[test]
    fn without_refresh_an_existing_block_is_left_untouched() {
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, false).unwrap();
        let path = repo.path().join("AGENTS.md");
        let mut hand_edited = fs::read_to_string(&path).unwrap();
        hand_edited = hand_edited.replace("Pulse is the local CLI", "HAND EDITED TEXT");
        fs::write(&path, &hand_edited).unwrap();

        let report = initialize_repository(repo.path(), false, false).unwrap();
        assert_eq!(report.status, RepositoryInitStatus::Unchanged);
        let after = fs::read_to_string(&path).unwrap();
        assert!(after.contains("HAND EDITED TEXT"));
    }

    #[test]
    fn refresh_merges_the_block_region_and_never_touches_anything_outside_it() {
        // Plan 0025 G2 semantics: the block region merges like any other
        // refreshable file — a user edit survives (the old test asserted
        // the edit was overwritten); the outside content is byte-identical
        // in every outcome.
        let repo = tempfile::tempdir().unwrap();
        fs::write(
            repo.path().join("AGENTS.md"),
            "# My repo rules\n\nBe nice.\n\nHAND MARKER: keep me\n",
        )
        .unwrap();
        initialize_repository(repo.path(), false, false).unwrap();
        let agents_path = repo.path().join("AGENTS.md");
        let mut hand_edited = fs::read_to_string(&agents_path).unwrap();
        hand_edited = hand_edited.replace("Pulse is the local CLI", "STALE TEXT");
        fs::write(&agents_path, &hand_edited).unwrap();

        let report = initialize_repository(repo.path(), true, false).unwrap();
        let block = report
            .refreshed
            .iter()
            .find(|file| file.file == "agents-block")
            .unwrap();
        assert_eq!(block.action, RefreshAction::Merged, "{block:?}");
        let after = fs::read_to_string(&agents_path).unwrap();
        assert!(after.starts_with("# My repo rules\n\nBe nice.\n"));
        assert!(
            after.contains("HAND MARKER: keep me"),
            "outside content intact"
        );
        // The user's edit survives the merge (the template did not change
        // since init, so the merge keeps the user's side).
        assert!(
            after.contains("STALE TEXT"),
            "a merge never clobbers: {after}"
        );
        assert!(after.contains(AGENTS_BLOCK_BEGIN));
    }

    #[test]
    fn refresh_ages_the_block_region_to_the_new_template_when_the_user_did_not_edit() {
        let repo = tempfile::tempdir().unwrap();
        fs::write(
            repo.path().join("AGENTS.md"),
            "# My repo rules\n\nBe nice.\n",
        )
        .unwrap();
        initialize_repository(repo.path(), false, false).unwrap();
        let base_path = repo.path().join(".pulse/base/agents-block.md");
        let shipped = fs::read_to_string(&base_path).unwrap();
        // Age base AND the on-disk region together: what an older template
        // shipped, before the constant moved on.
        let aged = shipped.replace("## Pulse", "## Pulse (aged)");
        fs::write(&base_path, &aged).unwrap();
        let agents = fs::read_to_string(repo.path().join("AGENTS.md")).unwrap();
        fs::write(
            repo.path().join("AGENTS.md"),
            agents.replace("## Pulse\n", "## Pulse (aged)\n"),
        )
        .unwrap();

        let report = initialize_repository(repo.path(), true, false).unwrap();
        let block = report
            .refreshed
            .iter()
            .find(|file| file.file == "agents-block")
            .unwrap();
        assert_eq!(block.action, RefreshAction::Updated, "{block:?}");
        let after = fs::read_to_string(repo.path().join("AGENTS.md")).unwrap();
        assert!(after.starts_with("# My repo rules\n\nBe nice.\n"));
        assert!(
            after.contains("## Pulse\n"),
            "template side landed: {after}"
        );
        assert!(!after.contains("## Pulse (aged)"));
    }

    #[test]
    fn with_qa_templates_copies_the_scripts() {
        let repo = tempfile::tempdir().unwrap();
        let report = initialize_repository(repo.path(), false, true).unwrap();
        for (name, _) in QA_TEMPLATE_FILES {
            assert!(
                repo.path().join("scripts/qa").join(name).exists(),
                "missing scripts/qa/{name}"
            );
            assert!(report.created.contains(&format!("scripts/qa/{name}")));
        }
    }

    #[test]
    fn without_with_qa_templates_no_scripts_are_copied() {
        let repo = tempfile::tempdir().unwrap();
        initialize_repository(repo.path(), false, false).unwrap();
        assert!(!repo.path().join("scripts/qa").exists());
    }

    #[test]
    fn an_existing_qa_template_file_is_skipped_not_overwritten() {
        let repo = tempfile::tempdir().unwrap();
        fs::create_dir_all(repo.path().join("scripts/qa")).unwrap();
        fs::write(repo.path().join("scripts/qa/ui.mjs"), "// hand written\n").unwrap();

        let report = initialize_repository(repo.path(), true, true).unwrap();
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
        initialize_repository(repo.path(), false, true).unwrap();
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
