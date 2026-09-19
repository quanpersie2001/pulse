//! `pulse note` / `pulse events tail` integration tests (plan 0022 §6, §4.3).
//!
//! Rewritten for plan 0022 P1.3/P1.4: `note` is now `pulse note <id> <text>
//! [--friction] [--from actor]` against `issues.jsonl` (no more `work
//! create`/`sync`/authority grants/`JsonGraphStore`). The event-log
//! mechanics this file also covers (one line per day, torn-tail handling,
//! since-cursor across a day boundary) are untouched by the plan and kept
//! close to their v2 shape. `pulse events compact` (the legacy
//! one-file-per-event layout converter) is gone with the v2 repositories
//! that could ever have produced that layout (F3, P1.12).

use std::fs;
use std::process::Output;

use serde_json::Value;

#[allow(dead_code)]
#[path = "common/bin.rs"]
mod common_bin;
#[allow(dead_code)]
#[path = "common/fixture_repo.rs"]
mod common_fixture_repo;
#[allow(dead_code)]
#[path = "common/git.rs"]
mod common_git;

use crate::common_fixture_repo::TestRepo;

const ACTOR: &str = "human:tester";

fn setup_ticket(repo: &TestRepo) -> String {
    repo.pulse_ok(&["init", "--json"]);
    let created = repo.pulse_ok(&[
        "work",
        "new",
        "ticket",
        "Classify token failures",
        "--risk",
        "low",
        "--surface",
        "cli",
        "--actor",
        ACTOR,
        "--json",
    ]);
    created["id"].as_str().unwrap().to_string()
}

fn error_code(output: &Output) -> String {
    assert!(!output.status.success());
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    err["code"].as_str().unwrap().to_string()
}

#[test]
fn note_is_recorded_and_surfaces_in_tail() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ticket(&repo);

    repo.pulse_ok(&[
        "note",
        &ticket_id,
        "API A changed the token error mapping",
        "--from",
        ACTOR,
        "--json",
    ]);
    repo.pulse_ok(&[
        "note",
        &ticket_id,
        "Understood; adapting the client",
        "--from",
        ACTOR,
        "--json",
    ]);

    let out = repo.pulse_ok(&["events", "tail", "--id", &ticket_id, "--json"]);
    let events: Vec<Value> = serde_json::from_value(out).unwrap();
    let notes: Vec<&Value> = events
        .iter()
        .filter(|event| event["event_type"] == "note.recorded")
        .collect();
    assert_eq!(notes.len(), 2);
    assert_eq!(
        notes[0]["payload"]["text"],
        "API A changed the token error mapping"
    );
}

#[test]
fn tail_since_cursor_excludes_older_events() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ticket(&repo);
    repo.pulse_ok(&["note", &ticket_id, "first", "--from", ACTOR, "--json"]);
    let first: Vec<Value> =
        serde_json::from_value(repo.pulse_ok(&["events", "tail", "--id", &ticket_id, "--json"]))
            .unwrap();
    let first_note_id = first
        .iter()
        .find(|event| event["event_type"] == "note.recorded")
        .and_then(|event| event["id"].as_str())
        .unwrap()
        .to_string();

    repo.pulse_ok(&["note", &ticket_id, "second", "--from", ACTOR, "--json"]);

    let out = repo.pulse_ok(&[
        "events",
        "tail",
        "--id",
        &ticket_id,
        "--since",
        &first_note_id,
        "--json",
    ]);
    let events: Vec<Value> = serde_json::from_value(out).unwrap();
    assert!(!events
        .iter()
        .any(|event| event["payload"]["text"] == "first"));
    let notes: Vec<&Value> = events
        .iter()
        .filter(|event| event["event_type"] == "note.recorded")
        .collect();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0]["payload"]["text"], "second");
    assert!(notes[0]["id"].as_str().unwrap() > first_note_id.as_str());
}

#[test]
fn tail_human_output_is_bounded_lines() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ticket(&repo);
    repo.pulse_ok(&[
        "note",
        &ticket_id,
        "hello from the operator",
        "--from",
        ACTOR,
        "--json",
    ]);
    let output = repo.pulse(&["events", "tail", "--id", &ticket_id]);
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text
        .lines()
        .find(|line| line.contains("note.recorded"))
        .expect("human tail prints one line per event");
    assert!(line.contains("hello from the operator"));
}

#[test]
fn notes_are_bounded_to_the_latest_fifty_on_the_record() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ticket(&repo);
    for index in 0..52 {
        repo.pulse_ok(&[
            "note",
            &ticket_id,
            &format!("note {index:02}"),
            "--from",
            ACTOR,
            "--json",
        ]);
    }
    let shown = repo.pulse_ok(&["work", "show", &ticket_id, "--json"]);
    let notes = shown["notes"].as_array().unwrap();
    assert_eq!(notes.len(), 50, "plan 0022 §4.3 keeps at most 50 notes");
    assert_eq!(
        notes[0]["text"], "note 02",
        "oldest notes are dropped first"
    );
    assert_eq!(notes[49]["text"], "note 51");
}

#[test]
fn note_on_a_nonexistent_id_is_refused() {
    let repo = TestRepo::from_fixture("minimal-service");
    repo.pulse_ok(&["init", "--json"]);
    let output = repo.pulse(&["note", "TK-ffff", "hi", "--from", ACTOR]);
    assert_eq!(error_code(&output), "issue_not_found");
}

/// Plan 0022 §6.2: every actor kind may add a note — human, `agent:worker*`
/// and `agent:review-*`/`agent:qa-*` alike. No grant/policy check exists.
#[test]
fn any_actor_kind_may_record_a_note() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ticket(&repo);
    for actor in ["human:tester", "agent:worker", "agent:review-correctness"] {
        let noted = repo.pulse_ok(&[
            "note",
            &ticket_id,
            "note from an actor",
            "--from",
            actor,
            "--json",
        ]);
        assert_eq!(
            noted["notes"].as_array().unwrap().last().unwrap()["from"],
            actor
        );
    }
}

/// Plan 0022 §6, note table row: `--friction` marks a note as friction; the
/// default kind is a plain note.
#[test]
fn note_kind_defaults_to_note_and_records_friction_when_asked() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ticket(&repo);

    let plain = repo.pulse_ok(&[
        "note",
        &ticket_id,
        "Reviewer picked up the contract change",
        "--from",
        ACTOR,
        "--json",
    ]);
    assert_eq!(
        plain["notes"].as_array().unwrap().last().unwrap()["kind"],
        "note"
    );

    let friction = repo.pulse_ok(&[
        "note",
        &ticket_id,
        "pulse work packet needed three tries to name the story",
        "--friction",
        "--from",
        ACTOR,
        "--json",
    ]);
    assert_eq!(
        friction["notes"].as_array().unwrap().last().unwrap()["kind"],
        "friction"
    );

    let tail = repo.pulse_ok(&["events", "tail", "--id", &ticket_id, "--json"]);
    let kinds: Vec<&Value> = tail
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["event_type"] == "note.recorded")
        .map(|event| &event["payload"]["kind"])
        .collect();
    assert_eq!(kinds, vec!["note", "friction"]);
}

// ---------------------------------------------------------------------------
// Decision 0011: one JSONL file per day (unchanged by plan 0022)
// ---------------------------------------------------------------------------

#[test]
fn events_are_appended_as_one_line_per_day_file() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ticket(&repo);
    for message in ["first note", "second note", "third note"] {
        repo.pulse_ok(&["note", &ticket_id, message, "--from", ACTOR, "--json"]);
    }

    let root = repo.path().join(".pulse/events");
    let mut day_files = Vec::new();
    for entry in fs::read_dir(&root).unwrap() {
        let path = entry.unwrap().path();
        assert!(
            !path.is_dir(),
            "no day directory may remain: {}",
            path.display()
        );
        assert_eq!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("jsonl"),
            "unexpected file in the event log: {}",
            path.display()
        );
        day_files.push(path);
    }
    assert_eq!(day_files.len(), 1, "one day, one file");

    let bytes = fs::read(&day_files[0]).unwrap();
    assert_eq!(*bytes.last().unwrap(), b'\n', "the file ends on a record");
    let lines: Vec<&[u8]> = bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .collect();
    assert!(lines.len() >= 3, "every event is one line");

    let mut ids = Vec::new();
    for line in &lines {
        let event: Value = serde_json::from_slice(line).expect("each line parses alone");
        assert!(!line.contains(&b'\n'));
        ids.push(event["id"].as_str().unwrap().to_string());
    }
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(
        ids, sorted,
        "line order is write order, which is ULID order"
    );
}

#[test]
fn a_torn_trailing_line_is_reported_then_truncated_by_the_next_append() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ticket(&repo);
    repo.pulse_ok(&[
        "note",
        &ticket_id,
        "before the crash",
        "--from",
        ACTOR,
        "--json",
    ]);

    let day_file = fs::read_dir(repo.path().join(".pulse/events"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("jsonl"))
        .expect("day file");
    let intact = fs::read(&day_file).unwrap();
    let complete_lines = intact.iter().filter(|byte| **byte == b'\n').count();

    let mut torn = intact.clone();
    torn.extend_from_slice(b"{\"schema_version\":1,\"id\":\"evt_tor");
    fs::write(&day_file, &torn).unwrap();

    let output = repo.pulse(&["events", "tail", "--json"]);
    assert!(
        output.status.success(),
        "a torn tail must not fail the read"
    );
    let events: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(events.len(), complete_lines);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("events_torn_tail"),
        "torn tail must be reported, got: {stderr}"
    );

    repo.pulse_ok(&[
        "note",
        &ticket_id,
        "after the crash",
        "--from",
        ACTOR,
        "--json",
    ]);
    let repaired = fs::read(&day_file).unwrap();
    assert!(!repaired.windows(8).any(|w| w == b"evt_tor{"));
    let output = repo.pulse(&["events", "tail", "--json"]);
    let events: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(events.len(), complete_lines + 1);
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("events_torn_tail"),
        "the torn line is gone once a writer has been through"
    );
    let messages: Vec<&str> = events
        .iter()
        .filter(|event| event["event_type"] == "note.recorded")
        .map(|event| event["payload"]["text"].as_str().unwrap())
        .collect();
    assert_eq!(messages, vec!["before the crash", "after the crash"]);
}

#[test]
fn the_since_cursor_crosses_a_day_boundary() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ticket(&repo);
    repo.pulse_ok(&["note", &ticket_id, "day one", "--from", ACTOR, "--json"]);

    let events_dir = repo.path().join(".pulse/events");
    let today = fs::read_dir(&events_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("jsonl"))
        .expect("day file");

    let yesterday = events_dir.join("2000-01-01.jsonl");
    fs::rename(&today, &yesterday).unwrap();
    repo.pulse_ok(&["note", &ticket_id, "day two", "--from", ACTOR, "--json"]);
    assert!(yesterday.exists() && today.exists(), "two day files");

    let all: Vec<Value> =
        serde_json::from_value(repo.pulse_ok(&["events", "tail", "--json"])).unwrap();
    let cursor = all
        .iter()
        .find(|event| event["payload"]["text"] == "day one")
        .map(|event| event["id"].as_str().unwrap().to_string())
        .expect("day one event");

    let after: Vec<Value> =
        serde_json::from_value(repo.pulse_ok(&["events", "tail", "--since", &cursor, "--json"]))
            .unwrap();
    assert!(
        after
            .iter()
            .all(|event| event["id"].as_str().unwrap() > cursor.as_str()),
        "the cursor is exclusive across files"
    );
    let messages: Vec<&str> = after
        .iter()
        .filter(|event| event["event_type"] == "note.recorded")
        .map(|event| event["payload"]["text"].as_str().unwrap())
        .collect();
    assert_eq!(messages, vec!["day two"]);
}

// ---------------------------------------------------------------------------
// Plan 0025 F3: the reverse doc gate — handoff advises, `docs check
// --ticket` reports, nothing blocks.
// ---------------------------------------------------------------------------

/// A target repo with `docs/api.md` (frontmatter `applies_to: ["api/**"]`)
/// and `api/x.py` in the committed baseline, plus a claimed low-risk ticket
/// with the given `touches`, whose edits (api code, plus the doc when
/// `edit_doc_too`) are verified and handed off. Returns the repo, the
/// ticket id and the handoff's JSON output.
fn setup_stale_docs_repo(touches: &[&str], edit_doc_too: bool) -> (TestRepo, String, Value) {
    let repo = TestRepo::from_fixture("minimal-service");
    // Payload files live outside the target repo: an untracked JSON in the
    // tree is exactly what `handoff_unreserved_changes` exists to refuse.
    let harness = tempfile::tempdir().unwrap();
    repo.pulse_ok(&["init", "--json"]);
    fs::create_dir_all(repo.path().join("docs")).unwrap();
    fs::write(
        repo.path().join("docs/api.md"),
        "---\napplies_to: [\"api/**\"]\n---\n# API\nEndpoints live under api/.\n",
    )
    .unwrap();
    fs::create_dir_all(repo.path().join("api")).unwrap();
    fs::write(repo.path().join("api/x.py"), "def handler(): ...\n").unwrap();
    common_git::commit_all(repo.path());

    let created = repo.pulse_ok(&[
        "work",
        "new",
        "ticket",
        "API work",
        "--risk",
        "low",
        "--surface",
        "cli",
        "--json",
    ]);
    let ticket_id = created["id"].as_str().unwrap().to_string();
    let ticket_json = harness.path().join("ticket-payload.json");
    fs::write(
        &ticket_json,
        serde_json::to_string(&serde_json::json!({
            "objective": "adjust the api handler",
            "context": {"anchors": ["api/x.py: the handler"]},
            "acceptance": [{"id": "AC-1", "when": "w", "then": "t"}],
            "verify": [{"name": "unit", "argv": ["true"]}],
            "touches": touches,
        }))
        .unwrap(),
    )
    .unwrap();
    repo.pulse_ok(&[
        "work",
        "update",
        &ticket_id,
        "--from",
        ticket_json.to_str().unwrap(),
        "--json",
    ]);
    repo.pulse_ok(&["work", "ready", &ticket_id, "--json"]);

    let claimed = repo.pulse_ok(&["claim", &ticket_id, "--actor", "agent:worker", "--json"]);
    let run_id = claimed["lease"]["run_id"].as_str().unwrap().to_string();

    // The ticket's edits: api code, plus the doc when the caller says so.
    fs::write(repo.path().join("api/x.py"), "def handler(): return 1\n").unwrap();
    if edit_doc_too {
        fs::write(
            repo.path().join("docs/api.md"),
            "---\napplies_to: [\"api/**\"]\n---\n# API\nEndpoints live under api/. The handler returns 1.\n",
        )
        .unwrap();
    }
    repo.pulse_ok(&["verify", &ticket_id, "--actor", "agent:worker", "--json"]);

    let handoff_json = harness.path().join("handoff-payload.json");
    fs::write(
        &handoff_json,
        serde_json::to_string(&serde_json::json!({
            "run_id": run_id,
            "summary": "done",
            "acceptance": [{"id": "AC-1", "status": "done", "how": "ran it"}],
        }))
        .unwrap(),
    )
    .unwrap();
    let handed = repo.pulse_ok(&[
        "handoff",
        &ticket_id,
        "--from",
        handoff_json.to_str().unwrap(),
        "--actor",
        "agent:worker",
        "--json",
    ]);
    (repo, ticket_id, handed)
}

#[test]
fn handoff_advises_and_docs_check_ticket_reports_when_code_moved_without_the_doc() {
    let (repo, ticket_id, handed) = setup_stale_docs_repo(&["api/**"], false);

    // The handoff output carries the advisory: the doc describing the
    // changed api code was not updated by the same hand.
    let staled = handed["docs_maybe_stale"].as_array().unwrap();
    assert_eq!(staled.len(), 1, "{handed}");
    assert_eq!(staled[0]["doc"], serde_json::json!("docs/api.md"));
    assert_eq!(staled[0]["pattern"], serde_json::json!("api/**"));
    assert_eq!(staled[0]["because"][0], serde_json::json!("api/x.py"));

    // The stored record itself stays clean — the advisory rides the CLI
    // payload, never the record (same tier as close's unclassified_friction).
    let stored = repo.pulse_ok(&["work", "show", &ticket_id, "--json"]);
    assert!(
        stored["docs_maybe_stale"].is_null(),
        "the record itself stays clean: {stored}"
    );

    let report = repo.pulse_ok(&["docs", "check", "--ticket", &ticket_id, "--json"]);
    assert_eq!(report["verdict"], "pass", "a low advisory must not fail");
    let findings = report["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1, "{report}");
    assert_eq!(findings[0]["severity"], "low");
    assert_eq!(findings[0]["status"], "open");
    assert_eq!(findings[0]["owner"], "docs/api.md");
    assert_eq!(findings[0]["ref"], "docs/api.md");
    let summary = findings[0]["summary"].as_str().unwrap();
    assert!(summary.contains("describes api/**"), "{summary}");
    assert!(summary.contains(&ticket_id), "{summary}");
    assert!(summary.contains("api/x.py"), "{summary}");

    // The event log carries the advisory from the handoff.
    let events: Vec<Value> =
        serde_json::from_value(repo.pulse_ok(&["events", "tail", "--id", &ticket_id, "--json"]))
            .unwrap();
    let advisory = events
        .iter()
        .find(|event| event["event_type"] == "docs.maybe_stale")
        .expect("the handoff emitted docs.maybe_stale");
    assert_eq!(
        advisory["payload"]["docs"][0]["doc"],
        serde_json::json!("docs/api.md")
    );
    assert_eq!(
        advisory["payload"]["docs"][0]["pattern"],
        serde_json::json!("api/**")
    );

    // Plain `docs check` stays advisory-free.
    let plain = repo.pulse_ok(&["docs", "check", "--json"]);
    assert_eq!(plain["verdict"], "pass");
    assert_eq!(plain["findings"].as_array().unwrap().len(), 0);

    // An unknown ticket is the usual not-found error.
    let missing = repo.pulse(&["docs", "check", "--ticket", "TK-zzzz", "--json"]);
    assert_eq!(error_code(&missing), "issue_not_found");
}

#[test]
fn a_doc_updated_alongside_its_code_is_never_advised() {
    // `touches` covers the doc too, so the worker may edit it — and did.
    let (repo, ticket_id, handed) = setup_stale_docs_repo(&["api/**", "docs/**"], true);

    assert!(
        handed["docs_maybe_stale"].is_null(),
        "no advisory when the doc moved with its code: {handed}"
    );

    let report = repo.pulse_ok(&["docs", "check", "--ticket", &ticket_id, "--json"]);
    assert_eq!(report["verdict"], "pass");
    assert_eq!(report["findings"].as_array().unwrap().len(), 0);

    let events: Vec<Value> =
        serde_json::from_value(repo.pulse_ok(&["events", "tail", "--id", &ticket_id, "--json"]))
            .unwrap();
    assert!(
        !events
            .iter()
            .any(|event| event["event_type"] == "docs.maybe_stale"),
        "no advisory when the doc moved with its code"
    );
}
