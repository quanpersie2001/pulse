//! Application composition layer for cross-domain Pulse operations.
//!
//! `graph::{model,validation,read}` stay as graph-owned pure/value layers.
//! This module is the sanctioned place for coherent operations that compose the
//! graph store with documentation, evidence, policy and source/content checks.

pub mod frontier;
pub mod lifecycle;
pub mod readiness;
pub mod shaping;
