//! Safe repository enrollment across Pulse-owned Core domains.
//!
//! This module owns the cross-domain initialization order for a target
//! repository. It touches graph, evidence, documentation, knowledge, authority
//! and their durable source roots under one repository write lock. Every owner
//! preflights existing canonical state before the first canonical write, so a
//! later domain conflict cannot leave earlier domains newly enrolled.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::kernel::guidance::GuidanceOutcome;
use crate::storage::transaction::recover_prepared_transactions;
use crate::storage::WriteGuard;
use crate::{PulseError, Result};

const PROPOSED_IGNORE_ENTRIES: [&str; 2] = [".pulse/runtime/", ".pulse/cache/"];

/// Default role commands recorded at init. Role names are fixed; commands
/// are plain argv lines parsed without a shell and can be re-pointed at any
/// agent or script by editing this file. The agent defaults are valid
/// Claude Code headless invocations (decision 13.2): a positional pointer
/// prompt that directs the agent to the Pulse-written bootstrap prompt for
/// the ticket. `--output-format text` keeps the agent's own final JSON line
/// as the last stdout line, which is the runner output contract.
const DEFAULT_RUNNER_ROLES_JSON: &str = r#"
{
  "worker": {
    "command": "claude -p --output-format text --dangerously-skip-permissions 'Pulse worker run. First read .pulse/runtime/run/{ticket}/worker-prompt.md in this repository and follow those instructions exactly.'",
    "timeout_seconds": 3600
  },
  "reviewer": {
    "command": "claude -p --output-format text --dangerously-skip-permissions 'Pulse reviewer run. First read .pulse/runtime/run/{ticket}/reviewer-prompt.md in this repository and follow those instructions exactly.'",
    "timeout_seconds": 1800
  },
  "qa": {
    "command": "node scripts/qa-run.mjs {input}",
    "timeout_seconds": 900
  }
}
"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RepositoryInitStatus {
    Initialized,
    Unchanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RepositoryInitReport {
    pub schema_version: u32,
    pub code: String,
    pub status: RepositoryInitStatus,
    pub repository_id: String,
    pub authority_policy_revision: u64,
    pub created: Vec<String>,
    pub preserved: Vec<String>,
    pub proposed_ignore_entries: Vec<String>,
    /// Guidance files whose Pulse-owned region was hand-edited and therefore
    /// left untouched by `--refresh` (Decision 0009 §1: report, never
    /// overwrite). Empty on a normal init.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guidance_conflicts: Vec<String>,
}

/// Enroll a target repository into all current local-first Core domains.
///
/// The command never edits `.gitignore`; it reports the narrow runtime/cache
/// entries maintainers should review. Existing canonical state is preserved
/// and validated, while drift or a managed-path collision fails closed.
///
/// # Errors
///
/// Returns a typed error when the repository root is invalid, a managed path is
/// unsafe, existing domain state is incompatible, or durable initialization
/// cannot complete.
pub(crate) fn initialize_repository(
    repo_root: &Path,
    actor: Option<&str>,
    refresh: bool,
) -> Result<RepositoryInitReport> {
    let repo_root = crate::storage::paths::canonicalize_existing_dir(repo_root)?;
    let principal = initial_principal(&repo_root, actor)?;
    // The lock owner creates `.pulse/runtime/locks`, so reject managed symlinks
    // and file collisions before acquiring it, then revalidate under the lock.
    preflight_managed_paths(&repo_root)?;
    let _guard = WriteGuard::acquire(&repo_root)?;
    recover_prepared_transactions(&repo_root)?;

    preflight_managed_paths(&repo_root)?;
    crate::graph::store::preflight_bootstrap(&repo_root)?;
    crate::evidence::manifest::preflight_bootstrap(&repo_root)?;
    crate::docs::manifest::preflight_bootstrap(&repo_root)?;
    crate::knowledge::manifest::preflight_bootstrap(&repo_root)?;
    crate::policy::authority::preflight_bootstrap(&repo_root)?;

    let mut created = Vec::new();
    let mut preserved = Vec::new();
    for relative in [
        "docs",
        "works",
        "knowledge/learnings",
        ".pulse/events",
        ".pulse/config",
    ] {
        ensure_directory(&repo_root, relative, &mut created, &mut preserved)?;
    }
    ensure_runner_roles_config(&repo_root, &mut created, &mut preserved)?;

    let graph = crate::graph::store::bootstrap(&repo_root)?;
    created.extend(graph.created);
    preserved.extend(graph.preserved);

    let evidence = crate::evidence::manifest::bootstrap(&repo_root)?;
    created.extend(evidence.created);
    preserved.extend(evidence.preserved);

    let docs = crate::docs::manifest::bootstrap_unlocked(&repo_root)?;
    created.extend(docs.created);
    preserved.extend(docs.preserved);

    let owner_actor = match principal.kind {
        crate::identity::actor::ActorKind::Human => format!("human:{}", principal.id),
        crate::identity::actor::ActorKind::Agent => format!("agent:{}", principal.id),
        crate::identity::actor::ActorKind::System => format!("system:{}", principal.id),
    };
    ensure_glossary(&repo_root, &owner_actor, &mut created, &mut preserved)?;

    let knowledge = crate::knowledge::manifest::bootstrap_unlocked(&repo_root)?;
    created.extend(knowledge.created);
    preserved.extend(knowledge.preserved);

    let authority = crate::policy::authority::bootstrap_default_deny(&repo_root, &principal)?;
    created.extend(authority.created);
    preserved.extend(authority.preserved);

    // Guidance last: it is prose in the source plane, and writing it only
    // after every canonical domain has enrolled keeps a domain conflict from
    // leaving instructions behind for a repository Pulse did not enrol.
    let mut guidance_conflicts = Vec::new();
    let mut guidance_changed = false;
    for (relative, outcome) in [
        (
            "AGENTS.md",
            crate::kernel::guidance::write_agents_block(&repo_root, refresh)?,
        ),
        (
            "PULSE.md",
            crate::kernel::guidance::write_pulse_md(&repo_root)?,
        ),
    ] {
        let path = repo_root.join(relative);
        match outcome {
            GuidanceOutcome::Written | GuidanceOutcome::Refreshed => {
                guidance_changed = true;
                created.push(path);
            }
            GuidanceOutcome::Preserved | GuidanceOutcome::Unchanged => preserved.push(path),
            GuidanceOutcome::Modified => {
                preserved.push(path);
                guidance_conflicts.push(relative.to_string());
            }
        }
    }

    let created = stable_relative_paths(&repo_root, created)?;
    let preserved = stable_relative_paths(&repo_root, preserved)?;
    let status = if created.is_empty() && !authority.changed && !guidance_changed {
        RepositoryInitStatus::Unchanged
    } else {
        RepositoryInitStatus::Initialized
    };

    Ok(RepositoryInitReport {
        schema_version: 1,
        code: "repository_initialized".to_string(),
        status,
        repository_id: evidence.manifest.repository_id,
        authority_policy_revision: authority.policy.revision,
        created,
        preserved,
        proposed_ignore_entries: PROPOSED_IGNORE_ENTRIES
            .iter()
            .map(|entry| (*entry).to_string())
            .collect(),
        guidance_conflicts,
    })
}

fn initial_principal(
    repo_root: &Path,
    actor: Option<&str>,
) -> Result<crate::policy::AuthorityPrincipal> {
    let actor = match actor {
        Some(actor) if !actor.trim().is_empty() => actor.trim().to_string(),
        Some(_) => {
            return Err(PulseError::validation(
                "init_actor_invalid",
                "--actor must use kind:id syntax with a non-empty id",
            ));
        }
        None => git_user_name(repo_root)?
            .map(|name| format!("human:{name}"))
            .ok_or_else(|| {
                PulseError::validation(
                    "init_actor_required",
                    "pulse init requires --actor kind:id when Git user.name is unavailable",
                )
            })?,
    };

    let (kind, id) = actor.split_once(':').ok_or_else(|| {
        PulseError::validation(
            "init_actor_invalid",
            "--actor must use kind:id syntax (kind is human, agent, or system)",
        )
    })?;
    if id.trim().is_empty() || id.len() > 128 {
        return Err(PulseError::validation(
            "init_actor_invalid",
            "--actor id must contain between 1 and 128 characters",
        ));
    }
    let kind = match kind {
        "human" => crate::identity::actor::ActorKind::Human,
        "agent" => crate::identity::actor::ActorKind::Agent,
        "system" => crate::identity::actor::ActorKind::System,
        _ => {
            return Err(PulseError::validation(
                "init_actor_invalid",
                "--actor kind must be human, agent, or system",
            ));
        }
    };
    Ok(crate::policy::AuthorityPrincipal {
        kind,
        id: id.to_string(),
        grants: crate::policy::CORE_GRANTS
            .iter()
            .map(|grant| (*grant).to_string())
            .collect(),
    })
}

fn git_user_name(repo_root: &Path) -> Result<Option<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["config", "--get", "user.name"])
        .output()
        .map_err(|error| PulseError::io(repo_root, error))?;
    if !output.status.success() {
        return Ok(None);
    }
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name.is_empty() {
        Ok(None)
    } else {
        Ok(Some(name))
    }
}

fn preflight_managed_paths(repo_root: &Path) -> Result<()> {
    for relative in [
        ".pulse",
        ".pulse/workgraph",
        ".pulse/workgraph/schemas",
        ".pulse/workgraph/nodes",
        ".pulse/workgraph/edges",
        ".pulse/evidence",
        ".pulse/evidence/receipts",
        ".pulse/evidence/artifacts",
        ".pulse/evidence/artifacts/sha256",
        ".pulse/docs",
        ".pulse/knowledge",
        ".pulse/knowledge/entries",
        ".pulse/knowledge/relations",
        ".pulse/policy",
        ".pulse/events",
        ".pulse/runtime",
        ".pulse/runtime/locks",
        ".pulse/runtime/transactions",
        ".pulse/cache",
        "docs",
        "works",
        "knowledge",
        "knowledge/learnings",
    ] {
        let path = repo_root.join(relative);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(PulseError::io(&path, error)),
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(PulseError::validation(
                "repository_init_path_conflict",
                format!(
                    "managed repository path must be a real directory: {}",
                    path.display()
                ),
            ));
        }
    }
    for relative in [
        ".pulse/workgraph/manifest.json",
        ".pulse/workgraph/schemas/node.schema.json",
        ".pulse/workgraph/schemas/edge.schema.json",
        ".pulse/evidence/manifest.json",
        ".pulse/docs/registry.json",
        ".pulse/knowledge/manifest.json",
        ".pulse/policy/authority.json",
        ".pulse/runtime/locks/workgraph.lock",
        "AGENTS.md",
        "PULSE.md",
    ] {
        let path = repo_root.join(relative);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(PulseError::io(&path, error)),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(PulseError::validation(
                "repository_init_path_conflict",
                format!(
                    "managed repository path must be a real file: {}",
                    path.display()
                ),
            ));
        }
    }
    reject_symlinks_recursively(&repo_root.join(".pulse/runtime/transactions"))?;
    Ok(())
}

fn reject_symlinks_recursively(root: &Path) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root).map_err(|error| PulseError::io(root, error))? {
        let entry = entry.map_err(|error| PulseError::io(root, error))?;
        let file_type = entry
            .file_type()
            .map_err(|error| PulseError::io(entry.path(), error))?;
        if file_type.is_symlink() {
            return Err(PulseError::validation(
                "repository_init_path_conflict",
                format!(
                    "managed repository path must not be a symlink: {}",
                    entry.path().display()
                ),
            ));
        }
        if file_type.is_dir() {
            reject_symlinks_recursively(&entry.path())?;
        }
    }
    Ok(())
}

fn ensure_directory(
    repo_root: &Path,
    relative: &str,
    created: &mut Vec<PathBuf>,
    preserved: &mut Vec<PathBuf>,
) -> Result<()> {
    let path = repo_root.join(relative);
    if path.exists() {
        preserved.push(path);
    } else {
        fs::create_dir_all(&path).map_err(|error| PulseError::io(&path, error))?;
        created.push(path);
    }
    Ok(())
}

/// Bootstrap `.pulse/config/runners.json` with the default role commands.
/// Existing configuration is preserved untouched.
fn ensure_runner_roles_config(
    repo_root: &Path,
    created: &mut Vec<PathBuf>,
    preserved: &mut Vec<PathBuf>,
) -> Result<()> {
    let path = repo_root.join(".pulse/config/runners.json");
    if path.exists() {
        preserved.push(path);
        return Ok(());
    }
    fs::write(&path, DEFAULT_RUNNER_ROLES_JSON).map_err(|error| PulseError::io(&path, error))?;
    created.push(path);
    Ok(())
}

/// Seed content for the domain glossary `pulse-grill` writes into.
const GLOSSARY_SEED: &str = "\
# Glossary

Terms this repository uses with a fixed meaning. Vocabulary only: what a word
denotes here, not how anything works. Rules, state machines and error
taxonomies are separate documents.

`pulse-grill` writes a term here the moment it is settled, so the next
conversation does not relitigate it.
";

/// Create `docs/domain/glossary.md` and register it as `DOC-GLOSSARY`.
///
/// Decision 0009 gives `pulse-grill` one instruction about vocabulary: record
/// a settled term immediately. That needs a destination that exists and is
/// routable before the first conversation, or the skill's first act is to
/// invent a location and every repository invents a different one.
///
/// Both halves are idempotent and never overwrite: an existing file or an
/// existing `DOC-GLOSSARY` record is preserved as the repository's own.
///
/// # Errors
/// Returns an error when the file cannot be written or the registry cannot be
/// read or updated for a reason other than the record already existing.
fn ensure_glossary(
    repo_root: &Path,
    principal: &str,
    created: &mut Vec<PathBuf>,
    preserved: &mut Vec<PathBuf>,
) -> Result<()> {
    let relative = "docs/domain/glossary.md";
    let path = repo_root.join(relative);
    if path.exists() {
        preserved.push(path.clone());
    } else {
        let parent = path.parent().expect("glossary path has a parent");
        fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
        fs::write(&path, GLOSSARY_SEED).map_err(|error| PulseError::io(&path, error))?;
        created.push(path.clone());
    }

    // Inside the init fence: the write guard is a non-reentrant flock, so both
    // the read and the write use the unlocked variants.
    let registry = crate::docs::manifest::bootstrap_unlocked(repo_root)?.registry;
    if registry
        .documents
        .iter()
        .any(|document| document.id == "DOC-GLOSSARY" || document.path == relative)
    {
        return Ok(());
    }
    let record = crate::docs::model::DocumentRecord {
        id: "DOC-GLOSSARY".to_string(),
        revision: 1,
        path: relative.to_string(),
        summary: "Terms this repository uses with a fixed meaning.".to_string(),
        owner: principal.to_string(),
        kind: crate::docs::model::DocumentKind::Domain,
        status: crate::docs::model::DocumentStatus::Approved,
        scope: crate::docs::model::DocumentScope::default(),
        tags: Vec::new(),
        generated: None,
        superseded_by: None,
    };
    crate::docs::registry::DocsRegistryStore::new(repo_root).register_unlocked(
        registry.revision,
        record,
        crate::docs::registry::OperationContext {
            actor: principal.to_string(),
            now: chrono::Utc::now(),
        },
    )?;
    Ok(())
}

fn stable_relative_paths(repo_root: &Path, paths: Vec<PathBuf>) -> Result<Vec<String>> {
    let mut relative = BTreeSet::new();
    for path in paths {
        let value = path.strip_prefix(repo_root).map_err(|_| {
            PulseError::validation(
                "repository_init_path_escape",
                format!(
                    "initialized path escaped repository root: {}",
                    path.display()
                ),
            )
        })?;
        relative.insert(value.to_string_lossy().replace('\\', "/"));
    }
    Ok(relative.into_iter().collect())
}
