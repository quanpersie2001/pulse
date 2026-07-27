//! Shared helpers for integration tests.
//!
//! Each integration-test crate (`tests/<domain>.rs`) is a separate binary, so a
//! helper compiled into one crate is invisible to the others. To share code here
//! without producing `dead_code` warnings under `-D warnings`, helpers are split
//! into self-contained files and each crate includes only the files it uses via a
//! direct `#[path = "common/<file>.rs"] mod <name>;` declaration at its crate
//! root.
//!
//! - `fixture_repo` is declared here as a normal submodule and pulled in by
//!   `tests/target_repo.rs` (which uses the whole `TestRepo` surface).
//! - `bin.rs`, `git.rs` and `canon.rs` are standalone includable units (CLI
//!   binary resolver, git plumbing, canonical-JSON writer) wired selectively
//!   into the `graph`, `docs` and `knowledge` crates.

pub mod fixture_repo;
