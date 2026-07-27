//! S7-I3 CLI contract smoke tests: verify the new `work contract`, `work
//! qa-impact`, and `work shaping` commands parse, emit stable JSON and honor
//! authority/CAS at the CLI boundary. Logic-depth coverage lives in the API
//! test file; this file covers the CLI wiring.

use chrono::Utc;
use pulse::canonical_json::hash_bytes;
use pulse::evidence::model::*;
use pulse::policy::{AuthorityPolicy, AuthorityPrincipal};
use serde_json::{json, Value};
use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn bin() -> String {
    std::env::var("CARGO_BIN_EXE_pulse").unwrap_or_else(|_| "target/debug/pulse".to_string())
}

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

fn run_err(repo: &TempDir, args: &[&str]) -> (Value, i32) {
    let output = run(repo, args);
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value = serde_json::from_slice(&output.stderr).unwrap_or_else(|_| {
        panic!(
            "non-json stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    let code = output.status.code().unwrap_or(-1);
    (value, code)
}

fn write_json(path: &std::path::Path, value: &impl serde::Serialize) {
    fs::write(
        path,
        pulse::canonical_json::to_canonical_bytes(value).unwrap(),
    )
    .unwrap();
}

fn write_policy(repo: &TempDir, grants: &[&str]) {
    let policy = AuthorityPolicy {
        schema_version: 1,
        revision: 1,
        principals: vec![AuthorityPrincipal {
            kind: ActorKind::Human,
            id: "tester".to_string(),
            grants: grants.iter().map(|g| g.to_string()).collect(),
        }],
    };
    let path = repo.path().join(".pulse/policy/authority.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_json(&path, &policy);
}

fn git(repo: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn commit_all(repo: &std::path::Path) -> String {
    if !repo.join(".git").exists() {
        git(repo, &["init"]);
        git(repo, &["config", "user.email", "test@example.com"]);
        git(repo, &["config", "user.name", "Test User"]);
    }
    git(repo, &["add", "."]);
    git(repo, &["commit", "--allow-empty", "-m", "snapshot"]);
    git(repo, &["rev-parse", "HEAD"])
}

fn create_ticket(repo: &TempDir) -> (String, Value) {
    let created = run_ok(
        repo,
        &[
            "work",
            "create",
            "--kind",
            "ticket",
            "--title",
            "Contract ticket",
            "--role",
            "implementation",
            "--risk",
            "low",
            "--materialization",
            "R1",
            "--json",
        ],
    );
    let id = created["value"]["id"].as_str().unwrap().to_string();
    (id, created["value"].clone())
}

fn contract_request_json(node: &Value) -> Value {
    let content_dir = node["content_dir"].as_str().unwrap();
    json!({
        "role": "implementation",
        "implementation": {
            "mode": "guided",
            "work_surface": "code",
            "plan_policy": "none",
            "semantic_impact": "no_behavior_or_public_risk_change",
            "effort": {},
            "verification_profile": "service-change",
            "brief": {
                "path": format!("{content_dir}/ticket.md"),
                "content_hash": format!("sha256:{}", "a".repeat(64))
            },
            "objective": "Distinguish expired and invalid tokens.",
            "current_behavior": "Both map to InvalidToken.",
            "target_behavior": "Expired maps to TokenExpired.",
            "code_anchors": [{"path": "src/auth.rs"}],
            "documentation_anchors": [],
            "configuration_anchors": [],
            "data_anchors": [],
            "research_refs": [],
            "required_changes": [{"id": "CHG-1", "summary": "Introduce error."}],
            "invariants": [{"id": "INV-1", "summary": "No secret leak."}],
            "acceptance": [{"id": "AC-1", "summary": "Expired classified."}],
            "scope": {},
            "implementation_freedom": [{"id": "FREE-1", "summary": "Helper free."}],
            "required_decisions": [],
            "shared_approach_refs": [],
            "expected_evidence": [],
            "expected_handoff": []
        }
    })
}

#[test]
fn contract_set_and_show_cli_round_trip() {
    let repo = tempfile::tempdir().unwrap();
    let (id, node) = create_ticket(&repo);
    let revision = node["revision"].as_u64().unwrap();

    let file = repo.path().join("contract.json");
    write_json(&file, &contract_request_json(&node));

    let out = run_ok(
        &repo,
        &[
            "work",
            "contract",
            "set",
            &id,
            "--file",
            file.to_str().unwrap(),
            "--expected-revision",
            &revision.to_string(),
            "--actor",
            "human:tester",
            "--json",
        ],
    );
    assert_eq!(out["code"], "updated");
    assert_eq!(out["value"]["revision"], revision + 1);
    assert_eq!(
        out["value"]["contract_revision"],
        node["contract_revision"].as_u64().unwrap() + 1
    );

    let shown = run_ok(&repo, &["work", "contract", "show", &id, "--json"]);
    assert_eq!(shown["code"], "ok");
    assert_eq!(shown["ticket_id"], id);
    assert!(shown["implementation"].is_object());
}

#[test]
fn qa_impact_cli_authority_and_cas() {
    let repo = tempfile::tempdir().unwrap();
    let (id, node) = create_ticket(&repo);

    // No policy → authority-gated posture denied.
    let (err, code) = run_err(
        &repo,
        &[
            "work",
            "qa-impact",
            "set",
            &id,
            "--posture",
            "none",
            "--rationale",
            "Internal refactor.",
            "--expected-revision",
            "1",
            "--actor",
            "human:tester",
            "--json",
        ],
    );
    assert_eq!(err["code"], "readiness_policy_missing");
    assert_ne!(code, 0);

    write_policy(&repo, &["qa.none.approve"]);
    let out = run_ok(
        &repo,
        &[
            "work",
            "qa-impact",
            "set",
            &id,
            "--posture",
            "none",
            "--rationale",
            "Internal refactor.",
            "--expected-revision",
            "1",
            "--actor",
            "human:tester",
            "--json",
        ],
    );
    assert_eq!(out["code"], "updated");
    assert_eq!(out["value"]["qa"]["impact"]["posture"], "none");

    let shown = run_ok(&repo, &["work", "qa-impact", "show", &id, "--json"]);
    assert_eq!(shown["qa"]["impact"]["posture"], "none");

    // Stale CAS rejected.
    let _ = node;
    let (err, _code) = run_err(
        &repo,
        &[
            "work",
            "qa-impact",
            "set",
            &id,
            "--posture",
            "none",
            "--rationale",
            "again",
            "--expected-revision",
            "1",
            "--actor",
            "human:tester",
            "--json",
        ],
    );
    assert_eq!(err["code"], "cas_conflict");
}

#[test]
fn shaping_apply_show_invalidate_cli() {
    let repo = tempfile::tempdir().unwrap();
    let (id, node) = create_ticket(&repo);
    let content_dir = node["content_dir"].as_str().unwrap();
    let revision = node["revision"].as_u64().unwrap();
    let contract_revision = node["contract_revision"].as_u64().unwrap();
    write_policy(
        &repo,
        &["shape.apply", "shape.approve.R1", "shape.invalidate"],
    );

    // Prepare content + git baseline + shaping receipt.
    let rel = format!("{content_dir}/ticket.md");
    let path = repo.path().join(&rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"# Ticket\ncontract content").unwrap();
    let content_hash = hash_bytes(&fs::read(&path).unwrap());
    let source_commit = commit_all(repo.path());
    let manifest = pulse::evidence::bootstrap(repo.path()).unwrap().manifest;
    let receipt = ReceiptEnvelope {
        schema_version: 1,
        receipt_version: 1,
        id: "rcpt_01J00000000000000000000020".to_string(),
        kind: ReceiptKind::ShapingValidation,
        result: ReceiptResult::Passed,
        actor: ActorRef {
            kind: ActorKind::Human,
            id: "tester".to_string(),
        },
        recorded_at: Utc::now(),
        subject: SubjectRef {
            kind: "work".to_string(),
            id: id.clone(),
        },
        bindings: ReceiptBindings {
            work: vec![WorkBinding {
                id: id.clone(),
                revision,
            }],
            source: Some(SourceBinding {
                kind: "git_commit".to_string(),
                commit: source_commit,
                repository_id: manifest.repository_id.clone(),
            }),
            content: vec![ContentBinding {
                path: rel.clone(),
                sha256: content_hash,
            }],
            artifacts: vec![],
            graph_fingerprint_observed: None,
        },
        payload: ReceiptPayload::ShapingValidation(ShapingValidationPayload {
            payload_version: 1,
            owning_work: ShapingWorkBinding {
                id: id.clone(),
                revision_observed: revision,
                contract_revision,
            },
            materialization: "R1".to_string(),
            shape_mode: ShapeMode::FocusedBranches,
            source_posture: SourcePosture::CleanGitCommit,
            destination: None,
            map: None,
            affected_work: vec![],
            branches: vec![],
            fog: vec![],
            out_of_scope: vec![],
            resolution_pointers: vec![],
            approval: ShapingApproval {
                approved_by: ActorRef {
                    kind: ActorKind::Human,
                    id: "tester".to_string(),
                },
                reference: "PULSE.md#human-judgment-boundaries".to_string(),
            },
            reconciliation: None,
            remaining_uncertainty: vec![],
        }),
    };
    let receipt_file = repo.path().join("receipt.json");
    write_json(&receipt_file, &receipt);
    run_ok(
        &repo,
        &[
            "evidence",
            "receipt",
            "record",
            "--file",
            receipt_file.to_str().unwrap(),
            "--json",
        ],
    );

    let applied = run_ok(
        &repo,
        &[
            "work",
            "shaping",
            "apply",
            &id,
            "--receipt",
            "rcpt_01J00000000000000000000020",
            "--expected-revision",
            &revision.to_string(),
            "--actor",
            "human:tester",
            "--json",
        ],
    );
    assert_eq!(applied["code"], "applied");
    // Pointer-only: contract_revision unchanged.
    assert_eq!(applied["value"]["contract_revision"], contract_revision);

    let shown = run_ok(&repo, &["work", "shaping", "show", &id, "--json"]);
    assert_eq!(shown["code"], "ok");
    assert_eq!(
        shown["shaping"]["receipt"]["id"],
        "rcpt_01J00000000000000000000020"
    );

    let invalidated = run_ok(
        &repo,
        &[
            "work",
            "shaping",
            "invalidate",
            &id,
            "--expected-revision",
            &(revision + 1).to_string(),
            "--reason",
            "inputs changed",
            "--actor",
            "human:tester",
            "--json",
        ],
    );
    assert_eq!(invalidated["code"], "invalidated");
}
