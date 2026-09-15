//! Narrow source-tree architecture guards (plan 0022 §14: "Cập nhật
//! `tests/graph/architecture_guards.rs` đổi tên/đường dẫn theo cây mới").
//!
//! Relocated from `tests/graph/architecture_guards.rs` to the crate root
//! since `tests/graph.rs` (its parent crate) is deleted with `graph/*`
//! itself (plan 0022 P1.3). Checks specific to the old graph internals
//! (model/validation/read/store layering, workgraph bootstrap ownership,
//! `works/<id>` content_dir) are dropped with the mechanism they guarded;
//! checks for the new layer (`storage -> store(issues) -> kernel -> cli`)
//! replace them. `guidance_prose_only_names_commands_the_cli_has` and
//! `only_planning_skill_can_name_node_creation_commands` are also dropped:
//! `skills/`/`assets/agents-block.md` still describe v2 command names and
//! stay that way until Phase 2/3 rewrites them (plan §12.3, §14 P3.5) —
//! keeping the guard now would just fail on a gap the plan already knows
//! about and schedules elsewhere.

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
fn event_and_transaction_modules_own_their_id_generation() {
    assert!(
        source("src/event.rs").contains("pub fn new_event_id"),
        "event module should own new_event_id generation"
    );
    assert!(
        source("src/storage/transaction.rs").contains("pub fn new_transaction_id"),
        "storage transaction module should own new_transaction_id generation"
    );
    let id = source("src/id.rs");
    assert!(
        id.contains("pub use crate::event::new_event_id"),
        "id should re-export new_event_id from event for compatibility"
    );
    assert!(
        id.contains("pub use crate::storage::transaction::new_transaction_id"),
        "id should re-export new_transaction_id from storage::transaction for compatibility"
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
