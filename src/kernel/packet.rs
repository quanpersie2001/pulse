//! `pulse packet <id>` (plan 0022 §9): the one bounded JSON a worker reads
//! before doing anything.
//!
//! `docs.applicable` comes from `docs::applicable::applicable` (A3);
//! plan 0025 F1 demoted it to a frontmatter hint — the worker grep/globs
//! `docs/` first, and `docs::stale` (F3) is where `applies_to` does its
//! real work now. `learnings` from `learn::recall::applicable` (A2).
//! `last_verdicts`
//! resolves each `ticket.verdicts[role].receipt` to its lane receipt and
//! returns what a reworking worker must read: the still-open findings plus
//! the failing acceptance criteria and qa cases. A receipt that cannot be
//! read does not fail the whole packet — that entry is tagged
//! `findings_unavailable` instead. Packet staleness has one fence: `source`
//! (plan §9 — "Không fingerprint từng input; fence duy nhất là `source`").

use std::path::Path;

use serde_json::{json, Value};

use crate::error::Result;
use crate::kernel::issues::{find, require};
use crate::source;
use crate::store::issues::read_all;

fn strip_runtime_fields(record: &Value) -> Value {
    let mut record = record.clone();
    if let Some(object) = record.as_object_mut() {
        object.remove("lease");
        object.remove("verdicts");
    }
    record
}

fn story_view(story: &Value) -> Value {
    json!({
        "id": story.get("id"),
        "outcome": story.get("outcome"),
        "rules": story.get("rules").cloned().unwrap_or(json!([])),
        "exceptions": story.get("exceptions").cloned().unwrap_or(json!([])),
        "approach": story.get("approach"),
        // The reviewer's QA oracle (dogfood ST-1, F4): the worker should know
        // what will be independently checked, not just its own acceptance.
        "qa_cases": story.get("qa_cases").cloned().unwrap_or(json!([])),
    })
}

fn epic_view(epic: &Value) -> Value {
    json!({
        "id": epic.get("id"),
        "outcome": epic.get("outcome"),
        "out_of_scope": epic.get("out_of_scope").cloned().unwrap_or(json!([])),
    })
}

fn decision_view(decision: &Value) -> Value {
    json!({
        "id": decision.get("id"),
        "title": decision.get("title"),
        "decision": decision.get("decision"),
        "consequences": decision.get("consequences"),
    })
}

fn blockers(records: &[Value], ticket: &Value) -> Vec<Value> {
    ticket
        .get("deps")
        .and_then(Value::as_array)
        .map(|deps| {
            deps.iter()
                .filter(|dep| dep.get("type").and_then(Value::as_str) == Some("blocked_by"))
                .filter_map(|dep| dep.get("id").and_then(Value::as_str))
                .map(|id| {
                    let status = find(records, id)
                        .and_then(|record| record.get("status"))
                        .cloned()
                        .unwrap_or(Value::Null);
                    json!({"id": id, "status": status})
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Keep only the fields a reworking worker needs from a finding, dropping
/// resolved ones (plan 0025 A3).
fn open_findings(payload: &Value) -> Vec<Value> {
    payload
        .get("findings")
        .and_then(Value::as_array)
        .map(|findings| {
            findings
                .iter()
                // Decision 0027 C3: `unconfirmed` is what a panel's round 2
                // already lowered — a suspicion the reconciliation rejected.
                // Only `open` is rework; handing a worker the rejected ones
                // turns a cleared doubt into a demand.
                .filter(|f| f.get("status").and_then(Value::as_str) == Some("open"))
                .map(|f| {
                    json!({
                        "id": f.get("id"),
                        "ref": f.get("ref"),
                        "summary": f.get("summary"),
                        "owner": f.get("owner"),
                        "check": f.get("check"),
                        "severity": f.get("severity"),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn failed_acceptance(payload: &Value) -> Vec<Value> {
    payload
        .get("acceptance")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| entry.get("status").and_then(Value::as_str) == Some("fail"))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

fn failed_cases(payload: &Value) -> Vec<Value> {
    payload
        .get("cases")
        .and_then(Value::as_array)
        .map(|cases| {
            cases
                .iter()
                .filter(|case| case.get("status").and_then(Value::as_str) != Some("pass"))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// # Errors
/// Never fails on a verdict whose receipt is missing or unreadable: that
/// entry carries `findings_unavailable: true` instead, so one broken receipt
/// cannot make the packet unreadable.
fn last_verdicts(repo_root: &Path, ticket: &Value) -> Result<Vec<Value>> {
    let Some(verdicts) = ticket.get("verdicts").and_then(Value::as_object) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for (lane, verdict) in verdicts {
        let mut entry = json!({
            "lane": lane,
            "verdict": verdict.get("verdict"),
            "commit": verdict.get("commit"),
        });
        let receipt_id = verdict.get("receipt").and_then(Value::as_str);
        let payload = receipt_id
            .and_then(|id| crate::evidence::receipt::load_receipt(repo_root, id).ok())
            .map(|receipt| receipt.payload);
        match payload {
            Some(payload) => {
                entry["findings"] = json!(open_findings(&payload));
                entry["failed_acceptance"] = json!(failed_acceptance(&payload));
                entry["failed_cases"] = json!(failed_cases(&payload));
            }
            None => {
                entry["findings_unavailable"] = json!(true);
            }
        }
        out.push(entry);
    }
    Ok(out)
}

/// The compact `{id, summary, do, avoid, check}` shape plan §9 wants in the
/// packet, built from a learning's opaque body via
/// `learn::store::sections`/`bullet_items`. `Check` is `- ` bulleted in the
/// body like `Do`/`Avoid` (plan §11.1's example), but the packet shape wants
/// one string (plan §9: `"check":"…"`), so multiple check bullets join with
/// `"; "`.
fn learning_view(repo_root: &Path, learning: &crate::learn::store::Learning) -> Value {
    let sections = crate::learn::store::sections(&learning.body);
    let text = |name: &str| sections.get(name).cloned().unwrap_or_default();
    let check_items = crate::learn::store::bullet_items(&text("Check"));
    let check = if check_items.is_empty() {
        text("Check")
    } else {
        check_items.join("; ")
    };
    // Plan 0025 E2: `enforced` says which learnings `pulse verify` will run
    // (active + a check argv) — a candidate never runs, whatever it cites.
    let enforced =
        learning.frontmatter.status == "active" && !learning.frontmatter.check_argv.is_empty();
    // Plan 0025 E4: a stale cite means the code moved since the learning was
    // written — use with care; nothing is auto-retired.
    let stale = !crate::learn::stale_cites(repo_root, learning).is_empty();
    json!({
        "id": learning.frontmatter.id,
        "status": learning.frontmatter.status,
        "summary": text("Summary"),
        "do": crate::learn::store::bullet_items(&text("Do")),
        "avoid": crate::learn::store::bullet_items(&text("Avoid")),
        "check": check,
        "check_argv": learning.frontmatter.check_argv,
        "enforced": enforced,
        "stale": stale,
    })
}

fn recent_notes(ticket: &Value, limit: usize) -> Vec<Value> {
    ticket
        .get("notes")
        .and_then(Value::as_array)
        .map(|notes| {
            let start = notes.len().saturating_sub(limit);
            notes[start..].to_vec()
        })
        .unwrap_or_default()
}

/// # Errors
/// `issue_not_found` if `id` does not exist; propagates a git error from
/// [`source::snapshot`].
pub fn build_packet(repo_root: &Path, id: &str) -> Result<Value> {
    let records = read_all(repo_root)?;
    let ticket = require(&records, id)?;

    let story = ticket
        .get("story")
        .and_then(Value::as_str)
        .and_then(|story_id| find(&records, story_id));
    let epic = story
        .and_then(|story| story.get("epic").and_then(Value::as_str))
        .and_then(|epic_id| find(&records, epic_id));
    let decisions: Vec<Value> = ticket
        .pointer("/context/decisions")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .filter_map(|decision_id| find(&records, decision_id))
                .map(decision_view)
                .collect()
        })
        .unwrap_or_default();

    let snapshot = source::snapshot(repo_root, &[])?;
    // Candidates included, clearly tagged with their status (dogfood ST-1,
    // F23): the old exclude-candidates rule made the activate bar
    // (usage.helpful >= 1) unreachable — a learning nobody sees can never be
    // cited as helpful by a handoff. The worker can now cite a candidate and
    // its handoff feeds the two-sided activation bar.
    let learnings: Vec<Value> = crate::learn::recall::applicable(repo_root, id, true)?
        .iter()
        .map(|learning| learning_view(repo_root, learning))
        .collect();
    let docs_applicable: Vec<Value> = crate::docs::applicable::applicable(repo_root, id)?
        .iter()
        .map(|m| json!({"path": m.path, "why": m.why, "lines": m.lines}))
        .collect();

    Ok(json!({
        "issue": strip_runtime_fields(ticket),
        "story": story.map(story_view),
        "epic": epic.map(epic_view),
        "decisions": decisions,
        "blockers": blockers(&records, ticket),
        "docs": {"applicable": docs_applicable, "map": "docs/README.md"},
        "learnings": learnings,
        "checkpoint": ticket.get("checkpoints").and_then(Value::as_array).and_then(|cps| cps.last()).cloned(),
        "last_verdicts": last_verdicts(repo_root, ticket)?,
        "notes": recent_notes(ticket, 8),
        "source": {"commit": snapshot.commit, "dirty": !snapshot.dirty_paths.is_empty()},
        "protocol": {
            // The lease's run id, so a checkpoint or handoff written from
            // this packet correlates with the `run.started` event that
            // claimed the Ticket (dogfood ST-1, F11 — two id spaces used to
            // coexist in the same event field). `null` until `pulse claim`.
            "run_id": ticket.pointer("/lease/run_id").cloned().unwrap_or(Value::Null),
            "claim": format!("pulse claim {id}"),
            "checkpoint": format!("pulse checkpoint {id} --from <path>"),
            // Decision 0026: the worker runs the declared verify[] itself
            // before handing off; the receipt is what the gate reads.
            "verify": format!("pulse verify {id}"),
            "handoff": format!("pulse handoff {id} --from <path>"),
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;

    fn git_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(StdCommand::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .status()
                .unwrap()
                .success());
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "test"]);
        std::fs::write(dir.path().join("README.md"), "x\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "init"]);
        dir
    }

    #[test]
    fn builds_a_packet_with_resolved_story_epic_and_blockers() {
        let repo = git_repo();
        crate::store::issues::mutate(repo.path(), |mut records| {
            records.push(json!({
                "schema": 3, "id": "EP-1111", "kind": "epic", "title": "e",
                "status": "active", "revision": 1,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "outcome": "epic outcome",
            }));
            records.push(json!({
                "schema": 3, "id": "ST-1111", "kind": "story", "title": "s",
                "status": "ready", "revision": 1,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "epic": "EP-1111", "outcome": "story outcome",
                "rules": [{"id": "BR-1", "text": "x"}],
            }));
            records.push(json!({
                "schema": 3, "id": "TK-9999", "kind": "ticket", "title": "blocker",
                "status": "done", "revision": 1,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "role": "implementation",
            }));
            records.push(json!({
                "schema": 3, "id": "TK-a3f9", "kind": "ticket", "title": "t",
                "status": "active", "revision": 1,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "role": "implementation", "story": "ST-1111",
                "deps": [{"type": "blocked_by", "id": "TK-9999"}],
                "notes": [{"at": "t", "from": "human:x", "kind": "note", "text": "n1"}],
            }));
            Ok(records)
        })
        .unwrap();

        let packet = build_packet(repo.path(), "TK-a3f9").unwrap();
        assert_eq!(packet["issue"]["id"], "TK-a3f9");
        assert_eq!(packet["story"]["outcome"], "story outcome");
        assert_eq!(packet["epic"]["outcome"], "epic outcome");
        assert_eq!(packet["blockers"][0]["id"], "TK-9999");
        assert_eq!(packet["blockers"][0]["status"], "done");
        assert_eq!(packet["notes"][0]["text"], "n1");
        assert_eq!(packet["source"]["dirty"], false);
        assert!(packet["issue"].get("lease").is_none());
    }

    #[test]
    fn notes_are_bounded_to_the_last_eight() {
        let repo = git_repo();
        let notes: Vec<Value> = (0..10)
            .map(|i| json!({"at": "t", "from": "human:x", "kind": "note", "text": format!("n{i}")}))
            .collect();
        crate::store::issues::mutate(repo.path(), |mut records| {
            records.push(json!({
                "schema": 3, "id": "TK-a3f9", "kind": "ticket", "title": "t",
                "status": "draft", "revision": 1,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "role": "implementation", "notes": notes,
            }));
            Ok(records)
        })
        .unwrap();
        let packet = build_packet(repo.path(), "TK-a3f9").unwrap();
        let packet_notes = packet["notes"].as_array().unwrap();
        assert_eq!(packet_notes.len(), 8);
        assert_eq!(packet_notes[0]["text"], "n2");
        assert_eq!(packet_notes[7]["text"], "n9");
    }

    #[test]
    fn packet_carries_learnings_with_status_including_candidates() {
        let repo = git_repo();
        crate::store::issues::mutate(repo.path(), |mut records| {
            records.push(json!({
                "schema": 3, "id": "TK-a3f9", "kind": "ticket", "title": "t",
                "status": "draft", "revision": 1,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "role": "implementation",
                "context": {"anchors": ["src/auth/refresh.rs"]},
            }));
            Ok(records)
        })
        .unwrap();
        let active = crate::learn::store::Learning {
            frontmatter: crate::learn::store::Frontmatter {
                id: "LRN-1111".to_string(),
                status: "active".to_string(),
                kind: "failure".to_string(),
                applies_to: vec!["src/auth/**".to_string()],
                tags: vec![],
                from: vec![],
                expected_signal: String::new(),
                usage: crate::learn::store::UsageCounts::default(),
                    check_argv: vec![],
                    check_cwd: None,
                    cites: vec![],
            },
            body: "## Summary\nrotation must be atomic\n## Do\n- use a transaction\n## Avoid\n- split read/write\n## Check\n- run it 10x\n".to_string(),
        };
        let mut candidate = active.clone();
        candidate.frontmatter.id = "LRN-2222".to_string();
        candidate.frontmatter.status = "candidate".to_string();
        crate::learn::store::write(repo.path(), &active).unwrap();
        crate::learn::store::write(repo.path(), &candidate).unwrap();

        let packet = build_packet(repo.path(), "TK-a3f9").unwrap();
        // Dogfood ST-1 F23: candidates ride along, tagged with their status —
        // the old exclude-candidates rule made the activation bar
        // (usage.helpful >= 1) unreachable, since nobody could cite them.
        let learnings = packet["learnings"].as_array().unwrap();
        assert_eq!(learnings.len(), 2);
        let by_id = |id: &str| {
            learnings
                .iter()
                .find(|l| l["id"] == id)
                .unwrap_or_else(|| panic!("missing {id}"))
        };
        let active = by_id("LRN-1111");
        assert_eq!(active["status"], "active");
        assert_eq!(active["summary"], "rotation must be atomic");
        assert_eq!(active["do"], json!(["use a transaction"]));
        assert_eq!(active["avoid"], json!(["split read/write"]));
        assert_eq!(active["check"], "run it 10x");
        let candidate = by_id("LRN-2222");
        assert_eq!(candidate["status"], "candidate");
    }

    #[test]
    fn packet_carries_applicable_docs() {
        let repo = git_repo();
        crate::store::issues::mutate(repo.path(), |mut records| {
            records.push(json!({
                "schema": 3, "id": "TK-a3f9", "kind": "ticket", "title": "t",
                "status": "draft", "revision": 1,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "role": "implementation",
                "context": {"anchors": ["src/auth/refresh.rs"]},
            }));
            Ok(records)
        })
        .unwrap();
        std::fs::create_dir_all(repo.path().join("docs")).unwrap();
        std::fs::write(
            repo.path().join("docs/auth.md"),
            "---\napplies_to: [\"src/auth/**\"]\n---\n# Auth\n",
        )
        .unwrap();
        std::fs::write(repo.path().join("docs/unrelated.md"), "# Unrelated\n").unwrap();

        let packet = build_packet(repo.path(), "TK-a3f9").unwrap();
        let applicable = packet["docs"]["applicable"].as_array().unwrap();
        assert_eq!(applicable.len(), 1);
        assert_eq!(applicable[0]["path"], "docs/auth.md");
        assert_eq!(packet["docs"]["map"], "docs/README.md");
    }

    #[test]
    fn packet_carries_open_findings_from_the_last_lane_verdict() {
        let repo = git_repo();
        let receipt = crate::evidence::receipt::record_receipt(
            repo.path(),
            None,
            crate::evidence::receipt::NewReceipt {
                kind: "lane".to_string(),
                subject: crate::evidence::receipt::ReceiptSubject {
                    id: "TK-a3f9".to_string(),
                    revision: None,
                },
                actor: "agent:review-correctness".to_string(),
                source: crate::evidence::receipt::ReceiptSource {
                    commit: "c".to_string(),
                    dirty_hash: "sha256:0".to_string(),
                },
                run_id: None,
                payload: json!({
                    "role": "review-correctness",
                    "verdict": "fail",
                    "findings": [
                        {"id": "F-1", "ref": "src/a.rs:10", "summary": "broken", "owner": "agent:worker",
                         "check": {"argv": ["true"], "exit": 0}, "severity": "high", "status": "open"},
                        {"id": "F-2", "ref": "src/b.rs:1", "summary": "old", "owner": "agent:worker",
                         "severity": "low", "status": "resolved"},
                        // Decision 0027 C3: a panel round-2 downgrade is not
                        // rework; the worker must not be handed it back.
                        {"id": "F-3", "ref": "src/c.rs:1", "summary": "maybe", "owner": "agent:worker",
                         "severity": "low", "status": "unconfirmed"},
                    ],
                    "acceptance": [
                        {"id": "AC-1", "status": "fail", "how": "no"},
                        {"id": "AC-2", "status": "pass", "how": "yes"},
                    ],
                    "cases": [
                        {"id": "QA-1", "status": "inconclusive"},
                        {"id": "QA-2", "status": "pass"},
                    ],
                }),
                artifact_paths: Vec::new(),
            },
        )
        .unwrap();
        crate::store::issues::mutate(repo.path(), |mut records| {
            records.push(json!({
                "schema": 3, "id": "TK-a3f9", "kind": "ticket", "title": "t",
                "status": "active", "revision": 2,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "role": "implementation",
                "verdicts": {
                    "review-correctness": {"receipt": receipt.id, "verdict": "fail", "commit": "c"}
                },
            }));
            Ok(records)
        })
        .unwrap();

        let packet = build_packet(repo.path(), "TK-a3f9").unwrap();
        let verdicts = packet["last_verdicts"].as_array().unwrap();
        assert_eq!(verdicts.len(), 1);
        assert_eq!(verdicts[0]["lane"], "review-correctness");
        assert_eq!(verdicts[0]["verdict"], "fail");
        assert_eq!(verdicts[0]["commit"], "c");
        let findings = verdicts[0]["findings"].as_array().unwrap();
        assert_eq!(
            findings.len(),
            1,
            "resolved and unconfirmed findings must be dropped"
        );
        assert_eq!(findings[0]["id"], "F-1");
        assert_eq!(findings[0]["ref"], "src/a.rs:10");
        assert_eq!(findings[0]["severity"], "high");
        assert_eq!(findings[0]["check"]["argv"][0], "true");
        assert_eq!(
            verdicts[0]["failed_acceptance"].as_array().unwrap().len(),
            1
        );
        assert_eq!(verdicts[0]["failed_acceptance"][0]["id"], "AC-1");
        assert_eq!(verdicts[0]["failed_cases"].as_array().unwrap().len(), 1);
        assert_eq!(verdicts[0]["failed_cases"][0]["id"], "QA-1");
        assert!(verdicts[0].get("findings_unavailable").is_none());
    }

    #[test]
    fn packet_marks_a_verdict_whose_receipt_is_missing() {
        let repo = git_repo();
        crate::store::issues::mutate(repo.path(), |mut records| {
            records.push(json!({
                "schema": 3, "id": "TK-a3f9", "kind": "ticket", "title": "t",
                "status": "active", "revision": 2,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "role": "implementation",
                "verdicts": {
                    "review-correctness": {"receipt": "01JNOPE00000000000000000000", "verdict": "fail", "commit": "c"}
                },
            }));
            Ok(records)
        })
        .unwrap();
        let packet = build_packet(repo.path(), "TK-a3f9").unwrap();
        let verdicts = packet["last_verdicts"].as_array().unwrap();
        assert_eq!(verdicts.len(), 1);
        assert_eq!(verdicts[0]["findings_unavailable"], true);
    }

    #[test]
    fn missing_ticket_is_reported() {
        let repo = git_repo();
        let err = build_packet(repo.path(), "TK-ffff").unwrap_err();
        assert_eq!(err.code(), "issue_not_found");
    }
}
