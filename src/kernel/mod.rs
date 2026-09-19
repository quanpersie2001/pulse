//! Application composition layer for cross-domain Pulse v3 operations.
//!
//! `store::issues` stays the pure(ish) persistence layer; this module is
//! where store, event log, roles, evidence, lane and profile compose.
//!
//! No module here dispatches an agent. Pulse owns the graph, the gates and
//! the evidence, and dispatching an agent is the host's job; every step an
//! agent takes enters through a CLI command (`claim`, `packet`, `checkpoint`,
//! `handoff`, `lane input`, `lane seal`, `close`) rather than through a
//! runner Pulse drives. `verify` is the one place Pulse spawns anything, and
//! only argv a record already declares: it observes and records, it never
//! chooses a command or drives a session (decision 0026).

pub mod checkpoint;
pub mod completion;
pub mod doctor;
pub mod frontier;
pub mod hook;
pub mod init;
pub mod issues;
pub mod lane;
pub mod metrics;
pub mod packet;
pub mod profile;
pub mod ready;
pub mod reservation;
pub mod roles;
pub mod scope;
pub mod skills;
pub mod verify;
