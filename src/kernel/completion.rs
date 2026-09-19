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
use crate::kernel::profile::{self, fence_ignore, profile_key};
use crate::kernel::reservation::touches_of;
use crate::kernel::roles::{authorize, Action};
use crate::kernel::scope;
use crate::source;
use crate::storage::WriteGuard;
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
    /// The run the handoff belongs to: must equal the lease's `run_id`, so
    /// a stale handoff.json from an earlier claim cannot describe this
    /// lease (decision 0025 B3).
    pub run_id: String,
    pub summary: String,
    #[serde(default)]
    pub changed_files: Vec<String>,
    #[serde(default)]
    pub acceptance: Vec<HandoffAcceptance>,
    #[serde(default)]
    /// Informational since decision 0026: the handoff gate judges the
    /// declared `verify[]` against the `verify` receipt `pulse verify`
    /// sealed, never against this claim. Kept so an existing `handoff.json`
    /// still parses (`deny_unknown_fields` stays on).
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

/// Every way this handoff fails plan §7.2, plus decision 0026's receipt
/// check: a declared `verify[]` must be backed by a `pulse verify` receipt
/// on the current fence whose results all exited zero.
///
/// # Errors
/// Propagates a receipt-list read failure or the fence's git failure: the
/// gate reports what it can prove, so an unreadable tree is an error rather
/// than a silent pass.
pub fn evaluate_handoff(
    repo_root: &Path,
    actor: &ActorRef,
    ticket: &Value,
    all_records: &[Value],
    input: &HandoffInput,
) -> Result<GateReport> {
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
    // Same code as the actor mismatch — the lease did not match. The run_id
    // binds the handoff to the claim that produced it (decision 0025 B3):
    // a handoff.json carried over from an earlier run proves nothing about
    // the tree this lease worked on.
    let lease_run_id = ticket.pointer("/lease/run_id").and_then(Value::as_str);
    if lease_run_id != Some(input.run_id.as_str()) {
        violations.push(violation(
            "handoff_lease_mismatch",
            // Dogfood 0025, F4: two parallel workers following the prompt's
            // bare `handoff.json` example overwrote each other's payload,
            // and the refusal read as a lease bug when the real cause was
            // the shared filename. Name the likely cause in the message.
            format!(
                "handoff run_id is {}, but the lease was claimed as {} — if you \
                 did write that run_id, your payload file was probably overwritten \
                 by a parallel worker's; keep scratch files per ticket \
                 (e.g. .pulse/runtime/handoff-tk-<id>.json)",
                input.run_id,
                lease_run_id.unwrap_or("<none>")
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
    // The handoff vocabulary has exactly one accepted value, `done` (plan
    // 0025 A1): a worker that is not finished checkpoints, it does not hand
    // off. Anything else (`partial`, `todo`, a typo) is a violation, not a
    // silently accepted progress report.
    for handed in &input.acceptance {
        if ticket_ac_ids.contains(&handed.id.as_str()) && handed.status != "done" {
            violations.push(violation(
                "handoff_acceptance_not_done",
                format!("acceptance {} is {}, not done", handed.id, handed.status),
            ));
        }
    }

    // Decision 0026 D2 + plan 0025 E2: the declared names are the ticket's
    // `verify[]` plus every enforceable `learning.<id>` (one function,
    // `verify::required_names`, so the handoff and the lane seal rule can
    // never disagree). The receipt `pulse verify` sealed — what Pulse
    // observed — is what judges them; `input.verify_results` is
    // informational now, so no check reads it.
    let required = crate::kernel::verify::required_names(repo_root, ticket)?;
    if !required.is_empty() {
        let receipts = list_receipts(repo_root)?.receipts;
        let id = ticket.get("id").and_then(Value::as_str).unwrap_or("");
        match latest_receipt(&receipts, "verify", id) {
            None => violations.push(violation(
                "handoff_verify_missing",
                "no `pulse verify` receipt exists for this ticket".to_string(),
            )),
            Some(receipt) => {
                let now = profile::fence_for(repo_root, ticket)?;
                // One comparison rule, never a hand-rolled one (decision
                // 0025 B6): a scoped ticket compares its scope hash, a
                // whole-tree ticket compares commit and hash.
                if !profile::same_fence(
                    ticket,
                    (&now.commit, &now.dirty_hash),
                    (&receipt.source.commit, &receipt.source.dirty_hash),
                ) {
                    violations.push(violation(
                        "handoff_verify_stale",
                        "tree changed since the last `pulse verify`; run it again".to_string(),
                    ));
                }
                let observed = receipt.payload.get("results").and_then(Value::as_array);
                let failing: Vec<&str> = observed
                    .map(|list| {
                        list.iter()
                            .filter(|result| result.get("exit").and_then(Value::as_i64) != Some(0))
                            .map(|result| {
                                result
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .unwrap_or("<unnamed>")
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let observed_names: Vec<&str> = observed
                    .map(|list| {
                        list.iter()
                            .filter_map(|result| result.get("name").and_then(Value::as_str))
                            .collect()
                    })
                    .unwrap_or_default();
                let absent: Vec<&str> = required
                    .iter()
                    .map(String::as_str)
                    .filter(|name| !observed_names.contains(name))
                    .collect();
                if !failing.is_empty() || !absent.is_empty() {
                    let mut parts = Vec::new();
                    if !failing.is_empty() {
                        parts.push(format!("exited non-zero: {}", failing.join(", ")));
                    }
                    if !absent.is_empty() {
                        parts.push(format!(
                            "absent from the last verify receipt (run `pulse verify` again): {}",
                            absent.join(", ")
                        ));
                    }
                    violations.push(violation(
                        "handoff_verify_failed",
                        format!(
                            "the last `pulse verify` did not pass — {}",
                            parts.join("; ")
                        ),
                    ));
                }
            }
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

    // Plan 0025 B6: an edit outside every ticket's declared scope must not
    // slip into review — the lane input carries only this ticket's scope,
    // so nobody would ever look at such a file. Only a scoped ticket
    // checks: an exclusive ticket owns the whole tree by definition. `done`
    // counts as reserving — the ticket closed, and its files may sit
    // uncommitted until the host commits them (decision 0025).
    if !touches_of(ticket).is_empty() {
        let reserved: Vec<String> = all_records
            .iter()
            .filter(|record| {
                matches!(
                    record.get("status").and_then(Value::as_str),
                    Some("active" | "verifying" | "done")
                )
            })
            .flat_map(touches_of)
            .collect();
        // A git failure here (not a repo, no commits) degrades to an empty
        // dirty list — the same lenience `changed_files_in_worktree` has;
        // the gate reports what it can see, never invents a violation.
        let dirty = source::snapshot(repo_root, &fence_ignore(repo_root))
            .map(|state| state.dirty_paths)
            .unwrap_or_default();
        let unreserved: Vec<&str> = dirty
            .iter()
            .map(String::as_str)
            .filter(|path| !scope::covers(&reserved, path))
            .collect();
        if !unreserved.is_empty() {
            let id = ticket.get("id").and_then(Value::as_str).unwrap_or("");
            violations.push(violation(
                "handoff_unreserved_changes",
                format!(
                    "changed but reserved by no ticket: {} — `pulse reserve {id} <path>` \
                     if it is yours, revert it if not",
                    unreserved.join(", ")
                ),
            ));
        }
    }

    Ok(GateReport { violations })
}

/// # Errors
/// `role_forbidden` if `actor` may not checkpoint/handoff; `gate_failed`
/// listing every [`evaluate_handoff`] violation otherwise.
pub fn handoff(repo_root: &Path, actor: &ActorRef, id: &str, input: HandoffInput) -> Result<Value> {
    authorize(actor, Action::CheckpointOrHandoff)?;
    // One lock for read -> evaluate -> receipt -> transition (plan 0025 A4).
    // `append_note`/`record_usage` take the lock themselves, so they stay
    // outside the block; `emit_event` and `record_receipt` do not.
    {
        let guard = WriteGuard::acquire(repo_root)?;
        let records = issues::read_all(repo_root)?;
        let ticket = require(&records, id)?;
        let report = evaluate_handoff(repo_root, actor, ticket, &records, &input)?;
        if !report.is_clean() {
            return Err(fail(&report));
        }

        let revision = ticket.get("revision").and_then(Value::as_u64);
        // Plan 0025 B6: the receipt carries the ticket's own fence — its
        // `touches` scope when it declares one, the whole tree otherwise.
        let source_snapshot = profile::fence_for(repo_root, ticket)?;
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

        issues::mutate_locked(&guard, repo_root, |records| {
            apply_to_record(records, id, |record| {
                let object = record.as_object_mut().expect("records are always objects");
                object.insert("status".to_string(), Value::String("verifying".to_string()));
                object.insert("lease".to_string(), Value::Null);
                bump(object);
                Ok(())
            })
        })?;
    }
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

/// Whether `needle` appears in `text` delimited by non-word characters on
/// both sides (a word character here is a letter, digit, `_` or `-`). So
/// `BR-1` is satisfied by "BR-1." or "(BR-1)" but not by `BR-10` (digit
/// after) nor `X-BR-1` (dash before) — plan 0025 F4. Hand-rolled because
/// the ids are short ASCII and a regex crate is one this plan avoids.
fn contains_word(text: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
    for (start, _) in text.match_indices(needle) {
        let end = start + needle.len();
        let before = text[..start].chars().next_back();
        let after = text[end..].chars().next();
        if !before.is_some_and(is_word) && !after.is_some_and(is_word) {
            return true;
        }
    }
    false
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

    // Plan 0025 B6: the fence is the ticket's own scope when it declares
    // `touches`, the whole tree otherwise.
    let now_source = profile::fence_for(repo_root, ticket)?;
    if !profile::same_fence(
        ticket,
        (&now_source.commit, &now_source.dirty_hash),
        (
            &handoff_receipt.source.commit,
            &handoff_receipt.source.dirty_hash,
        ),
    ) {
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
        let lane_receipts: Vec<&crate::evidence::receipt::ReceiptEnvelope> = receipts
            .iter()
            .filter(|r| {
                r.kind == "lane"
                    && r.subject.id == id
                    && r.payload.get("role").and_then(Value::as_str) == Some(lane)
            })
            .collect();
        // Decision 0027 C3: a lane the profile declares a panel for is
        // satisfied only by the reconciled receipt of the CURRENT round.
        // Seat receipts are `kind: "lane_seat"` and so never reach
        // `lane_receipts` at all; a `lane` receipt from a previous handoff
        // round is stale for the same reason a stale lane receipt is.
        let lane_receipt = match profile.panel(lane) {
            Some(panel) => match lane_receipts
                .iter()
                .copied()
                .filter(|receipt| {
                    receipt.payload.get("reconciled").and_then(Value::as_bool) == Some(true)
                        && receipt.payload.get("handoff").and_then(Value::as_str)
                            == Some(handoff_receipt.id.as_str())
                })
                .max_by(|a, b| a.id.cmp(&b.id))
            {
                Some(receipt) => Some(receipt),
                None => {
                    violations.push(violation(
                        "close_lane_not_satisfied",
                        format!(
                            "{lane}: panel of {} requires `pulse lane reconcile`",
                            panel.count
                        ),
                    ));
                    continue;
                }
            },
            None => lane_receipts.into_iter().max_by(|a, b| a.id.cmp(&b.id)),
        };
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
                // Plan 0025 B6: one fence rule for every ticket — scoped
                // tickets compare only their scope hash (another ticket's
                // commit landing in between is not this tree moving),
                // whole-tree tickets pin commit and hash.
                // `profile::same_fence` owns the rule; this branch only
                // words the violation.
                if !profile::same_fence(
                    ticket,
                    (&receipt.source.commit, &receipt.source.dirty_hash),
                    (
                        &handoff_receipt.source.commit,
                        &handoff_receipt.source.dirty_hash,
                    ),
                ) {
                    violations.push(violation(
                        "close_lane_not_satisfied",
                        format!(
                            "{lane}: receipt was sealed on a different tree state than the handoff"
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
    // One lock for read -> evaluate -> receipt -> transition (plan 0025 A4).
    let saved = {
        let guard = WriteGuard::acquire(repo_root)?;
        let records = issues::read_all(repo_root)?;
        let ticket = require(&records, id)?;
        let report = evaluate_close(repo_root, actor, ticket)?;
        if !report.is_clean() {
            return Err(fail(&report));
        }

        // Plan 0025 B6: the close receipt records the ticket's own fence.
        let now_source = profile::fence_for(repo_root, ticket)?;
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

        issues::mutate_locked(&guard, repo_root, |records| {
            apply_to_record(records, id, |record| {
                let object = record.as_object_mut().expect("records are always objects");
                object.insert("status".to_string(), Value::String("done".to_string()));
                bump(object);
                Ok(())
            })
        })?
    };
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

    // Plan 0025 E1: a friction the loop never classified must not silently
    // die with the Story. The skill caps one learning per Ticket and lets
    // leftover frictions stay Ticket-specific — so this gate does not demand
    // a learning, it demands a *classification*: cite the friction in a
    // learning or dismiss it with a reason. `done`/`cancelled` children both
    // count (a cancelled Ticket's frictions are still knowledge), and so
    // does the Story itself (story-scope friction from story-scope lanes).
    let mut friction_subjects = vec![story_id];
    friction_subjects.extend(
        children
            .iter()
            .filter_map(|child| child.get("id").and_then(Value::as_str)),
    );
    let unclassified = crate::learn::friction::unclassified_for(repo_root, &friction_subjects)?;
    if !unclassified.is_empty() {
        let items: Vec<String> = unclassified
            .iter()
            .map(|friction| format!("{}#{}", friction.subject, friction.key))
            .collect();
        violations.push(violation(
            "close_story_friction_unclassified",
            format!(
                "{} unclassified friction on {}: {} — classify each: \
                 `pulse learn add --friction …` or `pulse learn dismiss <id> --all --reason …`",
                items.len(),
                story_id,
                items.join(", ")
            ),
        ));
    }

    // Plan 0025 F4: a Story's rules and exceptions must survive in docs/, not
    // only in issues.jsonl. Mechanical check, no prose grading: every id
    // must appear verbatim as a word in at least one listed file, and every
    // listed file must sit under docs/ and exist (close-story already
    // demands a clean tree, so existence is the committed-or-clean test).
    let rule_ids: Vec<&str> = story
        .get("rules")
        .and_then(Value::as_array)
        .map(|rules| {
            rules
                .iter()
                .filter_map(|rule| rule.get("id").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    let exception_ids: Vec<&str> = story
        .get("exceptions")
        .and_then(Value::as_array)
        .map(|exceptions| {
            exceptions
                .iter()
                .filter_map(|exception| exception.get("id").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    let wanted_ids: Vec<&str> = rule_ids
        .iter()
        .copied()
        .chain(exception_ids.iter().copied())
        .filter(|id| !id.is_empty())
        .collect();
    if !wanted_ids.is_empty() {
        let docs_written: Vec<&str> = story
            .get("docs_written")
            .and_then(Value::as_array)
            .map(|paths| paths.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if docs_written.is_empty() {
            violations.push(violation(
                "close_story_docs_missing",
                format!(
                    "story has {} rule(s) and {} exception(s) and no docs_written; record \
                     where they now live: `pulse work update {story_id} --set \
                     docs_written='[\"docs/…\"]'`",
                    rule_ids.len(),
                    exception_ids.len()
                ),
            ));
        } else {
            let mut problems: Vec<String> = Vec::new();
            let mut bodies: Vec<String> = Vec::new();
            for entry in &docs_written {
                match crate::storage::safe_repo_relative(entry) {
                    Ok(path) => {
                        let under_docs = path
                            .components()
                            .next()
                            .is_some_and(|first| first.as_os_str() == "docs");
                        if !under_docs {
                            problems.push(format!("{entry} is not under docs/"));
                        } else {
                            match std::fs::read_to_string(repo_root.join(&path)) {
                                Ok(text) => bodies.push(text),
                                Err(_) => problems.push(format!("{entry} does not exist")),
                            }
                        }
                    }
                    Err(_) => problems.push(format!("{entry} is not a safe repo-relative path")),
                }
            }
            if !problems.is_empty() {
                violations.push(violation(
                    "close_story_docs_missing",
                    format!("docs_written has problems: {}", problems.join("; ")),
                ));
            }
            let missing: Vec<&str> = wanted_ids
                .iter()
                .copied()
                .filter(|id| !bodies.iter().any(|body| contains_word(body, id)))
                .collect();
            if !missing.is_empty() {
                violations.push(violation(
                    "close_story_docs_missing",
                    format!(
                        "rule/exception ids absent from every docs_written file: {} — each id \
                         must appear verbatim in at least one listed doc",
                        missing.join(", ")
                    ),
                ));
            }
        }
    }

    // Source fence (dogfood ST-1 F15): ticket-level close compares a handoff
    // snapshot, but a Story hands nothing off — the story milestone must
    // simply not close over uncommitted work. Committed changes are fine:
    // a post-close operator fix that lands as a commit and is re-validated
    // by story-scope qa is the legitimate flow the ST-1 dogfood ran into.
    let now_source = source::snapshot(repo_root, &fence_ignore(repo_root))?;
    if !now_source.dirty_paths.is_empty() {
        violations.push(violation(
            "close_story_source_dirty",
            format!(
                "uncommitted changes at story close: {:?}",
                now_source.dirty_paths
            ),
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
        // Coverage unions across every passing story-scope qa receipt on the
        // current HEAD: a multi-surface story legitimately splits its cases
        // across qa-api and qa-ui, and no single receipt can cover both
        // (dogfood ST-1, F17 — the target needed a hand-rolled qa-all lane
        // under the old latest-receipt-only rule).
        let covered: std::collections::HashSet<&str> = receipts
            .iter()
            .filter(|r| {
                r.kind == "lane"
                    && r.subject.id == story_id
                    && r.source.commit == now_source.commit
                    && r.payload
                        .get("role")
                        .and_then(Value::as_str)
                        .is_some_and(|role| role.starts_with("qa-"))
                    && r.payload.get("verdict").and_then(Value::as_str) == Some("pass")
            })
            .flat_map(|receipt| {
                receipt
                    .payload
                    .get("cases")
                    .and_then(Value::as_array)
                    .map(|cases| {
                        cases
                            .iter()
                            .filter(|c| c.get("status").and_then(Value::as_str) == Some("pass"))
                            .filter_map(|c| c.get("id").and_then(Value::as_str))
                            .collect::<Vec<&str>>()
                    })
                    .unwrap_or_default()
            })
            .collect();
        if high_priority_cases
            .iter()
            .all(|case| covered.contains(*case))
        {
            // covered — nothing to report
        } else {
            let uncovered: Vec<&str> = high_priority_cases
                .iter()
                .filter(|case| !covered.contains(*case))
                .copied()
                .collect();
            violations.push(violation(
                "close_story_qa_not_satisfied",
                if covered.is_empty() && receipts.iter().any(|r| r.subject.id == story_id) {
                    "no passing story-scope qa lane receipt on the current HEAD covers the \
                     high-priority qa_cases"
                        .to_string()
                } else {
                    format!(
                        "high-priority qa_cases not covered by a passing case in any \
                         story-scope qa receipt on HEAD: {}",
                        uncovered.join(", ")
                    )
                },
            ));
        }
    }

    Ok(GateReport { violations })
}

/// # Errors
/// `role_forbidden` if `actor` may not close; `gate_failed` listing every
/// [`evaluate_close_story`] violation otherwise.
pub fn close_story(repo_root: &Path, actor: &ActorRef, id: &str) -> Result<Value> {
    authorize(actor, Action::Close)?;
    // One lock for read -> evaluate -> receipt -> transition (plan 0025 A4).
    let saved = {
        let guard = WriteGuard::acquire(repo_root)?;
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

        issues::mutate_locked(&guard, repo_root, |records| {
            apply_to_record(records, id, |record| {
                let object = record.as_object_mut().expect("records are always objects");
                object.insert("status".to_string(), Value::String("done".to_string()));
                bump(object);
                Ok(())
            })
        })?
    };
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
            run_id: "run_1".to_string(),
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
        seal_passing_verify_receipt(repo.path(), &ticket);
        let report =
            evaluate_handoff(repo.path(), &agent("worker"), &ticket, &[], &full_handoff()).unwrap();
        assert!(report.is_clean(), "{:?}", report.violations);
    }

    #[test]
    fn handoff_not_active_is_reported() {
        let repo = git_repo();
        let mut ticket = active_ticket();
        ticket["status"] = json!("draft");
        let report =
            evaluate_handoff(repo.path(), &agent("worker"), &ticket, &[], &full_handoff()).unwrap();
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
            &[],
            &full_handoff(),
        )
        .unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "handoff_lease_mismatch"));
    }

    #[test]
    fn handoff_run_id_mismatch_is_reported() {
        // Decision 0025 B3: the handoff must carry the run_id of the claim
        // it belongs to, not of some earlier run.
        let repo = git_repo();
        let ticket = active_ticket();
        let mut input = full_handoff();
        input.run_id = "run_from-an-earlier-claim".to_string();
        let report = evaluate_handoff(repo.path(), &agent("worker"), &ticket, &[], &input).unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "handoff_lease_mismatch"));
        let mismatch = report
            .violations
            .iter()
            .find(|v| v.code == "handoff_lease_mismatch")
            .unwrap();
        assert!(mismatch.message.contains("run_id"), "{mismatch:?}");
        // Dogfood 0025, F4: the refusal must name the likely real cause —
        // a parallel worker overwriting a shared payload file — not just
        // restate that the lease did not match.
        assert!(mismatch.message.contains("overwritten"), "{mismatch:?}");
    }

    #[test]
    fn handoff_acceptance_missing_is_reported() {
        let repo = git_repo();
        let ticket = active_ticket();
        let mut input = full_handoff();
        input.acceptance.clear();
        let report = evaluate_handoff(repo.path(), &agent("worker"), &ticket, &[], &input).unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "handoff_acceptance_missing"));
    }

    #[test]
    fn handoff_without_a_verify_receipt_is_reported() {
        // Decision 0026: the gate reads the receipt, so a ticket that
        // declares verify[] cannot hand off on the worker's word. Clearing
        // `verify_results` changes nothing — that field is informational.
        let repo = git_repo();
        let ticket = active_ticket();
        let mut input = full_handoff();
        input.verify_results.clear();
        let report = evaluate_handoff(repo.path(), &agent("worker"), &ticket, &[], &input).unwrap();
        let missing = report
            .violations
            .iter()
            .find(|v| v.code == "handoff_verify_missing")
            .expect("no receipt means no handoff");
        assert!(missing.message.contains("pulse verify"), "{missing:?}");
    }

    #[test]
    fn handoff_verify_stale_is_reported_when_the_tree_moved_after_verifying() {
        let repo = git_repo();
        let ticket = active_ticket();
        seal_passing_verify_receipt(repo.path(), &ticket);
        // A change after the verify receipt: the commands were observed on a
        // tree that no longer exists.
        std::fs::write(repo.path().join("docs/x.md"), "moved on\n").unwrap();
        let report =
            evaluate_handoff(repo.path(), &agent("worker"), &ticket, &[], &full_handoff()).unwrap();
        let stale = report
            .violations
            .iter()
            .find(|v| v.code == "handoff_verify_stale")
            .expect("a tree that moved must stale the verify receipt");
        assert!(stale.message.contains("pulse verify"), "{stale:?}");
    }

    #[test]
    fn handoff_verify_failed_is_reported_for_a_failing_observed_result() {
        // The receipt Pulse sealed records a command that did not pass: the
        // handoff cannot call the ticket verified, whatever the worker says.
        let repo = git_repo();
        let ticket = active_ticket();
        seal_verify_receipt(repo.path(), &ticket, 1);
        let report =
            evaluate_handoff(repo.path(), &agent("worker"), &ticket, &[], &full_handoff()).unwrap();
        let failed = report
            .violations
            .iter()
            .find(|v| v.code == "handoff_verify_failed")
            .expect("a failing observed exit must block the handoff");
        assert!(failed.message.contains("unit"), "{failed:?}");
    }

    #[test]
    fn handoff_verify_failed_is_reported_for_a_command_the_receipt_never_saw() {
        // A verify command declared after the receipt was sealed was never
        // observed — the receipt stops being evidence for this ticket.
        let repo = git_repo();
        let mut ticket = active_ticket();
        seal_passing_verify_receipt(repo.path(), &ticket);
        ticket["verify"] = json!([
            {"name": "unit", "argv": ["true"]},
            {"name": "added-later", "argv": ["true"]},
        ]);
        let report =
            evaluate_handoff(repo.path(), &agent("worker"), &ticket, &[], &full_handoff()).unwrap();
        let failed = report
            .violations
            .iter()
            .find(|v| v.code == "handoff_verify_failed")
            .expect("an unobserved declaration must block the handoff");
        assert!(failed.message.contains("added-later"), "{failed:?}");
    }

    #[test]
    fn handoff_acceptance_not_done_is_reported() {
        let repo = git_repo();
        let ticket = active_ticket();
        let mut input = full_handoff();
        input.acceptance[0].status = "partial".to_string();
        let report = evaluate_handoff(repo.path(), &agent("worker"), &ticket, &[], &input).unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "handoff_acceptance_not_done"));
    }

    #[test]
    fn handoff_acceptance_not_done_is_reported_after_verifying() {
        // The old `handoff_verify_failed_is_reported` (which asserted on the
        // worker's own `verify_results`) is gone with decision 0026: a
        // failing *claim* proves nothing. The receipt-based cases live above.
        let repo = git_repo();
        let ticket = active_ticket();
        let mut input = full_handoff();
        input.acceptance[0].status = "partial".to_string();
        seal_passing_verify_receipt(repo.path(), &ticket);
        let report = evaluate_handoff(repo.path(), &agent("worker"), &ticket, &[], &input).unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "handoff_acceptance_not_done"));
    }

    // --- Plan 0025 B6: unreserved changes must not slip into review ---

    #[test]
    fn handoff_unreserved_changes_is_reported() {
        let repo = git_repo();
        let mut ticket = active_ticket();
        ticket["touches"] = json!(["src/**"]);
        // A file no ticket's `touches` names: with the lane input scoped to
        // this ticket, nobody would ever review it.
        std::fs::write(repo.path().join("loose.txt"), "whose is this?\n").unwrap();
        let report = evaluate_handoff(
            repo.path(),
            &agent("worker"),
            &ticket,
            &[ticket.clone()],
            &full_handoff(),
        )
        .unwrap();
        let unreserved = report
            .violations
            .iter()
            .find(|v| v.code == "handoff_unreserved_changes")
            .expect("a stray file must block the handoff");
        assert!(unreserved.message.contains("loose.txt"), "{unreserved:?}");
        assert!(
            unreserved.message.contains("pulse reserve"),
            "{unreserved:?}"
        );
    }

    #[test]
    fn a_stray_change_another_ticket_reserves_does_not_block_handoff() {
        // Worker-2's in-progress file is covered by worker-2's own ticket —
        // it is not this handoff's business (decision 0025).
        let repo = git_repo();
        let mut ticket = active_ticket();
        ticket["touches"] = json!(["src/**"]);
        let mut other = active_ticket();
        other["id"] = json!("TK-bbbb");
        other["touches"] = json!(["web/**"]);
        std::fs::create_dir_all(repo.path().join("web")).unwrap();
        std::fs::write(repo.path().join("web/nav.js"), "// worker-2\n").unwrap();
        seal_passing_verify_receipt(repo.path(), &ticket);
        let report = evaluate_handoff(
            repo.path(),
            &agent("worker"),
            &ticket,
            &[ticket.clone(), other],
            &full_handoff(),
        )
        .unwrap();
        assert!(report.is_clean(), "{:?}", report.violations);
    }

    #[test]
    fn handoff_documentation_missing_is_reported() {
        let repo = git_repo();
        let mut ticket = active_ticket();
        ticket["change"] = json!({"docs_to_update": ["docs/x.md"]});
        let report =
            evaluate_handoff(repo.path(), &agent("worker"), &ticket, &[], &full_handoff()).unwrap();
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
        let report = evaluate_handoff(repo.path(), &agent("worker"), &ticket, &[], &input).unwrap();
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
        // The receipt must describe the tree the handoff hands over, so it is
        // sealed after the doc edit.
        seal_passing_verify_receipt(repo.path(), &ticket);
        let report = evaluate_handoff(repo.path(), &agent("worker"), &ticket, &[], &input).unwrap();
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
        let report = evaluate_handoff(repo.path(), &agent("worker"), &ticket, &[], &input).unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "learning_unknown"));
    }

    #[test]
    fn handoff_learning_usage_invalid_is_reported() {
        let repo = git_repo();
        seed_learning(repo.path(), "LRN-1111", "active", &[]);
        let ticket = active_ticket();
        let mut input = full_handoff();
        input.learnings_used = vec![LearningUsage {
            id: "LRN-1111".to_string(),
            usage: "vibes".to_string(),
        }];
        let report = evaluate_handoff(repo.path(), &agent("worker"), &ticket, &[], &input).unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "handoff_learning_usage_invalid"));
    }

    /// A learning file with the given status and (optional) check argv, that
    /// matches `active_ticket()` by tag. The ticket itself gains the tag.
    fn seed_learning(repo: &Path, id: &str, status: &str, check_argv: &[&str]) {
        crate::learn::store::write(
            repo,
            &crate::learn::store::Learning {
                frontmatter: crate::learn::store::Frontmatter {
                    id: id.to_string(),
                    status: status.to_string(),
                    kind: "failure".to_string(),
                    applies_to: vec![],
                    tags: vec!["auth".to_string()],
                    from: vec![],
                    expected_signal: String::new(),
                    usage: crate::learn::store::UsageCounts::default(),
                    check_argv: check_argv.iter().map(|s| s.to_string()).collect(),
                    check_cwd: None,
                    cites: vec![],
                },
                body: "## Summary\ns\n".to_string(),
            },
        )
        .unwrap();
    }

    fn ticket_tagged_for_learning() -> Value {
        let mut ticket = active_ticket();
        ticket["tags"] = json!(["auth"]);
        ticket
    }

    // --- Plan 0025 E2: learning checks join the enforced set ---

    #[test]
    fn a_learning_activated_after_the_last_verify_requires_a_fresh_verify() {
        // The receipt was sealed before the learning existed, so its name is
        // absent from the results: the handoff must send the worker back to
        // `pulse verify`, naming the learning it has to read.
        let repo = git_repo();
        let ticket = ticket_tagged_for_learning();
        seal_passing_verify_receipt(repo.path(), &ticket);
        seed_learning(repo.path(), "LRN-1111", "active", &["true"]);
        let report =
            evaluate_handoff(repo.path(), &agent("worker"), &ticket, &[], &full_handoff()).unwrap();
        let failed = report
            .violations
            .iter()
            .find(|v| v.code == "handoff_verify_failed")
            .expect("a post-receipt activation must demand a fresh verify");
        assert!(failed.message.contains("learning.LRN-1111"), "{failed:?}");
        assert!(
            failed.message.contains("run `pulse verify` again"),
            "{failed:?}"
        );
    }

    #[test]
    fn a_learning_check_alone_makes_the_verify_receipt_required() {
        // No verify[] at all — but an active learning's check applies, so
        // "the ticket declares verify" is still true and the receipt gate
        // holds (plan 0025 E2).
        let repo = git_repo();
        let mut ticket = ticket_tagged_for_learning();
        ticket["verify"] = json!([]);
        seed_learning(repo.path(), "LRN-1111", "active", &["true"]);
        let report =
            evaluate_handoff(repo.path(), &agent("worker"), &ticket, &[], &full_handoff()).unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "handoff_verify_missing"));

        // With the receipt on file (the shape `pulse verify` seals — one
        // result per required name), the handoff is clean.
        seal_passing_verify_receipt(repo.path(), &ticket);
        let report =
            evaluate_handoff(repo.path(), &agent("worker"), &ticket, &[], &full_handoff()).unwrap();
        assert!(report.is_clean(), "{:?}", report.violations);
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
                    check_argv: vec![],
                    check_cwd: None,
                    cites: vec![],
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
        let ticket = active_ticket();
        crate::store::issues::mutate(repo.path(), |mut records| {
            records.push(ticket.clone());
            Ok(records)
        })
        .unwrap();
        seal_passing_verify_receipt(repo.path(), &ticket);
        handoff(repo.path(), &agent("worker"), "TK-a3f9", input).unwrap();
        let learning = crate::learn::store::read(repo.path(), "LRN-2222").unwrap();
        assert_eq!(learning.frontmatter.usage.helpful, 1);
    }

    #[test]
    fn handoff_action_seals_a_receipt_and_transitions_to_verifying() {
        let repo = git_repo();
        let ticket = active_ticket();
        crate::store::issues::mutate(repo.path(), |mut records| {
            records.push(ticket.clone());
            Ok(records)
        })
        .unwrap();
        seal_passing_verify_receipt(repo.path(), &ticket);
        let updated = handoff(repo.path(), &agent("worker"), "TK-a3f9", full_handoff()).unwrap();
        assert_eq!(updated["status"], "verifying");
        assert!(updated["lease"].is_null());
        let receipts = list_receipts(repo.path()).unwrap().receipts;
        // The verify receipt the gate read sits alongside the handoff's.
        assert_eq!(receipts.len(), 2);
        let handoff_receipt = receipts
            .iter()
            .find(|receipt| receipt.kind == "handoff")
            .expect("the handoff sealed its own receipt");
        assert_eq!(handoff_receipt.subject.id, "TK-a3f9");
    }

    #[test]
    fn concurrent_handoffs_record_exactly_one_receipt() {
        // Plan 0025 A4: read -> evaluate -> record_receipt -> mutate must be
        // one critical section, or two sessions hand the same Ticket off
        // twice (the second would also find it `verifying`, but only if it
        // reads after the first write; without the lock both read `active`).
        let repo = git_repo();
        let ticket = active_ticket();
        crate::store::issues::mutate(repo.path(), |mut records| {
            records.push(ticket.clone());
            Ok(records)
        })
        .unwrap();
        seal_passing_verify_receipt(repo.path(), &ticket);
        let repo_path = repo.path().to_path_buf();
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let repo_path = repo_path.clone();
                std::thread::spawn(move || {
                    handoff(&repo_path, &agent("worker"), "TK-a3f9", full_handoff()).is_ok()
                })
            })
            .collect();
        let mut winners = 0;
        for handle in handles {
            if handle.join().expect("thread") {
                winners += 1;
            }
        }
        assert_eq!(winners, 1, "exactly one handoff may pass the gate");
        let receipts = list_receipts(repo.path()).unwrap().receipts;
        assert_eq!(receipts.iter().filter(|r| r.kind == "handoff").count(), 1);
        let records = crate::store::issues::read_all(repo.path()).unwrap();
        assert_eq!(require(&records, "TK-a3f9").unwrap()["status"], "verifying");
    }

    fn verifying_ticket_with_handoff(repo: &Path) -> Value {
        let ticket = active_ticket();
        crate::store::issues::mutate(repo, |mut records| {
            records.push(ticket.clone());
            Ok(records)
        })
        .unwrap();
        // Decision 0026: a declared verify[] needs its receipt before any
        // handoff can pass, so the helper seals one the way `pulse verify`
        // would have.
        seal_passing_verify_receipt(repo, &ticket);
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

    /// Seal the `verify` receipt the handoff gate reads (decision 0026): one
    /// result per name the ticket declares, on the ticket's own fence, so
    /// `profile::same_fence` agrees with what `evaluate_handoff` computes a
    /// moment later.
    ///
    /// The ticket is taken by value because most fixtures here are not in the
    /// store; this mirrors what `pulse verify` would have sealed. Tests that
    /// care about the *running* of commands go through the real command
    /// instead (`tests/golden_path.rs`).
    fn seal_passing_verify_receipt(repo: &Path, ticket: &Value) {
        seal_verify_receipt(repo, ticket, 0);
    }

    /// [`seal_passing_verify_receipt`] with a chosen exit code for every
    /// declared name — the shape a failing `pulse verify` leaves behind.
    /// Results mirror `pulse verify`'s own two sources (plan 0025 E2): one
    /// entry per required name — `verify[]` entries with their argv, learning
    /// checks with a stub argv.
    fn seal_verify_receipt(repo: &Path, ticket: &Value, exit: i64) {
        let id = ticket["id"].as_str().unwrap_or("TK-a3f9");
        let declared: Vec<(String, Value)> = ticket
            .get("verify")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .map(|entry| {
                        (
                            entry
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                            entry.get("argv").cloned().unwrap_or_else(|| json!([])),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut results: Vec<Value> = crate::kernel::verify::required_names(repo, ticket)
            .unwrap()
            .into_iter()
            .map(|name| {
                let (argv, cwd) = declared
                    .iter()
                    .find(|(declared_name, _)| *declared_name == name)
                    .map(|(_, argv)| (argv.clone(), ticket_cwd_of(ticket, &name)))
                    .unwrap_or((json!(["true"]), Value::Null));
                json!({
                    "name": name,
                    "argv": argv,
                    "cwd": cwd,
                    "exit": exit,
                    "timed_out": false,
                    "duration_ms": 1,
                    "log": format!(".pulse/evidence/{id}/verify/{name}.log"),
                })
            })
            .collect();
        if results.is_empty() {
            // Mirror the legacy shape when nothing is required: one failing
            // result is enough for the stale/failing assertions below.
            results.push(json!({"name": "<none>", "exit": exit}));
        }
        let fence = profile::fence_for(repo, ticket).unwrap();
        record_receipt(
            repo,
            None,
            NewReceipt {
                kind: "verify".to_string(),
                subject: ReceiptSubject {
                    id: id.to_string(),
                    revision: None,
                },
                actor: "agent:worker".to_string(),
                source: ReceiptSource {
                    commit: fence.commit,
                    dirty_hash: fence.dirty_hash,
                },
                run_id: None,
                payload: json!({"results": results, "passed": exit == 0}),
                artifact_paths: Vec::new(),
            },
        )
        .unwrap();
    }

    fn ticket_cwd_of(ticket: &Value, name: &str) -> Value {
        ticket
            .get("verify")
            .and_then(Value::as_array)
            .and_then(|list| {
                list.iter()
                    .find(|entry| entry.get("name").and_then(Value::as_str) == Some(name))
                    .and_then(|entry| entry.get("cwd"))
                    .cloned()
            })
            .unwrap_or(Value::Null)
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
    fn close_lane_not_satisfied_when_lane_dirty_hash_differs() {
        let repo = git_repo();
        let ticket = verifying_ticket_with_handoff(repo.path());
        // Same commit as the handoff, different tree state: the lane judged a
        // worktree the handoff did not describe.
        record_receipt(
            repo.path(),
            None,
            NewReceipt {
                kind: "lane".to_string(),
                subject: ReceiptSubject {
                    id: "TK-a3f9".to_string(),
                    revision: None,
                },
                actor: "agent:review-correctness".to_string(),
                source: ReceiptSource {
                    commit: source::head_commit(repo.path()).unwrap(),
                    dirty_hash: "scope:sha256:different".to_string(),
                },
                run_id: None,
                payload: json!({"role": "review-correctness", "verdict": "pass", "findings": []}),
                artifact_paths: Vec::new(),
            },
        )
        .unwrap();
        let report = evaluate_close(repo.path(), &human("quan"), &ticket).unwrap();
        let lane_violation = report
            .violations
            .iter()
            .find(|v| v.code == "close_lane_not_satisfied")
            .expect("lane receipt on a different dirty_hash must be refused");
        assert!(
            lane_violation.message.contains("different tree state"),
            "{lane_violation:?}"
        );
    }

    /// A verifying `TK-a3f9` that declared `touches`, handed off under the
    /// scope fence its own record produces (plan 0025 B6).
    fn verifying_scoped_ticket_with_handoff(repo: &Path, touches: &[&str]) -> Value {
        let mut ticket = active_ticket();
        ticket["touches"] = json!(touches);
        crate::store::issues::mutate(repo, |mut records| {
            records.push(ticket.clone());
            Ok(records)
        })
        .unwrap();
        // Sealed after the record exists: a scoped ticket's fence is its
        // scope hash, so the helper needs the ticket's `touches`.
        seal_passing_verify_receipt(repo, &ticket);
        let updated = handoff(repo, &agent("worker"), "TK-a3f9", full_handoff()).unwrap();
        assert_eq!(updated["status"], "verifying");
        updated
    }

    /// The lane receipt a real scoped seal now records: source is the
    /// ticket's fence (`profile::fence_for`), not the whole tree.
    fn seal_passing_scoped_lane_receipt(repo: &Path, ticket: &Value, lane_actor: &str) {
        let fence = profile::fence_for(repo, ticket).unwrap();
        record_receipt(
            repo,
            None,
            NewReceipt {
                kind: "lane".to_string(),
                subject: ReceiptSubject {
                    id: ticket["id"].as_str().unwrap().to_string(),
                    revision: None,
                },
                actor: lane_actor.to_string(),
                source: ReceiptSource {
                    commit: fence.commit,
                    dirty_hash: fence.dirty_hash,
                },
                run_id: None,
                payload: json!({"role": "review-correctness", "verdict": "pass", "findings": []}),
                artifact_paths: Vec::new(),
            },
        )
        .unwrap();
    }

    #[test]
    fn close_survives_a_foreign_commit_when_the_ticket_has_touches() {
        // Decision 0025 B6 core property: HEAD is not a scoped ticket's
        // fence. src/lib.rs content at handoff equals content at close; the
        // only thing that moved is HEAD — so this close passes where the
        // whole-tree fence would have cried close_source_stale.
        let repo = git_repo();
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        std::fs::write(repo.path().join("src/lib.rs"), "fn a() {}\n").unwrap();
        let ticket = verifying_scoped_ticket_with_handoff(repo.path(), &["src/**"]);
        let run = |args: &[&str]| {
            assert!(StdCommand::new("git")
                .arg("-C")
                .arg(repo.path())
                .args(args)
                .status()
                .unwrap()
                .success());
        };
        run(&["add", "src/lib.rs"]);
        run(&["commit", "-q", "-m", "another ticket landed src"]);
        seal_passing_scoped_lane_receipt(repo.path(), &ticket, "agent:review-correctness");
        let report = evaluate_close(repo.path(), &human("quan"), &ticket).unwrap();
        assert!(
            !report
                .violations
                .iter()
                .any(|v| v.code == "close_source_stale"),
            "a foreign commit must not stale a scoped ticket: {:?}",
            report.violations
        );
        let closed = close(repo.path(), &human("quan"), "TK-a3f9").unwrap();
        assert_eq!(closed["status"], "done");
    }

    #[test]
    fn close_of_a_scoped_ticket_still_catches_an_in_scope_edit() {
        // The scope fence is narrower, not blind: an edit inside `touches`
        // between handoff and close still stales it.
        let repo = git_repo();
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        std::fs::write(repo.path().join("src/lib.rs"), "fn a() {}\n").unwrap();
        let ticket = verifying_scoped_ticket_with_handoff(repo.path(), &["src/**"]);
        std::fs::write(repo.path().join("src/lib.rs"), "fn a() { edited }\n").unwrap();
        seal_passing_scoped_lane_receipt(repo.path(), &ticket, "agent:review-correctness");
        let report = evaluate_close(repo.path(), &human("quan"), &ticket).unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "close_source_stale"));
    }

    #[test]
    fn close_human_required_is_reported_for_a_high_risk_profile() {
        let repo = git_repo();
        let mut ticket = active_ticket();
        ticket["surface"] = json!("api");
        ticket["risk"] = json!("high");
        crate::store::issues::mutate(repo.path(), |mut records| {
            records.push(ticket.clone());
            Ok(records)
        })
        .unwrap();
        seal_passing_verify_receipt(repo.path(), &ticket);
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

    // --- Plan 0025 E1: unclassified friction blocks the story, not the ticket ---

    #[test]
    fn close_story_friction_unclassified_is_reported() {
        let repo = git_repo();
        let (story, _ticket) = story_with_ticket(repo.path(), "done");
        crate::kernel::issues::append_note(
            repo.path(),
            &agent("worker"),
            "TK-2222",
            "the packet's anchors were wrong twice",
            crate::kernel::issues::NoteKind::Friction,
        )
        .unwrap();
        let all = crate::store::issues::read_all(repo.path()).unwrap();
        let report = evaluate_close_story(repo.path(), &story, &all).unwrap();
        let violation = report
            .violations
            .iter()
            .find(|v| v.code == "close_story_friction_unclassified")
            .expect("an unclassified friction must block the story");
        assert!(violation.message.contains("TK-2222#evt_"), "{violation:?}");
        assert!(
            violation
                .message
                .ends_with("`pulse learn dismiss <id> --all --reason …`"),
            "{violation:?}"
        );
        // A plain note is not friction: nothing to report.
        crate::kernel::issues::append_note(
            repo.path(),
            &agent("worker"),
            "TK-2222",
            "not friction",
            crate::kernel::issues::NoteKind::Note,
        )
        .unwrap();
        let all = crate::store::issues::read_all(repo.path()).unwrap();
        let report = evaluate_close_story(repo.path(), &story, &all).unwrap();
        assert_eq!(
            report
                .violations
                .iter()
                .filter(|v| v.code == "close_story_friction_unclassified")
                .count(),
            1,
            "adding a plain note must not add a second violation"
        );
    }

    #[test]
    fn close_story_passes_once_the_frictions_are_dismissed() {
        let repo = git_repo();
        let (story, _ticket) = story_with_ticket(repo.path(), "done");
        crate::kernel::issues::append_note(
            repo.path(),
            &agent("worker"),
            "TK-2222",
            "ticket-specific quirk",
            crate::kernel::issues::NoteKind::Friction,
        )
        .unwrap();
        crate::event::read_events(repo.path())
            .unwrap()
            .into_iter()
            .filter(|event| event.event_type == "note.recorded")
            .map(|event| event.id)
            .next_back()
            .unwrap();
        let all = crate::store::issues::read_all(repo.path()).unwrap();
        let report = evaluate_close_story(repo.path(), &story, &all).unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "close_story_friction_unclassified"));

        // `pulse learn dismiss <id> --all --reason …` settles the story: the
        // dismissal is a classification, the loop stays honest, the gate
        // opens — exactly the skill's "a friction that stays Ticket-specific
        // is that Ticket's business".
        crate::learn::dismiss(
            repo.path(),
            &agent("worker"),
            "TK-2222",
            &[],
            true,
            "ticket-specific: the anchors listed a renamed module",
        )
        .unwrap();
        let all = crate::store::issues::read_all(repo.path()).unwrap();
        let report = evaluate_close_story(repo.path(), &story, &all).unwrap();
        assert!(report.is_clean(), "{:?}", report.violations);
    }

    // --- Plan 0025 F4: a story's rules and exceptions must live in docs/ ---

    /// Patch ST-1111's rules/exceptions/docs_written in the store and return
    /// the fresh record.
    fn story_with_ids(repo: &Path, patch: Value) -> Value {
        crate::store::issues::mutate(repo, |mut records| {
            if let Some(object) = records
                .iter_mut()
                .find(|record| record["id"] == "ST-1111")
                .and_then(Value::as_object_mut)
            {
                for (key, value) in patch.as_object().into_iter().flatten() {
                    object.insert(key.clone(), value.clone());
                }
            }
            Ok(records)
        })
        .unwrap();
        let all = crate::store::issues::read_all(repo).unwrap();
        require(&all, "ST-1111").unwrap().clone()
    }

    fn write_story_doc(repo: &Path, rel: &str, text: &str) {
        let full = repo.join(rel);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }

    #[test]
    fn close_story_docs_missing_is_reported_when_rules_have_no_docs_written() {
        let repo = git_repo();
        let _ = story_with_ticket(repo.path(), "done");
        let story = story_with_ids(
            repo.path(),
            json!({"rules": [{"id": "BR-1", "text": "it works"}]}),
        );
        let all = crate::store::issues::read_all(repo.path()).unwrap();
        let report = evaluate_close_story(repo.path(), &story, &all).unwrap();
        let violation = report
            .violations
            .iter()
            .find(|v| v.code == "close_story_docs_missing")
            .expect("rules without docs_written must block the story");
        assert!(violation.message.contains("1 rule(s)"), "{violation:?}");
        assert!(violation.message.contains("docs_written"), "{violation:?}");
        assert!(
            violation.message.contains("pulse work update"),
            "{violation:?}"
        );
    }

    #[test]
    fn a_docs_written_path_outside_docs_or_missing_is_reported() {
        let repo = git_repo();
        std::fs::write(repo.path().join("outside.md"), "BR-1\n").unwrap();
        let _ = story_with_ticket(repo.path(), "done");
        let story = story_with_ids(
            repo.path(),
            json!({
                "rules": [{"id": "BR-1", "text": "it works"}],
                "docs_written": ["outside.md", "docs/absent.md"],
            }),
        );
        let all = crate::store::issues::read_all(repo.path()).unwrap();
        let report = evaluate_close_story(repo.path(), &story, &all).unwrap();
        let violation = report
            .violations
            .iter()
            .find(|v| v.code == "close_story_docs_missing")
            .expect("a path outside docs/ and an absent file must both block");
        assert!(
            violation.message.contains("outside.md is not under docs/"),
            "{violation:?}"
        );
        assert!(
            violation.message.contains("docs/absent.md does not exist"),
            "{violation:?}"
        );
    }

    #[test]
    fn a_rule_id_is_not_satisfied_by_a_longer_or_prefixed_id() {
        let repo = git_repo();
        write_story_doc(
            repo.path(),
            "docs/rules.md",
            "covered by X-BR-1 for the variant and BR-10 for the rest\n",
        );
        let _ = story_with_ticket(repo.path(), "done");
        let story = story_with_ids(
            repo.path(),
            json!({
                "rules": [{"id": "BR-1", "text": "it works"}],
                "docs_written": ["docs/rules.md"],
            }),
        );
        let all = crate::store::issues::read_all(repo.path()).unwrap();
        let report = evaluate_close_story(repo.path(), &story, &all).unwrap();
        let violation = report
            .violations
            .iter()
            .find(|v| v.code == "close_story_docs_missing")
            .expect("BR-10/X-BR-1 must not satisfy BR-1");
        assert!(violation.message.contains("BR-1"), "{violation:?}");
    }

    #[test]
    fn docs_written_covering_every_rule_and_exception_id_passes() {
        let repo = git_repo();
        write_story_doc(
            repo.path(),
            "docs/rules.md",
            "The behavior holds (BR-1); except when E-1 applies.\n",
        );
        let _ = story_with_ticket(repo.path(), "done");
        let story = story_with_ids(
            repo.path(),
            json!({
                "rules": [{"id": "BR-1", "text": "it works"}],
                "exceptions": [{"id": "E-1", "text": "except then"}],
                "docs_written": ["docs/rules.md"],
            }),
        );
        let all = crate::store::issues::read_all(repo.path()).unwrap();
        let report = evaluate_close_story(repo.path(), &story, &all).unwrap();
        assert!(
            !report
                .violations
                .iter()
                .any(|v| v.code == "close_story_docs_missing"),
            "{:?}",
            report.violations
        );
    }

    #[test]
    fn contains_word_demands_whole_ids() {
        assert!(contains_word("as per BR-1.", "BR-1"));
        assert!(contains_word("(BR-1) and E-1", "E-1"));
        assert!(contains_word("multi\nline BR-1\nok", "BR-1"));
        assert!(!contains_word("covered by BR-10 only", "BR-1"));
        assert!(!contains_word("see X-BR-1 for the variant", "BR-1"));
        assert!(!contains_word("nothing here", "BR-1"));
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

    /// Record a passing story-scope qa receipt covering `cases` at `commit`.
    fn seal_story_qa_receipt(repo: &Path, role: &str, cases: &[&str], commit: &str) {
        record_receipt(
            repo,
            None,
            NewReceipt {
                kind: "lane".to_string(),
                subject: ReceiptSubject {
                    id: "ST-1111".to_string(),
                    revision: None,
                },
                actor: format!("agent:{role}"),
                source: ReceiptSource {
                    commit: commit.to_string(),
                    dirty_hash: "sha256:0".to_string(),
                },
                run_id: None,
                payload: json!({
                    "role": role, "verdict": "pass",
                    "cases": cases.iter().map(|id| json!({"id": id, "status": "pass"})).collect::<Vec<_>>(),
                }),
                artifact_paths: Vec::new(),
            },
        )
        .unwrap();
    }

    #[test]
    fn close_story_source_dirty_is_reported() {
        // Dogfood ST-1 F15: story close had no source fence — uncommitted
        // work could slip under the milestone. A ticket-level close compares
        // a handoff snapshot; a story hands nothing off, so the fence is
        // simply "the tree is clean at story close". Committed changes are
        // fine: the legitimate post-close-fix flow commits and re-validates
        // through story-scope qa.
        let repo = git_repo();
        let (story, _ticket) = story_with_ticket(repo.path(), "done");
        std::fs::write(repo.path().join("uncommitted.txt"), "x\n").unwrap();
        let all = crate::store::issues::read_all(repo.path()).unwrap();
        let report = evaluate_close_story(repo.path(), &story, &all).unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "close_story_source_dirty"));
    }

    #[test]
    fn close_story_coverage_unions_across_passing_qa_receipts() {
        // Dogfood ST-1 F17: a multi-surface story legitimately splits its
        // high-priority cases across qa-api and qa-ui; requiring ONE receipt
        // to cover everything forced targets to invent a combined lane.
        let repo = git_repo();
        let (mut story, _ticket) = story_with_ticket(repo.path(), "done");
        story["qa_cases"] = json!([
            {"id": "QA-001", "intent": "x", "priority": "high"},
            {"id": "QA-002", "intent": "y", "priority": "high"},
        ]);
        let commit = source::head_commit(repo.path()).unwrap();
        seal_story_qa_receipt(repo.path(), "qa-api", &["QA-001"], &commit);
        seal_story_qa_receipt(repo.path(), "qa-ui", &["QA-002"], &commit);
        let all = crate::store::issues::read_all(repo.path()).unwrap();
        let report = evaluate_close_story(repo.path(), &story, &all).unwrap();
        assert!(report.is_clean(), "{:?}", report.violations);
    }

    #[test]
    fn close_story_qa_receipts_from_an_older_head_do_not_cover() {
        // Plan §7.4: qa receipts count "trên HEAD hiện tại" — a pass on a
        // commit that is no longer HEAD proves nothing about this tree.
        let repo = git_repo();
        let (mut story, _ticket) = story_with_ticket(repo.path(), "done");
        story["qa_cases"] = json!([{"id": "QA-001", "intent": "x", "priority": "high"}]);
        seal_story_qa_receipt(repo.path(), "qa-api", &["QA-001"], "commit-from-the-past");
        let all = crate::store::issues::read_all(repo.path()).unwrap();
        let report = evaluate_close_story(repo.path(), &story, &all).unwrap();
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "close_story_qa_not_satisfied"));
    }
}
