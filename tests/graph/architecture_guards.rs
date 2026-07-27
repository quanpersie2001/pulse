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
fn graph_internal_tree_exposes_layered_modules_with_compatibility_shims() {
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
        "src/kernel/shaping.rs",
        "src/kernel/lifecycle.rs",
        "src/kernel/frontier.rs",
    ] {
        assert!(
            repo_root().join(path).exists(),
            "missing expected graph layer file {path}"
        );
    }

    for (shim, target) in [
        ("src/graph/node.rs", "pub use crate::graph::model::node::*;"),
        ("src/graph/edge.rs", "pub use crate::graph::model::edge::*;"),
        (
            "src/graph/contract.rs",
            "pub use crate::graph::model::contract::*;",
        ),
        (
            "src/graph/lifecycle.rs",
            "pub use crate::graph::model::lifecycle::*;",
        ),
        (
            "src/graph/validate.rs",
            "pub use crate::graph::validation::graph::*;",
        ),
        (
            "src/graph/readiness.rs",
            "pub use crate::graph::read::readiness::*;",
        ),
        (
            "src/graph/frontier.rs",
            "pub use crate::graph::read::frontier::*;",
        ),
        (
            "src/graph/executability.rs",
            "pub use crate::graph::read::executability::*;",
        ),
    ] {
        assert!(
            source(shim).contains(target),
            "{shim} should remain a compatibility re-export"
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
        if display.ends_with("contracts.rs") {
            // QA-impact writes still authority-gate local contract mutations.
            continue;
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
        "build_shaping_snapshot",
        "build_decision_proofs",
        "build_docs_applicability",
        "build_content_bindings",
        "evaluate_transition_gate",
        "apply_shaping_with_context",
        "transition_node_gated_with_context",
        "build_execution_readiness_reports",
        "build_decision_branch_contexts",
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
        "kernel should compose docs, evidence and policy for readiness/shaping/lifecycle"
    );
}

#[test]
fn read_only_domain_entrypoints_remain_store_methods_backed_by_pure_modules() {
    let store = graph_store_facade_sources();
    for method in [
        "pub fn show_node(&self, id: &str)",
        "pub fn list_nodes(&self, kind: Option<WorkKind>)",
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
        "frontier::project_decision_frontier(",
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
        bootstrap.contains("crate::graph::manifest"),
        "graph::store::bootstrap should source schema templates from graph::manifest"
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
