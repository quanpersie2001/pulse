//! Knowledge-store integration tests.
//!
//! Exercises `src/knowledge`: boundary, store, relations, projection cache,
//! CLI contract, concurrency and crash-recovery coverage. Each submodule is
//! explicitly wired from `tests/knowledge/`.
//!
//! The shared CLI binary resolver lives in `tests/common` and is wired below;
//! the knowledge crate's own `write_json` writes raw `serde_json::Value` (a
//! different contract from the canonical writer), so it stays local.

#[path = "common/bin.rs"]
mod common_bin;

#[path = "knowledge/knowledge_boundary.rs"]
mod knowledge_boundary;
#[path = "knowledge/knowledge_cli_contract.rs"]
mod knowledge_cli_contract;
#[path = "knowledge/knowledge_crash_recovery_process.rs"]
mod knowledge_crash_recovery_process;
#[path = "knowledge/knowledge_process_concurrency.rs"]
mod knowledge_process_concurrency;
#[path = "knowledge/knowledge_projection_cache.rs"]
mod knowledge_projection_cache;
#[path = "knowledge/knowledge_relations.rs"]
mod knowledge_relations;
#[path = "knowledge/knowledge_store.rs"]
mod knowledge_store;
