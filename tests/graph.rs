//! Work-graph integration tests.
//!
//! This crate covers the deterministic, graph-owned domains: core work graph,
//! lifecycle, shaping, readiness, frontiers, node schema, authority policy,
//! decision acceptance and the CLI/contract surfaces. Each submodule is
//! explicitly wired from `tests/graph/`.
//!
//! Subprocess-spawning, timing-sensitive suites (multi-process CAS, failpoint
//! crash recovery, supersession process recovery) live in the dedicated
//! `tests/process.rs` crate so per-crate parallelism stays bounded and the
//! process/recovery surface has its own focused runnable target.
//!
//! Shared mechanical helpers (CLI binary resolver, git plumbing, canonical-JSON
//! writer) live in `tests/common` and are wired selectively below. Each included
//! helper is used by at least one submodule, so there are no dead-code warnings.

#[path = "common/bin.rs"]
mod common_bin;
#[path = "common/canon.rs"]
mod common_canon;
#[allow(dead_code)]
#[path = "common/fixture_repo.rs"]
mod common_fixture_repo;
#[path = "common/git.rs"]
mod common_git;

#[path = "graph/architecture_guards.rs"]
mod architecture_guards;
#[path = "graph/authority_policy.rs"]
mod authority_policy;
#[path = "graph/claim.rs"]
mod claim;
#[path = "graph/cli_lifecycle_contract.rs"]
mod cli_lifecycle_contract;
#[path = "graph/decision_acceptance.rs"]
mod decision_acceptance;
#[path = "graph/event_fingerprints.rs"]
mod event_fingerprints;
#[path = "graph/frontier.rs"]
mod frontier;
#[path = "graph/lifecycle.rs"]
mod lifecycle;
#[path = "graph/node_schema.rs"]
mod node_schema;
#[path = "graph/public_api_paths.rs"]
mod public_api_paths;
#[path = "graph/readiness.rs"]
mod readiness;
#[path = "graph/readiness_cli_contract.rs"]
mod readiness_cli_contract;
#[path = "graph/run_feasibility_contract.rs"]
mod run_feasibility_contract;
#[path = "graph/run_model.rs"]
mod run_model;
#[path = "graph/run_profile_registry.rs"]
mod run_profile_registry;
#[path = "graph/shaping_cli_contract.rs"]
mod shaping_cli_contract;
#[path = "graph/shaping_contract.rs"]
mod shaping_contract;
#[path = "graph/shaping_mutation.rs"]
mod shaping_mutation;
#[path = "graph/work_packet_cli_contract.rs"]
mod work_packet_cli_contract;
#[path = "graph/workgraph.rs"]
mod workgraph;
#[path = "graph/workgraph_read_models.rs"]
mod workgraph_read_models;
#[path = "graph/workgraph_transaction.rs"]
mod workgraph_transaction;
