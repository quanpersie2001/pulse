//! The scheduling frontier (decision 0025 B5): what can run right now.
//!
//! Read-only by construction — no lock, no mutation, no event — and purely
//! deterministic: the same store always yields the same answer, so a host
//! can re-ask between spawns without the world moving under it. The
//! dependencies are [`crate::kernel::scope`] for the overlap arithmetic and
//! [`crate::kernel::reservation::held_by_others`] for who currently holds
//! files; everything else is a plain store read.
//!
//! The greedy pass is deliberate: two tickets whose `touches` overlap
//! cannot both start, and the smaller id wins — a scheduling loss the host
//! retries next round, never silent interleaved edits (the same
//! conservative bias `kernel::scope` documents).

use std::path::Path;

use chrono::Utc;
use serde_json::{json, Value};

use crate::error::Result;
use crate::kernel::issues::require;
use crate::kernel::reservation::touches_of;
use crate::kernel::scope;
use crate::store::issues;

/// The ready tickets that can start now, and what blocks the rest.
///
/// A candidate is a `ready` Ticket (optionally filtered to one Story). It
/// becomes `runnable` when its files are free; otherwise it lands in
/// `waiting` with the reason spelled out:
///
/// * `blocked_by` — a dependency was reopened after the ready gate passed;
/// * `reserved` — its `touches` intersect files another Ticket currently
///   holds (`active` with a live lease, or `verifying`);
/// * `frontier` — its `touches` intersect a runnable Ticket already
///   accepted in this same pass.
///
/// `held` lists every Ticket currently holding files, so the host can show
/// who blocks whom without a second query.
///
/// # Errors
/// `issue_not_found` if `story` names no existing record.
pub fn frontier(repo_root: &Path, story: Option<&str>) -> Result<Value> {
    let records = issues::read_all(repo_root)?;
    if let Some(story) = story {
        require(&records, story)?;
    }
    let now = Utc::now();
    let held = crate::kernel::reservation::held_by_others(&records, "", now);

    let mut candidates: Vec<&Value> = records
        .iter()
        .filter(|record| {
            record.get("kind").and_then(Value::as_str) == Some("ticket")
                && record.get("status").and_then(Value::as_str) == Some("ready")
                && story.map_or(true, |story| {
                    record.get("story").and_then(Value::as_str) == Some(story)
                })
        })
        .collect();
    // The store writes records sorted by id; sort anyway so the pass stays
    // deterministic no matter how the file was hand-edited.
    candidates.sort_by_key(|record| record.get("id").and_then(Value::as_str).unwrap_or(""));

    let mut runnable: Vec<Value> = Vec::new();
    let mut waiting: Vec<Value> = Vec::new();
    for candidate in candidates {
        // The ready gate checked these at promotion, but a dep can be
        // reopened (reworked) afterwards — re-check instead of trusting it.
        if let Some(blocking) = open_blocked_by(&records, candidate) {
            waiting.push(entry(candidate, "blocked_by", blocking, None));
            continue;
        }
        let my_touches = touches_of(candidate);
        let reserved = held.iter().find_map(|other| {
            let (_, pattern) = scope::overlaps(&my_touches, &touches_of(other))?;
            Some((
                other.get("id").and_then(Value::as_str).unwrap_or("?"),
                pattern,
            ))
        });
        if let Some((blocked_on, pattern)) = reserved {
            waiting.push(entry(candidate, "reserved", blocked_on, Some(pattern)));
            continue;
        }
        let collides = runnable.iter().find_map(|taken| {
            let (_, pattern) = scope::overlaps(&my_touches, &touches_of(taken))?;
            Some((
                taken.get("id").and_then(Value::as_str).unwrap_or("?"),
                pattern,
            ))
        });
        if let Some((blocked_on, pattern)) = collides {
            waiting.push(entry(candidate, "frontier", blocked_on, Some(pattern)));
            continue;
        }
        runnable.push(summary(candidate));
    }

    let mut held_list: Vec<Value> = held
        .iter()
        .map(|record| {
            json!({
                "id": record.get("id").cloned().unwrap_or(Value::Null),
                "status": record.get("status").cloned().unwrap_or(Value::Null),
                "actor": record.pointer("/lease/actor").cloned().unwrap_or(Value::Null),
                "touches": record.get("touches").cloned().unwrap_or_else(|| json!([])),
            })
        })
        .collect();
    held_list.sort_by_key(|entry| {
        entry
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    });

    Ok(json!({
        "runnable": runnable,
        "waiting": waiting,
        "held": held_list,
    }))
}

/// The first `blocked_by` dep of `record` whose target is neither `done`
/// nor `cancelled`. A dep id that no longer exists counts as open — the
/// same call the ready gate makes, kept conservative here.
fn open_blocked_by<'a>(records: &'a [Value], record: &'a Value) -> Option<&'a str> {
    record.get("deps")?.as_array()?.iter().find_map(|dep| {
        if dep.get("type").and_then(Value::as_str) != Some("blocked_by") {
            return None;
        }
        let dep_id = dep.get("id").and_then(Value::as_str)?;
        let status = records
            .iter()
            .find(|target| target.get("id").and_then(Value::as_str) == Some(dep_id))
            .and_then(|target| target.get("status"))
            .and_then(Value::as_str)
            .unwrap_or("");
        (status != "done" && status != "cancelled").then_some(dep_id)
    })
}

/// The ticket summary every list carries (plan 0025 B5 output shape).
fn summary(record: &Value) -> Value {
    json!({
        "id": record.get("id").cloned().unwrap_or(Value::Null),
        "title": record.get("title").cloned().unwrap_or(Value::Null),
        "surface": record.get("surface").cloned().unwrap_or(Value::Null),
        "risk": record.get("risk").cloned().unwrap_or(Value::Null),
        "touches": record.get("touches").cloned().unwrap_or_else(|| json!([])),
    })
}

/// A waiting entry: the summary plus why, who blocks, and which pattern.
/// `pattern` is absent for `blocked_by` — there is no file to name.
fn entry(record: &Value, reason: &str, blocked_on: &str, pattern: Option<String>) -> Value {
    let mut value = summary(record);
    let object = value.as_object_mut().expect("summary is always an object");
    object.insert("reason".to_string(), json!(reason));
    object.insert("blocked_on".to_string(), json!(blocked_on));
    if let Some(pattern) = pattern {
        object.insert("pattern".to_string(), json!(pattern));
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ready_ticket(id: &str) -> Value {
        json!({
            "schema": 3, "id": id, "kind": "ticket", "title": format!("t-{id}"),
            "status": "ready", "revision": 1,
            "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
            "role": "implementation", "risk": "medium", "surface": "cli",
        })
    }

    fn ready_ticket_with_touches(id: &str, touches: &[&str]) -> Value {
        let mut ticket = ready_ticket(id);
        ticket["touches"] = json!(touches);
        ticket
    }

    fn seeded(records: &[Value]) -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        issues::mutate(repo.path(), |mut all| {
            all.extend(records.iter().cloned());
            Ok(all)
        })
        .unwrap();
        repo
    }

    fn ids(report: &Value, list: &str) -> Vec<String> {
        report[list]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["id"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn two_tickets_with_disjoint_touches_are_both_runnable() {
        let repo = seeded(&[
            ready_ticket_with_touches("TK-a3f9", &["src/api/**"]),
            ready_ticket_with_touches("TK-bbbb", &["web/**"]),
        ]);
        let report = frontier(repo.path(), None).unwrap();
        assert_eq!(ids(&report, "runnable"), vec!["TK-a3f9", "TK-bbbb"]);
        assert!(report["waiting"].as_array().unwrap().is_empty());
    }

    #[test]
    fn overlapping_tickets_serialize_and_the_smaller_id_runs_first() {
        let repo = seeded(&[
            ready_ticket_with_touches("TK-bbbb", &["src/lib.rs"]),
            ready_ticket_with_touches("TK-a3f9", &["src/**"]),
        ]);
        let report = frontier(repo.path(), None).unwrap();
        assert_eq!(ids(&report, "runnable"), vec!["TK-a3f9"]);
        let waiting = &report["waiting"][0];
        assert_eq!(waiting["id"], "TK-bbbb");
        assert_eq!(waiting["reason"], "frontier");
        assert_eq!(waiting["blocked_on"], "TK-a3f9");
        assert_eq!(waiting["pattern"], "src/**");
    }

    #[test]
    fn a_ticket_overlapping_a_held_ticket_waits_with_reason_reserved() {
        let repo = tempfile::tempdir().unwrap();
        issues::mutate(repo.path(), |mut r| {
            let mut held = ready_ticket_with_touches("TK-a3f9", &["src/**"]);
            held["status"] = json!("verifying");
            held["lease"] = Value::Null;
            r.push(held);
            r.push(ready_ticket_with_touches("TK-bbbb", &["src/lib.rs"]));
            Ok(r)
        })
        .unwrap();
        let report = frontier(repo.path(), None).unwrap();
        let waiting = &report["waiting"][0];
        assert_eq!(waiting["id"], "TK-bbbb");
        assert_eq!(waiting["reason"], "reserved");
        assert_eq!(waiting["blocked_on"], "TK-a3f9");
        // The held list names the reviewer so the host can show who blocks.
        assert_eq!(report["held"][0]["id"], "TK-a3f9");
        assert_eq!(report["held"][0]["status"], "verifying");
        assert!(report["held"][0]["actor"].is_null());
    }

    #[test]
    fn an_exclusive_ticket_after_a_runnable_one_waits() {
        // A ticket without `touches` is exclusive (decision 0025): once
        // something else is runnable, it must wait — it claims everything.
        let repo = seeded(&[
            ready_ticket_with_touches("TK-a3f9", &["web/**"]),
            ready_ticket("TK-bbbb"),
        ]);
        let report = frontier(repo.path(), None).unwrap();
        assert_eq!(ids(&report, "runnable"), vec!["TK-a3f9"]);
        let waiting = &report["waiting"][0];
        assert_eq!(waiting["id"], "TK-bbbb");
        assert_eq!(waiting["reason"], "frontier");
    }

    #[test]
    fn an_exclusive_ticket_first_makes_everything_after_it_wait() {
        let repo = seeded(&[
            ready_ticket_with_touches("TK-bbbb", &["web/**"]),
            ready_ticket("TK-a3f9"),
        ]);
        let report = frontier(repo.path(), None).unwrap();
        assert_eq!(ids(&report, "runnable"), vec!["TK-a3f9"]);
        assert_eq!(ids(&report, "waiting"), vec!["TK-bbbb"]);
    }

    #[test]
    fn a_reopened_blocked_by_dep_blocks_even_with_free_files() {
        // The ready gate checked this edge at promotion; TK-a3f9 has since
        // gone back to `ready` (reopened), so TK-bbbb may not start.
        let mut blocker = ready_ticket_with_touches("TK-a3f9", &["src/api/**"]);
        blocker["status"] = json!("ready");
        let mut blocked = ready_ticket_with_touches("TK-bbbb", &["web/**"]);
        blocked["deps"] = json!([{"type": "blocked_by", "id": "TK-a3f9"}]);
        let repo = seeded(&[blocker, blocked]);
        let report = frontier(repo.path(), None).unwrap();
        assert_eq!(ids(&report, "runnable"), vec!["TK-a3f9"]);
        let waiting = &report["waiting"][0];
        assert_eq!(waiting["id"], "TK-bbbb");
        assert_eq!(waiting["reason"], "blocked_by");
        assert_eq!(waiting["blocked_on"], "TK-a3f9");
        assert!(waiting.get("pattern").is_none());
    }

    #[test]
    fn the_story_filter_narrows_candidates() {
        let story = json!({
            "schema": 3, "id": "ST-1111", "kind": "story", "title": "s",
            "status": "ready", "revision": 1,
            "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
            "outcome": "o",
        });
        let repo = seeded(&[
            story,
            ready_ticket_with_touches("TK-a3f9", &["src/api/**"]),
            {
                let mut ticket = ready_ticket_with_touches("TK-bbbb", &["web/**"]);
                ticket["story"] = json!("ST-1111");
                ticket
            },
        ]);
        let report = frontier(repo.path(), Some("ST-1111")).unwrap();
        assert_eq!(ids(&report, "runnable"), vec!["TK-bbbb"]);

        let err = frontier(repo.path(), Some("ST-missing")).unwrap_err();
        assert_eq!(err.code(), "issue_not_found");
    }
}
