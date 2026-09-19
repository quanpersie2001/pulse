//! `pulse serve` integration tests (Decision 0023): workspace discovery,
//! the read-only payload builders, lenient JSONL reads, and the evidence
//! path-traversal guard. The HTTP layer itself is a thin router over these
//! functions and is exercised by hand (`pulse serve --open`).

#[path = "common/bin.rs"]
mod common_bin;

use pulse::serve::{api, discovery, registry};
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
    assert_eq!(payload["recent_events"], json!([]));
}

#[test]
fn board_payload_carries_the_newest_events_oldest_first() {
    let repo = tempfile::tempdir().unwrap();
    write_issues(repo.path(), &[sample_issue("TK-1", "ticket", "active")]);
    let start = chrono::Utc::now();
    for revision in 0..205 {
        pulse::event::emit_event(
            repo.path(),
            "issue.updated",
            "human:quan",
            "TK-1",
            json!({"revision_after": revision}),
            start + chrono::Duration::milliseconds(revision),
        )
        .unwrap();
    }

    let payload = api::board_payload(repo.path());
    let events = payload["recent_events"].as_array().unwrap();
    assert_eq!(events.len(), 200);
    assert_eq!(events[0]["payload"]["revision_after"], 5);
    assert_eq!(events[199]["payload"]["revision_after"], 204);
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
fn issue_payload_links_story_and_ticket_evidence_both_ways() {
    let repo = tempfile::tempdir().unwrap();
    let mut ticket = sample_issue("TK-1", "ticket", "done");
    ticket["story"] = json!("ST-1");
    let mut other = sample_issue("TK-2", "ticket", "done");
    other["story"] = json!("ST-9");
    write_issues(
        repo.path(),
        &[sample_issue("ST-1", "story", "done"), ticket, other],
    );
    for (owner, file) in [
        ("ST-1", "shots/QA-004.png"),
        ("TK-1", "review.json"),
        ("TK-2", "unrelated.json"),
    ] {
        let path = repo.path().join(".pulse/evidence").join(owner).join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x").unwrap();
    }

    let ticket = api::issue_payload(repo.path(), "TK-1").unwrap();
    assert_eq!(
        ticket["related_evidence"],
        json!([{"issue": "ST-1", "path": "shots/QA-004.png", "size": 1}])
    );
    let story = api::issue_payload(repo.path(), "ST-1").unwrap();
    assert_eq!(
        story["related_evidence"],
        json!([{"issue": "TK-1", "path": "review.json", "size": 1}])
    );
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

// ---- registry (Decision 0023 §5 as amended) ----
// These use the `_at`/`_into` pure forms: mutating process env from
// parallel Rust tests is a race (a set_var can collide with another
// thread's getenv), so the CLI-level registration test spawns the real
// binary with a scratch PULSE_REGISTRY instead.

/// Register a repo, hide it when the repo disappears, and stay idempotent.
#[test]
fn registry_registers_filters_dead_and_is_idempotent() {
    let registry = tempfile::tempdir().unwrap();
    let registry_file = registry.path().join("projects.json");
    let repo = tempfile::tempdir().unwrap();
    write_issues(repo.path(), &[sample_issue("EP-1", "epic", "draft")]);

    registry::register_into(&registry_file, repo.path()).unwrap();
    registry::register_into(&registry_file, repo.path()).unwrap(); // idempotent
    let roots = registry::registered_roots_at(&registry_file);
    assert_eq!(roots.len(), 1);
    assert!(roots[0].ends_with(repo.path().file_name().unwrap()));

    // The repo disappearing hides it from the registry read.
    fs::remove_file(repo.path().join(".pulse/issues.jsonl")).unwrap();
    assert!(registry::registered_roots_at(&registry_file).is_empty());
}

/// `discover_merged` unions the registry with a workspace scan and
/// dedupes by project id.
#[test]
fn discover_merged_unions_registry_and_scan_without_duplicates() {
    let registry = tempfile::tempdir().unwrap();
    let registry_file = registry.path().join("projects.json");

    let workspace = tempfile::tempdir().unwrap();
    let scanned = workspace.path().join("only-on-disk");
    write_issues(&scanned, &[sample_issue("EP-1", "epic", "draft")]);
    registry::register_into(&registry_file, &scanned).unwrap();

    let scanned_id = discovery::project_id(&scanned.canonicalize().unwrap());
    // No workspace: registry only, one entry.
    assert_eq!(
        discovery::discover_merged(Some(&registry_file), None).len(),
        1
    );
    // Workspace scan: same repo registered AND scanned, deduped to one.
    let merged = discovery::discover_merged(Some(&registry_file), Some(workspace.path()));
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].id, scanned_id);

    // A registered repo outside the scan root also lists, and an unknown
    // registry file lists nothing.
    let elsewhere = tempfile::tempdir().unwrap();
    write_issues(elsewhere.path(), &[sample_issue("EP-2", "epic", "draft")]);
    registry::register_into(&registry_file, elsewhere.path()).unwrap();
    assert_eq!(
        discovery::discover_merged(Some(&registry_file), None).len(),
        2
    );
    assert_eq!(
        discovery::discover_merged(Some(registry.path().join("nope.json").as_path()), None).len(),
        0
    );
}

/// `pulse init` registers the repo through the CLI layer (best-effort):
/// spawn the real binary with a scratch registry, like golden_path does.
#[test]
fn init_registers_into_the_registry() {
    let registry = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();

    // The kernel entry point never touches the registry — only the CLI does.
    pulse::kernel::init::initialize_repository(repo.path(), false, false).unwrap();
    assert!(registry::registered_roots_at(&registry.path().join("projects.json")).is_empty());

    let output = std::process::Command::new(common_bin::bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args(["init", "--json"])
        .env("PULSE_REGISTRY", registry.path().join("projects.json"))
        .output()
        .expect("run pulse init with a scratch registry");
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let roots = registry::registered_roots_at(&registry.path().join("projects.json"));
    assert_eq!(roots.len(), 1);
    assert!(roots[0].ends_with(repo.path().file_name().unwrap()));
}
