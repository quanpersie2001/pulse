//! `pulse::kernel::doctor` tests (plan 0022 §11.3 minimum + store errors).
//!
//! Doctor is a lenient reader over state the strict APIs refuse: a torn
//! store line, an unreadable receipt, an expired lease, an orphan evidence
//! dir, and the §10.4 context-threshold detector's three states. Each test
//! builds the minimal `.pulse/` tree by hand — no `pulse init`, so a store
//! layout change shows up here on purpose.

use std::fs;
use std::path::Path;

use pulse::event::emit_event;
use pulse::evidence::receipt::{record_receipt, NewReceipt, ReceiptSource, ReceiptSubject};
use pulse::identity::actor::{ActorKind, ActorRef};
use pulse::kernel::doctor::{self, DetectorStatus};

fn actor(id: &str) -> ActorRef {
    ActorRef {
        kind: ActorKind::Agent,
        id: id.to_string(),
    }
}

fn write_repo(path: &Path, store_lines: &[String]) {
    fs::create_dir_all(path.join(".pulse/receipts")).unwrap();
    fs::create_dir_all(path.join(".pulse/events")).unwrap();
    fs::create_dir_all(path.join(".pulse/hosts/claude-code")).unwrap();
    fs::create_dir_all(path.join(".pulse/runtime")).unwrap();
    fs::write(
        path.join(".pulse/issues.jsonl"),
        store_lines.join("\n") + "\n",
    )
    .unwrap();
    // The §10.4 host hooks: present by default so the detector warning
    // arms (a repo without them has no detector to warn about).
    fs::write(
        path.join(".pulse/hosts/claude-code/statusline.sh"),
        "#!/bin/sh\n",
    )
    .unwrap();
    fs::write(
        path.join(".pulse/hosts/claude-code/post-tool-use.sh"),
        "#!/bin/sh\n",
    )
    .unwrap();
}

fn ticket(id: &str, status: &str) -> String {
    format!(
        r#"{{"schema":3,"id":"{id}","kind":"ticket","title":"t","status":"{status}","role":"implementation","created_at":"2026-09-17T00:00:00Z","updated_at":"2026-09-17T00:00:00Z","revision":1}}"#
    )
}

#[test]
fn a_clean_repo_reports_only_the_unexercised_detector() {
    let repo = tempfile::tempdir().unwrap();
    write_repo(repo.path(), &[ticket("TK-0001", "ready")]);

    let report = doctor::run(repo.path()).unwrap();
    assert!(report.store_torn_lines.is_empty());
    assert!(report.unreadable_receipts.is_empty());
    assert!(report.expired_leases.is_empty());
    assert!(report.orphan_evidence.is_empty());
    assert!(report.host_hooks_installed);
    assert_eq!(
        report.detector_context_threshold,
        DetectorStatus::Unexercised,
        "no continue round-trip and no marker: the ST-1 `unexercised` warning"
    );
    assert_eq!(report.warning_count(), 1);
}

#[test]
fn a_torn_store_line_is_counted_and_the_checks_continue() {
    let repo = tempfile::tempdir().unwrap();
    write_repo(
        repo.path(),
        &[
            ticket("TK-0001", "active"),
            "{\"id\":\"TK-broken\".to_string()}".to_string(), // not JSON
            ticket("TK-0002", "ready"),
        ],
    );
    // The surviving active record carries an expired lease — the doctor
    // must see past the torn line to report it.
    fs::write(
        repo.path().join(".pulse/issues.jsonl"),
        format!(
            "{}\n{}\n{}\n",
            ticket("TK-0001", "active"),
            "{\"id\": broken json",
            r#"{"schema":3,"id":"TK-0003","kind":"ticket","title":"t","status":"active","role":"implementation","created_at":"2026-09-17T00:00:00Z","updated_at":"2026-09-17T00:00:00Z","revision":1,"lease":{"role":"worker","actor":"agent:worker","run_id":"run_x","expires_at":"2026-09-16T00:00:00Z"}}"#
        ),
    )
    .unwrap();

    let report = doctor::run(repo.path()).unwrap();
    assert_eq!(report.store_torn_lines, vec![2]);
    assert_eq!(report.expired_leases.len(), 1);
    assert_eq!(report.expired_leases[0].id, "TK-0003");
    assert_eq!(
        report.expired_leases[0].actor.as_deref(),
        Some("agent:worker")
    );
}

#[test]
fn an_unreadable_receipt_is_reported_not_hidden() {
    let repo = tempfile::tempdir().unwrap();
    write_repo(repo.path(), &[ticket("TK-0001", "ready")]);
    fs::write(
        repo.path().join(".pulse/receipts/2026-09.jsonl"),
        "{\"id\":\"rc_broken\",\n",
    )
    .unwrap();

    let report = doctor::run(repo.path()).unwrap();
    assert_eq!(report.unreadable_receipts.len(), 1);
    assert!(
        report.unreadable_receipts[0].contains("2026-09.jsonl"),
        "the path names the broken file: {:?}",
        report.unreadable_receipts
    );
}

#[test]
fn a_live_lease_or_non_active_ticket_is_never_reported_expired() {
    let repo = tempfile::tempdir().unwrap();
    let live = r#"{"schema":3,"id":"TK-00a1","kind":"ticket","title":"t","status":"active","role":"implementation","created_at":"2026-09-17T00:00:00Z","updated_at":"2026-09-17T00:00:00Z","revision":1,"lease":{"role":"worker","actor":"agent:worker","run_id":"run_y","expires_at":"2099-01-01T00:00:00Z"}}"#;
    let verifying = r#"{"schema":3,"id":"TK-00a2","kind":"ticket","title":"t","status":"verifying","role":"implementation","created_at":"2026-09-17T00:00:00Z","updated_at":"2026-09-17T00:00:00Z","revision":1,"lease":{"role":"worker","actor":"agent:worker","run_id":"run_z","expires_at":"2026-09-16T00:00:00Z"}}"#;
    write_repo(
        repo.path(),
        &[
            ticket("TK-0001", "ready"),
            live.to_string(),
            verifying.to_string(),
        ],
    );

    let report = doctor::run(repo.path()).unwrap();
    assert!(
        report.expired_leases.is_empty(),
        "{:?}",
        report.expired_leases
    );
    assert_eq!(report.warning_count(), 1); // detector only
}

#[test]
fn an_evidence_dir_no_receipt_names_is_orphan() {
    let repo = tempfile::tempdir().unwrap();
    write_repo(repo.path(), &[ticket("TK-0e57", "verifying")]);
    fs::create_dir_all(repo.path().join(".pulse/evidence/TK-0e57")).unwrap();
    fs::create_dir_all(repo.path().join(".pulse/evidence/TK-0170/shots")).unwrap();
    record_receipt(
        repo.path(),
        None,
        NewReceipt {
            kind: "handoff".to_string(),
            subject: ReceiptSubject {
                id: "TK-0e57".to_string(),
                revision: None,
            },
            actor: "agent:worker".to_string(),
            source: ReceiptSource {
                commit: "c".to_string(),
                dirty_hash: "d".to_string(),
            },
            run_id: None,
            payload: serde_json::json!({}),
            artifact_paths: vec![],
        },
    )
    .unwrap();

    let report = doctor::run(repo.path()).unwrap();
    assert_eq!(report.orphan_evidence, vec!["TK-0170".to_string()]);
}

#[test]
fn the_detector_states_are_marker_continue_and_unexercised() {
    let repo = tempfile::tempdir().unwrap();
    write_repo(repo.path(), &[ticket("TK-0001", "ready")]);

    // Fired but unconsumed: the marker is on disk.
    fs::write(repo.path().join(".pulse/runtime/context-threshold"), "").unwrap();
    let report = doctor::run(repo.path()).unwrap();
    assert_eq!(
        report.detector_context_threshold,
        DetectorStatus::MarkerPending
    );
    assert_eq!(report.warning_count(), 1);

    // Consumed and acted on: marker gone, a continue round-trip recorded.
    fs::remove_file(repo.path().join(".pulse/runtime/context-threshold")).unwrap();
    emit_event(
        repo.path(),
        "run.completed",
        actor("worker").as_kind_id().as_str(),
        "TK-0001",
        serde_json::json!({"outcome": "continue"}),
        chrono::Utc::now(),
    )
    .unwrap();
    let report = doctor::run(repo.path()).unwrap();
    assert_eq!(report.detector_context_threshold, DetectorStatus::Exercised);
    assert_eq!(
        report.warning_count(),
        0,
        "a fully clean repo is doctor-clean"
    );
}
