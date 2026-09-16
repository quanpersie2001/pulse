//! Lane output validation and sealing (plan 0022 §8.4).
//!
//! A lane writes `.pulse/evidence/<id>/<role>.json` and prints
//! `{"status":"done"}`; this reads that file, applies the seal-time MUST
//! rules, and records the one `lane` receipt.

use std::fs;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{PulseError, Result};
use crate::event::emit_event;
use crate::evidence::receipt::{
    list_receipts, record_receipt, NewReceipt, ReceiptEnvelope, ReceiptSource, ReceiptSubject,
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
    /// `None` for a command that was still running when the report was
    /// written — the QA lanes' detached `start` (A8.1) has no exit code to
    /// report; every other entry carries the real exit code.
    pub exit: Option<i64>,
    /// `Some(true)` only for a `start` spawned into its own process group
    /// and never awaited; omitted (not serialized) for ordinary runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detached: Option<bool>,
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

/// Build `.pulse/runtime/run/<id>/<role>-input.json` (plan §8.3): a lane
/// receives the claim to check, never the worker's narrative. `ticket` may
/// itself be a Story record (a qa-* lane run at story scope); `story` is
/// the parent Story otherwise, when known.
///
/// # Errors
/// Propagates a receipt-listing I/O error.
pub fn lane_input(
    repo_root: &Path,
    ticket: &Value,
    story: Option<&Value>,
    role: &str,
) -> Result<Value> {
    let id = ticket.get("id").and_then(Value::as_str).unwrap_or_default();
    let handoff_commit = latest_handoff_commit(repo_root, id)?;

    let mut object = serde_json::Map::new();
    object.insert(
        "id".to_string(),
        ticket.get("id").cloned().unwrap_or(Value::Null),
    );
    object.insert(
        "title".to_string(),
        ticket.get("title").cloned().unwrap_or(Value::Null),
    );
    object.insert(
        "objective".to_string(),
        ticket.get("objective").cloned().unwrap_or(Value::Null),
    );
    object.insert(
        "change".to_string(),
        ticket.get("change").cloned().unwrap_or_else(|| json!({})),
    );
    object.insert(
        "acceptance".to_string(),
        ticket
            .get("acceptance")
            .cloned()
            .unwrap_or_else(|| json!([])),
    );
    object.insert(
        "verify".to_string(),
        ticket.get("verify").cloned().unwrap_or_else(|| json!([])),
    );
    object.insert(
        "non_scope".to_string(),
        ticket
            .get("non_scope")
            .cloned()
            .unwrap_or_else(|| json!([])),
    );
    object.insert(
        "changed_files".to_string(),
        json!(changed_files_since(repo_root, &handoff_commit)),
    );
    object.insert(
        "evidence_dir".to_string(),
        json!(format!(".pulse/evidence/{id}")),
    );
    object.insert("handoff_commit".to_string(), json!(handoff_commit));

    if role == "review-adversarial" {
        object.insert(
            "story".to_string(),
            json!({
                "rules": story.and_then(|s| s.get("rules")).cloned().unwrap_or_else(|| json!([])),
                "exceptions": story.and_then(|s| s.get("exceptions")).cloned().unwrap_or_else(|| json!([])),
            }),
        );
    }

    if role.starts_with("qa-") {
        object.insert(
            "qa_cases".to_string(),
            json!(resolve_qa_cases(ticket, story)),
        );
        if role.starts_with("qa-ui") {
            object.insert("viewports".to_string(), json!(["1280x800", "375x812"]));
        }
    }

    Ok(Value::Object(object))
}

/// The commit the most recent `handoff` receipt for `ticket_id` recorded,
/// or empty when none exists yet (a story-scope qa run has no handoff of
/// its own).
fn latest_handoff_commit(repo_root: &Path, ticket_id: &str) -> Result<String> {
    let receipts = list_receipts(repo_root)?.receipts;
    Ok(receipts
        .iter()
        .filter(|r| r.kind == "handoff" && r.subject.id == ticket_id)
        .max_by(|a, b| a.id.cmp(&b.id))
        .map(|r| r.source.commit.clone())
        .unwrap_or_default())
}

/// `git diff --name-only <handoff_commit>` plus `git status --porcelain`,
/// filtered to drop `.pulse/**` (plan §8.3).
fn changed_files_since(repo_root: &Path, handoff_commit: &str) -> Vec<String> {
    let mut files = std::collections::BTreeSet::new();
    if !handoff_commit.is_empty() {
        if let Ok(output) = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(["diff", "--name-only", handoff_commit])
            .output()
        {
            if output.status.success() {
                files.extend(
                    String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .map(str::to_string),
                );
            }
        }
    }
    if let Ok(output) = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["status", "--porcelain"])
        .output()
    {
        if output.status.success() {
            files.extend(
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .filter_map(|line| line.get(3..))
                    .map(|path| path.trim_matches('"').to_string()),
            );
        }
    }
    files
        .into_iter()
        .filter(|path| !path.starts_with(".pulse/"))
        .collect()
}

/// Resolve `qa_cases` per plan §8.3: every case in the Story when `ticket`
/// is itself a Story record (story-scope qa run), otherwise only the cases
/// `ticket.qa_cases` names.
fn resolve_qa_cases(ticket: &Value, story: Option<&Value>) -> Vec<Value> {
    let is_story = ticket.get("kind").and_then(Value::as_str) == Some("story");
    let Some(story) = (if is_story { Some(ticket) } else { story }) else {
        return Vec::new();
    };
    let all_cases: Vec<Value> = story
        .get("qa_cases")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if is_story {
        return all_cases;
    }
    let wanted: Vec<&str> = ticket
        .get("qa_cases")
        .and_then(Value::as_array)
        .map(|ids| ids.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    all_cases
        .into_iter()
        .filter(|case| {
            case.get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| wanted.contains(&id))
        })
        .collect()
}

/// Validate `.pulse/evidence/<ticket_id>/<role>.json` and seal the lane
/// receipt.
///
/// # Errors
/// `lane_mutated_workspace` if the tree changed since `dirty_hash_before`
/// (no receipt). `lane_output_invalid` if the file is absent or fails
/// schema. `lane_commit_mismatch` if `environment.commit` is not current
/// HEAD (no receipt).
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
            "lane_output_invalid",
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

    /// A8.1: the QA lanes record their detached `start` truthfully — no exit
    /// code while it is still running, plus the `detached` flag — and the
    /// lane schema must accept that shape (a report failing to parse here
    /// would turn every qa lane into `lane_output_invalid`). Ordinary
    /// entries keep a real exit and no flag.
    #[test]
    fn command_run_accepts_a_detached_start_without_an_exit_code() {
        let output: LaneOutput = serde_json::from_value(json!({
            "verdict": "inconclusive", "acceptance": [], "cases": [], "findings": [],
            "commands_run": [
                {"argv": ["python3", "-m", "http.server", "18080"], "exit": null, "detached": true},
                {"argv": ["pkill", "-f", "http.server 18080"], "exit": 0},
            ],
            "environment": {"commit": "abc1234"},
        }))
        .unwrap();
        assert_eq!(output.commands_run[0].exit, None);
        assert_eq!(output.commands_run[0].detached, Some(true));
        assert_eq!(output.commands_run[1].exit, Some(0));
        assert_eq!(output.commands_run[1].detached, None);
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
    fn lane_input_never_contains_worker_narrative() {
        let repo = git_repo();
        let ticket = json!({
            "id": "TK-a3f9", "kind": "ticket", "title": "t",
            "objective": "do the thing", "change": {"required": ["x"]},
            "acceptance": [{"id": "AC-1", "when": "x", "then": "y"}],
            "verify": [], "non_scope": [],
            "checkpoints": [{"at": "t", "run_id": "r1"}],
            "notes": [{"at": "t", "from": "human:x", "kind": "note", "text": "n"}],
            "lease": {"role": "worker", "actor": "agent:worker", "run_id": "r1", "expires_at": "t"},
            "verdicts": {"review-correctness": {"receipt": "01J", "verdict": "pass", "commit": "c"}},
            "open_questions": [{"q": "which?", "disposition": "blocking"}],
        });
        let input = lane_input(repo.path(), &ticket, None, "review-correctness").unwrap();
        let object = input.as_object().unwrap();
        for forbidden in [
            "checkpoints",
            "notes",
            "lease",
            "verdicts",
            "open_questions",
        ] {
            assert!(
                !object.contains_key(forbidden),
                "unexpected key {forbidden}"
            );
        }
        assert_eq!(input["id"], "TK-a3f9");
        assert_eq!(input["objective"], "do the thing");
    }

    #[test]
    fn adversarial_input_carries_story_rules() {
        let repo = git_repo();
        let ticket = json!({
            "id": "TK-a3f9", "kind": "ticket", "title": "t", "story": "ST-1111",
        });
        let story = json!({
            "id": "ST-1111", "kind": "story",
            "rules": [{"id": "BR-1", "text": "must not do x"}],
            "exceptions": [{"id": "E-1", "text": "unless y"}],
        });
        let input = lane_input(repo.path(), &ticket, Some(&story), "review-adversarial").unwrap();
        assert_eq!(input["story"]["rules"][0]["id"], "BR-1");
        assert_eq!(input["story"]["exceptions"][0]["id"], "E-1");
        // review-correctness never gets a `story` key at all.
        let plain = lane_input(repo.path(), &ticket, Some(&story), "review-correctness").unwrap();
        assert!(plain.get("story").is_none());
    }

    #[test]
    fn qa_input_resolves_story_cases() {
        let repo = git_repo();
        let story = json!({
            "id": "ST-1111", "kind": "story",
            "qa_cases": [
                {"id": "QA-001", "intent": "a", "priority": "high"},
                {"id": "QA-002", "intent": "b", "priority": "low"},
            ],
        });

        // Ticket scope: only the cases the Ticket names.
        let ticket = json!({
            "id": "TK-a3f9", "kind": "ticket", "title": "t", "story": "ST-1111",
            "qa_cases": ["QA-001"],
        });
        let input = lane_input(repo.path(), &ticket, Some(&story), "qa-ui").unwrap();
        let cases = input["qa_cases"].as_array().unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0]["id"], "QA-001");
        assert_eq!(input["viewports"], json!(["1280x800", "375x812"]));

        // Story scope: every case in the Story.
        let input = lane_input(repo.path(), &story, None, "qa-api").unwrap();
        let cases = input["qa_cases"].as_array().unwrap();
        assert_eq!(cases.len(), 2);
        assert!(input.get("viewports").is_none());
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
