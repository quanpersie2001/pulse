//! Fixed action/role authorization matrix (plan 0022 §6.2).
//!
//! Replaces the v2 authority policy file: there is no config here, and none
//! is planned. Widening this matrix is a product decision (an ADR), not a
//! runtime grant, so it stays a `match` rather than data.

use crate::error::{PulseError, Result};
use crate::identity::actor::{ActorKind, ActorRef};

/// One of the action families plan §6.2 rows gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// `new`/`update`/`ready`/`dep`/`transition`.
    MutateGraph,
    /// `checkpoint`/`handoff`.
    CheckpointOrHandoff,
    /// A lane's own receipt, recorded through `pulse lane seal`.
    LaneReceipt,
    /// `close`/`close-story`.
    Close,
    /// `note`/`learn add`.
    NoteOrLearnAdd,
    /// Accepting a `decision` record.
    AcceptDecision,
    /// `verify` — running the argv a record declares and recording what was
    /// observed (decision 0026). Deliberately open to every human and agent
    /// actor: the worker runs it before handing off, and a review lane runs
    /// it again to check the claim independently. It is not a graph mutation
    /// — the commands come *from* the record, so an agent cannot use this to
    /// introduce what Pulse runs.
    Verify,
}

/// An agent role is a worker (may checkpoint/handoff) or a lane (may record
/// a lane receipt) — the exact runner role name beyond that prefix does not
/// matter to authorization.
fn is_lane_role(agent_id: &str) -> bool {
    agent_id.starts_with("review-") || agent_id.starts_with("qa-") || agent_id.starts_with("check-")
}

/// # Errors
/// `role_forbidden` when `actor` may not perform `action`, naming both.
pub fn authorize(actor: &ActorRef, action: Action) -> Result<()> {
    let allowed = match actor.kind {
        ActorKind::Human => true,
        ActorKind::Agent => match action {
            Action::CheckpointOrHandoff => !is_lane_role(&actor.id),
            Action::LaneReceipt => is_lane_role(&actor.id),
            Action::NoteOrLearnAdd => true,
            Action::Verify => true,
            Action::MutateGraph | Action::Close | Action::AcceptDecision => false,
        },
        ActorKind::System => false,
    };
    if allowed {
        return Ok(());
    }
    let kind = match actor.kind {
        ActorKind::Human => "human",
        ActorKind::Agent => "agent",
        ActorKind::System => "system",
    };
    Err(PulseError::kernel(
        "role_forbidden",
        format!("{kind}:{} may not perform {action:?}", actor.id),
        "human actors can do everything; agent:worker* may only checkpoint/handoff; \
         agent:review-*/qa-*/check-* may only record a lane receipt through `pulse lane seal`; \
         every actor may add a note or a learning, and every human or agent actor may run \
         `pulse verify` (never `system`)",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn human(id: &str) -> ActorRef {
        ActorRef {
            kind: ActorKind::Human,
            id: id.to_string(),
        }
    }

    fn agent(id: &str) -> ActorRef {
        ActorRef {
            kind: ActorKind::Agent,
            id: id.to_string(),
        }
    }

    #[test]
    fn human_may_perform_every_action() {
        for action in [
            Action::MutateGraph,
            Action::CheckpointOrHandoff,
            Action::LaneReceipt,
            Action::Close,
            Action::NoteOrLearnAdd,
            Action::AcceptDecision,
            Action::Verify,
        ] {
            assert!(authorize(&human("quan"), action).is_ok());
        }
    }

    #[test]
    fn worker_may_checkpoint_and_handoff_but_not_mutate_graph_or_close() {
        assert!(authorize(&agent("worker"), Action::CheckpointOrHandoff).is_ok());
        assert!(authorize(&agent("worker-continue"), Action::CheckpointOrHandoff).is_ok());
        assert_eq!(
            authorize(&agent("worker"), Action::MutateGraph)
                .unwrap_err()
                .code(),
            "role_forbidden"
        );
        assert_eq!(
            authorize(&agent("worker"), Action::Close)
                .unwrap_err()
                .code(),
            "role_forbidden"
        );
        assert!(authorize(&agent("worker"), Action::LaneReceipt).is_err());
    }

    #[test]
    fn lane_roles_may_record_a_lane_receipt_but_not_checkpoint_or_handoff() {
        for role in [
            "review-correctness",
            "review-adversarial",
            "qa-ui",
            "qa-api",
            "check-docs",
        ] {
            assert!(authorize(&agent(role), Action::LaneReceipt).is_ok());
            assert!(authorize(&agent(role), Action::CheckpointOrHandoff).is_err());
        }
    }

    #[test]
    fn every_actor_may_add_a_note_or_learning() {
        assert!(authorize(&human("quan"), Action::NoteOrLearnAdd).is_ok());
        assert!(authorize(&agent("worker"), Action::NoteOrLearnAdd).is_ok());
        assert!(authorize(&agent("review-correctness"), Action::NoteOrLearnAdd).is_ok());
    }

    #[test]
    fn every_human_or_agent_actor_may_verify() {
        // Decision 0026: the worker verifies before handing off and the lane
        // verifies again, so both sides of the evidence gate need this. What
        // Pulse runs still comes from the record, which no agent may edit.
        assert!(authorize(&human("quan"), Action::Verify).is_ok());
        assert!(authorize(&agent("worker"), Action::Verify).is_ok());
        assert!(authorize(&agent("review-correctness"), Action::Verify).is_ok());
        assert!(authorize(&agent("qa-ui"), Action::Verify).is_ok());
    }

    #[test]
    fn system_actor_may_not_perform_any_gated_action() {
        let system = ActorRef {
            kind: ActorKind::System,
            id: "cron".to_string(),
        };
        assert!(authorize(&system, Action::NoteOrLearnAdd).is_err());
        assert!(authorize(&system, Action::MutateGraph).is_err());
        assert!(authorize(&system, Action::Verify).is_err());
    }

    #[test]
    fn forbidden_error_carries_a_hint() {
        let err = authorize(&agent("worker"), Action::Close).unwrap_err();
        assert!(err.hint().is_some());
    }
}
