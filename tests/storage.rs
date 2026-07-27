//! Storage-layer integration tests.
//!
//! Groups the storage primitive coverage that exercises `src/storage`. Each
//! submodule is explicitly wired from `tests/storage/` because a crate-root
//! integration test resolves bare `mod` declarations against `tests/`.

#[path = "storage/storage_primitives.rs"]
mod storage_primitives;
