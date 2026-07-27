//! Neutral actor/principal identity types.
//!
//! `ActorRef` and `ActorKind` are shared across the evidence, event, policy and
//! kernel domains. They live here so no single consuming domain owns the
//! identity vocabulary. The persisted serialization shape is unchanged: this is
//! pure ownership relocation, and [`crate::evidence::model`] re-exports both
//! types for compatibility with the historical `pulse::evidence::model::*`
//! path used by receipts, tests and the CLI.

use serde::{Deserialize, Serialize};

/// Typed reference to the actor that performed or authorized an action.
///
/// Serialization is stable: `{"kind": "human|agent|system", "id": "..."}`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActorRef {
    pub kind: ActorKind,
    pub id: String,
}

/// Kind of actor. Serialized as snake_case to match the receipt/event contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    Human,
    Agent,
    System,
}
