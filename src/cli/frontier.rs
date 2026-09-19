//! Thin CLI adapter for `pulse frontier` (decision 0025 B5).
//!
//! Purely a renderer over the kernel's read-only report: the host asks
//! what can run, Pulse answers from the store, nothing is dispatched.

use std::path::Path;

use serde_json::Value;

use crate::cli::output::render;
use crate::kernel::frontier;
use crate::PulseError;

pub(crate) fn handle(repo_root: &Path, story: Option<&str>, json: bool) -> Result<(), PulseError> {
    let report = frontier::frontier(repo_root, story)?;
    render(json, &report, render_human(&report))
}

fn field_str<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("?")
}

/// Three blocks in `work list`'s voice — one line per ticket, the blocking
/// ticket and pattern named where there is one.
fn render_human(report: &Value) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push("Runnable now:".to_string());
    let empty: Vec<Value> = Vec::new();
    let runnable = report["runnable"].as_array().unwrap_or(&empty);
    if runnable.is_empty() {
        lines.push("  (nothing — every ready ticket is waiting or held)".to_string());
    }
    for ticket in runnable {
        lines.push(format!(
            "  {} [{} {}] {} · touches: {}",
            field_str(ticket, "id"),
            field_str(ticket, "surface"),
            field_str(ticket, "risk"),
            field_str(ticket, "title"),
            touches_field(ticket),
        ));
    }

    lines.push("Waiting:".to_string());
    let waiting = report["waiting"].as_array().unwrap_or(&empty);
    if waiting.is_empty() {
        lines.push("  (nothing)".to_string());
    }
    for ticket in waiting {
        let why = match field_str(ticket, "reason") {
            "blocked_by" => format!("blocked_by {}", field_str(ticket, "blocked_on")),
            "reserved" => format!(
                "reserved: {} holds {}",
                field_str(ticket, "blocked_on"),
                field_str(ticket, "pattern")
            ),
            "frontier" => format!(
                "frontier: {} holds {}",
                field_str(ticket, "blocked_on"),
                field_str(ticket, "pattern")
            ),
            other => format!("reason: {other}"),
        };
        lines.push(format!(
            "  {} [{} {}] {} · {}",
            field_str(ticket, "id"),
            field_str(ticket, "surface"),
            field_str(ticket, "risk"),
            field_str(ticket, "title"),
            why,
        ));
    }

    lines.push("Held:".to_string());
    let held = report["held"].as_array().unwrap_or(&empty);
    if held.is_empty() {
        lines.push("  (no ticket is holding files)".to_string());
    }
    for ticket in held {
        let actor = ticket
            .get("actor")
            .and_then(Value::as_str)
            .unwrap_or("in review");
        lines.push(format!(
            "  {} [{}, {}] {}",
            field_str(ticket, "id"),
            field_str(ticket, "status"),
            actor,
            touches_field(ticket),
        ));
    }

    lines.join("\n")
}

fn touches_field(ticket: &Value) -> String {
    let touches = ticket
        .get("touches")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<&str>>()
                .join(", ")
        })
        .unwrap_or_default();
    if touches.is_empty() {
        "<exclusive>".to_string()
    } else {
        touches
    }
}
