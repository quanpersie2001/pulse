//! Narrow source-tree architecture guards for the refactor baseline.
//!
//! These are intentionally cheap source scans. They guard only architectural
//! seams that would be easy to break during module moves and hard to notice from
//! behavior tests alone.

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn source(path: &str) -> String {
    fs::read_to_string(repo_root().join(path)).unwrap_or_else(|error| {
        panic!("failed to read {path}: {error}");
    })
}

fn rust_sources(root: &str) -> Vec<(PathBuf, String)> {
    let root = repo_root().join(root);
    let mut pending = vec![root.clone()];
    let mut sources = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).unwrap_or_else(|error| {
            panic!("failed to read {}: {error}", dir.display());
        }) {
            let entry = entry.expect("source directory entry should be readable");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                let body = fs::read_to_string(&path).unwrap_or_else(|error| {
                    panic!("failed to read {}: {error}", path.display());
                });
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

fn store_sources() -> String {
    combined_sources(&["src/graph/store"])
}

fn graph_store_facade_sources() -> String {
    combined_sources(&["src/graph/store", "src/kernel"])
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
    // `src/cli/run.rs` and `src/runner/` are the current configured-runner
    // surface (PRODUCT §5.3/§5.8); the paths below are the daemon-era
    // runtime-authority tree that must stay absent.
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
        "src/schema/run",
        "src/schema/assignment-workspace.schema.json",
        "src/schema/prepared-assignment.schema.json",
    ] {
        assert!(
            !repo_root().join(obsolete).exists(),
            "obsolete runtime authority still exists at {obsolete}"
        );
    }
}

#[test]
fn generated_and_router_surfaces_are_absent() {
    // Decision 0009 lifted the ban on `skills/`: a guidance-only surface came
    // back. What 0007 removed stays removed — a generated `dist/`, and any
    // router or workflow engine competing with the CLI for lifecycle.
    assert!(
        !repo_root().join("dist").exists(),
        "generated surface still exists at dist"
    );

    for contract_doc in ["README.md", "CONTRIBUTING.md", "AGENTS.md"] {
        let body = source(contract_doc);
        for legacy_marker in ["pulse:workflow", "skills/workflow", "plugin marketplace"] {
            assert!(
                !body.contains(legacy_marker),
                "{contract_doc} advertises removed legacy surface `{legacy_marker}`"
            );
        }
    }
}

/// Every `pulse …` command written in guidance prose must exist in the CLI.
///
/// Decision 0009 §5: skills are guidance, the CLI is authority, so a command
/// in a skill is a literal `pulse` command. Without this guard prose drifts
/// silently — `pulse work list --status active` shipped in the AGENTS block
/// while `--status` did not exist, so the one command an agent runs to recover
/// a session failed.
///
/// The guard walks clap's real command tree rather than matching strings.
/// It checks command *prefixes*, not whole lines: guidance necessarily writes
/// `pulse work packet` without an id, because ids exist only at run time.
#[test]
fn guidance_prose_only_names_commands_the_cli_has() {
    let mut checked = 0_usize;
    for (path, body) in guidance_sources() {
        for mention in pulse_command_mentions(&body) {
            for candidate in expand_alternatives(&mention) {
                verify_command(&candidate, &path);
                checked += 1;
            }
        }
    }
    assert!(
        checked >= 10,
        "expected the guidance surface to name commands; only {checked} found. \
         A guard that checks nothing passes for the wrong reason."
    );
}

/// Markdown carrying guidance prose: the AGENTS block template plus every
/// skill.
fn guidance_sources() -> Vec<(String, String)> {
    let mut sources = vec![(
        "assets/agents-block.md".to_string(),
        source("assets/agents-block.md"),
    )];
    sources.extend(skill_markdown_sources());
    sources
}

fn skill_markdown_sources() -> Vec<(String, String)> {
    let skills = repo_root().join("skills");
    if !skills.exists() {
        return Vec::new();
    }

    let mut pending = vec![skills];
    let mut sources = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("read skills dir") {
            let path = entry.expect("skills entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                let relative = path
                    .strip_prefix(repo_root())
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                sources.push((relative, fs::read_to_string(&path).expect("read skill")));
            }
        }
    }
    sources.sort_by(|left, right| left.0.cmp(&right.0));
    sources
}

/// `pulse-planning` is the single owner of graph shape decisions.
///
/// Decision 0019 allows other skills to transition the state they gate, but
/// creating nodes or dependency edges elsewhere would duplicate the planning
/// discipline and let it drift between guidance files.
#[test]
fn only_planning_skill_can_name_node_creation_commands() {
    let sources = skill_markdown_sources();
    assert!(
        !sources.is_empty(),
        "expected the skill surface to contain Markdown"
    );

    for (path, body) in sources {
        if path.starts_with("skills/pulse-planning/") {
            continue;
        }
        for forbidden in ["pulse work create", "pulse graph edge add"] {
            assert!(
                !body.contains(forbidden),
                "{path} names `{forbidden}`; only pulse-planning may own graph shape"
            );
        }
    }
}

/// Every `pulse …` mention in `body`, each cut at the first separator that
/// cannot be part of one command.
fn pulse_command_mentions(body: &str) -> Vec<String> {
    let mut mentions = Vec::new();
    for (index, _) in body.match_indices("pulse ") {
        // Only at a word boundary, so `pulse-grill` and `.pulse/` are skipped.
        let preceded_by = body[..index].chars().next_back();
        if preceded_by.is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '/' || c == '.') {
            continue;
        }
        let rest = &body[index..];
        let end = rest
            .find(['\n', ',', '|', ')', '`', ';', '"'])
            .unwrap_or(rest.len());
        let mention = rest[..end].trim().trim_end_matches('.').trim().to_string();
        if mention.split_whitespace().count() > 1 {
            mentions.push(mention);
        }
    }
    mentions
}

/// Expand a `a/b/c` shorthand into one candidate command per alternative.
///
/// Guidance writes `pulse work list/show/packet` to name three read paths at
/// once; each alternative must exist on its own.
fn expand_alternatives(mention: &str) -> Vec<Vec<String>> {
    let tokens: Vec<&str> = mention.split_whitespace().collect();
    let mut candidates: Vec<Vec<String>> = vec![Vec::new()];
    for token in tokens {
        if token.contains('/')
            && !token.starts_with("--")
            && !token.contains('<')
            && token.split('/').all(|part| {
                !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            })
        {
            let mut expanded = Vec::new();
            for candidate in &candidates {
                for part in token.split('/') {
                    let mut next = candidate.clone();
                    next.push(part.to_string());
                    expanded.push(next);
                }
            }
            candidates = expanded;
        } else {
            for candidate in candidates.iter_mut() {
                candidate.push(token.to_string());
            }
        }
    }
    candidates
}

/// Resolve `tokens` against clap's command tree and assert every subcommand
/// and every long flag exists.
fn verify_command(tokens: &[String], origin: &str) {
    use clap::CommandFactory;
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
            let known = command_has_long(&current, flag) || command_has_long(&root, flag);
            assert!(
                known,
                "{origin}: `{}` uses --{flag}, which `{}` does not accept",
                tokens.join(" "),
                path.join(" ")
            );
            continue;
        }
        if positional_reached || token.starts_with('<') {
            // A placeholder or an argument value: subcommand walking is done.
            positional_reached = true;
            continue;
        }
        let descend = current
            .get_subcommands()
            .find(|sub| sub.get_name() == token)
            .cloned();
        if let Some(sub) = descend {
            current = sub;
            path.push(token.clone());
            continue;
        }
        // Not a subcommand. Clap's own structure says whether that is legal:
        // a command that has subcommands requires one, so an unmatched token
        // there is a typo, not a positional. A leaf command takes positionals,
        // so the token is an argument value and walking is done.
        assert!(
            current.get_subcommands().next().is_none(),
            "{origin}: `{}` names `{token}`, which is not a subcommand of `{}`. \
             Available: {}",
            tokens.join(" "),
            path.join(" "),
            current
                .get_subcommands()
                .map(clap::Command::get_name)
                .collect::<Vec<_>>()
                .join(", ")
        );
        positional_reached = true;
    }
}

/// Whether `command` declares a long flag named `long`.
fn command_has_long(command: &clap::Command, long: &str) -> bool {
    command
        .get_arguments()
        .any(|arg| arg.get_long() == Some(long))
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
fn work_content_dir_contract_is_enforced_by_rust() {
    let node_model = source("src/graph/model/node.rs");
    assert!(
        node_model.contains("content_dir: format!(\"works/{id}\")"),
        "Rust Node construction must keep the exact works/<id> content_dir rule"
    );
    let graph_validation = source("src/graph/validation/graph.rs");
    assert!(
        graph_validation.contains("Path::new(\"works\").join(&node.id)"),
        "Rust graph validation must enforce content_dir == works/<node-id>"
    );
}

#[test]
fn core_domains_do_not_depend_on_daemon_runtime() {
    for root in [
        "src/docs",
        "src/evidence",
        "src/graph",
        "src/kernel",
        "src/knowledge",
        "src/storage",
    ] {
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
fn graph_internal_tree_exposes_layered_modules_without_shims() {
    for path in [
        "src/graph/model/node.rs",
        "src/graph/model/edge.rs",
        "src/graph/model/contract.rs",
        "src/graph/model/lifecycle.rs",
        "src/graph/validation/contract.rs",
        "src/graph/validation/graph.rs",
        "src/graph/read/executability.rs",
        "src/graph/read/readiness.rs",
        "src/graph/read/frontier.rs",
        "src/graph/read/rollup.rs",
        "src/graph/read/traversal.rs",
        "src/graph/read/projection.rs",
        "src/graph/store/mod.rs",
        "src/kernel/mod.rs",
        "src/kernel/readiness.rs",
        "src/kernel/lifecycle.rs",
        "src/kernel/frontier.rs",
    ] {
        assert!(
            repo_root().join(path).exists(),
            "missing expected graph layer file {path}"
        );
    }

    for shim in [
        "src/graph/node.rs",
        "src/graph/edge.rs",
        "src/graph/contract.rs",
        "src/graph/lifecycle.rs",
        "src/graph/manifest.rs",
        "src/graph/projection.rs",
        "src/graph/shaping.rs",
        "src/graph/traversal.rs",
        "src/graph/validate.rs",
        "src/graph/readiness.rs",
        "src/graph/frontier.rs",
        "src/graph/executability.rs",
        "src/graph/rollup.rs",
        "src/graph/read/shaping.rs",
        "src/graph/store/contracts.rs",
        "src/kernel/shaping.rs",
    ] {
        assert!(
            !repo_root().join(shim).exists(),
            "graph compatibility shim must be removed: {shim}"
        );
    }
}

#[test]
fn graph_model_layer_does_not_depend_on_upper_layers() {
    for (path, src) in rust_sources("src/graph/model") {
        for forbidden in [
            "use std::fs",
            "use crate::storage",
            "crate::storage::",
            "use crate::docs",
            "crate::docs::",
            "use crate::evidence",
            "crate::evidence::",
            "use crate::policy",
            "crate::policy::",
            "use crate::graph::store",
            "crate::graph::store::",
            "use crate::graph::readiness",
            "crate::graph::readiness::",
            "use crate::graph::read::readiness",
            "crate::graph::read::readiness::",
        ] {
            assert!(
                !src.contains(forbidden),
                "{} model layer must not depend on `{forbidden}`",
                path.display()
            );
        }
    }
}

#[test]
fn graph_validation_layer_depends_on_model_not_store() {
    let combined = rust_sources("src/graph/validation")
        .into_iter()
        .map(|(_, source)| source)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        combined.contains("crate::graph::model") || combined.contains("crate::graph::node"),
        "validation layer should consume graph model types"
    );
    for forbidden in ["use crate::graph::store", "crate::graph::store::"] {
        assert!(
            !combined.contains(forbidden),
            "validation layer must not depend on `{forbidden}`"
        );
    }
}

#[test]
fn graph_pure_evaluators_do_not_import_persistence_or_filesystem_modules() {
    for (path, src) in rust_sources("src/graph/read") {
        for forbidden in [
            "use std::fs",
            "std::fs::",
            "use crate::storage",
            "crate::storage::",
            "use crate::graph::store",
            "crate::graph::store::",
        ] {
            assert!(
                !src.contains(forbidden),
                "{} pure evaluator must not import `{forbidden}`",
                path.display()
            );
        }
    }
}

#[test]
fn graph_store_cross_domain_imports_are_limited_to_compatibility_or_mutation_boundaries() {
    for (path, src) in rust_sources("src/graph/store") {
        let display = path.display().to_string();
        for forbidden in ["crate::docs::", "use crate::docs"] {
            assert!(
                !src.contains(forbidden),
                "graph store should not import docs services directly: {display}"
            );
        }
        if display.ends_with("supersession.rs") || display.ends_with("mod.rs") {
            // Supersession remains graph lifecycle mutation backed by evidence
            // receipts; `mod.rs` exposes the public ReceiptReference type.
            continue;
        }
        for forbidden in [
            "crate::evidence::",
            "use crate::evidence",
            "crate::policy::",
            "use crate::policy",
        ] {
            assert!(
                !src.contains(forbidden),
                "unexpected graph store cross-domain import `{forbidden}` in {display}"
            );
        }
    }
}

#[test]
fn cross_domain_orchestration_lives_in_kernel_not_graph_store() {
    let store = store_sources();
    let kernel = combined_sources(&["src/kernel"]);

    for forbidden in [
        "build_readiness_snapshot",
        "build_docs_applicability",
        "build_content_bindings",
        "evaluate_transition_gate",
        "transition_node_gated_with_context",
        "build_execution_readiness_reports",
    ] {
        assert!(
            !store.contains(forbidden),
            "graph store should not own cross-domain orchestration `{forbidden}`"
        );
        assert!(
            kernel.contains(forbidden),
            "kernel should own cross-domain orchestration `{forbidden}`"
        );
    }

    assert!(
        !store.contains("crate::docs::"),
        "graph store should not import documentation services directly"
    );
    assert!(
        kernel.contains("crate::docs::")
            && kernel.contains("crate::evidence::")
            && kernel.contains("crate::policy::"),
        "kernel should compose docs, evidence and policy for readiness/lifecycle"
    );
}

#[test]
fn read_only_domain_entrypoints_remain_store_methods_backed_by_pure_modules() {
    let store = graph_store_facade_sources();
    for method in [
        "pub fn show_node(&self, id: &str)",
        "pub fn list_nodes(&self, filter: &NodeFilter)",
        "pub fn validate(&self)",
        "pub fn export(&self)",
        "pub fn executability(&self, id: &str)",
        "pub fn rollup(&self, id: &str)",
        "pub fn neighborhood(&self, id: &str, depth: usize)",
        "pub fn affected_by(",
        "pub fn readiness(&self, id: &str)",
        "pub fn frontier(",
    ] {
        assert!(
            store.contains(method),
            "JsonGraphStore lost read-only entrypoint containing `{method}`"
        );
    }

    for pure_call in [
        "structural_executability(",
        "rollup(&projection, id)",
        "neighborhood(&projection, id, depth)",
        "affected_by(&projection, id, relation_filter)",
        "evaluate_readiness(",
        "frontier::project_execution_frontier(",
    ] {
        assert!(
            store.contains(pure_call),
            "JsonGraphStore read-only entrypoints should keep delegating through `{pure_call}`"
        );
    }
}

#[test]
fn identity_module_owns_shared_actor_types() {
    let actor = source("src/identity/actor.rs");
    assert!(
        actor.contains("pub struct ActorRef"),
        "neutral identity::actor should own ActorRef"
    );
    assert!(
        actor.contains("pub enum ActorKind"),
        "neutral identity::actor should own ActorKind"
    );
    assert!(
        source("src/identity/mod.rs").contains("pub mod actor;"),
        "identity module should expose the actor submodule"
    );
    // Evidence re-exports the neutral identity vocabulary so the historical
    // `pulse::evidence::model::{ActorRef, ActorKind}` path stays stable.
    assert!(
        source("src/evidence/model.rs").contains("pub use crate::identity::actor"),
        "evidence::model should re-export the neutral identity actor types"
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
    assert!(
        !id.contains("pub fn edge_id"),
        "dead edge_id helper should be removed from id"
    );
    // Work/learning identity stays under the neutral work-identity owner.
    assert!(id.contains("pub enum WorkKind"));
    assert!(id.contains("pub struct WorkId"));
    assert!(id.contains("pub fn format_id"));
}

#[test]
fn storage_generic_primitives_do_not_import_graph_domain() {
    // Generic storage (atomic/lock/paths/transaction) must stay below the graph
    // domain. Only `storage/mod.rs` may reference graph, and only through the
    // compatibility re-export of workgraph bootstrap ownership.
    for (path, src) in rust_sources("src/storage") {
        let display = path.display().to_string();
        if display.ends_with("mod.rs") {
            continue;
        }
        for forbidden in ["crate::graph", "use crate::graph"] {
            assert!(
                !src.contains(forbidden),
                "generic storage primitive {display} must not depend on `{forbidden}`"
            );
        }
    }
}

#[test]
fn workgraph_bootstrap_ownership_lives_in_graph_store() {
    let bootstrap = source("src/graph/store/bootstrap.rs");
    assert!(
        bootstrap.contains("pub fn bootstrap(repo_root: &Path)"),
        "graph::store::bootstrap should own the workgraph bootstrap function"
    );
    assert!(
        bootstrap.contains("crate::graph::model::manifest"),
        "graph::store::bootstrap should source schema templates from graph::model::manifest"
    );
    let storage = source("src/storage/mod.rs");
    assert!(
        storage.contains("pub use crate::graph::store::"),
        "storage should re-export workgraph bootstrap through the graph store facade"
    );
    assert!(
        !storage.contains("crate::graph::manifest"),
        "storage must no longer import graph::manifest directly; ownership moved to graph"
    );
    // Generic primitives remain in storage.
    for primitive in [
        "pub fn atomic_write",
        "pub fn read_json",
        "pub fn create_new",
        "pub fn safe_repo_relative",
    ] {
        assert!(
            storage.contains(primitive),
            "storage should retain generic primitive `{primitive}`"
        );
    }
}
