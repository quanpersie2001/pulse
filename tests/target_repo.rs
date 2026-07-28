//! Target-repository fixture integration tests.
//!
//! Covers the shared `tests/common/fixture_repo` helper and the tracked
//! `tests/fixtures/target-repos/minimal-service` template used to run Pulse
//! against isolated target repositories from integration tests. The submodule
//! is explicitly wired from `tests/target_repo/`.

#[path = "common/mod.rs"]
mod common;

#[path = "target_repo/target_repo_fixture.rs"]
mod target_repo_fixture;
#[path = "target_repo/work_packet_target_repo.rs"]
mod work_packet_target_repo;
