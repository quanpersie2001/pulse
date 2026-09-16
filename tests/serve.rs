//! `pulse serve` integration tests (Decision 0023): workspace discovery,
//! the read-only payload builders, lenient JSONL reads, and the evidence
//! path-traversal guard. The HTTP layer itself is a thin router over these
//! functions and is exercised by hand (`pulse serve --open`).

use pulse::serve::{api, discovery};
use serde_json::{json, Value};
use std::fs;

fn write_issues(root: &std::path::Path, lines: &[Value]) {
    let dir = root.join(".pulse");
    fs::create_dir_all(&dir).unwrap();
    let body = lines
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(dir.join("issues.jsonl"), body).unwrap();
}

fn sample_issue(id: &str, kind: &str, status: &str) -> Value {
    json!({"id": id, "kind": kind, "status": status, "title": id,
           "created_at": "2026-09-17T00:00:00Z", "updated_at": "2026-09-17T00:00:00Z"})
}

#[test]
fn discovery_finds_pulse_projects_and_skips_heavy_dirs() {
    let workspace = tempfile::tempdir().unwrap();
    let a = workspace.path().join("project-a");
    let b = workspace.path().join("group").join("project-b");
    let deep = workspace
        .path()
        .join("l1")
        .join("l2")
        .join("l3")
        .join("l4")
        .join("l5")
        .join("too-deep");
    let vendored = workspace.path().join("node_modules").join("hostile");
    write_issues(&a, &[sample_issue("EP-1", "epic", "draft")]);
    write_issues(&b, &[sample_issue("TK-1", "ticket", "done")]);
    write_issues(&deep, &[sample_issue("EP-9", "epic", "draft")]);
    write_issues(&vendored, &[sample_issue("EP-8", "epic", "draft")]);
    // A plain directory with no .pulse/ is not a project.
    fs::create_dir_all(workspace.path().join("plain")).unwrap();

    let projects = discovery::discover(workspace.path());
    let names: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();

    assert_eq!(names, vec!["project-a", "project-b"], "found: {projects:?}");
    let a_entry = &projects[0];
    assert_eq!(a_entry.counts.epic, 1);
    let b_entry = &projects[1];
    assert_eq!(b_entry.counts.ticket, 1);
    assert_eq!(b_entry.counts.tickets_done, 1);
    assert_eq!(b_entry.counts.tickets_open, 0);
}

#[test]
fn project_ids_are_stable_short_hex() {
    let workspace = tempfile::tempdir().unwrap();
    let a = workspace.path().join("project-a");
    write_issues(&a, &[sample_issue("EP-1", "epic", "draft")]);

    let first = discovery::discover(workspace.path());
    let second = discovery::discover(workspace.path());
    assert_eq!(first[0].id, second[0].id);
    assert_eq!(first[0].id.len(), 12, "id is a 12-char hex prefix");
    assert!(first[0].id.chars().all(|c| c.is_ascii_hexdigit()));
    // The path must not leak into the id.
    assert!(!first[0].id.contains("project"));
}

#[test]
fn resolve_maps_pid_to_root_and_unknown_pid_is_none() {
    let workspace = tempfile::tempdir().unwrap();
    let a = workspace.path().join("project-a");
    write_issues(&a, &[sample_issue("EP-1", "epic", "draft")]);

    let entry = &discovery::discover(workspace.path())[0];
    let resolved = discovery::resolve(workspace.path(), &entry.id).unwrap();
    assert!(resolved.ends_with("project-a"));
    assert!(discovery::resolve(workspace.path(), "deadbeefdeadbeef").is_none());
}

#[test]
fn board_payload_reports_records_and_counts_skipped_lines() {
    let repo = tempfile::tempdir().unwrap();
    let dir = repo.path().join(".pulse");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("issues.jsonl"),
        format!(
            "{}\nnot json at all\n\n{}\n",
            sample_issue("EP-1", "epic", "draft"),
            sample_issue("ST-1", "story", "ready")
        ),
    )
    .unwrap();

    let payload = api::board_payload(repo.path());
    assert_eq!(payload["issues"].as_array().unwrap().len(), 2);
    assert_eq!(payload["skipped_lines"], 1);
    assert!(payload["learnings"].is_array());
}

#[test]
fn issue_payload_carries_record_events_receipts_evidence() {
    let repo = tempfile::tempdir().unwrap();
    write_issues(repo.path(), &[sample_issue("TK-1", "ticket", "active")]);

    // Event trace via the real event log writer.
    pulse::event::emit_event(
        repo.path(),
        "issue.updated",
        "human:quan",
        "TK-1",
        json!({"revision_after": 2}),
        chrono::Utc::now(),
    )
    .unwrap();

    // Receipt via the real receipt writer, with one artifact on disk.
    let evidence = repo.path().join(".pulse/evidence/TK-1/shots");
    fs::create_dir_all(&evidence).unwrap();
    fs::write(evidence.join("QA-001.png"), b"png-bytes").unwrap();
    let receipt = pulse::evidence::receipt::record_receipt(
        repo.path(),
        None,
        pulse::evidence::receipt::NewReceipt {
            kind: "handoff".into(),
            subject: pulse::evidence::receipt::ReceiptSubject {
                id: "TK-1".into(),
                revision: None,
            },
            actor: "agent:worker".into(),
            source: pulse::evidence::receipt::ReceiptSource {
                commit: "abc123".into(),
                dirty_hash: "sha256:x".into(),
            },
            run_id: None,
            payload: json!({"summary": "done"}),
            artifact_paths: vec![".pulse/evidence/TK-1/shots/QA-001.png".into()],
        },
    )
    .unwrap();

    let payload = api::issue_payload(repo.path(), "TK-1").unwrap();
    assert_eq!(payload["issue"]["id"], "TK-1");
    assert_eq!(payload["receipts"].as_array().unwrap().len(), 1);
    assert_eq!(payload["receipts"][0]["id"], receipt.id);
    let events = payload["events"].as_array().unwrap();
    assert!(events
        .iter()
        .any(|e| e["event_type"] == "issue.updated" && e["subject"]["id"] == "TK-1"));
    let manifest = payload["evidence"].as_array().unwrap();
    assert!(manifest
        .iter()
        .any(|e| e["path"] == "shots/QA-001.png" && e["size"] == 9));

    // Unknown issue -> None.
    assert!(api::issue_payload(repo.path(), "TK-nope").is_none());
}

#[test]
fn evidence_file_serves_within_the_issue_dir_and_refuses_traversal() {
    let repo = tempfile::tempdir().unwrap();
    let shots = repo.path().join(".pulse/evidence/TK-1/shots");
    fs::create_dir_all(&shots).unwrap();
    fs::write(shots.join("QA-001.png"), b"png-bytes").unwrap();
    // A sibling secret outside TK-1's evidence dir.
    fs::create_dir_all(repo.path().join(".pulse/evidence")).unwrap();
    fs::write(repo.path().join(".pulse/evidence/secret.txt"), b"secret").unwrap();

    let (bytes, kind) = api::evidence_file(repo.path(), "TK-1", "shots/QA-001.png").unwrap();
    assert_eq!(bytes, b"png-bytes");
    assert_eq!(kind, "image/png");

    // Every traversal shape must be refused.
    for attempt in [
        "../secret.txt",
        "..",
        "shots/../../../secret.txt",
        "",
        "/etc/passwd",
    ] {
        assert!(
            api::evidence_file(repo.path(), "TK-1", attempt).is_none(),
            "traversal must fail: {attempt:?}"
        );
    }
}

#[test]
fn evidence_content_types_cover_the_common_artifacts() {
    let repo = tempfile::tempdir().unwrap();
    let dir = repo.path().join(".pulse/evidence/TK-1");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.json"), b"{}").unwrap();
    fs::write(dir.join("a.log"), b"log").unwrap();
    fs::write(dir.join("a.svg"), b"<svg/>").unwrap();

    let (bytes, kind) = api::evidence_file(repo.path(), "TK-1", "a.json").unwrap();
    assert_eq!(kind, "application/json");
    assert_eq!(bytes, b"{}");
    assert_eq!(
        api::evidence_file(repo.path(), "TK-1", "a.log").unwrap().1,
        "text/plain; charset=utf-8"
    );
    assert_eq!(
        api::evidence_file(repo.path(), "TK-1", "a.svg").unwrap().1,
        "image/svg+xml"
    );
}
