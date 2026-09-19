//! `pulse::kernel::doctor` tests (plan 0022 §11.3 minimum + store errors).
//!
//! Doctor is a lenient reader over state the strict APIs refuse: a torn
//! store line, an unreadable receipt, an expired lease, an orphan evidence
//! dir, and a lane prepared but never sealed. Each test builds the minimal
//! `.pulse/` tree by hand — no `pulse init`, so a store layout change shows
//! up here on purpose.

use std::fs;
use std::path::Path;

use pulse::evidence::receipt::{record_receipt, NewReceipt, ReceiptSource, ReceiptSubject};
use pulse::kernel::doctor;

fn write_repo(path: &Path, store_lines: &[String]) {
    fs::create_dir_all(path.join(".pulse/receipts")).unwrap();
    fs::create_dir_all(path.join(".pulse/events")).unwrap();
    fs::create_dir_all(path.join(".pulse/runtime")).unwrap();
    fs::write(
        path.join(".pulse/issues.jsonl"),
        store_lines.join("\n") + "\n",
    )
    .unwrap();
}

fn ticket(id: &str, status: &str) -> String {
    format!(
        r#"{{"schema":3,"id":"{id}","kind":"ticket","title":"t","status":"{status}","role":"implementation","created_at":"2026-09-17T00:00:00Z","updated_at":"2026-09-17T00:00:00Z","revision":1}}"#
    )
}

#[test]
fn a_clean_repo_is_doctor_clean() {
    let repo = tempfile::tempdir().unwrap();
    write_repo(repo.path(), &[ticket("TK-0001", "ready")]);

    let report = doctor::run(repo.path()).unwrap();
    assert!(report.store_torn_lines.is_empty());
    assert!(report.unreadable_receipts.is_empty());
    assert!(report.expired_leases.is_empty());
    assert!(report.orphan_evidence.is_empty());
    assert!(report.stale_lane_preparations.is_empty());
    assert_eq!(report.warning_count(), 0);
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
    assert_eq!(report.warning_count(), 0);
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
fn a_lane_prepared_but_never_sealed_is_reported_with_who_prepared_it() {
    let repo = tempfile::tempdir().unwrap();
    write_repo(repo.path(), &[ticket("TK-0001", "verifying")]);

    // What `pulse lane input` leaves behind, and `pulse lane seal` consumes.
    let dir = repo.path().join(".pulse/runtime/lane/TK-0001");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("review-correctness.snapshot.json"),
        serde_json::to_vec(&serde_json::json!({
            "at": "2026-09-17T10:00:00Z",
            "actor": "agent:review-correctness",
            "commit": "abc",
            "dirty_hash": "sha256:0",
            "dirty_paths": [],
        }))
        .unwrap(),
    )
    .unwrap();

    let report = doctor::run(repo.path()).unwrap();
    assert_eq!(report.stale_lane_preparations.len(), 1);
    let stale = &report.stale_lane_preparations[0];
    assert_eq!(stale.id, "TK-0001");
    assert_eq!(stale.role, "review-correctness");
    assert_eq!(stale.prepared_at.as_deref(), Some("2026-09-17T10:00:00Z"));
    assert_eq!(
        stale.prepared_by.as_deref(),
        Some("agent:review-correctness")
    );
    assert_eq!(report.warning_count(), 1);

    fs::remove_file(dir.join("review-correctness.snapshot.json")).unwrap();
    assert_eq!(doctor::run(repo.path()).unwrap().warning_count(), 0);
}

// --- Plan 0025 B6: dirty files only done tickets claim ---

/// A git repo with a committed `api/x.rs`, plus the hand-written `.pulse/`
/// tree the other tests use.
fn git_repo_with_store(records: &[String]) -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        assert!(std::process::Command::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(args)
            .status()
            .unwrap()
            .success());
    };
    fs::create_dir_all(repo.path().join("api")).unwrap();
    fs::write(repo.path().join("api/x.rs"), "fn a()\n").unwrap();
    run(&["init", "-q"]);
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "user.name", "test"]);
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "baseline"]);
    write_repo(repo.path(), records);
    repo
}

fn done_ticket_with_touches(id: &str, touches: &[&str]) -> String {
    format!(
        r#"{{"schema":3,"id":"{id}","kind":"ticket","title":"t","status":"done","role":"implementation","created_at":"2026-09-17T00:00:00Z","updated_at":"2026-09-17T00:00:00Z","revision":1,"touches":{}}}"#,
        serde_json::json!(touches)
    )
}

#[test]
fn a_dirty_path_only_done_tickets_claim_is_awaiting_commit() {
    let repo = git_repo_with_store(&[done_ticket_with_touches("TK-0001", &["api/**"])]);
    fs::write(
        repo.path().join("api/x.rs"),
        "fn a() + the accepted change\n",
    )
    .unwrap();

    let report = doctor::run(repo.path()).unwrap();
    assert_eq!(
        report.awaiting_commit.len(),
        1,
        "{:?}",
        report.awaiting_commit
    );
    assert_eq!(report.awaiting_commit[0].path, "api/x.rs");
    assert_eq!(report.awaiting_commit[0].held_by, vec!["TK-0001"]);
    assert_eq!(report.warning_count(), 1);

    // The same path claimed by an open ticket is work in progress, not a
    // finding — the commit-await rule is for accepted-but-uncommitted work.
    fs::write(
        repo.path().join(".pulse/issues.jsonl"),
        format!(
            "{}\n{}\n",
            done_ticket_with_touches("TK-0001", &["api/**"]),
            ticket_claiming("TK-0002", "active", &["api/**"])
        ),
    )
    .unwrap();
    let report = doctor::run(repo.path()).unwrap();
    assert!(report.awaiting_commit.is_empty());
    assert_eq!(report.warning_count(), 0);
}

fn ticket_claiming(id: &str, status: &str, touches: &[&str]) -> String {
    format!(
        r#"{{"schema":3,"id":"{id}","kind":"ticket","title":"t","status":"{status}","role":"implementation","created_at":"2026-09-17T00:00:00Z","updated_at":"2026-09-17T00:00:00Z","revision":1,"touches":{}}}"#,
        serde_json::json!(touches)
    )
}

#[test]
fn a_dirty_path_no_ticket_claims_is_not_awaiting_commit() {
    // Mid-run dirt outside every `touches` is normal parallel work, not a
    // reminder about accepted-but-uncommitted files (plan 0025 B6).
    let repo = git_repo_with_store(&[done_ticket_with_touches("TK-0001", &["api/**"])]);
    fs::write(repo.path().join("loose.txt"), "unclaimed\n").unwrap();

    let report = doctor::run(repo.path()).unwrap();
    assert!(report.awaiting_commit.is_empty());
}

// --- Plan 0025 E3/E4: suspect learnings and stale cites are findings ---

#[test]
fn a_suspect_learning_and_a_stale_cite_are_reported_but_nothing_is_retired() {
    let repo = tempfile::tempdir();
    let repo = repo.unwrap();
    fs::create_dir_all(repo.path().join(".pulse")).unwrap();
    fs::write(repo.path().join(".pulse/issues.jsonl"), "").unwrap();
    fs::write(
        repo.path().join("code.rs"),
        "line one\nline two\nline three\n",
    )
    .unwrap();
    // One suspect (misleading > helpful) with a live cite, one healthy
    // learning with a stale cite: both findings appear in one pass.
    pulse::learn::store::write(
        repo.path(),
        &pulse::learn::store::Learning {
            frontmatter: pulse::learn::store::Frontmatter {
                id: "LRN-aaaa".into(),
                status: "active".into(),
                kind: "failure".into(),
                applies_to: vec![],
                tags: vec![],
                from: vec![],
                expected_signal: String::new(),
                usage: pulse::learn::store::UsageCounts {
                    helpful: 1,
                    misleading: 2,
                    ..Default::default()
                },
                check_argv: vec![],
                check_cwd: None,
                cites: vec![pulse::learn::store::Cite {
                    path: "code.rs".into(),
                    lines: "1-2".into(),
                    sha256: pulse::canonical_json::hash_bytes(b"line one\nline two"),
                }],
            },
            body: "## Summary\ns\n".into(),
        },
    )
    .unwrap();
    pulse::learn::store::write(
        repo.path(),
        &pulse::learn::store::Learning {
            frontmatter: pulse::learn::store::Frontmatter {
                id: "LRN-bbbb".into(),
                status: "candidate".into(),
                kind: "constraint".into(),
                applies_to: vec![],
                tags: vec![],
                from: vec![],
                expected_signal: String::new(),
                usage: Default::default(),
                check_argv: vec![],
                check_cwd: None,
                cites: vec![pulse::learn::store::Cite {
                    path: "code.rs".into(),
                    lines: "1-3".into(),
                    sha256: "sha256:stale".into(),
                }],
            },
            body: "## Summary\ns\n".into(),
        },
    )
    .unwrap();

    let report = doctor::run(repo.path()).unwrap();
    assert_eq!(report.learning_suspects.len(), 1);
    assert_eq!(report.learning_suspects[0].id, "LRN-aaaa");
    assert_eq!(report.learning_suspects[0].misleading, 2);
    assert_eq!(report.stale_cites.len(), 1);
    assert_eq!(report.stale_cites[0].learning, "LRN-bbbb");
    assert_eq!(report.warning_count(), 2);

    // Nothing was retired by machine: statuses untouched, cites intact.
    let learning = pulse::learn::store::read(repo.path(), "LRN-aaaa").unwrap();
    assert_eq!(learning.frontmatter.status, "active");
    let learning = pulse::learn::store::read(repo.path(), "LRN-bbbb").unwrap();
    assert_eq!(learning.frontmatter.cites.len(), 1);
}
