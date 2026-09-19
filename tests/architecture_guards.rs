//! Narrow source-tree architecture guards (plan 0022 §14: "Cập nhật
//! `tests/graph/architecture_guards.rs` đổi tên/đường dẫn theo cây mới").
//!
//! Relocated from `tests/graph/architecture_guards.rs` to the crate root
//! since `tests/graph.rs` (its parent crate) is deleted with `graph/*`
//! itself (plan 0022 P1.3). Checks specific to the old graph internals
//! (model/validation/read/store layering, workgraph bootstrap ownership,
//! `works/<id>` content_dir) are dropped with the mechanism they guarded;
//! checks for the new layer (`storage -> store(issues) -> kernel -> cli`)
//! replace them. `only_planning_skill_can_name_node_creation_commands` is
//! also dropped: the v2 skill drafts that guard covered lived under
//! `references/pulse-v2-skills/` (F3, P1.12) as raw material for
//! `pulse-shape`/`pulse-plan`; that directory is deleted at P3.5 (ST-2 ran
//! the golden path through the new skills), and the guard is back (narrowed
//! to command parsing, no node-creation ownership rule) now that
//! `skills/pulse-shape` + `skills/pulse-plan` are a live surface — plan
//! §12.1: every `pulse …` named in the block and in `skills/**` must parse
//! against the real CLI.
//! `guidance_prose_only_names_commands_the_cli_has` is back, narrowed to the
//! AGENTS block `pulse init` now writes (P1.10, plan §12.1).

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn source(path: &str) -> String {
    fs::read_to_string(repo_root().join(path))
        .unwrap_or_else(|error| panic!("failed to read {path}: {error}"))
}

fn rust_sources(root: &str) -> Vec<(PathBuf, String)> {
    let root = repo_root().join(root);
    let mut pending = vec![root.clone()];
    let mut sources = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", dir.display()))
        {
            let entry = entry.expect("source directory entry should be readable");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                let body = fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
                sources.push((path, body));
            }
        }
    }
    sources.sort_by(|left, right| left.0.cmp(&right.0));
    sources
}

fn combined_sources(roots: &[&str]) -> String {
    roots
        .iter()
        .flat_map(|root| rust_sources(root))
        .map(|(_, source)| source)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn daemon_runtime_tree_is_absent() {
    assert!(
        !repo_root().join("src/daemon").exists(),
        "src/daemon must no longer exist in the source tree"
    );
    let library = source("src/lib.rs");
    for obsolete in [
        "pub mod daemon;",
        "pub mod run;",
        "pub mod process;",
        "pub mod workspace;",
        "pub mod assignment;",
    ] {
        assert!(
            !library.contains(obsolete),
            "obsolete public runtime contract remains: {obsolete}"
        );
    }
    for obsolete in [
        "src/process.rs",
        "src/run.rs",
        "src/workspace.rs",
        "src/assignment.rs",
        "src/cli/process.rs",
        "src/cli/daemon.rs",
        "src/kernel/assignment.rs",
        "src/kernel/assignment_store.rs",
        "src/kernel/run_store.rs",
    ] {
        assert!(
            !repo_root().join(obsolete).exists(),
            "obsolete runtime authority still exists at {obsolete}"
        );
    }
}

#[test]
fn generated_and_router_surfaces_are_absent() {
    assert!(
        !repo_root().join("dist").exists(),
        "generated surface still exists at dist"
    );
    for contract_doc in ["README.md", "CONTRIBUTING.md", "AGENTS.md"] {
        let path = repo_root().join(contract_doc);
        if !path.exists() {
            continue;
        }
        let body = source(contract_doc);
        for legacy_marker in ["pulse:workflow", "skills/workflow", "plugin marketplace"] {
            assert!(
                !body.contains(legacy_marker),
                "{contract_doc} advertises removed legacy surface `{legacy_marker}`"
            );
        }
    }
}

#[test]
fn current_contract_names_do_not_embed_version_suffixes() {
    for (path, body) in rust_sources("src") {
        for token in
            body.split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        {
            let uppercase_suffix = token.rsplit_once('V').is_some_and(|(base, suffix)| {
                !base.is_empty()
                    && base.chars().next().is_some_and(char::is_uppercase)
                    && !suffix.is_empty()
                    && suffix.chars().all(|character| character.is_ascii_digit())
            });
            let snake_suffix = token.rsplit_once("_v").is_some_and(|(base, suffix)| {
                !base.is_empty()
                    && !suffix.is_empty()
                    && suffix.chars().all(|character| character.is_ascii_digit())
            });
            assert!(
                !uppercase_suffix && !snake_suffix,
                "{} contains version-suffixed current contract name `{token}`",
                path.display()
            );
        }
    }
}

#[test]
fn core_domains_do_not_depend_on_daemon_runtime() {
    for root in ["src/evidence", "src/kernel", "src/storage", "src/store"] {
        for (path, body) in rust_sources(root) {
            assert!(
                !body.contains("crate::daemon"),
                "{} must not import daemon runtime ownership",
                path.display()
            );
        }
    }
}

#[test]
fn cli_has_no_direct_runtime_launch_path() {
    let cli = combined_sources(&["src/cli"]);
    for forbidden in [
        "PULSE_CODEX_EXECUTABLE",
        "\"app-server\"",
        "__run-supervisor",
        "RunRecord",
        "RunnerProfile",
    ] {
        assert!(
            !cli.contains(forbidden),
            "CLI must not contain a direct provider/runtime path: {forbidden}"
        );
    }
}

#[test]
fn cli_binary_remains_thin_adapter_over_public_library_paths() {
    let binary = source("src/bin/pulse.rs");
    assert!(
        binary.contains("use pulse::cli;"),
        "src/bin/pulse.rs should delegate through the library CLI facade"
    );
    assert!(
        binary.contains("cli::run(cli::parse())"),
        "src/bin/pulse.rs should only parse and run through the library CLI facade"
    );
    assert!(
        binary.lines().count() <= 100,
        "src/bin/pulse.rs should remain a thin adapter under 100 LOC"
    );
    for forbidden in [
        "JsonGraphStore",
        "show_node(",
        "create_node",
        "add_edge(",
        "pulse::docs::",
        "pulse::evidence::",
        "pulse::knowledge::",
        "mod graph",
        "#[path",
    ] {
        assert!(
            !binary.contains(forbidden),
            "CLI binary must not contain direct domain/store wiring: {forbidden}"
        );
    }
}

#[test]
fn identity_module_owns_shared_actor_types() {
    let actor = source("src/identity/actor.rs");
    assert!(
        actor.contains("pub struct ActorRef"),
        "identity::actor should own ActorRef"
    );
    assert!(
        actor.contains("pub enum ActorKind"),
        "identity::actor should own ActorKind"
    );
    assert!(
        source("src/identity/mod.rs").contains("pub mod actor;"),
        "identity module should expose the actor submodule"
    );
}

#[test]
fn event_module_owns_its_id_generation() {
    // The v2 transaction machinery (and with it new_transaction_id) went
    // with storage/transaction.rs — 0022-open A7, deleted 2026-09-16.
    assert!(
        source("src/event.rs").contains("pub fn new_event_id"),
        "event module should own new_event_id generation"
    );
    let id = source("src/id.rs");
    assert!(
        id.contains("pub use crate::event::new_event_id"),
        "id should re-export new_event_id from event for compatibility"
    );
    assert!(id.contains("pub enum WorkKind"));
    assert!(id.contains("pub struct WorkId"));
    assert!(
        id.contains("pub fn generate_hash_id"),
        "id should own the plan 0022 §4.2 hash-id generator"
    );
}

#[test]
fn storage_does_not_depend_on_the_graph_domain_or_kernel() {
    // graph/* is deleted (plan 0022 P1.3); storage must never have needed
    // it back, and must stay below store/kernel in the new layer order
    // (storage -> store(issues) -> kernel -> cli).
    for (path, src) in rust_sources("src/storage") {
        for forbidden in ["crate::graph", "crate::store", "crate::kernel"] {
            assert!(
                !src.contains(forbidden),
                "generic storage primitive {} must not depend on `{forbidden}`",
                path.display()
            );
        }
    }
}

#[test]
fn store_issues_does_not_depend_on_kernel_or_cli() {
    for (path, src) in rust_sources("src/store") {
        for forbidden in ["crate::kernel", "crate::cli"] {
            assert!(
                !src.contains(forbidden),
                "{} must not depend on `{forbidden}` (store sits below kernel and cli)",
                path.display()
            );
        }
    }
}

#[test]
fn kernel_does_not_depend_on_cli() {
    for (path, src) in rust_sources("src/kernel") {
        assert!(
            !src.contains("crate::cli"),
            "{} must not depend on crate::cli (kernel sits below cli)",
            path.display()
        );
    }
}

/// Scans `text` for every `pulse <subcommand> ...` mention and parses it
/// against the real CLI (via clap), panicking (naming `source_label`) on an
/// unknown subcommand or flag. Returns how many mentions it actually
/// checked, so a caller can assert it found a sane minimum rather than
/// silently checking nothing. Shared by the AGENTS block guard and the
/// template guard below (plan §8.5: "guard test parse mọi `pulse …`
/// trong `templates/**` bằng clap (cùng cơ chế guard khối AGENTS)").
fn assert_pulse_mentions_parse(source_label: &str, text: &str) -> usize {
    use clap::CommandFactory;

    let mut checked = 0_usize;
    for (index, _) in text.match_indices("pulse ") {
        let preceded_by = text[..index].chars().next_back();
        if preceded_by.is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '/' || c == '.') {
            continue;
        }
        let rest = &text[index..];
        let end = rest
            .find(['\n', ',', '|', ')', '`', ';', '"'])
            .unwrap_or(rest.len());
        let mention = rest[..end].trim().trim_end_matches('.').trim();
        let tokens: Vec<&str> = mention.split_whitespace().collect();
        if tokens.len() <= 1 {
            continue;
        }
        checked += 1;

        let root = pulse::cli::Cli::command();
        let mut current = root.clone();
        let mut path = vec!["pulse".to_string()];
        let mut positional_reached = false;
        for token in tokens.iter().skip(1) {
            if token.starts_with("--") {
                let flag = token
                    .trim_start_matches("--")
                    .split('=')
                    .next()
                    .unwrap_or("");
                let known = current
                    .get_arguments()
                    .any(|arg| arg.get_long() == Some(flag))
                    || root.get_arguments().any(|arg| arg.get_long() == Some(flag));
                assert!(
                    known,
                    "{source_label}: `{mention}` uses --{flag}, which `{}` does not accept",
                    path.join(" ")
                );
                continue;
            }
            if positional_reached || token.starts_with('<') {
                positional_reached = true;
                continue;
            }
            let descend = current
                .get_subcommands()
                .find(|sub| sub.get_name() == *token)
                .cloned();
            if let Some(sub) = descend {
                current = sub;
                path.push((*token).to_string());
                continue;
            }
            assert!(
                current.get_subcommands().next().is_none(),
                "{source_label}: `{mention}` names `{token}`, which is not a subcommand of `{}`",
                path.join(" ")
            );
            positional_reached = true;
        }
    }
    checked
}

/// Plan §12.1: the AGENTS block `pulse init` writes is guidance prose, so
/// (Decision 0009 §5, still true in v3) every `pulse …` command it names
/// must be a real command. Without this guard the block can drift silently
/// from the CLI it documents.
#[test]
fn agents_block_only_names_commands_the_cli_has() {
    let repo = tempfile::tempdir().unwrap();
    pulse::kernel::init::initialize_repository(repo.path(), false, false).unwrap();
    let agents = fs::read_to_string(repo.path().join("AGENTS.md")).unwrap();

    let checked = assert_pulse_mentions_parse("AGENTS.md", &agents);
    assert!(
        checked >= 8,
        "expected the AGENTS block to name several commands; only {checked} found"
    );
}

/// Plan §8.5: the worker/review prompt templates `pulse init` seeds under
/// `.pulse/prompts/` are guidance prose, and name real `pulse` commands
/// (`checkpoint`, `handoff`, `note`) the agent following them is expected
/// to run — the same drift risk the AGENTS block guard covers. A8.4 moved
/// everything `pulse init` writes into a target repo under `templates/`,
/// so the guard now covers every markdown template there (prompts, the
/// AGENTS block seed, the PULSE.md/docs seeds) — not just `prompts/`.
#[test]
fn templates_only_name_commands_the_cli_has() {
    let templates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let mut pending = vec![templates_dir.clone()];
    let mut total_checked = 0_usize;
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let text = fs::read_to_string(&path).unwrap();
            let label = path.display().to_string();
            total_checked += assert_pulse_mentions_parse(&label, &text);
        }
    }
    assert!(
        total_checked >= 4,
        "expected the templates to name several commands; only {total_checked} found"
    );
}

/// Plan §12.1: `skills/pulse-shape` + `skills/pulse-plan` are a live
/// guidance surface (the v2 skills they replaced — raw material once kept
/// under `references/pulse-v2-skills/`, guarded by nothing — were deleted
/// at P3.5), so every `pulse …` command they name must parse against the
/// real CLI — same drift risk, and same mechanism, as the AGENTS block
/// guard above.
#[test]
fn skills_only_name_commands_the_cli_has() {
    let skills_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("skills");
    let mut pending = vec![skills_dir.clone()];
    let mut total_checked = 0_usize;
    let mut files = 0_usize;
    while let Some(dir) = pending.pop() {
        let entries = fs::read_dir(&dir)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", dir.display()));
        for entry in entries {
            let path = entry
                .expect("skills directory entry should be readable")
                .path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            files += 1;
            let text = fs::read_to_string(&path).unwrap();
            let label = path.display().to_string();
            total_checked += assert_pulse_mentions_parse(&label, &text);
        }
    }
    assert!(
        files >= 2,
        "expected the skills/ tree to hold at least the pulse-shape and pulse-plan SKILL.md files; found {files}"
    );
    assert!(
        total_checked >= 6,
        "expected the skills to name several commands; only {total_checked} found"
    );
}

/// Decision 0023: `serve` is a read-only composition domain over the same
/// storage/store/event/evidence reads the CLI uses — it must not reach
/// into `kernel` (which owns mutations) or `cli` (which owns transport),
/// or the read-only guarantee gets a mutation path by accident.
#[test]
fn serve_does_not_depend_on_kernel_or_cli() {
    for (path, source) in rust_sources("src/serve") {
        for line in source.lines() {
            let trimmed = line.trim_start();
            assert!(
                !trimmed.starts_with("use crate::kernel"),
                "{}: serve must not depend on kernel: {line}",
                path.display()
            );
            assert!(
                !trimmed.starts_with("use crate::cli"),
                "{}: serve must not depend on cli: {line}",
                path.display()
            );
        }
    }
}
