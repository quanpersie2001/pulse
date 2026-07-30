//! Long-lived Pulse runtime control plane.
//!
//! Core remains independently usable and owns repository semantics. The daemon
//! owns host-local projects, workspaces, provider sessions, managed processes,
//! assignment provisioning state and the authoritative runtime timeline.

pub mod application;
pub mod assignment;
pub mod permissions;
pub mod persistence;
pub mod process;
pub mod project;
pub mod protocol;
pub mod provider;
pub mod session;
pub mod timeline;
pub mod transport;
pub mod workspace;

pub use application::DaemonApplication;
pub use persistence::{daemon_home, DaemonState, StateStore};
pub use protocol::{DaemonRequest, DaemonResponse};
