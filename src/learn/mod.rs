//! Learnings (plan 0022 §11): a friction -> learning -> check ratchet
//! living at `.pulse/learnings/LRN-<hash>.md`, one file per learning.
//!
//! `store` owns the file shape (parse/render/read/write/list); `recall`
//! matches a Ticket/Story's anchors/tags against `applies_to`/`tags` for the
//! packet and `pulse learn applicable`. This module owns the three
//! authorized mutations plan §6.2's role matrix names: `add` (any actor),
//! `activate`/`retire` (human only) — plus `record_usage`, called from
//! `kernel::completion::handoff` while sealing a handoff's
//! `learnings_used[]`, which needs no separate authorization check since
//! handoff itself is already gated.

pub mod recall;
pub mod store;

use std::path::Path;

use chrono::Utc;
use serde_json::json;

use crate::error::{PulseError, Result};
use crate::event::emit_event;
use crate::identity::actor::ActorRef;
use crate::kernel::roles::{authorize, Action};
use crate::storage::WriteGuard;
use store::{Frontmatter, Learning, UsageCounts};

/// `pulse learn add --from <file>` supplies a complete learning file
/// (frontmatter + body) to register as a fresh candidate; the `Fields`
/// variant synthesizes a minimal one (`Summary` = `title`, `Check` =
/// `expected_signal` — the one concrete, verifiable thing the submitter
/// supplied; `Do`/`Avoid` left empty for a human/agent to fill in from
/// experience rather than padded with placeholders).
pub enum AddInput {
    FromFile(String),
    Fields {
        title: String,
        kind: String,
        applies_to: Vec<String>,
        tags: Vec<String>,
        expected_signal: String,
    },
}

fn synthesize_body(title: &str, expected_signal: &str) -> String {
    format!("## Summary\n{title}\n## Do\n## Avoid\n## Check\n- {expected_signal}\n")
}

/// # Errors
/// `role_forbidden` if `actor` may not add a learning; `learning_invalid` if
/// `AddInput::FromFile`'s text fails to parse; `learning_kind_invalid` if
/// the resolved kind is not one of [`store::KINDS`].
pub fn add(repo_root: &Path, actor: &ActorRef, input: AddInput) -> Result<Learning> {
    authorize(actor, Action::NoteOrLearnAdd)?;

    let (frontmatter, body) = match input {
        AddInput::FromFile(text) => {
            let learning = store::parse(&text)?;
            (learning.frontmatter, learning.body)
        }
        AddInput::Fields {
            title,
            kind,
            applies_to,
            tags,
            expected_signal,
        } => {
            let body = synthesize_body(&title, &expected_signal);
            (
                Frontmatter {
                    id: String::new(),
                    status: "candidate".to_string(),
                    kind,
                    applies_to,
                    tags,
                    from: Vec::new(),
                    expected_signal,
                    usage: UsageCounts::default(),
                },
                body,
            )
        }
    };

    if !store::KINDS.contains(&frontmatter.kind.as_str()) {
        return Err(PulseError::kernel(
            "learning_kind_invalid",
            format!("{} is not a valid learning kind", frontmatter.kind),
            "kind must be one of failure, constraint, technique, routing",
        ));
    }

    let _guard = WriteGuard::acquire(repo_root)?;
    let now = Utc::now().to_rfc3339();
    let id = loop {
        let candidate = crate::id::generate_learning_hash_id(&frontmatter.kind, &now);
        if !store::exists(repo_root, &candidate) {
            break candidate;
        }
    };

    let learning = Learning {
        frontmatter: Frontmatter {
            id: id.clone(),
            status: "candidate".to_string(),
            usage: UsageCounts::default(),
            ..frontmatter
        },
        body,
    };
    store::write(repo_root, &learning)?;
    emit_event(
        repo_root,
        "learning.added",
        actor.as_kind_id(),
        &id,
        json!({"kind": learning.frontmatter.kind, "status": "candidate"}),
        Utc::now(),
    )?;
    Ok(learning)
}

/// # Errors
/// `role_forbidden`; `learning_not_found`; `learning_not_yet_helpful` if
/// `usage.helpful` is still 0 (plan §11.2's "hai nửa": a learning only
/// activates after a handoff has already recorded it as helpful once).
pub fn activate(repo_root: &Path, actor: &ActorRef, id: &str) -> Result<Learning> {
    authorize(actor, Action::MutateGraph)?;
    let _guard = WriteGuard::acquire(repo_root)?;
    let mut learning = store::read(repo_root, id)?;
    if learning.frontmatter.usage.helpful < 1 {
        return Err(PulseError::kernel(
            "learning_not_yet_helpful",
            format!(
                "{id} has usage.helpful = {}, needs at least 1 to activate",
                learning.frontmatter.usage.helpful
            ),
            "record a handoff with learnings_used=[{\"id\":\"<id>\",\"usage\":\"helpful\"}] before activating",
        ));
    }
    learning.frontmatter.status = "active".to_string();
    store::write(repo_root, &learning)?;
    Ok(learning)
}

/// # Errors
/// `role_forbidden`; `learning_not_found`.
pub fn retire(repo_root: &Path, actor: &ActorRef, id: &str, reason: &str) -> Result<Learning> {
    authorize(actor, Action::MutateGraph)?;
    let _guard = WriteGuard::acquire(repo_root)?;
    let mut learning = store::read(repo_root, id)?;
    learning.frontmatter.status = "retired".to_string();
    store::write(repo_root, &learning)?;
    emit_event(
        repo_root,
        "learning.retired",
        actor.as_kind_id(),
        id,
        json!({"reason": reason}),
        Utc::now(),
    )?;
    Ok(learning)
}

/// Bump `usage.<usage>` on learning `id` by one. Called from
/// `kernel::completion::handoff` for each `learnings_used[]` entry, after
/// `evaluate_handoff` has already confirmed every id exists (plan §7.2's
/// `learning_unknown` gate) — the `learning_not_found` here is defense in
/// depth, not the primary check.
///
/// # Errors
/// `learning_not_found`; `learning_usage_invalid` if `usage` is not
/// `helpful`, `not_needed` or `misleading`.
pub fn record_usage(repo_root: &Path, id: &str, usage: &str) -> Result<()> {
    let _guard = WriteGuard::acquire(repo_root)?;
    let mut learning = store::read(repo_root, id)?;
    match usage {
        "helpful" => learning.frontmatter.usage.helpful += 1,
        "not_needed" => learning.frontmatter.usage.not_needed += 1,
        "misleading" => learning.frontmatter.usage.misleading += 1,
        other => {
            return Err(PulseError::kernel(
                "learning_usage_invalid",
                format!("{other} is not a valid learning usage"),
                "usage must be helpful, not_needed or misleading",
            ));
        }
    }
    store::write(repo_root, &learning)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::actor::ActorKind;

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
    fn add_from_fields_synthesizes_a_candidate_with_summary_from_title() {
        let repo = tempfile::tempdir().unwrap();
        let learning = add(
            repo.path(),
            &agent("worker"),
            AddInput::Fields {
                title: "rotation must be atomic".to_string(),
                kind: "failure".to_string(),
                applies_to: vec!["src/auth/**".to_string()],
                tags: vec!["security".to_string()],
                expected_signal: String::new(),
            },
        )
        .unwrap();
        assert!(learning.frontmatter.id.starts_with("LRN-"));
        assert_eq!(learning.frontmatter.status, "candidate");
        assert!(learning.body.contains("rotation must be atomic"));
        assert!(store::exists(repo.path(), &learning.frontmatter.id));
    }

    #[test]
    fn add_rejects_an_unknown_kind() {
        let repo = tempfile::tempdir().unwrap();
        let err = add(
            repo.path(),
            &agent("worker"),
            AddInput::Fields {
                title: "t".to_string(),
                kind: "vibes".to_string(),
                applies_to: vec![],
                tags: vec![],
                expected_signal: String::new(),
            },
        )
        .unwrap_err();
        assert_eq!(err.code(), "learning_kind_invalid");
    }

    #[test]
    fn add_from_file_preserves_from_and_forces_candidate_status() {
        let repo = tempfile::tempdir().unwrap();
        let text = "\
---
id: LRN-0000
status: active
kind: constraint
applies_to: [\"src/**\"]
tags: []
from: [\"TK-a3f9\"]
expected_signal: \"\"
usage: {helpful: 3, not_needed: 0, misleading: 0}
---
## Summary
s
";
        let learning = add(
            repo.path(),
            &human("quan"),
            AddInput::FromFile(text.to_string()),
        )
        .unwrap();
        assert_ne!(learning.frontmatter.id, "LRN-0000");
        assert_eq!(learning.frontmatter.status, "candidate");
        assert_eq!(learning.frontmatter.from, vec!["TK-a3f9"]);
        assert_eq!(learning.frontmatter.usage, UsageCounts::default());
    }

    #[test]
    fn activate_requires_at_least_one_helpful_usage() {
        let repo = tempfile::tempdir().unwrap();
        let learning = add(
            repo.path(),
            &agent("worker"),
            AddInput::Fields {
                title: "t".to_string(),
                kind: "failure".to_string(),
                applies_to: vec![],
                tags: vec![],
                expected_signal: String::new(),
            },
        )
        .unwrap();
        let err = activate(repo.path(), &human("quan"), &learning.frontmatter.id).unwrap_err();
        assert_eq!(err.code(), "learning_not_yet_helpful");

        record_usage(repo.path(), &learning.frontmatter.id, "helpful").unwrap();
        let activated = activate(repo.path(), &human("quan"), &learning.frontmatter.id).unwrap();
        assert_eq!(activated.frontmatter.status, "active");
    }

    #[test]
    fn activate_and_retire_are_human_only() {
        let repo = tempfile::tempdir().unwrap();
        let learning = add(
            repo.path(),
            &agent("worker"),
            AddInput::Fields {
                title: "t".to_string(),
                kind: "failure".to_string(),
                applies_to: vec![],
                tags: vec![],
                expected_signal: String::new(),
            },
        )
        .unwrap();
        assert_eq!(
            activate(repo.path(), &agent("worker"), &learning.frontmatter.id)
                .unwrap_err()
                .code(),
            "role_forbidden"
        );
        assert_eq!(
            retire(
                repo.path(),
                &agent("worker"),
                &learning.frontmatter.id,
                "stale"
            )
            .unwrap_err()
            .code(),
            "role_forbidden"
        );
    }

    #[test]
    fn retire_sets_status_and_records_a_reason_event() {
        let repo = tempfile::tempdir().unwrap();
        let learning = add(
            repo.path(),
            &agent("worker"),
            AddInput::Fields {
                title: "t".to_string(),
                kind: "failure".to_string(),
                applies_to: vec![],
                tags: vec![],
                expected_signal: String::new(),
            },
        )
        .unwrap();
        let retired = retire(
            repo.path(),
            &human("quan"),
            &learning.frontmatter.id,
            "stale",
        )
        .unwrap();
        assert_eq!(retired.frontmatter.status, "retired");
    }

    #[test]
    fn record_usage_rejects_an_unknown_value() {
        let repo = tempfile::tempdir().unwrap();
        let learning = add(
            repo.path(),
            &agent("worker"),
            AddInput::Fields {
                title: "t".to_string(),
                kind: "failure".to_string(),
                applies_to: vec![],
                tags: vec![],
                expected_signal: String::new(),
            },
        )
        .unwrap();
        let err = record_usage(repo.path(), &learning.frontmatter.id, "vibes").unwrap_err();
        assert_eq!(err.code(), "learning_usage_invalid");
    }
}
