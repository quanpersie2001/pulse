//! Agent-to-agent communication over the append-only event log.
//!
//! Implements PRODUCT §5.7: `pulse note` records an event targeting a Ticket
//! and every note surfaces in the Ticket's packet and in `events tail`.
//! There is no delivery guarantee, no mailbox and no broker — the event log
//! is the entire channel.

use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::event::read_events;
use crate::graph::store::JsonGraphStore;
use crate::{PulseError, PulseResult};

/// Maximum notes surfaced in a packet (latest wins).
pub const MAX_PACKET_NOTES: usize = 8;

/// Maximum characters of a note message kept verbatim.
pub const MAX_NOTE_CHARS: usize = 2_000;

/// What a note is about. `Friction` marks harness friction: the close gate
/// turns every friction note on a Ticket into a learning `candidate` with
/// scope `harness` (Decision 0009 §4).
///
/// Persisted as the `kind` key of the `note.recorded` payload. Notes recorded
/// before the key existed carry no `kind` and read back as `Note`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    #[default]
    Note,
    Friction,
}

impl NoteKind {
    /// Payload spelling of this kind.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Friction => "friction",
        }
    }
}

impl JsonGraphStore {
    /// Record a note targeting a Ticket. Requires the `note` grant.
    pub fn record_note(
        &self,
        work_id: &str,
        message: &str,
        actor: &str,
        kind: NoteKind,
    ) -> PulseResult<NoteRecorded> {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return Err(PulseError::validation(
                "note_message_missing",
                "note message must not be empty",
            ));
        }
        // Notes are tracked-plane text (Decision 0012 §4): rewrite in-repo
        // absolute paths, refuse secret-shaped strings.
        let cleaned = crate::evidence::redaction::clean_text(&self.repo_root, "message", trimmed)?;
        if cleaned.chars().count() > MAX_NOTE_CHARS {
            return Err(PulseError::validation(
                "note_message_too_long",
                format!("note message must stay within {MAX_NOTE_CHARS} characters"),
            ));
        }
        // The target must exist so notes cannot bind to typos. Notes may
        // target any work node: Epic, Story, Ticket or Decision (Decision
        // 0013 §5).
        self.show_node(work_id)?;
        crate::policy::authorize(
            &crate::policy::load_authority_policy(&self.repo_root)?,
            &crate::policy::parse_actor(actor),
            &["note"],
        )?;
        let event_id = format!("evt_{}", ulid::Ulid::new());
        crate::event::write_event(
            &self.repo_root,
            &crate::event::EventEnvelope::new(
                event_id,
                "note.recorded",
                actor,
                work_id,
                json!({
                    "work_id": work_id,
                    "message": cleaned,
                    "kind": kind.as_str(),
                }),
                Utc::now(),
            ),
        )?;
        Ok(NoteRecorded {
            schema_version: 1,
            code: "note_recorded".to_string(),
            work_id: work_id.to_string(),
            message: cleaned,
            kind,
            recorded_by: actor.to_string(),
        })
    }
}

/// Result of recording one note.
#[derive(Debug, Clone, Serialize)]
pub struct NoteRecorded {
    pub schema_version: u32,
    pub code: String,
    /// The targeted work node; `ticket_id` remains accepted as an alias
    /// when reading (Decision 0013 §5).
    #[serde(alias = "ticket_id")]
    pub work_id: String,
    pub message: String,
    pub kind: NoteKind,
    pub recorded_by: String,
}

/// Latest notes targeting `ticket_id`, oldest first, bounded for the packet.
/// Reads never fail because of one damaged event file.
pub fn list_notes_for_ticket(repo_root: &Path, ticket_id: &str) -> Vec<String> {
    let mut notes: Vec<(String, String)> = read_events(repo_root)
        .unwrap_or_default()
        .into_iter()
        .filter(|event| event.event_type == "note.recorded")
        .filter(|event| event.subject.id == ticket_id)
        .filter_map(|event| {
            let message = event
                .payload
                .get("message")
                .and_then(|value| value.as_str())?
                .to_string();
            Some((event.id, message))
        })
        .collect();
    notes.sort_by(|left, right| left.0.cmp(&right.0));
    notes
        .into_iter()
        .map(|(_, message)| {
            if message.chars().count() > 500 {
                let truncated: String = message.chars().take(500).collect();
                format!("{truncated}…")
            } else {
                message
            }
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .take(MAX_PACKET_NOTES)
        .rev()
        .collect()
}

/// Every friction note targeting `ticket_id`, oldest first.
///
/// Unlike [`list_notes_for_ticket`] this is neither truncated nor capped: it
/// feeds learning candidates at the close gate (Decision 0009 §4), where
/// dropping or clipping a report would silently lose the evidence. Notes
/// recorded before the payload carried a `kind` read back as
/// [`NoteKind::Note`] and are therefore never treated as friction.
pub fn list_friction_for_ticket(repo_root: &Path, ticket_id: &str) -> Vec<String> {
    let mut friction: Vec<(String, String)> = read_events(repo_root)
        .unwrap_or_default()
        .into_iter()
        .filter(|event| event.event_type == "note.recorded")
        .filter(|event| event.subject.id == ticket_id)
        .filter(|event| {
            event.payload.get("kind").and_then(|value| value.as_str())
                == Some(NoteKind::Friction.as_str())
        })
        .filter_map(|event| {
            let message = event
                .payload
                .get("message")
                .and_then(|value| value.as_str())?
                .to_string();
            Some((event.id, message))
        })
        .collect();
    friction.sort_by(|left, right| left.0.cmp(&right.0));
    friction.into_iter().map(|(_, message)| message).collect()
}

/// Whether an event targets `ticket_id` (by subject or payload binding).
pub fn event_targets_ticket(event: &crate::event::EventEnvelope, ticket_id: &str) -> bool {
    if event.subject.id == ticket_id {
        return true;
    }
    event
        .payload
        .get("ticket_id")
        .and_then(|value| value.as_str())
        .is_some_and(|bound| bound == ticket_id)
}
