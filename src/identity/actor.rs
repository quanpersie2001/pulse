//! Neutral actor/principal identity types.
//!
//! `ActorRef` and `ActorKind` are shared across the evidence, event, policy and
//! kernel domains. They live here so no single consuming domain owns the
//! identity vocabulary. The persisted serialization shape is unchanged: this is
//! pure ownership relocation, and [`crate::evidence::model`] re-exports both
//! types for compatibility with the historical `pulse::evidence::model::*`
//! path used by receipts, tests and the CLI.

use serde::{Deserialize, Serialize};

use crate::error::{PulseError, Result};

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

impl ActorRef {
    /// `kind:id`, the wire/CLI spelling plan 0022 §6.1 defines.
    pub fn as_kind_id(&self) -> String {
        let kind = match self.kind {
            ActorKind::Human => "human",
            ActorKind::Agent => "agent",
            ActorKind::System => "system",
        };
        format!("{kind}:{}", self.id)
    }
}

/// Parse `kind:id` strictly (plan 0022 §6.1). `kind` must be `human` or
/// `agent` — a CLI-supplied actor is never `system`, which is reserved for
/// internal automation that has no explicit human or agent behind it.
///
/// # Errors
/// `actor_invalid` when there is no `:` separator, `kind` is not
/// `human`/`agent`, or `id` is empty.
pub fn parse_actor(raw: &str) -> Result<ActorRef> {
    let Some((kind, id)) = raw.split_once(':') else {
        return Err(PulseError::kernel(
            "actor_invalid",
            format!("actor {raw} is missing a kind:id separator"),
            "use human:<name> or agent:<role>, e.g. human:quan or agent:worker",
        ));
    };
    let kind = match kind {
        "human" => ActorKind::Human,
        "agent" => ActorKind::Agent,
        other => {
            return Err(PulseError::kernel(
                "actor_invalid",
                format!("actor kind {other} is not human or agent"),
                "use human:<name> or agent:<role>, e.g. human:quan or agent:worker",
            ))
        }
    };
    if id.is_empty() {
        return Err(PulseError::kernel(
            "actor_invalid",
            format!("actor {raw} has an empty id"),
            "use human:<name> or agent:<role>, e.g. human:quan or agent:worker",
        ));
    }
    Ok(ActorRef {
        kind,
        id: id.to_string(),
    })
}

/// Resolve the effective actor for a CLI invocation (plan 0022 §6.1): an
/// explicit `--from`/`--actor` value wins; otherwise `PULSE_ACTOR`;
/// otherwise `git config user.name` as `human:<name>`.
///
/// # Errors
/// `actor_invalid` if an explicit or `PULSE_ACTOR` value is malformed;
/// `actor_invalid` if none is given and `git config user.name` is unset.
pub fn resolve_actor(repo_root: &std::path::Path, explicit: Option<&str>) -> Result<ActorRef> {
    if let Some(raw) = explicit {
        return parse_actor(raw);
    }
    if let Ok(raw) = std::env::var("PULSE_ACTOR") {
        if !raw.trim().is_empty() {
            return parse_actor(raw.trim());
        }
    }
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["config", "user.name"])
        .output();
    if let Ok(output) = output {
        if output.status.success() {
            let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !name.is_empty() {
                return Ok(ActorRef {
                    kind: ActorKind::Human,
                    id: name,
                });
            }
        }
    }
    Err(PulseError::kernel(
        "actor_invalid",
        "no actor given and git config user.name is unset",
        "pass --from human:<name>, set PULSE_ACTOR, or run `git config user.name <name>`",
    ))
}
