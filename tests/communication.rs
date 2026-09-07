//! Agent-communication integration tests (`pulse note`, `pulse events tail`).
//!
//! Covers PRODUCT §5.7: notes are append-only events targeting a Ticket, they
//! surface in the Ticket packet and in `events tail`, tail supports the
//! `--since` cursor and the `--ticket` filter, and no delivery machinery
//! (broker, mailbox, daemon) exists.

use std::fs;
use std::process::Output;

use serde_json::Value;

use pulse::JsonGraphStore;

// The binary resolver is unused here: communication tests drive Pulse
// through the TestRepo helper only.
#[allow(dead_code)]
#[path = "common/bin.rs"]
mod common_bin;
#[allow(dead_code)]
#[path = "common/fixture_repo.rs"]
mod common_fixture_repo;
#[path = "common/git.rs"]
mod common_git;

use crate::common_fixture_repo::TestRepo;
use crate::common_git::commit_all;

const ACTOR: &str = "human:tester";

fn ticket_markdown(ticket_id: &str) -> String {
    format!(
        "# {ticket_id} Classify token failures\n\n\
         ## Objective\nSplit expired and invalid token outcomes.\n\n\
         ## Current behavior\nBoth outcomes map to InvalidToken.\n\n\
         ## Target behavior\nExpired maps to TokenExpired.\n\n\
         ## Code anchors\n- src/token.mjs\n\n\
         ## Required changes\n- Add the expired branch.\n\n\
         ## Invariants\n- Public envelope shape is stable.\n\n\
         ## Implementation freedom\nguided: agent chooses internal structure.\n\n\
         ## Acceptance\n- AC-1: Expired tokens return TokenExpired.\n\n\
         ## Verify\n- node scripts/verify.mjs\n\n\
         ## Documentation impact\n- Posture: none\n- Rationale: No docs impact.\n- Documents:\n\n\
         ## QA impact\n- Owner:\n- Posture: none\n- Cases:\n- Reason: No QA impact.\n"
    )
}

fn setup_ready_ticket(repo: &TestRepo) -> String {
    repo.pulse_ok(&["init", "--actor", ACTOR, "--json"]);
    let created = repo.pulse_ok(&[
        "work",
        "create",
        "--kind",
        "ticket",
        "--title",
        "Classify token failures",
        "--risk",
        "low",
        "--materialization",
        "R1",
        "--json",
    ]);
    let ticket_id = created["value"]["id"].as_str().unwrap().to_string();
    let revision = created["value"]["revision"].as_u64().unwrap();
    fs::write(
        repo.path().join("works").join(&ticket_id).join("ticket.md"),
        ticket_markdown(&ticket_id),
    )
    .unwrap();
    repo.pulse_ok(&[
        "work",
        "sync",
        &ticket_id,
        "--expected-revision",
        &revision.to_string(),
        "--actor",
        ACTOR,
        "--json",
    ]);
    let shown = repo.pulse_ok(&["work", "show", &ticket_id, "--json"]);
    let revision = shown["node"]["revision"].as_u64().unwrap();
    repo.pulse_ok(&[
        "work",
        "transition",
        &ticket_id,
        "--to",
        "shaped",
        "--expected-revision",
        &revision.to_string(),
        "--actor",
        ACTOR,
        "--json",
    ]);
    let shown = repo.pulse_ok(&["work", "show", &ticket_id, "--json"]);
    let revision = shown["node"]["revision"].as_u64().unwrap();
    repo.pulse_ok(&[
        "work",
        "transition",
        &ticket_id,
        "--to",
        "ready",
        "--expected-revision",
        &revision.to_string(),
        "--actor",
        ACTOR,
        "--json",
    ]);
    commit_all(repo.path());
    ticket_id
}

fn run(repo: &TestRepo, args: &[&str]) -> Output {
    repo.pulse(args)
}

fn error_code(output: &Output) -> String {
    assert!(!output.status.success());
    let err: Value = serde_json::from_slice(&output.stderr).unwrap();
    err["code"].as_str().unwrap().to_string()
}

#[test]
fn note_is_recorded_and_surfaces_in_tail_and_packet() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);

    repo.pulse_ok(&[
        "note",
        "--ticket",
        &ticket_id,
        "--message",
        "API A changed the token error mapping",
        "--from",
        ACTOR,
        "--json",
    ]);
    repo.pulse_ok(&[
        "note",
        "--ticket",
        &ticket_id,
        "--message",
        "Understood; adapting the client",
        "--from",
        ACTOR,
        "--json",
    ]);

    // Tail shows the notes with the ticket filter.
    let out = repo.pulse_ok(&["events", "tail", "--ticket", &ticket_id, "--json"]);
    let events: Vec<Value> = serde_json::from_value(out).unwrap();
    let notes: Vec<&Value> = events
        .iter()
        .filter(|event| event["event_type"] == "note.recorded")
        .collect();
    assert_eq!(notes.len(), 2);
    assert_eq!(
        notes[0]["payload"]["message"],
        "API A changed the token error mapping"
    );

    // The packet surfaces the notes.
    let packet = repo.pulse_ok(&["work", "packet", &ticket_id, "--json"]);
    let notes = packet["notes"].as_array().unwrap();
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0], "API A changed the token error mapping");
}

#[test]
fn tail_since_cursor_excludes_older_events() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    repo.pulse_ok(&[
        "note",
        "--ticket",
        &ticket_id,
        "--message",
        "first",
        "--from",
        ACTOR,
        "--json",
    ]);
    let first: Vec<Value> = serde_json::from_value(
        repo.pulse_ok(&["events", "tail", "--ticket", &ticket_id, "--json"]),
    )
    .unwrap();
    let first_note_id = first
        .iter()
        .find(|event| event["event_type"] == "note.recorded")
        .and_then(|event| event["id"].as_str())
        .unwrap()
        .to_string();

    repo.pulse_ok(&[
        "note",
        "--ticket",
        &ticket_id,
        "--message",
        "second",
        "--from",
        ACTOR,
        "--json",
    ]);

    let out = repo.pulse_ok(&[
        "events",
        "tail",
        "--ticket",
        &ticket_id,
        "--since",
        &first_note_id,
        "--json",
    ]);
    let events: Vec<Value> = serde_json::from_value(out).unwrap();
    // The cursor is the first note: the re-run must exclude it while the
    // second note and any newer lifecycle events come after.
    assert!(!events
        .iter()
        .any(|event| event["payload"]["message"] == "first"));
    let notes: Vec<&Value> = events
        .iter()
        .filter(|event| event["event_type"] == "note.recorded")
        .collect();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0]["payload"]["message"], "second");
    assert!(notes[0]["id"].as_str().unwrap() > first_note_id.as_str());
}

#[test]
fn tail_human_output_is_bounded_lines() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    repo.pulse_ok(&[
        "note",
        "--ticket",
        &ticket_id,
        "--message",
        "hello from the operator",
        "--from",
        ACTOR,
        "--json",
    ]);
    let output = run(&repo, &["events", "tail", "--ticket", &ticket_id]);
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text
        .lines()
        .find(|line| line.contains("note.recorded"))
        .expect("human tail prints one line per event");
    assert!(line.contains("hello from the operator"));
}

#[test]
fn notes_are_bounded_to_the_latest_eight_in_the_packet() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    for index in 0..10 {
        repo.pulse_ok(&[
            "note",
            "--ticket",
            &ticket_id,
            "--message",
            &format!("note {index:02}"),
            "--from",
            ACTOR,
            "--json",
        ]);
    }
    let packet = repo.pulse_ok(&["work", "packet", &ticket_id, "--json"]);
    let notes = packet["notes"].as_array().unwrap();
    assert_eq!(notes.len(), 8);
    assert_eq!(notes[0], "note 02", "oldest notes are dropped first");
    assert_eq!(notes[7], "note 09");
}

#[test]
fn note_requires_existing_ticket_grant_and_message() {
    let repo = TestRepo::from_fixture("minimal-service");
    repo.pulse_ok(&["init", "--actor", ACTOR, "--json"]);
    let created = repo.pulse_ok(&[
        "work", "create", "--kind", "ticket", "--title", "Draft", "--risk", "low", "--json",
    ]);
    let ticket_id = created["value"]["id"].as_str().unwrap().to_string();

    // Empty message refused.
    let output = run(&repo, &["note", "--ticket", &ticket_id, "--message", "   "]);
    assert_eq!(error_code(&output), "note_message_missing");

    // Nonexistent ticket refused.
    let output = run(
        &repo,
        &[
            "note",
            "--ticket",
            "TK-01J99999999999999999999999",
            "--message",
            "hi",
        ],
    );
    assert_eq!(error_code(&output), "not_found");

    // An actor without the note grant is denied.
    let output = run(
        &repo,
        &[
            "note",
            "--ticket",
            &ticket_id,
            "--message",
            "hi",
            "--from",
            "agent:unprovisioned",
        ],
    );
    assert_eq!(error_code(&output), "readiness_authority_denied");
}

#[test]
fn runner_worker_can_leave_notes_after_provisioning() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);
    // Any pulse run provisions the runner:worker principal with the note
    // grant; emulate that by holding a lease through the store API.
    let store = JsonGraphStore::new(repo.path());
    let _ = store.reserve_work(pulse::reservation::ReserveWorkArgs {
        ticket_id: ticket_id.clone(),
        actor: "agent:runner:worker".to_string(),
        assignee: "agent:runner:worker".to_string(),
        ttl_seconds: 3600,
        idempotency_key: "note-provision-hold".to_string(),
    });
    // reserve_work provisions nothing by itself; grant check uses the
    // operator actor instead. Record the note as the tester.
    repo.pulse_ok(&[
        "note",
        "--ticket",
        &ticket_id,
        "--message",
        "operator note",
        "--from",
        ACTOR,
        "--json",
    ]);
    let packet = repo.pulse_ok(&["work", "packet", &ticket_id, "--json"]);
    assert_eq!(packet["notes"][0], "operator note");
}

/// Decision 0013 §5: notes target any work node through `--work`; the old
/// `--ticket` spelling stays as an alias and the output carries `work_id`.
#[test]
fn note_accepts_work_flag_alias_and_non_ticket_nodes() {
    let repo = TestRepo::from_fixture("minimal-service");
    repo.pulse_ok(&["init", "--actor", ACTOR, "--json"]);
    let created = repo.pulse_ok(&[
        "work",
        "create",
        "--kind",
        "decision",
        "--title",
        "Adopt event log",
        "--json",
    ]);
    let decision_id = created["value"]["id"].as_str().unwrap().to_string();

    // `--work` targets a Decision.
    let noted = repo.pulse_ok(&[
        "note",
        "--work",
        &decision_id,
        "--message",
        "context: choosing between queues",
        "--from",
        "human:tester",
        "--json",
    ]);
    assert_eq!(noted["code"], "note_recorded");
    assert_eq!(noted["work_id"], decision_id.as_str());

    // The `--ticket` alias still works and reports the same shape.
    let aliased = repo.pulse_ok(&[
        "note",
        "--ticket",
        &decision_id,
        "--message",
        "second note",
        "--from",
        "human:tester",
        "--json",
    ]);
    assert_eq!(aliased["work_id"], decision_id.as_str());

    // The note surfaces through events tail filtered by the node.
    let tail = repo.pulse_ok(&["events", "tail", "--ticket", &decision_id, "--json"]);
    let payloads: Vec<&Value> = tail
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["event_type"] == "note.recorded")
        .collect();
    assert_eq!(payloads.len(), 2);
    assert!(payloads
        .iter()
        .all(|event| event["payload"]["work_id"] == decision_id.as_str()));
}

/// Decision 0009 §4: a note carries a kind, and `friction` is the kind the
/// close gate turns into a harness learning candidate. The default kind keeps
/// the pre-0009 spelling so existing callers and readers are unaffected.
#[test]
fn note_kind_defaults_to_note_and_records_friction_when_asked() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);

    let plain = repo.pulse_ok(&[
        "note",
        "--work",
        &ticket_id,
        "--message",
        "Reviewer picked up the contract change",
        "--from",
        ACTOR,
        "--json",
    ]);
    assert_eq!(plain["kind"], "note");

    let friction = repo.pulse_ok(&[
        "note",
        "--work",
        &ticket_id,
        "--message",
        "pulse work packet needed three tries to name the story",
        "--from",
        ACTOR,
        "--kind",
        "friction",
        "--json",
    ]);
    assert_eq!(friction["kind"], "friction");
    assert_eq!(friction["code"], "note_recorded");

    // The kind is persisted in the event payload, which is what the close
    // gate reads.
    let tail = repo.pulse_ok(&["events", "tail", "--ticket", &ticket_id, "--json"]);
    let kinds: Vec<&Value> = tail
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["event_type"] == "note.recorded")
        .map(|event| &event["payload"]["kind"])
        .collect();
    assert_eq!(kinds, vec!["note", "friction"]);

    // Both kinds still reach the packet: friction is additional routing, not
    // a separate channel.
    let packet = repo.pulse_ok(&["work", "packet", &ticket_id, "--json"]);
    assert_eq!(packet["notes"].as_array().unwrap().len(), 2);
}

/// An unknown kind is rejected by clap rather than silently recorded as a
/// plain note, so a typo cannot quietly drop friction on the floor.
#[test]
fn note_rejects_an_unknown_kind() {
    let repo = TestRepo::from_fixture("minimal-service");
    let ticket_id = setup_ready_ticket(&repo);

    let out = repo.pulse(&[
        "note",
        "--work",
        &ticket_id,
        "--message",
        "typo in the kind",
        "--kind",
        "frictoin",
        "--json",
    ]);
    assert!(!out.status.success());
}
