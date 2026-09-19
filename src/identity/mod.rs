//! Neutral identity ownership.
//!
//! Shared actor/principal identity vocabulary that crosses the evidence, event,
//! policy and kernel domains. Keeping it here avoids letting any one consuming
//! domain own the types the others depend on.

pub mod actor;

pub use actor::{ActorKind, ActorRef};
