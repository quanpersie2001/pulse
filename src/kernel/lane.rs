//! Lane output validation and sealing (plan 0022 §8.4).
//!
//! A lane writes `.pulse/evidence/<id>/<role>.json` and prints
//! `{"status":"done"}`; this reads that file, applies the seal-time MUST
//! rules, and records the one `lane` receipt.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{PulseError, Result};
use crate::event::emit_event;
use crate::evidence::receipt::{
    record_receipt, NewReceipt, ReceiptEnvelope, ReceiptSource, ReceiptSubject,
};
use crate::kernel::issues::{apply_to_record, bump, require};
use crate::source::{self, Source};
use crate::store::issues;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceResult {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub how: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaseResult {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub observation: String,
    #[serde(default)]
    pub artifacts: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckSpec {
    pub argv: Vec<String>,
    pub exit: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub id: String,
    #[serde(default, rename = "ref")]
    pub reference: String,
    pub summary: String,
    pub owner: String,
    #[serde(default)]
    pub check: Option<CheckSpec>,
    pub severity: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommandRun {
    pub argv: Vec<String>,
    pub exit: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    pub commit: String,
    #[serde(default)]
    pub server: Option<String>,
    #[serde(default)]
    pub tool: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LaneOutput {
    pub verdict: String,
    #[serde(default)]
    pub acceptance: Vec<AcceptanceResult>,
    #[serde(default)]
    pub cases: Vec<CaseResult>,
    #[serde(default)]
    pub findings: Vec<Finding>,
    #[serde(default)]
    pub commands_run: Vec<CommandRun>,
    pub environment: Environment,
}

const IMAGE_EXTENSIONS: [&str; 3] = [".png", ".jpg", ".jpeg"];

fn evidence_dir(repo_root: &Path, ticket_id: &str) -> std::path::PathBuf {
    repo_root.join(".pulse/evidence").join(ticket_id)
}

fn artifact_exists(repo_root: &Path, ticket_id: &str, relative: &str) -> bool {
    evidence_dir(repo_root, ticket_id).join(relative).exists()
}

/// Apply plan §8.4's seal-time corrections in place. Returns whether the
/// verdict was corrected from `pass` (for the `lane_verdict_corrected`
/// event).
fn apply_seal_corrections(
    output: &mut LaneOutput,
    repo_root: &Path,
    ticket_id: &str,
    role: &str,
) -> bool {
    let mut corrected = false;

    if role.starts_with("qa-ui") {
        for case in &mut output.cases {
            if case.status == "pass" {
                let has_image = case.artifacts.iter().any(|artifact| {
                    IMAGE_EXTENSIONS.iter().any(|ext| artifact.ends_with(ext))
                        && artifact_exists(repo_root, ticket_id, artifact)
                });
                if !has_image {
                    case.status = "inconclusive".to_string();
                }
            }
        }
    } else if role.starts_with("qa-api") {
        for case in &mut output.cases {
            if case.status == "pass" {
                let has_artifact = case
                    .artifacts
                    .iter()
                    .any(|artifact| artifact_exists(repo_root, ticket_id, artifact));
                if !has_artifact {
                    case.status = "inconclusive".to_string();
                }
            }
        }
    }

    // The two MUST rules below key off the *originally reported* verdict,
    // not each other — a `pass` corrected to `fail` (because an acceptance
    // failed) must not then also be downgraded to `inconclusive` by the
    // "fail without a checkable finding" rule, which is about a worker that
    // itself reported failure without evidence, a different situation.
    let original_verdict = output.verdict.clone();

    if original_verdict == "pass"
        && output
            .cases
            .iter()
            .any(|case| case.status == "inconclusive")
    {
        output.verdict = "inconclusive".to_string();
    }

    let has_failed_acceptance = output.acceptance.iter().any(|ac| ac.status == "fail");
    let has_open_high_finding = output
        .findings
        .iter()
        .any(|f| f.severity == "high" && f.status == "open");
    if original_verdict == "pass" && (has_failed_acceptance || has_open_high_finding) {
        output.verdict = "fail".to_string();
        corrected = true;
    }

    let has_any_check = output.findings.iter().any(|f| f.check.is_some());
    if original_verdict == "fail" && !has_any_check {
        output.verdict = "inconclusive".to_string();
    }

    corrected
}

/// Validate `.pulse/evidence/<ticket_id>/<role>.json` and seal the lane
/// receipt.
///
/// # Errors
/// `lane_mutated_workspace` if the tree changed since `dirty_hash_before`
/// (no receipt). `lane_output_missing`/`lane_output_invalid` if the file is
/// absent or fails schema. `lane_commit_mismatch` if `environment.commit`
/// is not current HEAD (no receipt).
pub fn validate_and_seal(
    repo_root: &Path,
    ticket_id: &str,
    role: &str,
    before: &Source,
    fence_ignore: &[String],
) -> Result<ReceiptEnvelope> {
    let now_source = source::snapshot(repo_root, fence_ignore)?;
    if now_source.dirty_hash != before.dirty_hash {
        return Err(PulseError::kernel(
            "lane_mutated_workspace",
            format!(
                "the tree changed while {role} ran: dirty paths now {:?}",
                now_source.dirty_paths
            ),
            "a lane may only write under .pulse/evidence/<id>/; nothing else may change",
        ));
    }

    let output_path = evidence_dir(repo_root, ticket_id).join(format!("{role}.json"));
    let bytes = fs::read(&output_path).map_err(|_| {
        PulseError::kernel(
            "lane_output_missing",
            format!("{} does not exist", output_path.display()),
            "the lane must write its output before printing {\"status\":\"done\"}",
        )
    })?;
    let mut output: LaneOutput = serde_json::from_slice(&bytes).map_err(|error| {
        PulseError::kernel(
            "lane_output_invalid",
            format!("{} fails schema: {error}", output_path.display()),
            "see plan 0022 §8.4 for the lane output shape",
        )
    })?;

    if output.environment.commit != now_source.commit {
        return Err(PulseError::kernel(
            "lane_commit_mismatch",
            format!(
                "environment.commit {} does not match HEAD {}",
                output.environment.commit, now_source.commit
            ),
            "the lane must record the commit it actually ran against",
        ));
    }

    let corrected = apply_seal_corrections(&mut output, repo_root, ticket_id, role);

    let artifact_paths: Vec<String> = output
        .cases
        .iter()
        .flat_map(|case| case.artifacts.iter())
        .map(|artifact| format!(".pulse/evidence/{ticket_id}/{artifact}"))
        .collect();

    let receipt = record_receipt(
        repo_root,
        None,
        NewReceipt {
            kind: "lane".to_string(),
            subject: ReceiptSubject {
                id: ticket_id.to_string(),
                revision: None,
            },
            actor: format!("agent:{role}"),
            source: ReceiptSource {
                commit: now_source.commit.clone(),
                dirty_hash: now_source.dirty_hash.clone(),
            },
            run_id: None,
            payload: json!({
                "role": role,
                "verdict": output.verdict,
                "acceptance": output.acceptance,
                "cases": output.cases,
                "findings": output.findings,
                "commands_run": output.commands_run,
                "environment": output.environment,
            }),
            artifact_paths,
        },
    )?;

    let records = issues::read_all(repo_root)?;
    if find_ticket(&records, ticket_id).is_some() {
        issues::mutate(repo_root, |records| {
            apply_to_record(records, ticket_id, |record| {
                let object = record.as_object_mut().expect("records are always objects");
                let verdicts = object
                    .entry("verdicts")
                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                verdicts.as_object_mut().expect("verdicts is always an object").insert(
                    role.to_string(),
                    json!({"receipt": receipt.id, "verdict": output.verdict, "commit": now_source.commit}),
                );
                bump(object);
                Ok(())
            })
        })?;
    }

    if corrected {
        emit_event(
            repo_root,
            "receipt.recorded",
            format!("agent:{role}"),
            ticket_id,
            json!({"lane_verdict_corrected": true, "role": role}),
            chrono::Utc::now(),
        )?;
    }

    Ok(receipt)
}

fn find_ticket<'a>(records: &'a [Value], id: &str) -> Option<&'a Value> {
    require(records, id).ok()
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
        issues::mutate(dir.path(), |mut records| {
            records.push(json!({
                "schema": 3, "id": "TK-a3f9", "kind": "ticket", "title": "t",
                "status": "verifying", "revision": 1,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "role": "implementation",
            }));
            Ok(records)
        })
        .unwrap();
        dir
    }

    fn write_lane_output(repo: &Path, ticket_id: &str, role: &str, output: &Value) {
        let dir = evidence_dir(repo, ticket_id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(format!("{role}.json")),
            serde_json::to_vec(output).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn lane_that_mutates_workspace_gets_no_receipt() {
        let repo = git_repo();
        let before = source::snapshot(repo.path(), &[]).unwrap();
        write_lane_output(
            repo.path(),
            "TK-a3f9",
            "review-correctness",
            &json!({
                "verdict": "pass", "acceptance": [], "cases": [], "findings": [], "commands_run": [],
                "environment": {"commit": before.commit},
            }),
        );
        // The lane also touched a tracked file outside .pulse/evidence/ — a
        // workspace mutation the seal must catch.
        std::fs::write(repo.path().join("README.md"), "mutated\n").unwrap();

        let err = validate_and_seal(repo.path(), "TK-a3f9", "review-correctness", &before, &[])
            .unwrap_err();
        assert_eq!(err.code(), "lane_mutated_workspace");
        assert!(record_receipt_count(repo.path()) == 0);
    }

    fn record_receipt_count(repo_root: &Path) -> usize {
        crate::evidence::receipt::list_receipts(repo_root)
            .unwrap()
            .receipts
            .len()
    }

    #[test]
    fn qa_ui_pass_without_screenshot_is_inconclusive() {
        let repo = git_repo();
        let before = source::snapshot(repo.path(), &[]).unwrap();
        write_lane_output(
            repo.path(),
            "TK-a3f9",
            "qa-ui",
            &json!({
                "verdict": "pass",
                "acceptance": [],
                "cases": [{"id": "QA-001", "status": "pass", "observation": "looks fine", "artifacts": []}],
                "findings": [],
                "commands_run": [],
                "environment": {"commit": before.commit},
            }),
        );
        let receipt = validate_and_seal(repo.path(), "TK-a3f9", "qa-ui", &before, &[]).unwrap();
        assert_eq!(receipt.payload["verdict"], "inconclusive");
        assert_eq!(receipt.payload["cases"][0]["status"], "inconclusive");
    }

    #[test]
    fn qa_ui_pass_with_a_real_screenshot_stays_pass() {
        let repo = git_repo();
        let evidence = evidence_dir(repo.path(), "TK-a3f9").join("shots");
        fs::create_dir_all(&evidence).unwrap();
        fs::write(evidence.join("QA-001.png"), b"fake png").unwrap();
        let before = source::snapshot(repo.path(), &[]).unwrap();
        write_lane_output(
            repo.path(),
            "TK-a3f9",
            "qa-ui",
            &json!({
                "verdict": "pass",
                "acceptance": [],
                "cases": [{"id": "QA-001", "status": "pass", "observation": "ok", "artifacts": ["shots/QA-001.png"]}],
                "findings": [],
                "commands_run": [],
                "environment": {"commit": before.commit},
            }),
        );
        let receipt = validate_and_seal(repo.path(), "TK-a3f9", "qa-ui", &before, &[]).unwrap();
        assert_eq!(receipt.payload["verdict"], "pass");
    }

    #[test]
    fn pass_with_a_failed_acceptance_is_corrected_to_fail() {
        let repo = git_repo();
        let before = source::snapshot(repo.path(), &[]).unwrap();
        write_lane_output(
            repo.path(),
            "TK-a3f9",
            "review-correctness",
            &json!({
                "verdict": "pass",
                "acceptance": [{"id": "AC-1", "status": "fail", "how": "it does not"}],
                "cases": [], "findings": [], "commands_run": [],
                "environment": {"commit": before.commit},
            }),
        );
        let receipt =
            validate_and_seal(repo.path(), "TK-a3f9", "review-correctness", &before, &[]).unwrap();
        assert_eq!(receipt.payload["verdict"], "fail");
    }

    #[test]
    fn fail_without_any_checkable_finding_is_corrected_to_inconclusive() {
        let repo = git_repo();
        let before = source::snapshot(repo.path(), &[]).unwrap();
        write_lane_output(
            repo.path(),
            "TK-a3f9",
            "review-correctness",
            &json!({
                "verdict": "fail",
                "acceptance": [],
                "cases": [],
                "findings": [{"id": "F-1", "summary": "vague", "owner": "src/x.rs", "severity": "low", "status": "open"}],
                "commands_run": [],
                "environment": {"commit": before.commit},
            }),
        );
        let receipt =
            validate_and_seal(repo.path(), "TK-a3f9", "review-correctness", &before, &[]).unwrap();
        assert_eq!(receipt.payload["verdict"], "inconclusive");
    }

    #[test]
    fn commit_mismatch_is_refused_without_a_receipt() {
        let repo = git_repo();
        let before = source::snapshot(repo.path(), &[]).unwrap();
        write_lane_output(
            repo.path(),
            "TK-a3f9",
            "review-correctness",
            &json!({
                "verdict": "pass", "acceptance": [], "cases": [], "findings": [], "commands_run": [],
                "environment": {"commit": "0".repeat(40)},
            }),
        );
        let err = validate_and_seal(repo.path(), "TK-a3f9", "review-correctness", &before, &[])
            .unwrap_err();
        assert_eq!(err.code(), "lane_commit_mismatch");
        assert_eq!(record_receipt_count(repo.path()), 0);
    }

    #[test]
    fn a_clean_pass_updates_the_ticket_verdicts_map() {
        let repo = git_repo();
        let before = source::snapshot(repo.path(), &[]).unwrap();
        write_lane_output(
            repo.path(),
            "TK-a3f9",
            "review-correctness",
            &json!({
                "verdict": "pass", "acceptance": [], "cases": [], "findings": [], "commands_run": [],
                "environment": {"commit": before.commit},
            }),
        );
        validate_and_seal(repo.path(), "TK-a3f9", "review-correctness", &before, &[]).unwrap();
        let records = issues::read_all(repo.path()).unwrap();
        let ticket = require(&records, "TK-a3f9").unwrap();
        assert_eq!(ticket["verdicts"]["review-correctness"]["verdict"], "pass");
    }
}
