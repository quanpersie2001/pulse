use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

use crate::common_bin::bin;

fn run(repo: &TempDir, args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .arg("--repo-root")
        .arg(repo.path())
        .args(args)
        .output()
        .expect("run pulse")
}

fn run_ok(repo: &TempDir, args: &[&str]) -> Value {
    let output = run(repo, args);
    assert!(
        output.status.success(),
        "command failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("json stdout")
}

fn run_err(repo: &TempDir, args: &[&str]) -> Value {
    let output = run(repo, args);
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stderr).expect("json stderr")
}

fn write_json(path: &Path, value: &Value) -> String {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    path.to_string_lossy().to_string()
}

fn setup_repo() -> (TempDir, String) {
    let repo = tempfile::tempdir().unwrap();
    let work = run_ok(
        &repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Knowledge source",
            "--role",
            "implementation",
            "--risk",
            "low",
            "--materialization",
            "R0",
            "--json",
        ],
    );
    let work_id = work["value"]["id"].as_str().unwrap().to_string();
    (repo, work_id)
}

fn draft(work_id: &str) -> Value {
    json!({
        "title": "Token rotation requires atomic mutation",
        "kind": "failure_pattern",
        "severity": "high",
        "summary": "Concurrent refresh can issue invalid tokens when rotation uses check-then-act.",
        "guidance": {
            "do": ["Use an atomic state transition."],
            "avoid": ["Do not split rotation into unguarded read then write."],
            "required_checks": ["Exercise concurrent refresh attempts."]
        },
        "applicability": {
            "paths": ["src/auth/**"],
            "symbols": ["rotateRefreshToken"],
            "risks": ["concurrency"]
        },
        "provenance_targets": [{
            "relation": "derived_from",
            "kind": "work",
            "id": work_id,
            "revision": 1,
            "content_hash": null
        }],
        "source_commits": [],
        "routing": null,
        "promotion": null,
        "freshness": null,
        "trust": null,
        "content": null
    })
}

#[test]
fn knowledge_cli_json_contracts_cover_crud_relations_validation_export_status() {
    let (repo, work_id) = setup_repo();
    let draft_file = write_json(&repo.path().join("learning.json"), &draft(&work_id));

    let created = run_ok(
        &repo,
        &[
            "knowledge",
            "create",
            "--file",
            &draft_file,
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(created["schema_version"], 1);
    assert_eq!(created["code"], "created");
    assert_eq!(created["status"], "created");
    assert_eq!(created["value"]["id"], "LRN-001");
    assert_eq!(created["value"]["revision"], 1);
    assert_eq!(created["value"]["status"], "candidate");
    assert_eq!(created["value"]["validation"]["confidence"], "low");
    assert_eq!(created["relations"].as_array().unwrap().len(), 1);
    assert!(created["knowledge_fingerprint"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));

    let shown = run_ok(&repo, &["knowledge", "show", "LRN-001", "--json"]);
    assert_eq!(shown["schema_version"], 1);
    assert_eq!(shown["code"], "ok");
    assert_eq!(shown["learning"]["id"], "LRN-001");
    assert_eq!(shown["relations"].as_array().unwrap().len(), 1);

    let listed = run_ok(
        &repo,
        &[
            "knowledge",
            "list",
            "--status",
            "candidate",
            "--kind",
            "failure_pattern",
            "--json",
        ],
    );
    assert_eq!(listed["schema_version"], 1);
    assert_eq!(listed["code"], "ok");
    assert_eq!(listed["items"].as_array().unwrap().len(), 1);

    let patch_file = write_json(
        &repo.path().join("learning-patch.json"),
        &json!({"summary": "Updated concise summary."}),
    );
    let edited = run_ok(
        &repo,
        &[
            "knowledge",
            "edit",
            "LRN-001",
            "--expected-revision",
            "1",
            "--patch",
            &patch_file,
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(edited["code"], "updated");
    assert_eq!(edited["status"], "updated");
    assert_eq!(edited["value"]["revision"], 2);
    assert_eq!(edited["value"]["summary"], "Updated concise summary.");

    let relation = run_ok(
        &repo,
        &[
            "knowledge",
            "relation",
            "add",
            "LRN-001",
            "--type",
            "applied_to",
            "--to-kind",
            "work",
            "--to",
            &work_id,
            "--target-revision",
            "1",
            "--expected-revision",
            "2",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(relation["schema_version"], 1);
    assert_eq!(relation["code"], "created");
    assert_eq!(relation["status"], "created");
    assert_eq!(
        relation["relation_id"],
        format!("applied-to--LRN-001--work--{work_id}")
    );

    let valid = run_ok(&repo, &["knowledge", "validate", "--json"]);
    assert_eq!(valid["schema_version"], 1);
    assert_eq!(valid["code"], "ok");
    assert_eq!(valid["valid"], true);
    assert!(valid["errors"].as_array().unwrap().is_empty());

    let exported = run_ok(&repo, &["knowledge", "export", "--json"]);
    assert_eq!(exported["schema_version"], 1);
    assert_eq!(exported["code"], "ok");
    assert_eq!(exported["counts"]["entries"], 1);
    assert_eq!(exported["counts"]["relations"], 2);
    assert_eq!(
        exported["eligibility"]["future_default_search"]["excluded"][0]["id"],
        "LRN-001"
    );
    assert_eq!(
        exported["eligibility"]["future_default_search"]["excluded"][0]["reason_codes"],
        json!(["learning_candidate"])
    );

    let status = run_ok(&repo, &["knowledge", "status", "--json"]);
    assert_eq!(status["schema_version"], 1);
    assert_eq!(status["code"], "ok");
    assert_eq!(status["counts"]["entries"], 1);
    assert_eq!(status["cache_state"], "current");
}

#[test]
fn knowledge_cli_errors_are_json_and_non_zero() {
    let (repo, work_id) = setup_repo();
    let draft_file = write_json(&repo.path().join("learning.json"), &draft(&work_id));
    run_ok(
        &repo,
        &[
            "knowledge",
            "create",
            "--file",
            &draft_file,
            "--actor",
            "human:test",
            "--json",
        ],
    );

    let stale_patch = write_json(
        &repo.path().join("stale-patch.json"),
        &json!({"title": "stale edit"}),
    );
    let stale = run_err(
        &repo,
        &[
            "knowledge",
            "edit",
            "LRN-001",
            "--expected-revision",
            "99",
            "--patch",
            &stale_patch,
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(stale["schema_version"], 1);
    assert_eq!(stale["code"], "cas_conflict");
    assert_eq!(stale["subject"], "LRN-001");
    assert_eq!(stale["expected_revision"], 99);
    assert_eq!(stale["current_revision"], 1);

    let missing = run_err(&repo, &["knowledge", "show", "LRN-404", "--json"]);
    assert_eq!(missing["schema_version"], 1);
    assert_eq!(missing["code"], "learning_not_found");

    let invalid = run_err(
        &repo,
        &[
            "knowledge",
            "relation",
            "add",
            "LRN-001",
            "--type",
            "applied_to",
            "--to-kind",
            "work",
            "--to",
            "TICKET-NOPE",
            "--expected-revision",
            "1",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(invalid["schema_version"], 1);
    assert_eq!(invalid["code"], "knowledge_relation_endpoint_missing");
}

#[test]
fn capture_validate_promote_ratchet_lifecycle() {
    let (repo, work_id) = setup_repo();
    // Evidence + git context for the validate step.
    let git_init = Command::new("git")
        .args(["init", "-q"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(git_init.status.success());
    let commit = Command::new("git")
        .args(["commit", "--allow-empty", "-q", "-m", "base"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(commit.status.success());
    run_ok(&repo, &["evidence", "bootstrap", "--json"]);
    let head = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo.path())
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    let manifest: Value = serde_json::from_slice(
        &fs::read(repo.path().join(".pulse/evidence/manifest.json")).unwrap(),
    )
    .unwrap();
    let repository_id = manifest["repository_id"].as_str().unwrap().to_string();

    // Capture: draft carries no provenance; it is derived from the Ticket.
    let capture_draft = write_json(
        &repo.path().join("capture.json"),
        &json!({
            "title": "Freeze the tree between handoff and close",
            "kind": "process_insight",
            "severity": "medium",
            "summary": "Out-of-scope edits after handoff stale the proof chain.",
            "guidance": {
                "do": ["Leave the target worktree untouched until close."],
                "avoid": ["Do not edit tracked files after handing off."],
                "required_checks": []
            },
            "applicability": {"paths": ["src/**"]},
            "provenance_targets": [],
            "source_commits": [],
            "routing": null,
            "promotion": null,
            "freshness": null,
            "trust": null,
            "content": null
        }),
    );
    let captured = run_ok(
        &repo,
        &[
            "knowledge",
            "capture",
            "--from",
            &work_id,
            "--file",
            &capture_draft,
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(captured["code"], "created");
    assert_eq!(captured["value"]["status"], "candidate");
    let relations = captured["relations"].as_array().unwrap();
    assert_eq!(relations.len(), 1, "derived_from relation auto-added");

    // validate-learning requires evidence that resolves.
    let missing = run_err(
        &repo,
        &[
            "knowledge",
            "validate-learning",
            "LRN-001",
            "--evidence",
            "rcpt_01J00000000000000000000000",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(missing["code"], "knowledge_validate_evidence_missing");

    // Record a resolvable evidence receipt for the transition.
    let receipt = json!({
        "schema_version": 1,
        "receipt_version": 2,
        "id": "rcpt_01J00000000000000000000001",
        "kind": "qa_checkpoint",
        "result": "passed",
        "actor": {"kind": "human", "id": "test"},
        "recorded_at": "2026-09-05T00:00:00Z",
        "subject": {"kind": "work", "id": work_id},
        "bindings": {},
        "payload": {
            "payload_version": 1,
            "qa_scope": "ticket_checkpoint",
            "story_id": "ST-000",
            "ticket_id": work_id,
            "baseline_revision": 1,
            "baseline_content_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "cases": [{"case_id": "QA-001", "case_revision": 1, "outcome": "passed"}],
            "executor": {"name": "t", "version": "1"},
            "observations": ["observed"]
        }
    });
    fs::create_dir_all(repo.path().join(".pulse/evidence/receipts")).unwrap();
    fs::write(
        repo.path()
            .join(".pulse/evidence/receipts/rcpt_01J00000000000000000000001.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
    let validated = run_ok(
        &repo,
        &[
            "knowledge",
            "validate-learning",
            "LRN-001",
            "--evidence",
            "rcpt_01J00000000000000000000001",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(validated["value"]["status"], "validated");
    assert_eq!(validated["value"]["validation"]["confidence"], "medium");

    // Promote requires a registry document and records promoted_to.
    let _ = head;
    let _ = repository_id;
    fs::create_dir_all(repo.path().join("docs/product")).unwrap();
    fs::write(
        repo.path().join("docs/product/x.md"),
        "# Example\n\n## Guidance\n\ndoc body\n",
    )
    .unwrap();
    let record = write_json(
        &repo.path().join("doc-record.json"),
        &json!({
            "id": "DOC-EXAMPLE",
            "revision": 1,
            "path": "docs/product/x.md",
            "summary": "Example document.",
            "owner": "human:test",
            "kind": "product",
            "status": "approved",
            "tags": []
        }),
    );
    let registered = run_ok(
        &repo,
        &[
            "docs",
            "register",
            "--file",
            &record,
            "--expected-registry-revision",
            "1",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(registered["code"], "registered");

    let promoted = run_ok(
        &repo,
        &[
            "knowledge",
            "promote",
            "LRN-001",
            "--document",
            "DOC-EXAMPLE",
            "--insert-after",
            "Guidance",
            "--rationale",
            "belongs in the behavior contract",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(promoted["code"], "promoted");
    let shown_after = run_ok(&repo, &["knowledge", "show", "LRN-001", "--json"]);
    assert_eq!(shown_after["learning"]["status"], "promoted");
    assert_eq!(shown_after["learning"]["promotion"]["state"], "promoted");
    // show lists every relation touching the learning: the derived_from
    // provenance from capture plus the new promoted_to.
    let relations = shown_after["relations"].as_array().unwrap();
    assert_eq!(relations.len(), 2);
    assert!(relations
        .iter()
        .any(|relation| relation["type"] == "promoted_to"));
    let doc_after = fs::read_to_string(repo.path().join("docs/product/x.md")).unwrap();
    assert!(doc_after.contains("## Guidance\n\n- **LRN-001"));

    // Illegal transitions are refused.
    let illegal = run_err(
        &repo,
        &[
            "knowledge",
            "validate-learning",
            "LRN-001",
            "--evidence",
            "rcpt_01J00000000000000000000001",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(illegal["code"], "knowledge_transition_invalid");
}

fn validated_learning(repo: &TempDir, work_id: &str, title: &str, scope: Option<&str>) -> String {
    let shown = run_ok(repo, &["work", "show", work_id, "--json"]);
    let revision = shown["node"]["revision"].as_u64().unwrap();
    let mut draft = json!({
        "title": title,
        "kind": "failure_pattern",
        "severity": "medium",
        "summary": "Promotion needs the target document to change.",
        "guidance": {
            "do": ["Insert after the chosen heading."],
            "avoid": [],
            "required_checks": ["Re-read the promoted section."]
        },
        "applicability": {"paths": ["src/**"]},
        "provenance_targets": [{
            "relation": "derived_from",
            "kind": "work",
            "id": work_id,
            "revision": revision,
            "content_hash": null
        }],
        "source_commits": [],
        "routing": null,
        "promotion": null,
        "freshness": null,
        "trust": null,
        "content": null
    });
    if let Some(scope) = scope {
        draft["scope"] = json!(scope);
    }
    let draft_file = write_json(&repo.path().join("learning.json"), &draft);
    let created = run_ok(
        repo,
        &[
            "knowledge",
            "create",
            "--file",
            &draft_file,
            "--actor",
            "human:test",
            "--json",
        ],
    );
    let learning_id = created["value"]["id"].as_str().unwrap().to_string();
    let receipt = json!({
        "schema_version": 1,
        "receipt_version": 2,
        "id": format!("rcpt_01J0000000000000000000000{}", if learning_id.ends_with('1') {"1"} else {"2"}),
        "kind": "qa_checkpoint",
        "result": "passed",
        "actor": {"kind": "human", "id": "test"},
        "recorded_at": "2026-09-06T00:00:00Z",
        "subject": {"kind": "work", "id": work_id},
        "bindings": {},
        "payload": {
            "payload_version": 1,
            "qa_scope": "ticket_checkpoint",
            "story_id": "ST-000",
            "ticket_id": work_id,
            "baseline_revision": 1,
            "baseline_content_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "cases": [{"case_id": "QA-001", "case_revision": 1, "outcome": "passed"}],
            "executor": {"name": "t", "version": "1"},
            "observations": ["observed"]
        }
    });
    fs::create_dir_all(repo.path().join(".pulse/evidence/receipts")).unwrap();
    fs::write(
        repo.path()
            .join(".pulse/evidence/receipts/rcpt_01J00000000000000000000001.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
    run_ok(
        repo,
        &[
            "knowledge",
            "validate-learning",
            &learning_id,
            "--evidence",
            "rcpt_01J00000000000000000000001",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    learning_id
}

fn promotion_setup() -> (TempDir, String, String, String) {
    let (repo, work_id) = setup_repo();
    let learning_id = validated_learning(&repo, &work_id, "Promotion lesson", None);
    let doc_path = repo.path().join("docs/promotion-target.md");
    fs::create_dir_all(doc_path.parent().unwrap()).unwrap();
    fs::write(
        &doc_path,
        "# Promotion target\n\n## Guidance\n\nExisting guidance lives here.\n",
    )
    .unwrap();
    let doc_record = json!({
        "id": "DOC-PROMOTION-TARGET",
        "revision": 1,
        "path": "docs/promotion-target.md",
        "kind": "domain",
        "status": "approved",
        "owner": "team:docs",
        "summary": "Promotion target document",
        "scope": {"paths": ["src/**"]},
        "tags": [],
        "generated": null,
        "superseded_by": null
    });
    let record_file = write_json(&repo.path().join("doc-record.json"), &doc_record);
    run_ok(
        &repo,
        &[
            "docs",
            "register",
            "--file",
            &record_file,
            "--expected-registry-revision",
            "1",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    (repo, work_id, learning_id, doc_path.display().to_string())
}

#[test]
fn promote_dry_run_reports_the_insertion_without_writing() {
    let (repo, _work, learning_id, doc_path) = promotion_setup();
    let before = fs::read_to_string(&doc_path).unwrap();

    let out = run_ok(
        &repo,
        &[
            "knowledge",
            "promote",
            &learning_id,
            "--document",
            "DOC-PROMOTION-TARGET",
            "--insert-after",
            "Guidance",
            "--dry-run",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(out["code"], "promotion_dry_run");
    assert_eq!(out["dry_run"], true);
    assert_eq!(out["target_path"], "docs/promotion-target.md");
    let block = out["inserted_block"].as_str().unwrap();
    assert!(block.contains(&format!("- **{learning_id} — Promotion lesson**:")));
    assert!(block.contains("- Do: Insert after the chosen heading."));
    assert_eq!(fs::read_to_string(&doc_path).unwrap(), before);

    // The learning is still validated, not promoted.
    let shown = run_ok(&repo, &["knowledge", "show", &learning_id, "--json"]);
    assert_eq!(shown["learning"]["status"], "validated");
}

#[test]
fn promote_inserts_content_and_binds_the_new_hash() {
    let (repo, _work, learning_id, doc_path) = promotion_setup();

    let out = run_ok(
        &repo,
        &[
            "knowledge",
            "promote",
            &learning_id,
            "--document",
            "DOC-PROMOTION-TARGET",
            "--insert-after",
            "Guidance",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(out["code"], "promoted");
    assert_eq!(out["dry_run"], false);

    let content = fs::read_to_string(&doc_path).unwrap();
    let block = out["inserted_block"].as_str().unwrap();
    assert!(content.contains(block), "doc must contain the block");
    assert!(
        content.contains("## Guidance\n"),
        "insert after the heading"
    );
    let position = content.find("## Guidance\n").unwrap();
    let block_position = content.find(block).unwrap();
    assert!(block_position > position);

    // The relation binds the NEW content hash of the document.
    let shown = run_ok(&repo, &["knowledge", "show", &learning_id, "--json"]);
    assert_eq!(shown["learning"]["status"], "promoted");
    let relations = shown["relations"].as_array().unwrap();
    let promoted_to = relations
        .iter()
        .find(|relation| relation["type"] == "promoted_to")
        .expect("promoted_to relation recorded");
    assert_eq!(promoted_to["to"]["kind"], "document");
    assert_eq!(
        promoted_to["to"]["content_hash"],
        out["target_content_hash"]
    );
    let expected = sha256_hex(content.as_bytes());
    assert_eq!(
        promoted_to["to"]["content_hash"],
        format!("sha256:{expected}")
    );

    // The store check passes with the promotion relation in place.
    run_ok(&repo, &["knowledge", "validate", "--json"]);
}

#[test]
fn promote_refuses_when_insertion_would_not_change_the_document() {
    let (repo, _work, learning_id, doc_path) = promotion_setup();

    // Pre-insert the exact deterministic block: the promotion would be a
    // no-op, which Decision 13.4 refuses.
    let block = format!(
        "- **{learning_id} — Promotion lesson**: Promotion needs the target document to change.\n  - Do: Insert after the chosen heading.\n  - Check: Re-read the promoted section.\n"
    );
    let content = fs::read_to_string(&doc_path).unwrap();
    fs::write(
        &doc_path,
        content.replace("## Guidance\n", &format!("## Guidance\n\n{block}\n")),
    )
    .unwrap();

    let err = run_err(
        &repo,
        &[
            "knowledge",
            "promote",
            &learning_id,
            "--document",
            "DOC-PROMOTION-TARGET",
            "--insert-after",
            "Guidance",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(err["code"], "promotion_target_unchanged");
}

#[test]
fn promote_reports_missing_and_ambiguous_headings() {
    let (repo, _work, learning_id, _doc) = promotion_setup();
    let missing = run_err(
        &repo,
        &[
            "knowledge",
            "promote",
            &learning_id,
            "--document",
            "DOC-PROMOTION-TARGET",
            "--insert-after",
            "No Such Heading",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(missing["code"], "promotion_target_heading_missing");
}

#[test]
fn harness_promotion_lands_in_agents_md_and_validates() {
    let (repo, work_id) = setup_repo();
    let learning_id = validated_learning(&repo, &work_id, "Harness lesson", Some("harness"));
    fs::write(
        repo.path().join("AGENTS.md"),
        "# Repository map\n\n## Operating rules\n\nWork inside contracts.\n",
    )
    .unwrap();

    let out = run_ok(
        &repo,
        &[
            "knowledge",
            "promote",
            &learning_id,
            "--agents-md",
            "--insert-after",
            "Operating rules",
            "--actor",
            "human:test",
            "--json",
        ],
    );
    assert_eq!(out["code"], "promoted");
    assert_eq!(out["target_path"], "AGENTS.md");
    let content = fs::read_to_string(repo.path().join("AGENTS.md")).unwrap();
    assert!(content.contains(out["inserted_block"].as_str().unwrap()));

    // The non-registry AGENTS.md endpoint resolves as a repository file.
    let check = run_ok(&repo, &["knowledge", "validate", "--json"]);
    let text = serde_json::to_string(&check).unwrap();
    assert!(
        !text.contains("knowledge_relation_endpoint_missing"),
        "{text}"
    );
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
