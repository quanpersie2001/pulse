//! Target-repository fixture integration tests.
//!
//! Covers the shared `tests/common/fixture_repo` helper, the tracked
//! `tests/fixtures/target-repos/minimal-service` template, and `pulse init`
//! against an isolated target repository. Each submodule is explicitly wired
//! from `tests/target_repo/`.

#[path = "common/mod.rs"]
mod common;
#[allow(dead_code)]
#[path = "common/bin.rs"]
mod common_bin;

#[path = "target_repo/repository_init.rs"]
mod repository_init;
#[path = "target_repo/target_repo_fixture.rs"]
mod target_repo_fixture;
