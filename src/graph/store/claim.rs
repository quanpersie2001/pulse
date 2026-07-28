//! Claim pipeline re-exports (P2S2-I8).
//!
//! The full `JsonGraphStore::claim_work` implementation lives in
//! `src/kernel/assignment.rs` — the sanctioned cross-domain composition
//! layer. This module re-exports the public types only, keeping graph/store
//! free of docs/policy/evidence imports per the architecture guards.
//!
//! See `proposals/phase2-slice2-atomic-reservation-workspace-binding.md`.

pub use crate::kernel::assignment::{ClaimArgs, ClaimWorkOutcome};
