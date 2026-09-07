//! Thin CLI adapter for the event-log communication surface (`pulse note`,
//! `pulse events tail`).

use std::io::Write;
use std::thread::sleep;
use std::time::Duration;

use crate::cli::output::render;
use crate::event::{read_events, EventEnvelope};
use crate::kernel::communication::NoteKind;
use crate::{JsonGraphStore, PulseError};

pub(crate) fn handle_note(
    store: &JsonGraphStore,
    ticket: &str,
    message: &str,
    from: &str,
    kind: NoteKind,
    json: bool,
) -> Result<(), PulseError> {
    let note = store.record_note(ticket, message, from, kind)?;
    render(
        json,
        &note,
        format!(
            "{} recorded for {} by {}",
            note.kind.as_str(),
            note.work_id,
            note.recorded_by
        ),
    )
}

pub(crate) fn handle_tail(
    store: &JsonGraphStore,
    since: &str,
    ticket: Option<&str>,
    follow: bool,
    json: bool,
) -> Result<(), PulseError> {
    let mut cursor = since.to_string();
    let batch = |store: &JsonGraphStore, cursor: &mut String| -> Vec<EventEnvelope> {
        let events = read_events(store.repo_root()).unwrap_or_default();
        let mut matched = Vec::new();
        for event in events {
            if event.id.as_str() <= cursor.as_str() {
                continue;
            }
            if let Some(ticket) = ticket {
                if !crate::kernel::communication::event_targets_ticket(&event, ticket) {
                    continue;
                }
            }
            matched.push(event);
        }
        if let Some(last) = matched.last() {
            *cursor = last.id.clone();
        }
        matched
    };

    let matched = batch(store, &mut cursor);
    if !follow {
        if json {
            // One-shot JSON reads return a single JSON array.
            println!(
                "{}",
                serde_json::to_string_pretty(&matched).unwrap_or_else(|_| "[]".to_string())
            );
        } else {
            print_human_lines(&matched);
        }
        return Ok(());
    }

    // Streaming mode: newline-delimited output, one event per line.
    print_batch(&matched, json);
    loop {
        sleep(Duration::from_millis(1000));
        let matched = batch(store, &mut cursor);
        print_batch(&matched, json);
    }
}

fn print_batch(events: &[EventEnvelope], json: bool) {
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    for event in events {
        let line = if json {
            serde_json::to_string(event).unwrap_or_else(|_| "{}".to_string())
        } else {
            human_line(event)
        };
        writeln!(lock, "{line}").ok();
    }
    lock.flush().ok();
}

fn print_human_lines(events: &[EventEnvelope]) {
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    for event in events {
        writeln!(lock, "{}", human_line(event)).ok();
    }
    lock.flush().ok();
}

fn human_line(event: &EventEnvelope) -> String {
    format!(
        "{} {} {} {}: {}",
        event.id,
        event.occurred_at.to_rfc3339(),
        event.event_type,
        event.subject.id,
        event
            .payload
            .get("message")
            .or_else(|| event.payload.get("summary"))
            .and_then(|value| value.as_str())
            .unwrap_or(""),
    )
}
