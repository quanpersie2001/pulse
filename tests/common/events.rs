//! Reading the append-only event log from tests (Decision 0011).
//!
//! Standalone includable unit — see `tests/common/mod.rs` for why helpers are
//! wired in per crate with `#[path]`.
//!
//! Every suite used to walk `.pulse/events/<date>/` itself. When Decision 0011
//! made the layout `<date>.jsonl`, each of those copies broke separately, so
//! the walk lives here once now.

#![allow(dead_code)]

use std::fs;
use std::path::Path;

use pulse::event::EventEnvelope;

/// Every event recorded in `repo_root`, in ULID (chronological) order.
///
/// Reads both layouts: the `<date>.jsonl` day files written today and the
/// legacy `<date>/evt_*.json` files that `pulse events compact` converts.
pub fn read_events(repo_root: &Path) -> Vec<EventEnvelope> {
    let root = repo_root.join(".pulse/events");
    if !root.exists() {
        return Vec::new();
    }
    let mut events = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read event dir") {
            let path = entry.expect("event dir entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            match path.extension().and_then(|ext| ext.to_str()) {
                Some("jsonl") => {
                    let bytes = fs::read(&path).expect("read day file");
                    for line in bytes.split(|byte| *byte == b'\n') {
                        if line.is_empty() {
                            continue;
                        }
                        events.push(
                            serde_json::from_slice::<EventEnvelope>(line)
                                .expect("parse event line"),
                        );
                    }
                }
                Some("json") => {
                    let bytes = fs::read(&path).expect("read legacy event");
                    events.push(
                        serde_json::from_slice::<EventEnvelope>(&bytes).expect("parse event file"),
                    );
                }
                _ => {}
            }
        }
    }
    events.sort_by(|left, right| left.id.cmp(&right.id));
    events
}

/// Number of events recorded in `repo_root`.
pub fn count_events(repo_root: &Path) -> usize {
    read_events(repo_root).len()
}

/// Events whose `event_type` equals `event_type`, in order.
pub fn events_of_type(repo_root: &Path, event_type: &str) -> Vec<EventEnvelope> {
    read_events(repo_root)
        .into_iter()
        .filter(|event| event.event_type == event_type)
        .collect()
}
