//! Handoff, close and close-story gates (plan 0022 §7.2-7.4).
//!
//! Each gate follows `kernel::ready`'s shape: an `evaluate_*` function
//! collects every violation into a report (never short-circuits at the
//! first one, and each condition keeps its own error code so a test can
//! target it directly); the action function (`handoff`/`close`/
//! `close_story`) runs the gate and, only on a clean report, seals a
//! receipt and transitions status.
//!
//! Close gate condition 3 (plan §7.3) consolidates "no lane receipt" /
//! "lane receipt on a stale commit" / "lane verdict isn't pass" / "lane has
//! an unresolved high finding" / "lane actor is the handoff actor" into one
//! `close_lane_not_satisfied` code (message says which); the plan doesn't
//! enumerate distinct codes for this gate the way it does for the ready
//! gate's six conditions, and five near-identical codes for one row of the
//! table would cost more than it explains.

use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{PulseError, Result};
use crate::event::emit_event;
use crate::evidence::receipt::{
    list_receipts, record_receipt, NewReceipt, ReceiptSource, ReceiptSubject,
};
use crate::identity::actor::{ActorKind, ActorRef};
use crate::kernel::issues::{apply_to_record, bump, require};
use crate::kernel::profile::{self, profile_key};
use crate::kernel::roles::{authorize, Action};
use crate::source;
use crate::store::issues;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub code: &'static str,
    pub message: String,
}

fn violation(code: &'static str, message: impl Into<String>) -> Violation {
    Violation {
        code,
        message: message.into(),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GateReport {
    pub violations: Vec<Violation>,
}

impl GateReport {
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }
}

fn fail(report: &GateReport) -> PulseError {
    let messages: Vec<String> = report
        .violations
        .iter()
        .map(|v| format!("{}: {}", v.code, v.message))
        .collect();
    PulseError::kernel(
        "gate_failed",
        messages.join("; "),
        "fix every violation listed; the gate reports all of them at once",
    )
}

fn fence_ignore(repo_root: &Path) -> Vec<String> {
    profile::load(repo_root)
        .map(|config| config.fence_ignore)
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Handoff (plan §7.2)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HandoffAcceptance {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub how: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyResult {
    pub name: String,
    pub exit: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LearningUsage {
    pub id: String,
    pub usage: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HandoffInput {
    pub summary: String,
    #[serde(default)]
    pub changed_files: Vec<String>,
    #[serde(default)]
    pub acceptance: Vec<HandoffAcceptance>,
    #[serde(default)]
    pub verify_results: Vec<VerifyResult>,
    #[serde(default)]
    pub docs_updated: Vec<String>,
    #[serde(default)]
    pub learnings_used: Vec<LearningUsage>,
    #[serde(default)]
    pub friction: Vec<String>,
    #[serde(default)]
    pub open_risks: Vec<String>,
}

/// Files git considers changed against HEAD, tracked or not — what plan
/// §7.2's "nằm trong `git diff --name-only`" check reads against.
fn changed_files_in_worktree(repo_root: &Path) -> Vec<String> {
    let diff = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["diff", "--name-only", "HEAD"])
        .output()
        .ok();
    let untracked = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["ls-files", "--others", "--exclude-standard"])
        .output()
        .ok();
    let mut files = Vec::new();
    for output in [diff, untracked].into_iter().flatten() {
        if output.status.success() {
            files.extend(
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .map(str::to_string),
            );
        }
    }
    files
}

pub fn evaluate_handoff(
    repo_root: &Path,
    actor: &ActorRef,
    ticket: &Value,
    input: &HandoffInput,
) -> GateReport {
    let mut violations = Vec::new();

    let status = ticket.get("status").and_then(Value::as_str).unwrap_or("");
    if status != "active" {
        violations.push(violation(
            "handoff_not_active",
            format!("ticket is {status}, not active"),
        ));
    }

    let lease_actor = ticket.pointer("/lease/actor").and_then(Value::as_str);
    if lease_actor != Some(actor.as_kind_id().as_str()) {
        violations.push(violation(
            "handoff_lease_mismatch",
            format!(
                "lease is held by {}, not the calling actor {}",
                lease_actor.unwrap_or("<none>"),
                actor.as_kind_id()
            ),
        ));
    }

    let ticket_ac_ids: Vec<&str> = ticket
        .get("acceptance")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|ac| ac.get("id").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    let handed_off_ac_ids: std::collections::HashSet<&str> =
        input.acceptance.iter().map(|ac| ac.id.as_str()).collect();
    for id in &ticket_ac_ids {
        if !handed_off_ac_ids.contains(id) {
            violations.push(violation(
                "handoff_acceptance_missing",
                format!("acceptance {id} has no status in the handoff"),
            ));
        }
    }

    let verify_names: Vec<&str> = ticket
        .get("verify")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|v| v.get("name").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    let handed_off_verify_names: std::collections::HashSet<&str> = input
        .verify_results
        .iter()
        .map(|v| v.name.as_str())
        .collect();
    for name in &verify_names {
        if !handed_off_verify_names.contains(name) {
            violations.push(violation(
                "handoff_verify_missing",
                format!("verify {name} has no result in the handoff"),
            ));
        }
    }

    let docs_to_update: Vec<&str> = ticket
        .pointer("/change/docs_to_update")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if !docs_to_update.is_empty() {
        let docs_updated: std::collections::HashSet<&str> =
            input.docs_updated.iter().map(String::as_str).collect();
        let changed = changed_files_in_worktree(repo_root);
        for doc in &docs_to_update {
            if !docs_updated.contains(doc) {
                violations.push(violation(
                    "handoff_documentation_missing",
                    format!("{doc} is required by change.docs_to_update but not in docs_updated"),
                ));
            } else if !changed.iter().any(|f| f == doc) {
                violations.push(violation(
                    "handoff_documentation_not_diffed",
                    format!("{doc} is claimed as updated but git sees no change to it"),
                ));
            }
        }
    }

    for used in &input.learnings_used {
        if !crate::learn::store::exists(repo_root, &used.id) {
            violations.push(violation(
                "learning_unknown",
                format!("{} is not a known learning id", used.id),
            ));
        } else if !matches!(used.usage.as_str(), "helpful" | "not_needed" | "misleading") {
            violations.push(violation(
                "handoff_learning_usage_invalid",
                format!(
                    "{}: usage must be helpful, not_needed or misleading, got {}",
                    used.id, used.usage
                ),
            ));
        }
    }

    GateReport { violations }
}

/// # Errors
/// `role_forbidden` if `actor` may not checkpoint/handoff; `gate_failed`
/// listing every [`evaluate_handoff`] violation otherwise.
pub fn handoff(repo_root: &Path, actor: &ActorRef, id: &str, input: HandoffInput) -> Result<Value> {
    authorize(actor, Action::CheckpointOrHandoff)?;
    let records = issues::read_all(repo_root)?;
    let ticket = require(&records, id)?;
    let report = evaluate_handoff(repo_root, actor, ticket, &input);
    if !report.is_clean() {
        return Err(fail(&report));
    }

    let revision = ticket.get("revision").and_then(Value::as_u64);
    let source_snapshot = source::snapshot(repo_root, &fence_ignore(repo_root))?;
    record_receipt(
        repo_root,
        None,
        NewReceipt {
            kind: "handoff".to_string(),
            subject: ReceiptSubject {
                id: id.to_string(),
                revision,
            },
            actor: actor.as_kind_id(),
            source: ReceiptSource {
                commit: source_snapshot.commit.clone(),
                dirty_hash: source_snapshot.dirty_hash.clone(),
            },
            run_id: None,
            payload: serde_json::to_value(&input)?,
            artifact_paths: Vec::new(),
        },
    )?;

    issues::mutate(repo_root, |records| {
        apply_to_record(records, id, |record| {
            let object = record.as_object_mut().expect("records are always objects");
            object.insert("status".to_string(), Value::String("verifying".to_string()));
            object.insert("lease".to_string(), Value::Null);
            bump(object);
            Ok(())
        })
    })?;
    emit_event(
        repo_root,
        "issue.transitioned",
        actor.as_kind_id(),
        id,
        serde_json::json!({"to": "verifying"}),
        Utc::now(),
    )?;

    for line in &input.friction {
        crate::kernel::issues::append_note(
            repo_root,
            actor,
            id,
            line,
            crate::kernel::issues::NoteKind::Friction,
        )?;
    }

    for used in &input.learnings_used {
        crate::learn::record_usage(repo_root, &used.id, &used.usage)?;
    }

    // Each friction note is its own read-mutate-write cycle (kernel::issues
    // owns note-appending independently), so re-read rather than trust the
    // `saved` snapshot from before the loop.
    let final_records = issues::read_all(repo_root)?;
    Ok(require(&final_records, id)?.clone())
}

// ---------------------------------------------------------------------------
// Close (plan §7.3)
// ---------------------------------------------------------------------------

fn latest_receipt<'a>(
    receipts: &'a [crate::evidence::receipt::ReceiptEnvelope],
    kind: &str,
    subject_id: &str,
) -> Option<&'a crate::evidence::receipt::ReceiptEnvelope> {
    receipts
        .iter()
        .filter(|r| r.kind == kind && r.subject.id == subject_id)
        .max_by(|a, b| a.id.cmp(&b.id))
}

fn has_blocking_open_question(ticket: &Value) -> bool {
    ticket
        .get("open_questions")
        .and_then(Value::as_array)
        .map(|questions| {
            questions.iter().any(|q| {
                !matches!(
                    q.get("disposition").and_then(Value::as_str),
                    Some("resolved" | "rejected" | "delegated" | "deferred")
                )
            })
        })
        .unwrap_or(false)
}

pub fn evaluate_close(repo_root: &Path, actor: &ActorRef, ticket: &Value) -> Result<GateReport> {
    let mut violations = Vec::new();
    let id = ticket.get("id").and_then(Value::as_str).unwrap_or("");

    let status = ticket.get("status").and_then(Value::as_str).unwrap_or("");
    if status != "verifying" {
        violations.push(violation(
            "close_not_verifying",
            format!("ticket is {status}, not verifying"),
        ));
    }

    let receipts = list_receipts(repo_root)?.receipts;
    let handoff_receipt = latest_receipt(&receipts, "handoff", id);
    let Some(handoff_receipt) = handoff_receipt else {
        violations.push(violation(
            "close_handoff_missing",
            "no handoff receipt exists for this ticket",
        ));
        return Ok(GateReport { violations });
    };

    let now_source = source::snapshot(repo_root, &fence_ignore(repo_root))?;
    if now_source.commit != handoff_receipt.source.commit
        || now_source.dirty_hash != handoff_receipt.source.dirty_hash
    {
        violations.push(violation(
            "close_source_stale",
            format!(
                "tree changed since handoff: dirty paths now {:?}",
                now_source.dirty_paths
            ),
        ));
    }

    let role = ticket
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("implementation");
    let surface = ticket.get("surface").and_then(Value::as_str);
    let risk = ticket.get("risk").and_then(Value::as_str);
    let key = profile_key(role, surface, risk);
    let config = profile::load(repo_root)?;
    let profile = profile::profile_for(&config, &key)?;

    for lane in &profile.lanes {
        let lane_receipt = receipts
            .iter()
            .filter(|r| {
                r.kind == "lane"
                    && r.subject.id == id
                    && r.payload.get("role").and_then(Value::as_str) == Some(lane)
            })
            .max_by(|a, b| a.id.cmp(&b.id));
        match lane_receipt {
            None => violations.push(violation(
                "close_lane_not_satisfied",
                format!("{lane}: no lane receipt exists"),
            )),
            Some(receipt) => {
                let verdict = receipt.payload.get("verdict").and_then(Value::as_str);
                let unresolved_high = receipt
                    .payload
                    .get("findings")
                    .and_then(Value::as_array)
                    .map(|findings| {
                        findings.iter().any(|f| {
                            f.get("severity").and_then(Value::as_str) == Some("high")
                                && f.get("status").and_then(Value::as_str) != Some("resolved")
                        })
                    })
                    .unwrap_or(false);
                if receipt.source.commit != handoff_receipt.source.commit {
                    violations.push(violation(
                        "close_lane_not_satisfied",
                        format!(
                            "{lane}: receipt is on commit {}, handoff is on {}",
                            receipt.source.commit, handoff_receipt.source.commit
                        ),
                    ));
                } else if receipt.actor == handoff_receipt.actor {
                    violations.push(violation(
                        "close_lane_not_satisfied",
                        format!(
                            "{lane}: receipt actor {} is the same as the handoff actor",
                            receipt.actor
                        ),
                    ));
                } else if verdict != Some("pass") {
                    violations.push(violation(
                        "close_lane_not_satisfied",
                        format!(
                            "{lane}: verdict is {}, not pass",
                            verdict.unwrap_or("<none>")
                        ),
                    ));
                } else if unresolved_high {
                    violations.push(violation(
                        "close_lane_not_satisfied",
                        format!("{lane}: has an unresolved high-severity finding"),
                    ));
                }
            }
        }
    }

    if profile.human_required() && actor.kind != ActorKind::Human {
        violations.push(violation(
            "close_human_required",
            "this profile requires a human actor to close",
        ));
    }

    if has_blocking_open_question(ticket) {
        violations.push(violation(
            "close_question_blocking",
            "an open question is blocking or missing a disposition",
        ));
    }

    Ok(GateReport { violations })
}

/// # Errors
/// `role_forbidden` if `actor` may not close; `gate_failed` listing every
/// [`evaluate_close`] violation (plus the human-required check, which needs
/// the calling actor) otherwise.
pub fn close(repo_root: &Path, actor: &ActorRef, id: &str) -> Result<Value> {
    authorize(actor, Action::Close)?;
    let records = issues::read_all(repo_root)?;
    let ticket = require(&records, id)?;
    let report = evaluate_close(repo_root, actor, ticket)?;
    if !report.is_clean() {
        return Err(fail(&report));
    }

    let now_source = source::snapshot(repo_root, &fence_ignore(repo_root))?;
    let revision = ticket.get("revision").and_then(Value::as_u64);
    record_receipt(
        repo_root,
        None,
        NewReceipt {
            kind: "close".to_string(),
            subject: ReceiptSubject {
                id: id.to_string(),
                revision,
            },
            actor: actor.as_kind_id(),
            source: ReceiptSource {
                commit: now_source.commit,
                dirty_hash: now_source.dirty_hash,
            },
            run_id: None,
            payload: serde_json::json!({}),
            artifact_paths: Vec::new(),
        },
    )?;

    let saved = issues::mutate(repo_root, |records| {
        apply_to_record(records, id, |record| {
            let object = record.as_object_mut().expect("records are always objects");
            object.insert("status".to_string(), Value::String("done".to_string()));
            bump(object);
            Ok(())
        })
    })?;
    emit_event(
        repo_root,
        "issue.transitioned",
        actor.as_kind_id(),
        id,
        serde_json::json!({"to": "done"}),
        Utc::now(),
    )?;
    Ok(require(&saved, id)?.clone())
}

// ---------------------------------------------------------------------------
// Close-story (plan §7.4)
// ---------------------------------------------------------------------------

pub fn evaluate_close_story(
    repo_root: &Path,
    story: &Value,
    all_records: &[Value],
) -> Result<GateReport> {
    let mut violations = Vec::new();
    let story_id = story.get("id").and_then(Value::as_str).unwrap_or("");

    let children: Vec<&Value> = all_records
        .iter()
        .filter(|record| {
            record.get("kind").and_then(Value::as_str) == Some("ticket")
                && record.get("story").and_then(Value::as_str) == Some(story_id)
        })
        .collect();
    let done_count = children
        .iter()
        .filter(|child| child.get("status").and_then(Value::as_str) == Some("done"))
        .count();
    let incomplete: Vec<&str> = children
        .iter()
        .filter(|child| {
            !matches!(
                child.get("status").and_then(Value::as_str),
                Some("done" | "cancelled")
            )
        })
        .filter_map(|child| child.get("id").and_then(Value::as_str))
        .collect();
    if !incomplete.is_empty() {
        violations.push(violation(
            "close_story_children_incomplete",
            format!("tickets not done|cancelled: {}", incomplete.join(", ")),
        ));
    }
    if done_count == 0 {
        violations.push(violation(
            "close_story_children_incomplete",
            "no child ticket is done",
        ));
    }

    let high_priority_cases: Vec<&str> = story
        .get("qa_cases")
        .and_then(Value::as_array)
        .map(|cases| {
            cases
                .iter()
                .filter(|case| case.get("priority").and_then(Value::as_str) == Some("high"))
                .filter_map(|case| case.get("id").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    if !high_priority_cases.is_empty() {
        let receipts = list_receipts(repo_root)?.receipts;
        let qa_receipt = receipts
            .iter()
            .filter(|r| {
                r.kind == "lane"
                    && r.subject.id == story_id
                    && r.payload
                        .get("role")
                        .and_then(Value::as_str)
                        .is_some_and(|role| role.starts_with("qa-"))
                    && r.payload.get("verdict").and_then(Value::as_str) == Some("pass")
            })
            .max_by(|a, b| a.id.cmp(&b.id));
        match qa_receipt {
            None => violations.push(violation(
                "close_story_qa_not_satisfied",
                "no passing story-scope qa lane receipt covers the high-priority qa_cases",
            )),
            Some(receipt) => {
                let covered: std::collections::HashSet<&str> = receipt
                    .payload
                    .get("cases")
                    .and_then(Value::as_array)
                    .map(|cases| {
                        cases
                            .iter()
                            .filter(|c| c.get("status").and_then(Value::as_str) == Some("pass"))
                            .filter_map(|c| c.get("id").and_then(Value::as_str))
                            .collect()
                    })
                    .unwrap_or_default();
                for case in &high_priority_cases {
                    if !covered.contains(case) {
                        violations.push(violation(
                            "close_story_qa_not_satisfied",
                            format!("{case} is not covered by a passing case in the qa receipt"),
                        ));
                    }
                }
            }
        }
    }

    Ok(GateReport { violations })
}

/// # Errors
/// `role_forbidden` if `actor` may not close; `gate_failed` listing every
/// [`evaluate_close_story`] violation otherwise.
pub fn close_story(repo_root: &Path, actor: &ActorRef, id: &str) -> Result<Value> {
    authorize(actor, Action::Close)?;
    let records = issues::read_all(repo_root)?;
    let story = require(&records, id)?;
    let report = evaluate_close_story(repo_root, story, &records)?;
    if !report.is_clean() {
        return Err(fail(&report));
    }

    let revision = story.get("revision").and_then(Value::as_u64);
    record_receipt(
        repo_root,
        None,
        NewReceipt {
            kind: "close_story".to_string(),
            subject: ReceiptSubject {
                id: id.to_string(),
                revision,
            },
            actor: actor.as_kind_id(),
            source: ReceiptSource {
                commit: source::head_commit(repo_root).unwrap_or_default(),
                dirty_hash: String::new(),
            },
            run_id: None,
            payload: serde_json::json!({}),
            artifact_paths: Vec::new(),
        },
    )?;

    let saved = issues::mutate(repo_root, |records| {
        apply_to_record(records, id, |record| {
            let object = record.as_object_mut().expect("records are always objects");
            object.insert("status".to_string(), Value::String("done".to_string()));
            bump(object);
            Ok(())
        })
    })?;
    emit_event(
        repo_root,
        "issue.transitioned",
        actor.as_kind_id(),
        id,
        serde_json::json!({"to": "done"}),
        Utc::now(),
    )?;
    Ok(require(&saved, id)?.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::process::Command as StdCommand;

    fn human(id: &str) -> ActorRef {
        ActorRef {
            kind: ActorKind::Human,
            id: id.to_string(),
        }
    }

    fn agent(id: &str) -> ActorRef {
        ActorRef {
            kind: ActorKind::Agent,
            id: id.to_string(),
        }
    }

    const PULSE_MD: &str = "\
profiles:
  cli-low: {lanes: [review-correctness]}
  api-high: {lanes: [review-correctness], human: required}
";

    fn git_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            let status = StdCommand::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .status()
                .unwrap();
            assert!(status.success());
        };
        std::fs::write(dir.path().join("PULSE.md"), PULSE_MD).unwrap();
        std::fs::create_dir_all(dir.path().join("docs")).unwrap();
        std::fs::write(dir.path().join("docs/x.md"), "x\n").unwrap();
        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "test"]);
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "init"]);
        dir
    }

    fn active_ticket() -> Value {
        json!({
            "schema": 3, "id": "TK-a3f9", "kind": "ticket", "title": "t",
            "status": "active", "revision": 1,
            "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
            "role": "implementation", "risk": "low", "surface": "cli",
            "acceptance": [{"id": "AC-1", "when": "x", "then": "y"}],
            "verify": [{"name": "unit", "argv": ["true"]}],
            "lease": {"role": "worker", "actor": "agent:worker", "run_id": "run_1", "expires_at": "2026-09-17T00:00:00Z"},
        })
    }

    fn full_handoff() -> HandoffInput {
        HandoffInput {
            summary: "done".to_string(),
            changed_files: vec![],
            acceptance: vec![HandoffAcceptance {
                id: "AC-1".to_string(),
                status: "done".to_string(),
                how: "ran it".to_string(),
            }],
            verify_results: vec![VerifyResult {
                name: "unit".to_string(),
                exit: 0,
            }],
            docs_updated: vec![],
            learnings_used: vec![],
            friction: vec![],
            open_risks: vec![],
        }
    }

    #[test]
    fn a_fully_satisfied_handoff_has_no_violations() {
        let repo = git_repo();
        let ticket = active_ticket();
        let report = evaluate_handoff(repo.path(), &agent("worker"), &ticket, &full_handoff());
        assert!(report.is_clean(), "{:?}", report.violations);
    }

    #[test]
    fn handoff_not_active_is_reported() {
        let repo = git_repo();
        let mut ticket = active_ticket();
        ticket["status"] = json!("draft");
        let report = evaluate_handoff(repo.path(), &agent("worker"), &ticket, &full_handoff());
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "handoff_not_active"));
    }

    #[test]
    fn handoff_lease_mismatch_is_reported() {
        let repo = git_repo();
        let ticket = active_ticket();
        let report = evaluate_handoff(
            repo.path(),
            &agent("someone-else"),
            &ticket,
            &full_handoff(),
        );
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "handoff_lease_mismatch"));
    }

    #[test]
    fn handoff_acceptance_missing_is_reported() {
        let repo = git_repo();
        let ticket = active_ticket();
        let mut input = full_handoff();
        input.acceptance.clear();
        let report = evaluate_handoff(repo.path(), &agent("worker"), &ticket, &input);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "handoff_acceptance_missing"));
    }

    #[test]
    fn handoff_verify_missing_is_reported() {
        let repo = git_repo();
        let ticket = active_ticket();
        let mut input = full_handoff();
        input.verify_results.clear();
        let report = evaluate_handoff(repo.path(), &agent("worker"), &ticket, &input);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "handoff_verify_missing"));
    }

    #[test]
    fn handoff_documentation_missing_is_reported() {
        let repo = git_repo();
        let mut ticket = active_ticket();
        ticket["change"] = json!({"docs_to_update": ["docs/x.md"]});
        let report = evaluate_handoff(repo.path(), &agent("worker"), &ticket, &full_handoff());
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "handoff_documentation_missing"));
    }

    #[test]
    fn handoff_documentation_not_diffed_is_reported() {
        let repo = git_repo();
        let mut ticket = active_ticket();
        ticket["change"] = json!({"docs_to_update": ["docs/x.md"]});
        let mut input = full_handoff();
        input.docs_updated = vec!["docs/x.md".to_string()];
        // docs/x.md is claimed updated but the worktree has no diff for it.
        let report = evaluate_handoff(repo.path(), &agent("worker"), &ticket, &input);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "handoff_documentation_not_diffed"));
    }

    #[test]
    fn handoff_documentation_updated_and_diffed_passes() {
        let repo = git_repo();
        let mut ticket = active_ticket();
        ticket["change"] = json!({"docs_to_update": ["docs/x.md"]});
        std::fs::write(repo.path().join("docs/x.md"), "changed\n").unwrap();
        let mut input = full_handoff();
        input.docs_updated = vec!["docs/x.md".to_string()];
        let report = evaluate_handoff(repo.path(), &agent("worker"), &ticket, &input);
        assert!(report.is_clean(), "{:?}", report.violations);
    }

    #[test]
    fn handoff_learning_unknown_is_reported() {
        let repo = git_repo();
        let ticket = active_ticket();
        let mut input = full_handoff();
        input.learnings_used = vec![LearningUsage {
            id: "LRN-ffff".to_string(),
            usage: "helpful".to_string(),
        }];
        let report = evaluate_handoff(repo.path(), &agent("worker"), &ticket, &input);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "learning_unknown"));
    }

    #[test]
    fn handoff_learning_usage_invalid_is_reported() {
        let repo = git_repo();
        crate::learn::store::write(
            repo.path(),
            &crate::learn::store::Learning {
                frontmatter: crate::learn::store::Frontmatter {
                    id: "LRN-1111".to_string(),
                    status: "active".to_string(),
                    kind: "failure".to_string(),
                    applies_to: vec![],
                    tags: vec![],
                    from: vec![],
                    expected_signal: String::new(),
                    usage: crate::learn::store::UsageCounts::default(),
                },
                body: "## Summary\ns\n".to_string(),
            },
        )
        .unwrap();
        let ticket = active_ticket();
        let mut input = full_handoff();
        input.learnings_used = vec![LearningUsage {
            id: "LRN-1111".to_string(),
            usage: "vibes".to_string(),
        }];
        let report = evaluate_handoff(repo.path(), &agent("worker"), &ticket, &input);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "handoff_learning_usage_invalid"));
    }

    #[test]
    fn handoff_action_records_learning_usage() {
        let repo = git_repo();
        crate::learn::store::write(
            repo.path(),
            &crate::learn::store::Learning {
                frontmatter: crate::learn::store::Frontmatter {
                    id: "LRN-2222".to_string(),
                    status: "active".to_string(),
                    kind: "failure".to_string(),
                    applies_to: vec![],
                    tags: vec![],
                    from: vec![],
                    expected_signal: String::new(),
                    usage: crate::learn::store::UsageCounts::default(),
                },
                body: "## Summary\ns\n".to_string(),
            },
        )
        .unwrap();
        crate::store::issues::mutate(repo.path(), |mut records| {
            records.push(active_ticket());
            Ok(records)
        })
        .unwrap();
        let mut input = full_handoff();
        input.learnings_used = vec![LearningUsage {
            id: "LRN-2222".to_string(),
            usage: "helpful".to_string(),
        }];
        handoff(repo.path(), &agent("worker"), "TK-a3f9", input).unwrap();
        let learning = crate::learn::store::read(repo.path(), "LRN-2222").unwrap();
        assert_eq!(learning.frontmatter.usage.helpful, 1);
    }

    #[test]
    fn handoff_action_seals_a_receipt_and_transitions_to_verifying() {
        let repo = git_repo();
        crate::store::issues::mutate(repo.path(), |mut records| {
            records.push(active_ticket());
            Ok(records)
        })
        .unwrap();
        let updated = handoff(repo.path(), &agent("worker"), "TK-a3f9", full_handoff()).unwrap();
        assert_eq!(updated["status"], "verifying");
        assert!(updated["lease"].is_null());
        let receipts = list_receipts(repo.path()).unwrap().receipts;
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].kind, "handoff");
    }

    fn verifying_ticket_with_handoff(repo: &Path) -> Value {
        crate::store::issues::mutate(repo, |mut records| {
            records.push(active_ticket());
            Ok(records)
        })
        .unwrap();
        let updated = handoff(repo, &agent("worker"), "TK-a3f9", full_handoff()).unwrap();
        assert_eq!(updated["status"], "verifying");
        updated
    }

    fn seal_passing_lane_receipt(repo: &Path, subject_id: &str, lane_actor: &str) {
        record_receipt(
            repo,
            None,
            NewReceipt {
                kind: "lane".to_string(),
                subject: ReceiptSubject {
                    id: subject_id.to_string(),
                    revision: None,
                },
                actor: lane_actor.to_string(),
                source: ReceiptSource {
                    commit: source::head_commit(repo).unwrap(),
                    dirty_hash: source::snapshot(repo, &[]).unwrap().dirty_hash,
                },
                run_id: None,
                payload: json!({"role": "review-correctness", "verdict": "pass", "findings": []}),
                artifact_paths: Vec::new(),
            },
        )
        .unwrap();
    }

    #[test]
    fn close_not_verifying_is_reported() {
        let repo = git_repo();
        let ticket = active_ticket();
        let report = evaluate_close(repo.path(), &human("quan"), &ticket).unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "close_not_verifying"));
    }

    #[test]
    fn close_handoff_missing_is_reported() {
        let repo = git_repo();
        let mut ticket = active_ticket();
        ticket["status"] = json!("verifying");
        let report = evaluate_close(repo.path(), &human("quan"), &ticket).unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "close_handoff_missing"));
    }

    #[test]
    fn close_source_stale_is_reported_when_the_tree_changed_since_handoff() {
        let repo = git_repo();
        let ticket = verifying_ticket_with_handoff(repo.path());
        std::fs::write(repo.path().join("new.txt"), "new\n").unwrap();
        let report = evaluate_close(repo.path(), &human("quan"), &ticket).unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "close_source_stale"));
    }

    #[test]
    fn close_lane_not_satisfied_when_no_lane_receipt_exists() {
        let repo = git_repo();
        let ticket = verifying_ticket_with_handoff(repo.path());
        let report = evaluate_close(repo.path(), &human("quan"), &ticket).unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "close_lane_not_satisfied"));
    }

    #[test]
    fn close_lane_not_satisfied_when_lane_actor_equals_handoff_actor() {
        let repo = git_repo();
        let ticket = verifying_ticket_with_handoff(repo.path());
        seal_passing_lane_receipt(repo.path(), "TK-a3f9", "agent:worker");
        let report = evaluate_close(repo.path(), &human("quan"), &ticket).unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "close_lane_not_satisfied"));
    }

    #[test]
    fn close_human_required_is_reported_for_a_high_risk_profile() {
        let repo = git_repo();
        crate::store::issues::mutate(repo.path(), |mut records| {
            let mut ticket = active_ticket();
            ticket["surface"] = json!("api");
            ticket["risk"] = json!("high");
            records.push(ticket);
            Ok(records)
        })
        .unwrap();
        let updated = handoff(repo.path(), &agent("worker"), "TK-a3f9", full_handoff()).unwrap();
        seal_passing_lane_receipt(repo.path(), "TK-a3f9", "agent:review-correctness");
        let report = evaluate_close(repo.path(), &agent("review-correctness"), &updated).unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "close_human_required"));
    }

    #[test]
    fn close_question_blocking_is_reported() {
        let repo = git_repo();
        let mut ticket = verifying_ticket_with_handoff(repo.path());
        ticket["open_questions"] = json!([{"q": "which?", "disposition": "blocking"}]);
        let report = evaluate_close(repo.path(), &human("quan"), &ticket).unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "close_question_blocking"));
    }

    #[test]
    fn close_action_passes_with_a_satisfied_lane_and_human_actor() {
        let repo = git_repo();
        let ticket = verifying_ticket_with_handoff(repo.path());
        seal_passing_lane_receipt(repo.path(), "TK-a3f9", "agent:review-correctness");
        let updated = close(repo.path(), &human("quan"), ticket["id"].as_str().unwrap()).unwrap();
        assert_eq!(updated["status"], "done");
        let receipts = list_receipts(repo.path()).unwrap().receipts;
        assert!(receipts.iter().any(|r| r.kind == "close"));
    }

    fn story_with_ticket(repo: &Path, ticket_status: &str) -> (Value, Value) {
        crate::store::issues::mutate(repo, |mut records| {
            records.push(json!({
                "schema": 3, "id": "ST-1111", "kind": "story", "title": "s",
                "status": "ready", "revision": 1,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "outcome": "x", "qa_cases": [],
            }));
            records.push(json!({
                "schema": 3, "id": "TK-2222", "kind": "ticket", "title": "t",
                "status": ticket_status, "revision": 1,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "role": "implementation", "story": "ST-1111",
            }));
            Ok(records)
        })
        .unwrap();
        let records = crate::store::issues::read_all(repo).unwrap();
        let story = require(&records, "ST-1111").unwrap().clone();
        let ticket = require(&records, "TK-2222").unwrap().clone();
        (story, ticket)
    }

    #[test]
    fn close_story_children_incomplete_is_reported() {
        let repo = git_repo();
        let (story, _ticket) = story_with_ticket(repo.path(), "active");
        let all = crate::store::issues::read_all(repo.path()).unwrap();
        let report = evaluate_close_story(repo.path(), &story, &all).unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "close_story_children_incomplete"));
    }

    #[test]
    fn close_story_passes_when_every_child_is_done() {
        let repo = git_repo();
        let (story, _ticket) = story_with_ticket(repo.path(), "done");
        let all = crate::store::issues::read_all(repo.path()).unwrap();
        let report = evaluate_close_story(repo.path(), &story, &all).unwrap();
        assert!(report.is_clean(), "{:?}", report.violations);
    }

    #[test]
    fn close_story_qa_not_satisfied_when_a_high_priority_case_is_uncovered() {
        let repo = git_repo();
        let (mut story, _ticket) = story_with_ticket(repo.path(), "done");
        story["qa_cases"] = json!([{"id": "QA-001", "intent": "x", "priority": "high"}]);
        let all = crate::store::issues::read_all(repo.path()).unwrap();
        let report = evaluate_close_story(repo.path(), &story, &all).unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "close_story_qa_not_satisfied"));
    }

    #[test]
    fn close_story_passes_when_the_high_priority_case_is_covered() {
        let repo = git_repo();
        let (mut story, _ticket) = story_with_ticket(repo.path(), "done");
        story["qa_cases"] = json!([{"id": "QA-001", "intent": "x", "priority": "high"}]);
        record_receipt(
            repo.path(),
            None,
            NewReceipt {
                kind: "lane".to_string(),
                subject: ReceiptSubject {
                    id: "ST-1111".to_string(),
                    revision: None,
                },
                actor: "agent:qa-ui".to_string(),
                source: ReceiptSource {
                    commit: source::head_commit(repo.path()).unwrap(),
                    dirty_hash: "sha256:0".to_string(),
                },
                run_id: None,
                payload: json!({
                    "role": "qa-ui", "verdict": "pass",
                    "cases": [{"id": "QA-001", "status": "pass"}],
                }),
                artifact_paths: Vec::new(),
            },
        )
        .unwrap();
        let all = crate::store::issues::read_all(repo.path()).unwrap();
        let report = evaluate_close_story(repo.path(), &story, &all).unwrap();
        assert!(report.is_clean(), "{:?}", report.violations);
    }
}
