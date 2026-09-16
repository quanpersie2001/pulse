//! Thin CLI adapter for `pulse events tail`.

use std::collections::HashSet;
use std::io::Write;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use crate::event::{read_event_log, EventEnvelope};
use crate::PulseError;

/// Report a day file whose last line did not parse (Decision 0011 §5).
fn report_torn_tails(paths: &[String]) {
    for path in paths {
        eprintln!(
            "{}",
            serde_json::json!({
                "schema_version": 1,
                "code": "events_torn_tail",
                "path": path,
                "message": "the last line of this day file did not parse; a crash \
                            interrupted an append. Events before it are intact and the \
                            next append truncates the torn line.",
            })
        );
    }
}

pub(crate) fn handle_tail(
    repo_root: &Path,
    since: &str,
    id: Option<&str>,
    follow: bool,
    json: bool,
) -> Result<(), PulseError> {
    let mut cursor = since.to_string();
    // Follow mode re-reads once a second; a torn tail stays torn until the
    // next append, so report each file once rather than every poll.
    let mut reported: HashSet<String> = HashSet::new();
    let batch = |cursor: &mut String, reported: &mut HashSet<String>| -> Vec<EventEnvelope> {
        let read = read_event_log(repo_root).unwrap_or_default();
        let fresh: Vec<String> = read
            .torn_tails
            .iter()
            .filter(|path| reported.insert((*path).clone()))
            .cloned()
            .collect();
        report_torn_tails(&fresh);
        let mut matched = Vec::new();
        for event in read.events {
            if event.id.as_str() <= cursor.as_str() {
                continue;
            }
            if let Some(id) = id {
                if event.subject.id != id {
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

    let matched = batch(&mut cursor, &mut reported);
    if !follow {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&matched).unwrap_or_else(|_| "[]".to_string())
            );
        } else {
            print_human_lines(&matched);
        }
        return Ok(());
    }

    print_batch(&matched, json);
    loop {
        sleep(Duration::from_millis(1000));
        let matched = batch(&mut cursor, &mut reported);
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
    // Prose events (notes) render their message; structured events (run /
    // checkpoint, whose facts are role/outcome/verdict and no prose) render
    // the payload compactly, so `events tail` is readable without opening
    // the day file (dogfood ST-1, F7/F8).
    let detail = event
        .payload
        .get("message")
        .or_else(|| event.payload.get("text"))
        .or_else(|| event.payload.get("summary"))
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| {
            let compact = serde_json::to_string(&event.payload).unwrap_or_default();
            if compact == "{}" {
                String::new()
            } else {
                compact
            }
        });
    format!(
        "{} {} {} {}: {}",
        event.id,
        event.occurred_at.to_rfc3339(),
        event.event_type,
        event.subject.id,
        detail,
    )
}
