//! Evidence integration tests.
//!
//! Exercises `src/evidence` receipt and artifact coverage. The submodule is
//! explicitly wired from `tests/evidence/`.

#[path = "evidence/evidence_receipts.rs"]
mod evidence_receipts;
#[path = "evidence/redaction.rs"]
mod redaction;
// The shared fixture helpers cover several crates; the evidence crate uses
// a subset, so unused helpers are expected here.
#[allow(dead_code)]
#[path = "common/bin.rs"]
mod common_bin;
#[allow(dead_code)]
#[path = "common/fixture_repo.rs"]
mod common_fixture_repo;
