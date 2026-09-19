//! Friction classification state, computed read-only (plan 0025 E1).
//!
//! Purpose: answer "which friction notes has nobody classified yet?" from
//! the two append-only stores — the event log and `.pulse/learnings/` —
//! without adding any field to a record's `notes[]`.
//!
//! State touched: none. This module takes no lock and performs no mutation;
//! it reads events ([`crate::event::read_events`]) and learning files
//! ([`super::store::list`]).
//!
//! Invariants:
//!
//! * a friction's stable key is the **id of the `note.recorded` event** that
//!   recorded it (`evt_…`), never an index into `notes[]`: the record keeps
//!   at most the newest 50 notes (plan 0022 §4.3) so indices drift, while
//!   the event log is append-only and unbounded (plan 0025 E1);
//! * "classified" is a *function of the log*, not a field on a note: a
//!   friction is `Learned` when some learning's `from` cites
//!   `"<subject>#<evt id>"` (any status — a retired learning still says the
//!   friction was understood), `Dismissed` when a `friction.dismissed` event
//!   records why it stays Ticket-specific, and `Unclassified` otherwise;
//! * `Learned` wins over `Dismissed` so dismissal is idempotent against a
//!   friction a learning already cites (the skill's "at most one learning"
//!   cap makes the pair, not a race).
//!
//! Allowed dependencies: `event`, `store::{issues, learn}` — the same reads
//! `recall` already makes. Never the CLI layer.

use std::collections::BTreeMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::error::Result;
use crate::event::{read_events, EventEnvelope};
use crate::store::issues::read_all;

use super::store;

/// Text reported for a friction whose event predates the payload carrying
/// the note text and whose record notes have been cut/archived: the key and
/// subject still identify it; only the words are gone.
const ARCHIVED_TEXT: &str = "<archived>";

/// Notes on the record carry an `at` timestamp taken microseconds apart from
/// the event's `occurred_at`; a match within this window is the same note.
const NOTE_MATCH_WINDOW_MS: i64 = 1000;

#[derive(Debug, Clone, Serialize)]
pub struct Friction {
    /// The `note.recorded` event id — the stable key every classification
    /// (learning citation, dismissal) is written against.
    pub key: String,
    /// The record the friction note sits on (`TK-…`/`ST-…`).
    pub subject: String,
    pub at: DateTime<Utc>,
    pub actor: String,
    pub text: String,
    pub state: FrictionState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FrictionState {
    Unclassified,
    Learned { learning: String },
    Dismissed { reason: String },
}

impl FrictionState {
    /// One word for the human line of `pulse learn friction`.
    pub fn label(&self) -> String {
        match self {
            FrictionState::Unclassified => "unclassified".to_string(),
            FrictionState::Learned { learning } => format!("learned:{learning}"),
            FrictionState::Dismissed { .. } => "dismissed".to_string(),
        }
    }
}

/// Every friction note in the event log, oldest first per subject, with the
/// classification the log and the learnings currently imply. `subject`
/// filters to one record's frictions.
///
/// # Errors
/// Propagates an event-log or learnings-directory read failure.
pub fn list(repo_root: &Path, subject: Option<&str>) -> Result<Vec<Friction>> {
    // `from` citations: "<subject>#<evt id>" -> learning id. A key cited by
    // two learnings keeps the first by id order (`store::list` sorts).
    let mut learned_by_key: BTreeMap<String, String> = BTreeMap::new();
    for learning in store::list(repo_root)? {
        for citation in &learning.frontmatter.from {
            if let Some((_, key)) = citation.split_once('#') {
                learned_by_key
                    .entry(key.to_string())
                    .or_insert_with(|| learning.frontmatter.id.clone());
            }
        }
    }

    // Dismissals: the newest `friction.dismissed` event per friction key
    // wins (events arrive sorted by ULID id, i.e. chronologically).
    let mut dismissed_by_key: BTreeMap<String, String> = BTreeMap::new();
    for event in read_events(repo_root)? {
        if event.event_type != "friction.dismissed" {
            continue;
        }
        if let Some(key) = event.payload.get("friction").and_then(|v| v.as_str()) {
            let reason = event
                .payload
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            dismissed_by_key.insert(key.to_string(), reason);
        }
    }

    let mut frictions = Vec::new();
    for event in read_events(repo_root)? {
        if event.event_type != "note.recorded" {
            continue;
        }
        let Some((is_friction, text)) = resolve_note(repo_root, &event)? else {
            continue;
        };
        if !is_friction {
            continue;
        }
        let key = event.id.clone();
        let state = if let Some(learning) = learned_by_key.get(&key) {
            FrictionState::Learned {
                learning: learning.clone(),
            }
        } else if let Some(reason) = dismissed_by_key.get(&key) {
            FrictionState::Dismissed {
                reason: reason.clone(),
            }
        } else {
            FrictionState::Unclassified
        };
        frictions.push(Friction {
            key,
            subject: event.subject.id.clone(),
            at: event.occurred_at,
            actor: event.actor.legacy_id(),
            text,
            state,
        });
    }
    frictions.sort_by(|a, b| (&a.subject, a.at, &a.key).cmp(&(&b.subject, b.at, &b.key)));
    if let Some(subject) = subject {
        frictions.retain(|friction| friction.subject == subject);
    }
    Ok(frictions)
}

/// Unclassified frictions on any of `subject_ids` — the list the
/// `close-story` gate blocks on and `pulse learn dismiss <id> --all`
/// settles, in the same (subject, time, key) order as [`list`].
///
/// # Errors
/// Propagates [`list`]'s read failures.
pub fn unclassified_for(repo_root: &Path, subject_ids: &[&str]) -> Result<Vec<Friction>> {
    Ok(list(repo_root, None)?
        .into_iter()
        .filter(|friction| {
            friction.state == FrictionState::Unclassified
                && subject_ids.contains(&friction.subject.as_str())
        })
        .collect())
}

/// Whether one `note.recorded` event names a friction, and the note's text.
///
/// Current events carry `{kind, text}` in the payload, so the record is not
/// touched. Events written before the payload carried the text fall back to
/// the subject record's `notes[]` (`at`+`from` within the match window);
/// when neither source has the words, [`ARCHIVED_TEXT`] stands in — an old
/// friction must still be classifiable, not error the whole listing.
fn resolve_note(repo_root: &Path, event: &EventEnvelope) -> Result<Option<(bool, String)>> {
    let kind = event.payload.get("kind").and_then(|v| v.as_str());
    let text = event.payload.get("text").and_then(|v| v.as_str());
    if let (Some(kind), Some(text)) = (kind, text) {
        return Ok(Some((kind == "friction", text.to_string())));
    }
    // Legacy event: look the note up on the record.
    let Some(kind) = kind else {
        return Ok(None);
    };
    let records = read_all(repo_root)?;
    let note_text = records
        .iter()
        .find(|record| record.get("id").and_then(|v| v.as_str()) == Some(event.subject.id.as_str()))
        .and_then(|record| record.get("notes"))
        .and_then(|notes| notes.as_array())
        .iter()
        .flat_map(|notes| notes.iter())
        .filter(|note| {
            note.get("kind").and_then(|v| v.as_str()) == Some(kind)
                && note.get("from").and_then(|v| v.as_str())
                    == Some(event.actor.legacy_id().as_str())
        })
        .find(|note| {
            note.get("at")
                .and_then(|v| v.as_str())
                .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
                .is_some_and(|at| {
                    (at.with_timezone(&Utc) - event.occurred_at)
                        .num_milliseconds()
                        .abs()
                        <= NOTE_MATCH_WINDOW_MS
                })
        })
        .and_then(|note| note.get("text").and_then(|v| v.as_str()))
        .map(str::to_string);
    Ok(Some((
        kind == "friction",
        note_text.unwrap_or_else(|| ARCHIVED_TEXT.to_string()),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::actor::{ActorKind, ActorRef};
    use serde_json::json;

    fn agent(id: &str) -> ActorRef {
        ActorRef {
            kind: ActorKind::Agent,
            id: id.to_string(),
        }
    }

    fn seed_ticket(repo: &Path, id: &str) {
        crate::store::issues::mutate(repo, |mut records| {
            records.push(json!({
                "schema": 3, "id": id, "kind": "ticket", "title": "t",
                "status": "done", "revision": 1,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "role": "implementation",
            }));
            Ok(records)
        })
        .unwrap();
    }

    fn note_friction(repo: &Path, subject: &str, text: &str) -> String {
        crate::kernel::issues::append_note(
            repo,
            &agent("worker"),
            subject,
            text,
            crate::kernel::issues::NoteKind::Friction,
        )
        .unwrap();
        read_events(repo)
            .unwrap()
            .into_iter()
            .filter(|event| event.event_type == "note.recorded")
            .map(|event| event.id)
            .next_back()
            .unwrap()
    }

    fn add_learning(repo: &Path, citations: &[String]) -> String {
        super::super::add(
            repo,
            &agent("worker"),
            super::super::AddInput::Fields {
                title: "t".to_string(),
                kind: "failure".to_string(),
                applies_to: vec![],
                tags: vec![],
                expected_signal: String::new(),
            },
            super::super::AddExtras {
                frictions: citations.to_vec(),
                ..Default::default()
            },
        )
        .unwrap()
        .frontmatter
        .id
    }

    fn state_of(frictions: &[Friction], key: &str) -> FrictionState {
        frictions
            .iter()
            .find(|friction| friction.key == key)
            .unwrap_or_else(|| panic!("no friction {key}"))
            .state
            .clone()
    }

    #[test]
    fn an_appended_friction_note_is_unclassified_then_learned_when_cited() {
        let repo = tempfile::tempdir().unwrap();
        seed_ticket(repo.path(), "TK-a3f9");
        let key = note_friction(repo.path(), "TK-a3f9", "rotation broke under load");

        let frictions = list(repo.path(), Some("TK-a3f9")).unwrap();
        assert_eq!(frictions.len(), 1);
        assert_eq!(frictions[0].text, "rotation broke under load");
        assert_eq!(frictions[0].actor, "agent:worker");
        assert_eq!(state_of(&frictions, &key), FrictionState::Unclassified);

        add_learning(repo.path(), &[format!("TK-a3f9#{key}")]);
        let learned = match state_of(&list(repo.path(), Some("TK-a3f9")).unwrap(), &key) {
            FrictionState::Learned { learning } => learning,
            other => panic!("expected learned, got {other:?}"),
        };
        assert!(learned.starts_with("LRN-"));
        assert!(unclassified_for(repo.path(), &["TK-a3f9"])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_dismissal_records_the_reason_and_is_idempotent() {
        let repo = tempfile::tempdir().unwrap();
        seed_ticket(repo.path(), "TK-a3f9");
        let key = note_friction(repo.path(), "TK-a3f9", "endpoint quirk");

        let first = super::super::dismiss(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            std::slice::from_ref(&key),
            false,
            "ticket-specific",
        )
        .unwrap();
        assert_eq!(first.dismissed, vec![key.clone()]);
        assert_eq!(
            state_of(&list(repo.path(), Some("TK-a3f9")).unwrap(), &key),
            FrictionState::Dismissed {
                reason: "ticket-specific".to_string()
            }
        );

        // Idempotent: a second dismissal skips instead of double-classifying.
        let second = super::super::dismiss(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            std::slice::from_ref(&key),
            false,
            "again",
        )
        .unwrap();
        assert!(second.dismissed.is_empty());
        assert_eq!(second.skipped, vec![key.clone()]);
        let dismissals = read_events(repo.path())
            .unwrap()
            .into_iter()
            .filter(|event| event.event_type == "friction.dismissed")
            .count();
        assert_eq!(dismissals, 1);
    }

    #[test]
    fn a_retired_learning_still_classifies_its_friction() {
        let repo = tempfile::tempdir().unwrap();
        seed_ticket(repo.path(), "TK-a3f9");
        let key = note_friction(repo.path(), "TK-a3f9", "rotation broke");
        let learning = add_learning(repo.path(), &[format!("TK-a3f9#{key}")]);
        super::super::retire(
            repo.path(),
            &crate::identity::actor::ActorRef {
                kind: ActorKind::Human,
                id: "quan".to_string(),
            },
            &learning,
            "stale",
        )
        .unwrap();
        let frictions = list(repo.path(), Some("TK-a3f9")).unwrap();
        assert!(matches!(
            state_of(&frictions, &key),
            FrictionState::Learned { .. }
        ));
    }

    #[test]
    fn frictions_of_other_subjects_do_not_mix() {
        let repo = tempfile::tempdir().unwrap();
        seed_ticket(repo.path(), "TK-a3f9");
        seed_ticket(repo.path(), "TK-bbbb");
        let mine = note_friction(repo.path(), "TK-a3f9", "mine");
        let theirs = note_friction(repo.path(), "TK-bbbb", "theirs");

        let only_a = unclassified_for(repo.path(), &["TK-a3f9"]).unwrap();
        assert_eq!(only_a.len(), 1);
        assert_eq!(only_a[0].key, mine);
        assert_ne!(only_a[0].key, theirs);
        // A learning citing one subject's friction never classifies the other's.
        add_learning(repo.path(), &[format!("TK-a3f9#{mine}")]);
        let only_b = unclassified_for(repo.path(), &["TK-bbbb"]).unwrap();
        assert_eq!(only_b.len(), 1);
        assert_eq!(only_b[0].key, theirs);
    }

    #[test]
    fn frictions_survive_the_fifty_note_cut_by_living_in_the_event_log() {
        let repo = tempfile::tempdir().unwrap();
        seed_ticket(repo.path(), "TK-a3f9");
        for index in 0..52 {
            note_friction(repo.path(), "TK-a3f9", &format!("friction {index:02}"));
        }
        // The record keeps only the newest 50 notes (plan 0022 §4.3) —
        // the event log keeps all 52, and classification reads the log.
        let records = crate::store::issues::read_all(repo.path()).unwrap();
        assert_eq!(records[0]["notes"].as_array().unwrap().len(), 50);
        assert_eq!(list(repo.path(), Some("TK-a3f9")).unwrap().len(), 52);
    }

    #[test]
    fn dismiss_refuses_a_key_that_is_not_that_subjects_friction() {
        let repo = tempfile::tempdir().unwrap();
        seed_ticket(repo.path(), "TK-a3f9");
        seed_ticket(repo.path(), "TK-bbbb");
        let theirs = note_friction(repo.path(), "TK-bbbb", "theirs");
        let err = super::super::dismiss(
            repo.path(),
            &agent("worker"),
            "TK-a3f9",
            &[theirs],
            false,
            "nope",
        )
        .unwrap_err();
        assert_eq!(err.code(), "friction_not_found");
        assert!(err.hint().is_some());
    }

    #[test]
    fn a_note_event_missing_its_text_falls_back_to_the_record_then_archived() {
        let repo = tempfile::tempdir().unwrap();
        seed_ticket(repo.path(), "TK-a3f9");
        let key = note_friction(repo.path(), "TK-a3f9", "kept on the record");
        // Rewrite the event the way a pre-text payload looks.
        let day = repo.path().join(".pulse/events");
        let file = std::fs::read_dir(&day)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("jsonl"))
            .unwrap();
        let rewritten: Vec<String> = std::fs::read_to_string(&file)
            .unwrap()
            .lines()
            .map(|line| {
                let mut event: serde_json::Value = serde_json::from_str(line).unwrap();
                if event["id"] == json!(key) {
                    event["payload"] = json!({"kind": "friction"});
                }
                serde_json::to_string(&event).unwrap()
            })
            .collect();
        std::fs::write(&file, rewritten.join("\n") + "\n").unwrap();

        let frictions = list(repo.path(), Some("TK-a3f9")).unwrap();
        assert_eq!(frictions[0].text, "kept on the record");

        // No matching note at all: the friction still lists, with a marker.
        let records = crate::store::issues::read_all(repo.path()).unwrap();
        crate::store::issues::mutate(repo.path(), |mut updated| {
            updated[0]
                .as_object_mut()
                .unwrap()
                .insert("notes".to_string(), json!([]));
            Ok(updated)
        })
        .unwrap();
        drop(records);
        let frictions = list(repo.path(), Some("TK-a3f9")).unwrap();
        assert_eq!(frictions[0].text, "<archived>");
    }
}
