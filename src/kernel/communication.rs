//! Agent-to-agent communication over the append-only event log.
//!
//! Implements PRODUCT §5.7: `pulse note` records an event targeting a Ticket
//! and every note surfaces in the Ticket's packet and in `events tail`.
//! There is no delivery guarantee, no mailbox and no broker — the event log
//! is the entire channel.

use std::path::Path;

use chrono::Utc;
use serde::Serialize;
use serde_json::json;

use crate::event::read_events;
use crate::graph::store::JsonGraphStore;
use crate::{PulseError, PulseResult};

/// Maximum notes surfaced in a packet (latest wins).
pub const MAX_PACKET_NOTES: usize = 8;

/// Maximum characters of a note message kept verbatim.
pub const MAX_NOTE_CHARS: usize = 2_000;

impl JsonGraphStore {
    /// Record a note targeting a Ticket. Requires the `note` grant.
    pub fn record_note(
        &self,
        ticket_id: &str,
        message: &str,
        actor: &str,
    ) -> PulseResult<NoteRecorded> {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return Err(PulseError::validation(
                "note_message_missing",
                "note message must not be empty",
            ));
        }
        if trimmed.chars().count() > MAX_NOTE_CHARS {
            return Err(PulseError::validation(
                "note_message_too_long",
                format!("note message must stay within {MAX_NOTE_CHARS} characters"),
            ));
        }
        // The target must exist so notes cannot bind to typos.
        self.show_node(ticket_id)?;
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
                ticket_id,
                json!({
                    "ticket_id": ticket_id,
                    "message": trimmed,
                }),
                Utc::now(),
            ),
        )?;
        Ok(NoteRecorded {
            schema_version: 1,
            code: "note_recorded".to_string(),
            ticket_id: ticket_id.to_string(),
            message: trimmed.to_string(),
            recorded_by: actor.to_string(),
        })
    }
}

/// Result of recording one note.
#[derive(Debug, Clone, Serialize)]
pub struct NoteRecorded {
    pub schema_version: u32,
    pub code: String,
    pub ticket_id: String,
    pub message: String,
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
