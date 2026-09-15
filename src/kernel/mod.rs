//! Application composition layer for cross-domain Pulse v3 operations.
//!
//! `store::issues` stays the pure(ish) persistence layer; this module is
//! where store, event log, roles and (later) evidence/lane/profile compose.
//! Plan 0022 §14 Phase 1 is rebuilding this layer from scratch — most of the
//! v2 kernel modules that used to live here (`completion`, `packet`, `run`,
//! `reservation`, `lifecycle`, `readiness`, `frontier`, `story_completion`,
//! `communication`, `documentation`, `guidance`) are deleted rather than
//! carried forward; their v3 replacements land in later P1.x commits.

pub mod checkpoint;
pub mod completion;
pub(crate) mod init;
pub mod issues;
pub mod lane;
pub mod packet;
pub mod profile;
pub mod ready;
pub mod reservation;
pub mod roles;
pub mod run;
