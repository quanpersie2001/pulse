//! Persistence for Pulse v3 records (plan 0022 §3-4).
//!
//! `issues` is the only store: one `.pulse/issues.jsonl` holding every
//! epic/story/ticket/decision record. Nothing above `storage` may bypass it
//! to touch `issues.jsonl` directly.

pub mod issues;
