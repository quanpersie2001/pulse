//! Documentation-system integration tests.
//!
//! Groups every `docs_*` coverage area: registry, index, projection,
//! applicability, impact, retrieval, search/get/tree and concurrency/recovery.
//! Each submodule is explicitly wired from `tests/docs/`.
//!
//! Shared mechanical helpers (CLI binary resolver, git plumbing, canonical-JSON
//! writer) live in `tests/common` and are wired selectively below; each included
//! helper is used by at least one submodule.

#[path = "common/bin.rs"]
mod common_bin;
#[path = "common/canon.rs"]
mod common_canon;
#[path = "common/git.rs"]
mod common_git;

#[path = "docs/docs_applicability.rs"]
mod docs_applicability;
#[path = "docs/docs_cli_contract.rs"]
mod docs_cli_contract;
#[path = "docs/docs_crash_recovery.rs"]
mod docs_crash_recovery;
#[path = "docs/docs_impact.rs"]
mod docs_impact;
#[path = "docs/docs_index.rs"]
mod docs_index;
#[path = "docs/docs_index_concurrency.rs"]
mod docs_index_concurrency;
#[path = "docs/docs_process_concurrency.rs"]
mod docs_process_concurrency;
#[path = "docs/docs_projection.rs"]
mod docs_projection;
#[path = "docs/docs_receipt_registry.rs"]
mod docs_receipt_registry;
#[path = "docs/docs_registry.rs"]
mod docs_registry;
#[path = "docs/docs_registry_schema.rs"]
mod docs_registry_schema;
#[path = "docs/docs_retrieval_eval.rs"]
mod docs_retrieval_eval;
#[path = "docs/docs_search_get_tree.rs"]
mod docs_search_get_tree;
#[path = "docs/docs_section_extraction.rs"]
mod docs_section_extraction;
