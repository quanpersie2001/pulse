//! Application composition layer for cross-domain Pulse operations.
//!
//! `graph::{model,validation,read}` stay as graph-owned pure/value layers.
//! This module is the sanctioned place for coherent operations that compose the
//! graph store with documentation, evidence, policy and source/content checks.

pub mod communication;
pub mod completion;
pub mod documentation;
pub mod frontier;
pub(crate) mod guidance;
pub(crate) mod init;
pub mod lifecycle;
pub mod packet;
pub mod readiness;
pub mod ready;
pub mod reservation;
pub mod roles;
pub mod run;
pub mod story_completion;

pub use run::DEFAULT_RUN_TTL_SECONDS;
