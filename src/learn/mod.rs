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

pub mod friction;
pub mod recall;
pub mod store;

use std::{fs, path::Path};

use crate::canonical_json::hash_bytes;
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

/// Everything `pulse learn add` can carry besides the learning content
/// itself (plan 0025 E1/E2): the frictions this learning classifies, and the
/// check argv/cwd that will be *enforced* once a human activates it.
#[derive(Debug, Clone, Default)]
pub struct AddExtras {
    /// `"<subject>#<evt id>"` citations appended to `frontmatter.from`.
    pub frictions: Vec<String>,
    /// Enforceable check argv; must be a non-empty list of non-empty strings.
    pub check_argv: Vec<String>,
    /// Repository-relative working directory for `check_argv`.
    pub check_cwd: Option<String>,
    /// Code citations (`<path>:<from>-<to>`), hashed from disk at add time
    /// (plan 0025 E4).
    pub cites: Vec<CiteSpec>,
}

/// A cite as the caller hands it over: path + line range, no hash yet —
/// Pulse computes the hash itself so the agent never types one (plan 0025
/// E4).
#[derive(Debug, Clone)]
pub struct CiteSpec {
    pub path: String,
    pub lines: String,
}

/// # Errors
/// `role_forbidden` if `actor` may not add a learning; `learning_invalid` if
/// `AddInput::FromFile`'s text fails to parse; `learning_invalid` if
/// the resolved kind is not one of [`store::KINDS`]; `learning_invalid` if
/// `extras.check_argv` is malformed or `extras.check_cwd` escapes the repo;
/// `friction_not_found` if an `extras.frictions` entry does not name a known
/// `"<subject>#<evt id>"` pair.
pub fn add(
    repo_root: &Path,
    actor: &ActorRef,
    input: AddInput,
    extras: AddExtras,
) -> Result<Learning> {
    authorize(actor, Action::NoteOrLearnAdd)?;

    // Whether `body` came from `synthesize_body` — only then does the Check
    // line follow the frontmatter's signal/argv below (plan 0025 E2: with a
    // check argv and no expected signal, the body records the command).
    let mut synthesized = false;
    let mut title = String::new();
    let (mut frontmatter, mut body) = match input {
        AddInput::FromFile(text) => {
            let learning = store::parse(&text)?;
            (learning.frontmatter, learning.body)
        }
        AddInput::Fields {
            title: fields_title,
            kind,
            applies_to,
            tags,
            expected_signal,
        } => {
            synthesized = true;
            title = fields_title;
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
                    check_argv: vec![],
                    check_cwd: None,
                    cites: vec![],
                },
                body,
            )
        }
    };

    if !store::KINDS.contains(&frontmatter.kind.as_str()) {
        return Err(PulseError::kernel(
            "learning_invalid",
            format!("{} is not a valid learning kind", frontmatter.kind),
            "kind must be one of failure, constraint, technique, routing",
        ));
    }

    // The enforceable check (plan 0025 E2): argv is validated here, at the
    // boundary, so a learning never carries a check `pulse verify` would
    // refuse at run time. `check_cwd` uses the same repository-relative rule
    // a `verify[]` entry's `cwd` uses.
    if !extras.check_argv.is_empty() {
        if extras.check_argv.iter().any(|part| part.trim().is_empty()) {
            return Err(PulseError::kernel(
                "learning_invalid",
                "--check-argv must be a JSON array of non-empty strings",
                r#"example: --check-argv '["cargo","test","--lib"]'"#,
            ));
        }
        frontmatter.check_argv = extras.check_argv;
    }
    if let Some(cwd) = &extras.check_cwd {
        crate::storage::safe_repo_relative(cwd).map_err(|_| {
            PulseError::kernel(
                "learning_invalid",
                format!("--check-cwd {cwd:?} is not a repository-relative path inside the repo"),
                "pass a directory under the repository root, like `web` or `services/api`",
            )
        })?;
        frontmatter.check_cwd = extras.check_cwd.clone();
    }
    // Plan 0025 E2: a synthesized body whose check is an argv shows the
    // argv in `## Check` — the file is the thing a human reads before
    // activating, so the command to be enforced must be visible in it.
    if synthesized && frontmatter.expected_signal.is_empty() && !frontmatter.check_argv.is_empty() {
        body = synthesize_body(&title, &frontmatter.check_argv.join(" "));
    }

    // Friction citations (plan 0025 E1): each value names the friction this
    // learning classifies, as `"<subject>#<evt id>"`. Validated read-only
    // before the write lock: `friction::list` is the same source the gates
    // read, so a citation accepted here really is what turns that friction
    // `Learned`.
    for value in &extras.frictions {
        let Some((subject, key)) = value.split_once('#') else {
            return Err(friction_not_found(value));
        };
        let known = friction::list(repo_root, Some(subject))?
            .iter()
            .any(|friction| friction.key == key);
        if !known {
            return Err(friction_not_found(value));
        }
        if !frontmatter.from.iter().any(|from| from == value) {
            frontmatter.from.push(value.clone());
        }
    }

    // Code citations (plan 0025 E4): validated and hashed from the file on
    // disk NOW, so the learning pins what the author actually read. The
    // same `<path>:<from>-<to>` grammar `--cite` takes; range/format
    // problems are `learning_cite_invalid` at add time, not at read time.
    for spec in &extras.cites {
        let sha256 = cite_hash(repo_root, &spec.path, &spec.lines)?;
        frontmatter.cites.push(store::Cite {
            path: spec.path.clone(),
            lines: spec.lines.clone(),
            sha256,
        });
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

fn friction_not_found(value: &str) -> PulseError {
    PulseError::kernel(
        "friction_not_found",
        format!("{value} does not name a recorded friction"),
        "list keys with `pulse learn friction <subject-id> --all`; a citation is \
         `<subject>#<evt id>` where the event is that record's `note.recorded` friction event",
    )
}

/// What one `pulse learn dismiss` call settled: the keys a
/// `friction.dismissed` event was written for, and the keys skipped because
/// they were already classified (a dismissal is idempotent — plan 0025 E1).
#[derive(Debug, Clone, Default)]
pub struct DismissOutcome {
    pub dismissed: Vec<String>,
    pub skipped: Vec<String>,
}

/// Record why each named friction stays Ticket-specific (plan 0025 E1).
/// One `friction.dismissed` event per friction, subject = the record id; no
/// store or learning file is mutated — the event *is* the classification.
/// Every actor may dismiss: a dismissal is a classification (the skill's
/// "a friction that stays Ticket-specific is that Ticket's business"), not
/// an erasure, and carries a reason precisely so it can be argued with.
///
/// With `all`, every unclassified friction of the subject is dismissed and
/// `keys` (if any) is the union with it. A key that is not a friction of
/// `subject` is `friction_not_found`; a key already classified is skipped.
///
/// # Errors
/// `role_forbidden`; `issue_not_found` if `subject` does not exist;
/// `friction_not_found` for an unknown key; `learning_invalid` never — an
/// empty `reason` is rejected by the CLI (`friction_reason_missing`) and
/// trusted here.
pub fn dismiss(
    repo_root: &Path,
    actor: &ActorRef,
    subject: &str,
    keys: &[String],
    all: bool,
    reason: &str,
) -> Result<DismissOutcome> {
    authorize(actor, Action::NoteOrLearnAdd)?;
    let records = crate::store::issues::read_all(repo_root)?;
    if !records
        .iter()
        .any(|record| record.get("id").and_then(|v| v.as_str()) == Some(subject))
    {
        return Err(PulseError::kernel(
            "issue_not_found",
            format!("no record with id {subject}"),
            "check the id with `pulse list`; ids are hash-based and never recycled",
        ));
    }

    let mut targets: Vec<String> = keys.to_vec();
    if all {
        for friction in friction::unclassified_for(repo_root, &[subject])? {
            if !targets.contains(&friction.key) {
                targets.push(friction.key);
            }
        }
    }
    let known: Vec<String> = friction::list(repo_root, Some(subject))?
        .iter()
        .map(|friction| friction.key.clone())
        .collect();
    let classified: Vec<String> = friction::list(repo_root, Some(subject))?
        .iter()
        .filter(|friction| friction.state != friction::FrictionState::Unclassified)
        .map(|friction| friction.key.clone())
        .collect();

    let mut outcome = DismissOutcome::default();
    for key in &targets {
        if !known.contains(key) {
            return Err(friction_not_found(&format!("{subject}#{key}")));
        }
        if classified.contains(key) {
            outcome.skipped.push(key.clone());
            continue;
        }
        emit_event(
            repo_root,
            "friction.dismissed",
            actor.as_kind_id(),
            subject,
            json!({"friction": key, "reason": reason}),
            Utc::now(),
        )?;
        outcome.dismissed.push(key.clone());
    }
    Ok(outcome)
}

/// The learning's code citations whose hash no longer matches the file on
/// disk (plan 0025 E4): the cited line range's bytes moved, or the file is
/// gone. Read-only: a stale cite is a *signal* that the learning may need a
/// human re-read — code can change back, and a changed file does not make a
/// lesson wrong — so nothing is auto-retired and recall still includes the
/// learning (`kernel::doctor` reports it as `learning_cite_stale`, the
/// packet tags the learning `stale: true`).
pub fn stale_cites<'a>(repo_root: &Path, learning: &'a Learning) -> Vec<&'a store::Cite> {
    learning
        .frontmatter
        .cites
        .iter()
        .filter(|cite| {
            cite_hash(repo_root, &cite.path, &cite.lines)
                .map(|hash| hash != cite.sha256)
                .unwrap_or(true) // unreadable/missing file = stale
        })
        .collect()
}

/// Hash the current bytes of one cited line range: the lines `from..=to`
/// (1-based, inclusive) joined with `\n` and no trailing newline — the rule
/// both `learn add --cite` (pinning) and [`stale_cites`] (re-checking) use,
/// so a cite only goes stale when the bytes really moved.
///
/// # Errors
/// `learning_cite_invalid` if `lines` is not a `"<from>-<to>"` range, the
/// path escapes the repo, or the range falls outside the file.
fn cite_hash(repo_root: &Path, path: &str, lines: &str) -> Result<String> {
    let relative = crate::storage::safe_repo_relative(path)?;
    let bytes = fs::read(repo_root.join(relative))
        .map_err(|error| PulseError::io(std::path::Path::new(path), error))?;
    let text = String::from_utf8_lossy(&bytes);
    let file_lines: Vec<&str> = text.lines().collect();
    let Some((from, to)) = parse_cite_lines(lines) else {
        return Err(PulseError::kernel(
            "learning_cite_invalid",
            format!("cite lines {lines:?} is not a \"<from>-<to>\" range"),
            "a cite is <path>:<from>-<to>, both 1-based line numbers, from <= to",
        ));
    };
    if from == 0 || to > file_lines.len() {
        return Err(PulseError::kernel(
            "learning_cite_invalid",
            format!(
                "cite range {from}-{to} is outside {path} ({} lines)",
                file_lines.len()
            ),
            "re-check the cited range; the file may have shrunk since the learning was added",
        ));
    }
    Ok(hash_bytes(
        file_lines[from - 1..=to - 1].join("\n").as_bytes(),
    ))
}

/// Parse a cite's `"<from>-<to>"` line range.
fn parse_cite_lines(lines: &str) -> Option<(usize, usize)> {
    let (from, to) = lines.split_once('-')?;
    Some((from.trim().parse().ok()?, to.trim().parse().ok()?))
}

/// Bump `usage.<usage>` on learning `id` by one. Called from
/// `kernel::completion::handoff` for each `learnings_used[]` entry, after
/// `evaluate_handoff` has already confirmed every id exists (plan §7.2's
/// `learning_unknown` gate) — the `learning_not_found` here is defense in
/// depth, not the primary check.
///
/// # Errors
/// `learning_not_found`; `learning_invalid` if `usage` is not
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
                "learning_invalid",
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
            AddExtras::default(),
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
            AddExtras::default(),
        )
        .unwrap_err();
        assert_eq!(err.code(), "learning_invalid");
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
            AddExtras::default(),
        )
        .unwrap();
        assert_ne!(learning.frontmatter.id, "LRN-0000");
        assert_eq!(learning.frontmatter.status, "candidate");
        assert_eq!(learning.frontmatter.from, vec!["TK-a3f9"]);
        assert_eq!(learning.frontmatter.usage, UsageCounts::default());
    }

    #[test]
    fn a_synthesized_check_shows_the_argv_when_no_signal_was_given() {
        // Plan 0025 E2: the file is what a human reads before activating, so
        // the enforced command must be visible in the `## Check` section.
        let repo = tempfile::tempdir().unwrap();
        let learning = add(
            repo.path(),
            &agent("worker"),
            AddInput::Fields {
                title: "rotation must be atomic".to_string(),
                kind: "failure".to_string(),
                applies_to: vec![],
                tags: vec![],
                expected_signal: String::new(),
            },
            AddExtras {
                check_argv: vec!["cargo".to_string(), "test".to_string()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            store::bullet_items(&store::sections(&learning.body)["Check"]),
            vec!["cargo test"]
        );
        // A supplied expected_signal wins over the argv — both may coexist.
        let learning = add(
            repo.path(),
            &agent("worker"),
            AddInput::Fields {
                title: "t".to_string(),
                kind: "failure".to_string(),
                applies_to: vec![],
                tags: vec![],
                expected_signal: "exactly one success".to_string(),
            },
            AddExtras {
                check_argv: vec!["cargo".to_string(), "test".to_string()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            store::bullet_items(&store::sections(&learning.body)["Check"]),
            vec!["exactly one success"]
        );
        assert_eq!(learning.frontmatter.check_argv, vec!["cargo", "test"]);
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
            AddExtras::default(),
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
            AddExtras::default(),
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
            AddExtras::default(),
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
            AddExtras::default(),
        )
        .unwrap();
        let err = record_usage(repo.path(), &learning.frontmatter.id, "vibes").unwrap_err();
        assert_eq!(err.code(), "learning_invalid");
    }

    // --- Plan 0025 E4: cites are hash-pinned, staleness is detected only ---

    fn cited_learning(repo: &tempfile::TempDir, body: &str) -> String {
        std::fs::write(repo.path().join("code.rs"), body).unwrap();
        add(
            repo.path(),
            &agent("worker"),
            AddInput::Fields {
                title: "t".to_string(),
                kind: "failure".to_string(),
                applies_to: vec![],
                tags: vec![],
                expected_signal: String::new(),
            },
            AddExtras {
                cites: vec![CiteSpec {
                    path: "code.rs".to_string(),
                    lines: "2-3".to_string(),
                }],
                ..Default::default()
            },
        )
        .unwrap()
        .frontmatter
        .id
    }

    #[test]
    fn a_fresh_cite_is_not_stale_and_pins_the_hash_of_its_lines() {
        let repo = tempfile::tempdir().unwrap();
        let id = cited_learning(&repo, "line one\nline two\nline three\nline four\n");
        let learning = store::read(repo.path(), &id).unwrap();
        assert_eq!(learning.frontmatter.cites.len(), 1);
        let cite = &learning.frontmatter.cites[0];
        assert_eq!(cite.sha256, hash_bytes(b"line two\nline three"));
        assert!(stale_cites(repo.path(), &learning).is_empty());
    }

    #[test]
    fn editing_a_line_inside_the_range_makes_the_cite_stale() {
        let repo = tempfile::tempdir().unwrap();
        let id = cited_learning(&repo, "line one\nline two\nline three\nline four\n");
        std::fs::write(
            repo.path().join("code.rs"),
            "line one\nline two EDITED\nline three\nline four\n",
        )
        .unwrap();
        let learning = store::read(repo.path(), &id).unwrap();
        assert_eq!(stale_cites(repo.path(), &learning).len(), 1);
    }

    #[test]
    fn editing_a_line_outside_the_range_does_not_make_the_cite_stale() {
        // Same line count, different bytes outside 2-3: the cited lines are
        // untouched, so the pin still holds.
        let repo = tempfile::tempdir().unwrap();
        let id = cited_learning(&repo, "line one\nline two\nline three\nline four\n");
        std::fs::write(
            repo.path().join("code.rs"),
            "line one EDITED\nline two\nline three\nline four\n",
        )
        .unwrap();
        let learning = store::read(repo.path(), &id).unwrap();
        assert!(stale_cites(repo.path(), &learning).is_empty());
    }

    #[test]
    fn deleting_the_cited_file_makes_the_cite_stale_not_an_error() {
        let repo = tempfile::tempdir().unwrap();
        let id = cited_learning(&repo, "line one\nline two\nline three\nline four\n");
        std::fs::remove_file(repo.path().join("code.rs")).unwrap();
        let learning = store::read(repo.path(), &id).unwrap();
        assert_eq!(stale_cites(repo.path(), &learning).len(), 1);
    }

    #[test]
    fn a_cite_outside_the_file_is_refused_at_add_time() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(repo.path().join("code.rs"), "only line\n").unwrap();
        let err = add(
            repo.path(),
            &agent("worker"),
            AddInput::Fields {
                title: "t".to_string(),
                kind: "failure".to_string(),
                applies_to: vec![],
                tags: vec![],
                expected_signal: String::new(),
            },
            AddExtras {
                cites: vec![CiteSpec {
                    path: "code.rs".to_string(),
                    lines: "2-3".to_string(),
                }],
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(err.code(), "learning_cite_invalid");
        assert!(err.hint().is_some());
    }
}
