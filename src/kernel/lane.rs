//! Lane preparation, output validation and sealing (plan 0022 §8.3-8.4).
//!
//! Three steps, each its own CLI command so the host (Claude Code's Task
//! tool, Codex's `spawn_agent`, or a human) owns dispatch and Pulse owns
//! truth:
//!
//! 1. [`prepare`] (`pulse lane input`) checks the lane is allowed to run,
//!    writes the lane's bounded input file, and records the pre-run source
//!    snapshot the mutation check compares against;
//! 2. the lane itself runs — outside Pulse — and writes
//!    `.pulse/evidence/<id>/<role>.json`;
//! 3. [`seal`] (`pulse lane seal`) applies the seal-time MUST rules and
//!    records the one `lane` receipt, under the *calling* actor's identity.
//!
//! The receipt carries the caller's actor rather than a name Pulse
//! synthesizes, so the close gate's independence condition ("the lane actor
//! is not the handoff actor") describes who actually reviewed. [`seal`]
//! refuses a lane sealed by the actor that handed the Ticket off.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{PulseError, Result};
use crate::event::emit_event;
use crate::evidence::receipt::{
    list_receipts, record_receipt, NewReceipt, ReceiptEnvelope, ReceiptSource, ReceiptSubject,
};
use crate::identity::actor::ActorRef;
use crate::kernel::issues::{apply_to_record, bump, find, require, set_status};
use crate::kernel::profile::{self, profile_key};
use crate::kernel::reservation::touches_of;
use crate::kernel::roles::{authorize, Action};
use crate::kernel::scope;
use crate::kernel::verify::{run_argv, write_observed_log, Observed};
use crate::source::Source;
use crate::storage::WriteGuard;
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

/// One lane run's name on disk (decision 0027 C2): a lane with no panel is
/// a single slot (`review-correctness`), a panel seat is `role.n`
/// (`review-correctness.2`). Every path a lane owns — input, snapshot,
/// output, log — is keyed on [`LaneSlot::stem`], so a seat's files can
/// never collide with the lane's or another seat's. Kept as one type so the
/// seat spelling lives in one place rather than in `if let Some(seat)` at
/// each call site.
#[derive(Debug, Clone, Copy)]
struct LaneSlot<'a> {
    role: &'a str,
    seat: Option<u32>,
}

impl LaneSlot<'_> {
    fn stem(&self) -> String {
        match self.seat {
            Some(seat) => format!("{}.{}", self.role, seat),
            None => self.role.to_string(),
        }
    }
}

fn evidence_dir(repo_root: &Path, ticket_id: &str) -> std::path::PathBuf {
    repo_root.join(".pulse/evidence").join(ticket_id)
}

fn artifact_exists(repo_root: &Path, ticket_id: &str, relative: &str) -> bool {
    evidence_dir(repo_root, ticket_id).join(relative).exists()
}

/// Everything the seal-time corrections need beyond the lane's own output:
/// which subject is being sealed, by whom, on which tree, and the receipts
/// already on file (decision 0026 D2 reads the sealing actor's own `verify`
/// receipt).
#[derive(Clone, Copy)]
struct SealContext<'a> {
    repo_root: &'a Path,
    ticket_id: &'a str,
    role: &'a str,
    actor: &'a ActorRef,
    /// The subject's declared `verify[]` names. Empty for a Story or any
    /// record that declares none, which is what makes the rule below
    /// inapplicable there.
    declared_verify: &'a [String],
    /// The subject's `touches`, so a receipt fence compares like-for-like
    /// (decision 0025 B6).
    touches: &'a [String],
    /// The tree being sealed — the fence the new receipt will carry.
    source: &'a Source,
    receipts: &'a [ReceiptEnvelope],
    /// Decision 0027 C3: the reconciled verdict is a different subject. Its
    /// `pass` is backed by every *seat's* own `verify` receipt, checked when
    /// each seat sealed; re-checking here would demand a receipt from the
    /// reconciliation actor, which is not a reviewer and never ran anything.
    /// When true, D2's rule below is already satisfied and is skipped.
    verify_rule_satisfied: bool,
}

/// Whether `actor` sealed a passing `verify` receipt for this subject on the
/// tree being sealed (decision 0026 D2).
fn has_own_verify_receipt(context: &SealContext<'_>) -> bool {
    context.receipts.iter().any(|receipt| {
        receipt.kind == "verify"
            && receipt.actor == context.actor.as_kind_id()
            && profile::same_fence_touches(
                context.touches,
                (&receipt.source.commit, &receipt.source.dirty_hash),
                (&context.source.commit, &context.source.dirty_hash),
            )
            && receipt.payload.get("passed").and_then(Value::as_bool) == Some(true)
            && receipt
                .payload
                .get("results")
                .and_then(Value::as_array)
                .is_some_and(|results| {
                    !results.is_empty()
                        && results
                            .iter()
                            .all(|result| result.get("exit").and_then(Value::as_i64) == Some(0))
                })
    })
}

/// Apply plan §8.4's seal-time corrections in place. Returns whether the
/// verdict was corrected from `pass` (for the `lane_verdict_corrected`
/// event).
fn apply_seal_corrections(output: &mut LaneOutput, context: SealContext<'_>) -> bool {
    let SealContext {
        repo_root,
        ticket_id,
        role,
        declared_verify,
        ..
    } = context;
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

    // Decision 0026 D2: a review lane that reports `pass` on a ticket with a
    // declared verify[] must have run `pulse verify` itself. The worker's
    // receipt is the worker's claim — an independent lane's whole value is
    // an independent observation. Symmetric with "qa-ui pass with no
    // screenshot" above: no evidence of its own, no pass. Deliberately not
    // applied to qa-*/check-* lanes, whose evidence is a case result or a
    // document rather than a command's output.
    if original_verdict == "pass"
        && role.starts_with("review-")
        && !declared_verify.is_empty()
        && !context.verify_rule_satisfied
        && !has_own_verify_receipt(&context)
    {
        output.verdict = "inconclusive".to_string();
        corrected = true;
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
    // The planner's how-to, written before any work started — part of the
    // claim to check, not the worker's narrative.
    object.insert(
        "description".to_string(),
        ticket.get("description").cloned().unwrap_or(Value::Null),
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
        // Plan 0025 B6: a scoped ticket's reviewer sees only files inside
        // its scope — a dirty file outside belongs to another ticket's
        // review, not this one.
        json!(changed_files_since(
            repo_root,
            &handoff_commit,
            &touches_of(ticket)
        )),
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
/// filtered to drop every fenced-out path (`.pulse/**`, the root harness
/// configs, and the target's `fence_ignore` — plan 0025 F3 unified this
/// with the snapshot rule, so a file that cannot stale a fence also cannot
/// reach a lane input or a stale-doc advisory) and — when `touches` is
/// non-empty — to keep only the subject's scope (plan 0025 B6).
fn changed_files_since(repo_root: &Path, handoff_commit: &str, touches: &[String]) -> Vec<String> {
    let ignore = profile::fence_ignore(repo_root);
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
        .filter(|path| !crate::source::is_fenced_out(path, &ignore))
        .filter(|path| touches.is_empty() || scope::covers(touches, path))
        .collect()
}

/// The files one ticket changed, as plan 0025 F3's reverse doc gate reads
/// them: the worktree's dirty paths and — when a handoff receipt exists —
/// the diff since that handoff commit, fenced and scope-filtered
/// ([`changed_files_since`]). One source for both the handoff advisory
/// (`docs_maybe_stale`) and `pulse docs check --ticket`, so the two can
/// never disagree about what "changed" means.
///
/// # Errors
/// Propagates a receipt-list read failure.
pub(crate) fn changed_files_for(repo_root: &Path, ticket: &Value) -> Result<Vec<String>> {
    let id = ticket.get("id").and_then(Value::as_str).unwrap_or("");
    let commit = latest_handoff_commit(repo_root, id)?;
    Ok(changed_files_since(repo_root, &commit, &touches_of(ticket)))
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

/// Validate `<ticket_id>`'s lane output and seal the lane receipt. A
/// `--seat n` run validates `.pulse/evidence/<ticket_id>/<role>.<n>.json`
/// instead and seals a `lane_seat` receipt (decision 0027 C2).
///
/// The workspace check and the receipt source use the subject record's own
/// fence (plan 0025 B6): its `touches` scope when the record declares one,
/// the whole tree otherwise. The record is resolved inside the lock, so a
/// caller cannot hand in a stale view of `touches`.
///
/// # Errors
/// `lane_mutated_workspace` if the fence changed since `dirty_hash_before`
/// (no receipt). `lane_output_invalid` if the file is absent or fails
/// schema. `lane_commit_mismatch` if `environment.commit` is not current
/// HEAD (no receipt). `lane_seat_actor_reused` if the same actor already
/// holds another seat of this round.
pub fn validate_and_seal(
    repo_root: &Path,
    actor: &ActorRef,
    ticket_id: &str,
    role: &str,
    before: &Source,
    seat: Option<u32>,
) -> Result<ReceiptEnvelope> {
    let slot = LaneSlot { role, seat };
    // One lock for read -> validate -> receipt -> verdict write (plan 0025
    // A4). The seal path is the read/evaluate/record/mutate sequence here,
    // not `seal` above it, so the guard lives in this function and callers
    // must not already hold one (`WriteGuard` is not re-entrant).
    let (receipt, corrected) = {
        let guard = WriteGuard::acquire(repo_root)?;
        let records = issues::read_all(repo_root)?;
        // A subject that is not in the store (a stray evidence directory)
        // fences the whole tree, like a record without `touches`.
        let touches = find_ticket(&records, ticket_id)
            .map(touches_of)
            .unwrap_or_default();
        let now_source = profile::fence_for_touches(repo_root, &touches)?;
        if now_source.dirty_hash != before.dirty_hash {
            return Err(PulseError::kernel(
                "lane_mutated_workspace",
                format!(
                    "the tree changed while {} ran: dirty paths now {:?}",
                    slot.stem(),
                    now_source.dirty_paths
                ),
                "a lane may only write under .pulse/evidence/<id>/; nothing else may change",
            ));
        }

        let output_path = evidence_dir(repo_root, ticket_id).join(format!("{}.json", slot.stem()));
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

        // Decision 0026 D2 needs the receipts on file plus the subject's
        // declared verify[] names, not just the lane's own output.
        let receipts = list_receipts(repo_root)?.receipts;

        // Decision 0027 C2: a seat belongs to one round, and one actor may
        // hold at most one seat per round — otherwise "N independent
        // reviewers" is one session writing N files. Resealing the *same*
        // seat is a rerun and stays allowed.
        let round = seat.map(|_| round_key(&receipts, ticket_id, &now_source.commit));
        if let (Some(seat), Some(round)) = (seat, round.as_deref()) {
            let reused = receipts.iter().any(|receipt| {
                receipt.kind == "lane_seat"
                    && receipt.subject.id == ticket_id
                    && receipt.payload.get("role").and_then(Value::as_str) == Some(role)
                    && receipt.payload.get("handoff").and_then(Value::as_str) == Some(round)
                    && receipt.payload.get("seat").and_then(Value::as_u64) != Some(u64::from(seat))
                    && receipt.actor == actor.as_kind_id()
            });
            if reused {
                return Err(PulseError::kernel(
                    "lane_seat_actor_reused",
                    format!(
                        "{} already sealed another seat of {role}'s panel this round",
                        actor.as_kind_id()
                    ),
                    "each seat is its own reviewer: --actor agent:<role>-<seat>",
                ));
            }
        }

        // Plan 0025 E2: "the ticket declares verify" means the required
        // name set — verify[] plus every enforceable `learning.<id>` — the
        // same function the handoff gate reads.
        let declared_verify: Vec<String> = find_ticket(&records, ticket_id)
            .map(|subject| crate::kernel::verify::required_names(repo_root, subject))
            .transpose()?
            .unwrap_or_default();
        let corrected = apply_seal_corrections(
            &mut output,
            SealContext {
                repo_root,
                ticket_id,
                role,
                actor,
                declared_verify: &declared_verify,
                touches: &touches,
                source: &now_source,
                receipts: &receipts,
                verify_rule_satisfied: false,
            },
        );

        let artifact_paths: Vec<String> = output
            .cases
            .iter()
            .flat_map(|case| case.artifacts.iter())
            .map(|artifact| format!(".pulse/evidence/{ticket_id}/{artifact}"))
            .collect();

        // Decision 0027 C2: a seat is not the lane's receipt — the close
        // gate reads only `kind: "lane"`, so a seat can never satisfy it,
        // and a seat's `fail` must not rework the Ticket before the panel
        // has been reconciled (that bounce belongs to `reconcile`).
        let mut payload = json!({
            "role": role,
            "verdict": output.verdict,
            "acceptance": output.acceptance,
            "cases": output.cases,
            "findings": output.findings,
            "commands_run": output.commands_run,
            "environment": output.environment,
        });
        if let Some(seat) = seat {
            payload["seat"] = json!(seat);
            payload["handoff"] = json!(round.clone().unwrap_or_default());
        }

        let receipt = record_receipt(
            repo_root,
            None,
            NewReceipt {
                kind: if seat.is_some() { "lane_seat" } else { "lane" }.to_string(),
                subject: ReceiptSubject {
                    id: ticket_id.to_string(),
                    revision: None,
                },
                actor: actor.as_kind_id(),
                // Plan 0025 B6: the receipt carries the subject's fence, so
                // the close gate can compare like with like.
                source: ReceiptSource {
                    commit: now_source.commit.clone(),
                    dirty_hash: now_source.dirty_hash.clone(),
                },
                run_id: None,
                payload,
                artifact_paths,
            },
        )?;

        if seat.is_none() && find_ticket(&records, ticket_id).is_some() {
            write_verdict(
                repo_root,
                &guard,
                ticket_id,
                role,
                &receipt.id,
                &output.verdict,
                &now_source.commit,
            )?;
        }

        (receipt, corrected)
    };

    if corrected {
        emit_event(
            repo_root,
            "receipt.recorded",
            actor.as_kind_id(),
            ticket_id,
            json!({"lane_verdict_corrected": true, "role": role}),
            chrono::Utc::now(),
        )?;
    }

    Ok(receipt)
}

/// The round a seat or reconcile receipt belongs to (decision 0027): the id
/// of the subject's latest `handoff` receipt, so a re-handoff opens a new
/// round and the old round's seats fall out of it. A subject with no handoff
/// of its own (a story-scope qa lane) keys on the commit instead.
fn round_key(receipts: &[ReceiptEnvelope], subject_id: &str, commit: &str) -> String {
    receipts
        .iter()
        .filter(|receipt| receipt.kind == "handoff" && receipt.subject.id == subject_id)
        .max_by(|left, right| left.id.cmp(&right.id))
        .map(|receipt| receipt.id.clone())
        .unwrap_or_else(|| format!("head:{commit}"))
}

/// Record `verdicts[role]` on the subject. One copy so the lane seal and the
/// panel reconciliation cannot write different shapes (decision 0027 C3).
fn write_verdict(
    repo_root: &Path,
    guard: &WriteGuard,
    ticket_id: &str,
    role: &str,
    receipt_id: &str,
    verdict: &str,
    commit: &str,
) -> Result<()> {
    issues::mutate_locked(guard, repo_root, |records| {
        apply_to_record(records, ticket_id, |record| {
            let object = record.as_object_mut().expect("records are always objects");
            let verdicts = object
                .entry("verdicts")
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
            verdicts
                .as_object_mut()
                .expect("verdicts is always an object")
                .insert(
                    role.to_string(),
                    json!({"receipt": receipt_id, "verdict": verdict, "commit": commit}),
                );
            bump(object);
            Ok(())
        })
    })?;
    Ok(())
}

/// Where `pulse lane input` records the pre-run source snapshot that
/// [`seal`] compares against. Under `.pulse/runtime/`, so it is ignored
/// state a `pulse init` `.gitignore` entry already covers: losing it costs
/// one `pulse lane input`, never a receipt.
fn snapshot_path(repo_root: &Path, ticket_id: &str, slot: LaneSlot<'_>) -> PathBuf {
    repo_root
        .join(".pulse/runtime/lane")
        .join(ticket_id)
        .join(format!("{}.snapshot.json", slot.stem()))
}

fn lane_input_path(repo_root: &Path, ticket_id: &str, slot: LaneSlot<'_>) -> PathBuf {
    repo_root
        .join(".pulse/runtime/lane")
        .join(ticket_id)
        .join(format!("{}-input.json", slot.stem()))
}

/// The profile keys a record routes through, own key first: its own
/// `<surface>-<risk>`, then — for a Story — each `qa_case`'s surface
/// (dogfood ST-1, F18). A Story classified `api` must still run its ui
/// cases' `qa-ui` lane without `--force`; Tickets keep the single-surface
/// rule.
fn profile_keys_for(record: &Value) -> Vec<String> {
    let record_role = record
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("implementation");
    let surface = record.get("surface").and_then(Value::as_str);
    let risk = record.get("risk").and_then(Value::as_str);
    let mut keys = vec![profile_key(record_role, surface, risk)];
    if record.get("kind").and_then(Value::as_str) != Some("story") {
        return keys;
    }
    for case_surface in record
        .get("qa_cases")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|case| case.get("surface").and_then(Value::as_str))
    {
        let key = profile_key(record_role, Some(case_surface), risk);
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

/// Whether `role` may run against `record` without `--force`: it is in the
/// record's own `surface-risk` profile, or — for a Story — in the profile of
/// any of its `qa_cases`' surfaces.
fn lane_in_profile(repo_root: &Path, record: &Value, role: &str) -> Result<bool> {
    let config = profile::load(repo_root)?;
    let mut keys = profile_keys_for(record);
    // The record's own profile must exist (a missing one is a data problem,
    // not a reason to run the lane); a case profile may be absent.
    let own = keys.remove(0);
    if profile::profile_for(&config, &own)?
        .lanes
        .iter()
        .any(|lane| lane == role)
    {
        return Ok(true);
    }
    for key in keys {
        if let Ok(profile) = profile::profile_for(&config, &key) {
            if profile.lanes.iter().any(|lane| lane == role) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// The panel `role` runs as for `record`, when one is declared (decision
/// 0027 C1). Routed exactly like [`lane_in_profile`], so a panel declared on
/// a Story's case profile applies to that case's lane.
fn panel_for(repo_root: &Path, record: &Value, role: &str) -> Result<Option<profile::Panel>> {
    let config = profile::load(repo_root)?;
    let mut keys = profile_keys_for(record);
    let own = keys.remove(0);
    if let Some(panel) = profile::profile_for(&config, &own)?.panel(role) {
        return Ok(Some(panel.clone()));
    }
    for key in keys {
        if let Ok(profile) = profile::profile_for(&config, &key) {
            if let Some(panel) = profile.panel(role) {
                return Ok(Some(panel.clone()));
            }
        }
    }
    Ok(None)
}

/// The hint every `lane_seat_invalid` carries. `PulseError::kernel` takes a
/// `&'static str`, so the count lives in the message, not the hint.
const SEAT_INVALID_HINT: &str =
    "a panel is declared in PULSE.md as `panels: {<role>: {count: N, quorum: M}}`; pass \
     `--seat <n>` only for a role that has one";

/// Enforce the seat rules shared by `prepare` and `seal` (decision 0027 C2):
/// a lane with a panel must pass `--seat` inside `1..=count`, and a lane
/// without one must not pass `--seat` at all.
fn check_seat(role: &str, panel: Option<&profile::Panel>, seat: Option<u32>) -> Result<()> {
    match (panel, seat) {
        (Some(panel), None) => Err(PulseError::kernel(
            "lane_seat_required",
            format!("{role} is a panel of {} in this profile", panel.count),
            "this lane is a panel: pass `--seat <n>` (1..=count) to `pulse lane input` and \
             `pulse lane seal`, then seal the reconciled verdict with `pulse lane reconcile`",
        )),
        (Some(panel), Some(seat)) if !(1..=panel.count).contains(&seat) => Err(PulseError::kernel(
            "lane_seat_invalid",
            format!(
                "{role} is a panel of {}, so --seat must be in 1..={} (got {seat})",
                panel.count, panel.count
            ),
            SEAT_INVALID_HINT,
        )),
        (None, Some(seat)) => Err(PulseError::kernel(
            "lane_seat_invalid",
            format!("{role} has no panel in this profile, so --seat {seat} is not accepted"),
            SEAT_INVALID_HINT,
        )),
        _ => Ok(()),
    }
}

/// The precondition every lane run shares (decision 0027: `prepare` and
/// `reconcile --prepare` must not drift): a Ticket must have handed off, and
/// a Story must carry `surface`/`risk` so its profile can be resolved.
fn require_lane_subject(record: &Value, id: &str) -> Result<()> {
    let kind = record.get("kind").and_then(Value::as_str).unwrap_or("");
    let status = record.get("status").and_then(Value::as_str).unwrap_or("");

    // A Ticket must have handed off; a Story has no `verifying` status in
    // its lifecycle at all (plan §4.7) — a story-scope qa lane (§10.6) runs
    // against whatever status the Story is currently in, most often `ready`.
    if kind == "ticket" && status != "verifying" {
        return Err(PulseError::kernel(
            "lane_not_verifying",
            format!("{id} is {status}, not verifying"),
            "a lane only runs against a Ticket that has handed off, or a Story for a story-scope qa lane",
        ));
    }

    // Unlike a Ticket, the ready gate never requires these on a Story, so a
    // Story can reach `ready` without them — that is a data problem
    // `--force` must not paper over, so this check runs either way.
    if kind == "story"
        && (record.get("surface").and_then(Value::as_str).is_none()
            || record.get("risk").and_then(Value::as_str).is_none())
    {
        return Err(PulseError::kernel(
            "profile_missing",
            format!("{id} has no surface/risk set; a story-scope lane run needs both to resolve a profile"),
            "set both first: `pulse work update <id> --set surface=<cli|api|ui|lib|docs> --set risk=<low|medium|high>`",
        ));
    }
    Ok(())
}

/// `pulse lane input <id> <role> [--seat <n>]`: authorize the lane against
/// the record's profile, write its bounded input file, and record the
/// pre-run source snapshot. Returns the input file's path — what the host
/// passes to the lane it dispatches.
///
/// A panel seat is the same run with `--seat n`: the input is byte-for-byte
/// the lane's (decision 0027 C2 — round 1 is blind), only the file names
/// carry the seat.
///
/// # Errors
/// `lane_not_verifying` if `id` is a Ticket that has not handed off.
/// `profile_missing` if a Story has no `surface`/`risk` to resolve a
/// profile with. `lane_not_in_profile` unless `force`. `lane_seat_required`
/// / `lane_seat_invalid` for a seat the profile does not accept. Propagates
/// a git error from [`source::snapshot`].
pub fn prepare(
    repo_root: &Path,
    actor: &ActorRef,
    id: &str,
    role: &str,
    force: bool,
    seat: Option<u32>,
) -> Result<PathBuf> {
    let records = issues::read_all(repo_root)?;
    let record = require(&records, id)?;
    require_lane_subject(record, id)?;

    if !force && !lane_in_profile(repo_root, record, role)? {
        return Err(PulseError::kernel(
            "lane_not_in_profile",
            format!("{role} is not in {id}'s profile"),
            "pass --force to run a lane outside the record's profile",
        ));
    }

    // `--force` runs a lane the profile does not list; such a lane has no
    // panel, so it has no seat either (decision 0027 C2).
    if force && seat.is_some() {
        return Err(PulseError::kernel(
            "lane_seat_invalid",
            format!("--force runs {role} outside {id}'s profile, so it has no panel to seat"),
            SEAT_INVALID_HINT,
        ));
    }
    let panel = if force {
        None
    } else {
        panel_for(repo_root, record, role)?
    };
    check_seat(role, panel.as_ref(), seat)?;
    let slot = LaneSlot { role, seat };

    let story = record
        .get("story")
        .and_then(Value::as_str)
        .and_then(|story_id| find(&records, story_id));
    let input_value = lane_input(repo_root, record, story, role)?;
    let input_path = lane_input_path(repo_root, id, slot);
    let dir = input_path
        .parent()
        .expect("lane input path always has a parent");
    fs::create_dir_all(dir).map_err(|error| PulseError::io(dir, error))?;
    fs::write(&input_path, serde_json::to_vec_pretty(&input_value)?)
        .map_err(|error| PulseError::io(&input_path, error))?;

    let evidence = evidence_dir(repo_root, id);
    fs::create_dir_all(&evidence).map_err(|error| PulseError::io(&evidence, error))?;

    // Plan 0025 B6: the pre-run snapshot is the subject's own fence, so a
    // parallel ticket's dirty files elsewhere never look like a lane
    // mutation.
    let before = profile::fence_for(repo_root, record)?;
    let snapshot = snapshot_path(repo_root, id, slot);
    write_snapshot_file(&snapshot, actor, &before)?;

    // Paired with the `run.completed` [`seal`] emits: a lane dispatched and
    // never sealed used to be invisible in `events tail` (dogfood ST-1, F8).
    let mut payload = json!({"role": role, "input": input_path.to_string_lossy()});
    if let Some(seat) = seat {
        payload["seat"] = json!(seat);
    }
    emit_event(
        repo_root,
        "run.started",
        actor.as_kind_id(),
        id,
        payload,
        chrono::Utc::now(),
    )?;
    Ok(input_path)
}

fn read_snapshot(repo_root: &Path, ticket_id: &str, slot: LaneSlot<'_>) -> Result<Source> {
    let path = snapshot_path(repo_root, ticket_id, slot);
    let bytes = fs::read(&path).map_err(|_| {
        PulseError::kernel(
            "lane_not_prepared",
            format!("no pre-run snapshot at {}", path.display()),
            "run `pulse lane input <id> <role>` before the lane, and seal the same run",
        )
    })?;
    let value: Value = serde_json::from_slice(&bytes).map_err(PulseError::from)?;
    Ok(source_from_snapshot_json(&value))
}

/// Read a snapshot file's `Source` back out. One copy so a lane snapshot and
/// a reconciliation snapshot parse identically (decision 0027 C3).
fn source_from_snapshot_json(value: &Value) -> Source {
    Source {
        commit: value
            .get("commit")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        dirty_hash: value
            .get("dirty_hash")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        dirty_paths: value
            .get("dirty_paths")
            .and_then(Value::as_array)
            .map(|paths| {
                paths
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// Write a pre-run snapshot in the one shape every lane run uses.
fn write_snapshot_file(path: &Path, actor: &ActorRef, source: &Source) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(&json!({
        "at": chrono::Utc::now().to_rfc3339(),
        "actor": actor.as_kind_id(),
        "commit": source.commit,
        "dirty_hash": source.dirty_hash,
        "dirty_paths": source.dirty_paths,
    }))?;
    fs::write(path, bytes).map_err(|error| PulseError::io(path, error))?;
    Ok(())
}

/// The actor that sealed the latest `handoff` receipt for `ticket_id`, if
/// any (a story-scope qa lane has no handoff of its own).
fn latest_handoff_actor(repo_root: &Path, ticket_id: &str) -> Result<Option<String>> {
    Ok(list_receipts(repo_root)?
        .receipts
        .into_iter()
        .filter(|receipt| receipt.kind == "handoff" && receipt.subject.id == ticket_id)
        .max_by(|left, right| left.id.cmp(&right.id))
        .map(|receipt| receipt.actor))
}

/// `pulse lane seal <id> <role> [--seat <n>]`: validate the lane's output
/// and record its receipt. A `fail` verdict on the lane reworks the Ticket
/// (`verifying -> active`); a seat's `fail` does not (decision 0027 C2 — the
/// panel's single verdict is `reconcile`'s to give).
///
/// # Errors
/// `role_forbidden` if `actor` may not record a lane receipt.
/// `lane_not_prepared` if no snapshot from `pulse lane input` exists.
/// `lane_actor_not_independent` if `actor` handed this Ticket off.
/// `lane_seat_required` / `lane_seat_invalid` / `lane_seat_actor_reused` for
/// a seat the profile does not accept, or one this actor already holds else-
/// where in the round. Propagates [`validate_and_seal`]'s validation errors.
pub fn seal(
    repo_root: &Path,
    actor: &ActorRef,
    id: &str,
    role: &str,
    seat: Option<u32>,
) -> Result<Value> {
    authorize(actor, Action::LaneReceipt)?;

    // Decision 0027 C2: the seat rules are enforced before anything is read,
    // so a lane that needs a `--seat` says so instead of failing later as
    // "not prepared".
    let records = issues::read_all(repo_root)?;
    let panel = match find_ticket(&records, id) {
        Some(record) => panel_for(repo_root, record, role)?,
        // A stray evidence directory for a ticket that is not in the store:
        // no profile, so no panel to seat.
        None => None,
    };
    check_seat(role, panel.as_ref(), seat)?;
    let slot = LaneSlot { role, seat };

    let before = read_snapshot(repo_root, id, slot)?;

    // Checked here, not only at close (plan §7.3 condition 3): a reviewer
    // that is the worker learns it now, with the evidence still fresh,
    // instead of at a close that refuses a receipt already on the record.
    if latest_handoff_actor(repo_root, id)?.as_deref() == Some(actor.as_kind_id().as_str()) {
        return Err(PulseError::kernel(
            "lane_actor_not_independent",
            format!("{} handed {id} off and cannot also seal a lane on it", actor.as_kind_id()),
            "a lane is evidence from someone else: dispatch it as its own actor (--actor agent:<lane>) from a session that did not do the work",
        ));
    }

    let receipt = match validate_and_seal(repo_root, actor, id, role, &before, seat) {
        Ok(receipt) => receipt,
        Err(error) => {
            let mut payload =
                json!({"role": role, "outcome": "inconclusive", "reason": error.code()});
            if let Some(seat) = seat {
                payload["seat"] = json!(seat);
            }
            emit_event(
                repo_root,
                "run.completed",
                actor.as_kind_id(),
                id,
                payload,
                chrono::Utc::now(),
            )?;
            return Err(error);
        }
    };

    // Only a sealed run consumes its snapshot: a rejected output can be
    // fixed and resealed against the same pre-run state.
    let _ = fs::remove_file(snapshot_path(repo_root, id, slot));

    let verdict = receipt
        .payload
        .get("verdict")
        .and_then(Value::as_str)
        .unwrap_or("inconclusive");
    let mut payload = json!({"role": role, "outcome": "sealed", "verdict": verdict});
    if let Some(seat) = seat {
        payload["seat"] = json!(seat);
    }
    emit_event(
        repo_root,
        "run.completed",
        actor.as_kind_id(),
        id,
        payload,
        chrono::Utc::now(),
    )?;

    // Decision 0027 C2: a seat's `fail` is a vote, not the panel's verdict —
    // `count` seats must not each drag the Ticket back to `active` before
    // the reconciliation has run. Only the lane's own receipt reworks.
    if verdict == "fail" && seat.is_none() {
        return bounce_to_active(repo_root, actor, id, role);
    }
    Ok(require(&issues::read_all(repo_root)?, id)?.clone())
}

/// A `fail` verdict reworks the Ticket (`verifying -> active`). One copy so
/// the lane seal and the panel reconciliation bounce identically
/// (decision 0027 C3).
fn bounce_to_active(repo_root: &Path, actor: &ActorRef, id: &str, role: &str) -> Result<Value> {
    let updated = set_status(repo_root, id, "active")?;
    emit_event(
        repo_root,
        "issue.transitioned",
        actor.as_kind_id(),
        id,
        json!({"to": "active", "reason": "rework", "lane": role}),
        chrono::Utc::now(),
    )?;
    Ok(updated)
}

// ---------------------------------------------------------------------------
// Panel reconciliation (decision 0027 C3)
// ---------------------------------------------------------------------------

/// One panel seat's round-2 votes. The schema is closed: an unknown key means
/// the seat did not read the prompt, and its file is treated as absent rather
/// than guessed at.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct VoteFile {
    votes: Vec<Vote>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Vote {
    rid: String,
    vote: String,
    #[serde(default)]
    of: Option<String>,
    #[serde(default)]
    how: String,
}

/// A finding as the round-2 input presents it: anonymized (no seat, actor or
/// original id), labelled with a stable `rid`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReconcileInputFinding {
    rid: String,
    #[serde(rename = "ref", default)]
    reference: String,
    summary: String,
    owner: String,
    severity: String,
    #[serde(default)]
    check: Option<CheckSpec>,
}

/// The per-acceptance tally `--prepare` writes so the seal does not have to
/// re-read every seat to count votes.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AcceptanceSplit {
    id: String,
    pass: u32,
    fail: u32,
    not_checked: u32,
}

/// The round-2 input file: the lane's own input (re-derived, never copied)
/// plus the anonymized findings and the acceptance tally. Parsed for those
/// two keys only — the rest is the lane input a prompt reads.
#[derive(Debug, Clone, Deserialize)]
struct ReconcileInput {
    findings: Vec<ReconcileInputFinding>,
    acceptance_split: Vec<AcceptanceSplit>,
}

/// rid -> where a finding came from. Kept out of the round-2 input so round 2
/// stays blind to authorship (decision 0027 C3); losing it means re-running
/// `--prepare`, never guessing.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReconcileMap {
    handoff: String,
    findings: BTreeMap<String, ReconcileFinding>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReconcileFinding {
    /// The seat that raised it (1-based).
    seat: u32,
    /// The seat receipt that carried it.
    receipt: String,
    /// The finding's id inside that receipt.
    id: String,
}

fn reconcile_input_path(repo_root: &Path, ticket_id: &str, role: &str) -> PathBuf {
    repo_root
        .join(".pulse/runtime/lane")
        .join(ticket_id)
        .join(format!("{role}-reconcile-input.json"))
}

fn reconcile_map_path(repo_root: &Path, ticket_id: &str, role: &str) -> PathBuf {
    repo_root
        .join(".pulse/runtime/lane")
        .join(ticket_id)
        .join(format!("{role}-reconcile-map.json"))
}

/// The pre-round-2 fence. Named after the lane's own snapshot so `pulse
/// doctor`'s `<stem>.snapshot.json` scan reports a reconciliation prepared
/// and never sealed, exactly as it reports an unsealed lane.
fn reconcile_snapshot_path(repo_root: &Path, ticket_id: &str, role: &str) -> PathBuf {
    repo_root
        .join(".pulse/runtime/lane")
        .join(ticket_id)
        .join(format!("{role}.reconcile.snapshot.json"))
}

fn not_prepared(id: &str, role: &str) -> PulseError {
    PulseError::kernel(
        "reconcile_not_prepared",
        format!("no prepared reconciliation for {role} on {id}"),
        "run `pulse lane reconcile <id> <role> --prepare` first, then seal the same round",
    )
}

/// The newest `lane_seat` receipt for each seat of `role` in `round`
/// (decision 0027: a re-handoff opens a new round and old seats fall out).
fn seat_receipts_for(
    receipts: &[ReceiptEnvelope],
    subject_id: &str,
    role: &str,
    round: &str,
) -> BTreeMap<u32, ReceiptEnvelope> {
    let mut latest: BTreeMap<u32, ReceiptEnvelope> = BTreeMap::new();
    for receipt in receipts {
        if receipt.kind != "lane_seat" || receipt.subject.id != subject_id {
            continue;
        }
        if receipt.payload.get("role").and_then(Value::as_str) != Some(role) {
            continue;
        }
        if receipt.payload.get("handoff").and_then(Value::as_str) != Some(round) {
            continue;
        }
        let Some(seat) = receipt
            .payload
            .get("seat")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
        else {
            continue;
        };
        match latest.get(&seat) {
            Some(previous) if previous.id >= receipt.id => {}
            _ => {
                latest.insert(seat, receipt.clone());
            }
        }
    }
    latest
}

fn read_reconcile_input(repo_root: &Path, id: &str, role: &str) -> Option<ReconcileInput> {
    fs::read(reconcile_input_path(repo_root, id, role))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

fn read_reconcile_map(repo_root: &Path, id: &str, role: &str) -> Option<ReconcileMap> {
    fs::read(reconcile_map_path(repo_root, id, role))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

fn read_reconcile_snapshot(repo_root: &Path, id: &str, role: &str) -> Option<Source> {
    let bytes = fs::read(reconcile_snapshot_path(repo_root, id, role)).ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    Some(source_from_snapshot_json(&value))
}

/// Parse a round-2 vote file. `None` means the file is absent *or* fails the
/// closed schema — either way the seat has no vote, which weakens the
/// outcome but never fails the whole reconciliation (decision 0027 §8).
fn parse_vote_file(bytes: &[u8]) -> Option<Vec<Vote>> {
    let file: VoteFile = serde_json::from_slice(bytes).ok()?;
    for vote in &file.votes {
        if !matches!(
            vote.vote.as_str(),
            "confirmed" | "refuted" | "duplicate" | "cannot_reproduce"
        ) {
            return None;
        }
        match vote.vote.as_str() {
            "duplicate" if vote.of.is_none() => return None,
            "duplicate" => {}
            _ if vote.of.is_some() => return None,
            _ => {}
        }
        if vote.vote == "confirmed" && vote.how.trim().is_empty() {
            return None;
        }
    }
    Some(file.votes)
}

/// rid -> the target it merges into, for findings whose `duplicate` votes
/// reach quorum. Majority target; ties pick the lexicographically smallest so
/// the result never depends on vote order.
fn duplicate_merges(
    votes: &BTreeMap<String, Vec<(u32, Vote)>>,
    quorum: u32,
) -> BTreeMap<String, String> {
    let mut merges = BTreeMap::new();
    for (rid, entries) in votes {
        let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
        for (_, vote) in entries {
            if vote.vote == "duplicate" {
                if let Some(target) = vote.of.as_deref() {
                    *counts.entry(target).or_default() += 1;
                }
            }
        }
        if counts.values().sum::<u32>() < quorum {
            continue;
        }
        if let Some((target, _)) = counts.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0))) {
            merges.insert(rid.clone(), (*target).to_string());
        }
    }
    merges
}

/// Follow a chain of duplicate merges to its root. A cycle has no root, and
/// the merge is skipped rather than looped (decision 0027 C3 step 2).
fn resolve_root(rid: &str, merges: &BTreeMap<String, String>) -> Option<String> {
    let mut seen = BTreeSet::new();
    let mut current = rid.to_string();
    loop {
        if !seen.insert(current.clone()) {
            return None;
        }
        match merges.get(&current) {
            Some(next) => current = next.clone(),
            None => return Some(current),
        }
    }
}

/// `pulse lane reconcile <id> <role> --prepare`: collect this round's seat
/// findings into one blind, deterministic round-2 input and snapshot the
/// fence the second round runs against (decision 0027 C3).
///
/// # Errors
/// `role_forbidden` if `actor` may not record a lane receipt.
/// `lane_seat_invalid` when `role` has no panel. `reconcile_seats_missing`
/// when a seat of the round has no receipt. Propagates git and I/O errors.
pub fn reconcile_prepare(
    repo_root: &Path,
    actor: &ActorRef,
    id: &str,
    role: &str,
) -> Result<PathBuf> {
    authorize(actor, Action::LaneReceipt)?;
    let records = issues::read_all(repo_root)?;
    let record = require(&records, id)?;
    require_lane_subject(record, id)?;
    let Some(panel) = panel_for(repo_root, record, role)? else {
        return Err(PulseError::kernel(
            "lane_seat_invalid",
            format!("{role} has no panel in {id}'s profile, so there is nothing to reconcile"),
            SEAT_INVALID_HINT,
        ));
    };

    let now_source = profile::fence_for(repo_root, record)?;
    let receipts = list_receipts(repo_root)?.receipts;
    let round = round_key(&receipts, id, &now_source.commit);
    let seat_receipts = seat_receipts_for(&receipts, id, role, &round);
    let missing: Vec<u32> = (1..=panel.count)
        .filter(|seat| !seat_receipts.contains_key(seat))
        .collect();
    if !missing.is_empty() {
        return Err(PulseError::kernel(
            "reconcile_seats_missing",
            format!(
                "{role}: seats {missing:?} of {} have no receipt in this round",
                panel.count
            ),
            "seal every seat first: `pulse lane seal <id> <role> --seat <n>`",
        ));
    }

    // Every finding the seats raised, sorted deterministically so RF-n is
    // stable between `--prepare` and the seal. Pulse does not deduplicate by
    // meaning — two seats may describe one bug two ways, and only a reviewer
    // (voting `duplicate` in round 2) can honestly say so (decision 0027,
    // "Không giải quyết").
    let mut raised: Vec<RaisedFinding> = Vec::new();
    for (seat, receipt) in &seat_receipts {
        for finding in envelope_findings(receipt) {
            raised.push(RaisedFinding {
                rid: String::new(),
                reference: finding.reference,
                summary: finding.summary,
                owner: finding.owner,
                severity: finding.severity,
                check: finding.check,
                seat: *seat,
                receipt: receipt.id.clone(),
                original_id: finding.id,
            });
        }
    }
    raised.sort_by(|a, b| {
        (
            &a.reference,
            &a.summary,
            &a.severity,
            &a.owner,
            a.seat,
            &a.original_id,
        )
            .cmp(&(
                &b.reference,
                &b.summary,
                &b.severity,
                &b.owner,
                b.seat,
                &b.original_id,
            ))
    });
    for (index, finding) in raised.iter_mut().enumerate() {
        finding.rid = format!("RF-{}", index + 1);
    }

    let mut split: BTreeMap<String, (u32, u32, u32)> = BTreeMap::new();
    for receipt in seat_receipts.values() {
        for acceptance in envelope_acceptance(receipt) {
            let entry = split.entry(acceptance.id).or_default();
            match acceptance.status.as_str() {
                "pass" => entry.0 += 1,
                "fail" => entry.1 += 1,
                _ => entry.2 += 1,
            }
        }
    }

    let story = record
        .get("story")
        .and_then(Value::as_str)
        .and_then(|story_id| find(&records, story_id));
    let mut input = lane_input(repo_root, record, story, role)?;
    if let Some(object) = input.as_object_mut() {
        object.insert(
            "findings".to_string(),
            json!(raised
                .iter()
                .map(|finding| json!({
                    "rid": finding.rid,
                    "ref": finding.reference,
                    "summary": finding.summary,
                    "owner": finding.owner,
                    "severity": finding.severity,
                    "check": finding.check,
                }))
                .collect::<Vec<Value>>()),
        );
        object.insert(
            "acceptance_split".to_string(),
            json!(split
                .iter()
                .map(|(ac, (pass, fail, not_checked))| json!({
                    "id": ac,
                    "pass": pass,
                    "fail": fail,
                    "not_checked": not_checked,
                }))
                .collect::<Vec<Value>>()),
        );
    }

    let map = ReconcileMap {
        handoff: round,
        findings: raised
            .iter()
            .map(|finding| {
                (
                    finding.rid.clone(),
                    ReconcileFinding {
                        seat: finding.seat,
                        receipt: finding.receipt.clone(),
                        id: finding.original_id.clone(),
                    },
                )
            })
            .collect(),
    };

    let input_path = reconcile_input_path(repo_root, id, role);
    let map_path = reconcile_map_path(repo_root, id, role);
    let snapshot = reconcile_snapshot_path(repo_root, id, role);
    for path in [&input_path, &map_path, &snapshot] {
        let dir = path.parent().expect("reconcile paths always have a parent");
        fs::create_dir_all(dir).map_err(|error| PulseError::io(dir, error))?;
    }
    fs::write(&input_path, serde_json::to_vec_pretty(&input)?)
        .map_err(|error| PulseError::io(&input_path, error))?;
    fs::write(&map_path, serde_json::to_vec_pretty(&map)?)
        .map_err(|error| PulseError::io(&map_path, error))?;
    write_snapshot_file(&snapshot, actor, &now_source)?;

    emit_event(
        repo_root,
        "run.started",
        actor.as_kind_id(),
        id,
        json!({"role": role, "phase": "reconcile"}),
        chrono::Utc::now(),
    )?;
    Ok(input_path)
}

/// One finding lifted out of a seat receipt before it is anonymized.
struct RaisedFinding {
    rid: String,
    reference: String,
    summary: String,
    owner: String,
    severity: String,
    check: Option<CheckSpec>,
    seat: u32,
    receipt: String,
    original_id: String,
}

/// The findings a seat receipt carries, tolerating a payload from another
/// version (a finding that does not parse is dropped, not fatal).
fn envelope_findings(receipt: &ReceiptEnvelope) -> Vec<Finding> {
    receipt
        .payload
        .get("findings")
        .and_then(Value::as_array)
        .map(|findings| {
            findings
                .iter()
                .filter_map(|value| serde_json::from_value::<Finding>(value.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn envelope_acceptance(receipt: &ReceiptEnvelope) -> Vec<AcceptanceResult> {
    receipt
        .payload
        .get("acceptance")
        .and_then(Value::as_array)
        .map(|acceptance| {
            acceptance
                .iter()
                .filter_map(|value| serde_json::from_value::<AcceptanceResult>(value.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// `pulse lane reconcile <id> <role>`: arbitrate the round's findings and
/// seal the one `lane` receipt the close gate reads (decision 0027 C3).
///
/// Step order is the decision's: fence, merge duplicates, machine-arbitrate
/// every `check.argv`, weigh the rest by quorum, then apply the existing
/// seal corrections. Defined commands run outside the store lock.
///
/// # Errors
/// `role_forbidden` if `actor` may not record a lane receipt.
/// `lane_seat_invalid` when `role` has no panel. `reconcile_not_prepared`
/// when `--prepare` state is missing. `lane_mutated_workspace` if the tree
/// moved since `--prepare`. `reconcile_seats_missing` if a seat receipt is
/// gone.
pub fn reconcile(
    repo_root: &Path,
    actor: &ActorRef,
    id: &str,
    role: &str,
    timeout: Duration,
) -> Result<Value> {
    authorize(actor, Action::LaneReceipt)?;
    let records = issues::read_all(repo_root)?;
    let record = require(&records, id)?;
    let Some(panel) = panel_for(repo_root, record, role)? else {
        return Err(PulseError::kernel(
            "lane_seat_invalid",
            format!("{role} has no panel in {id}'s profile, so there is nothing to reconcile"),
            SEAT_INVALID_HINT,
        ));
    };

    let before =
        read_reconcile_snapshot(repo_root, id, role).ok_or_else(|| not_prepared(id, role))?;
    let map = read_reconcile_map(repo_root, id, role).ok_or_else(|| not_prepared(id, role))?;
    let input = read_reconcile_input(repo_root, id, role).ok_or_else(|| not_prepared(id, role))?;

    // Step 1: the tree must still be the one the seats reviewed.
    let now_source = profile::fence_for(repo_root, record)?;
    if now_source.dirty_hash != before.dirty_hash {
        return Err(PulseError::kernel(
            "lane_mutated_workspace",
            format!(
                "the tree changed while the {role} panel ran: dirty paths now {:?}",
                now_source.dirty_paths
            ),
            "a panel seat may only write under .pulse/evidence/<id>/; nothing else may change",
        ));
    }

    let receipts = list_receipts(repo_root)?.receipts;
    let seat_receipts = seat_receipts_for(&receipts, id, role, &map.handoff);
    let missing_seats: Vec<u32> = (1..=panel.count)
        .filter(|seat| !seat_receipts.contains_key(seat))
        .collect();
    if !missing_seats.is_empty() {
        return Err(PulseError::kernel(
            "reconcile_seats_missing",
            format!(
                "{role}: seats {missing_seats:?} of {} have no receipt in this round",
                panel.count
            ),
            "seal every seat first: `pulse lane seal <id> <role> --seat <n>`",
        ));
    }

    let known: BTreeSet<&str> = input.findings.iter().map(|f| f.rid.as_str()).collect();

    // Read round-2 votes. A dead or malformed seat is an absence, not an
    // error: a finding short of quorum downgrades itself, so one reviewer
    // dying makes the outcome weaker, never broken (decision 0027 §8).
    let mut by_rid_votes: BTreeMap<String, Vec<(u32, Vote)>> = BTreeMap::new();
    let mut missing_votes: Vec<u32> = Vec::new();
    let mut invalid: u32 = 0;
    for seat in 1..=panel.count {
        let path = evidence_dir(repo_root, id).join(format!("{role}.reconcile.{seat}.json"));
        let parsed = fs::read(&path)
            .ok()
            .and_then(|bytes| parse_vote_file(&bytes));
        let Some(votes) = parsed else {
            missing_votes.push(seat);
            continue;
        };
        for vote in votes {
            if !known.contains(vote.rid.as_str()) {
                invalid += 1;
                continue;
            }
            if vote.vote == "duplicate" {
                let Some(target) = vote.of.as_deref() else {
                    invalid += 1;
                    continue;
                };
                if !known.contains(target) {
                    invalid += 1;
                    continue;
                }
            }
            by_rid_votes
                .entry(vote.rid.clone())
                .or_default()
                .push((seat, vote));
        }
    }

    // Step 2: merge duplicates that reached quorum (round-2 votes only).
    let merges = duplicate_merges(&by_rid_votes, panel.quorum);
    let mut root_of: BTreeMap<String, String> = BTreeMap::new();
    for rid in merges.keys() {
        if let Some(root) = resolve_root(rid, &merges) {
            if &root != rid {
                root_of.insert(rid.clone(), root);
            }
        }
    }

    // Support = the raiser's seat plus every seat that voted `confirmed`,
    // unioned through the merges. Counting seats (not votes) is what makes
    // "two of three reviewers" mean two reviewers.
    let mut support: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
    for finding in &input.findings {
        let mut seats = BTreeSet::new();
        if let Some(raised) = map.findings.get(&finding.rid) {
            seats.insert(raised.seat);
        }
        if let Some(votes) = by_rid_votes.get(&finding.rid) {
            for (seat, vote) in votes {
                if vote.vote == "confirmed" {
                    seats.insert(*seat);
                }
            }
        }
        support.insert(finding.rid.clone(), seats);
    }
    for (rid, root) in &root_of {
        if let Some(seats) = support.remove(rid) {
            support.entry(root.clone()).or_default().extend(seats);
        }
    }

    // Step 3: machine arbitration for a finding with a check runs outside
    // the store lock — a declared command may take minutes. Votes cannot
    // overturn this verdict (decision 0027 §5).
    let reconcile_dir = evidence_dir(repo_root, id).join("reconcile");
    let mut statuses: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut artifact_paths: Vec<String> = Vec::new();
    for finding in &input.findings {
        if root_of.contains_key(&finding.rid) {
            continue;
        }
        if let Some(check) = &finding.check {
            let log_path = reconcile_dir.join(format!("{}.log", finding.rid));
            let (status, observed) = match run_argv(repo_root, &check.argv, None, timeout) {
                Ok(observed) => {
                    let matched = observed.exit == Some(check.exit);
                    ((if matched { "resolved" } else { "open" }), observed)
                }
                Err(error) => (
                    "open",
                    Observed {
                        exit: None,
                        timed_out: false,
                        duration_ms: 0,
                        log: format!("<check not runnable: {error}>\n"),
                    },
                ),
            };
            write_observed_log(&log_path, &observed)?;
            artifact_paths.push(format!(
                ".pulse/evidence/{id}/reconcile/{}.log",
                finding.rid
            ));
            statuses.insert(
                finding.rid.clone(),
                (status.to_string(), finding.severity.clone()),
            );
        } else {
            let supporters = support.get(&finding.rid).map(BTreeSet::len).unwrap_or(0) as u32;
            if supporters >= panel.quorum {
                statuses.insert(
                    finding.rid.clone(),
                    ("open".to_string(), finding.severity.clone()),
                );
            } else {
                statuses.insert(
                    finding.rid.clone(),
                    ("unconfirmed".to_string(), "low".to_string()),
                );
            }
        }
    }

    // Step 4: acceptance by seat majority.
    let mut acceptance_out: Vec<AcceptanceResult> = Vec::new();
    for split in &input.acceptance_split {
        let status = if split.pass >= panel.quorum {
            "pass"
        } else if split.fail > panel.count - panel.quorum {
            "fail"
        } else {
            "not_checked"
        };
        acceptance_out.push(AcceptanceResult {
            id: split.id.clone(),
            status: status.to_string(),
            how: format!("panel: {} pass / {} fail", split.pass, split.fail),
        });
    }

    let mut findings_out: Vec<Finding> = Vec::new();
    for finding in &input.findings {
        if root_of.contains_key(&finding.rid) {
            continue;
        }
        let (status, severity) = statuses
            .get(&finding.rid)
            .cloned()
            .unwrap_or_else(|| ("open".to_string(), finding.severity.clone()));
        findings_out.push(Finding {
            id: finding.rid.clone(),
            reference: finding.reference.clone(),
            summary: finding.summary.clone(),
            owner: finding.owner.clone(),
            check: finding.check.clone(),
            severity,
            status,
        });
    }

    // Step 5: the raw verdict, then the existing seal corrections (which keep
    // "fail with no checkable finding is inconclusive").
    let raw_verdict = if acceptance_out.iter().any(|ac| ac.status == "fail")
        || findings_out
            .iter()
            .any(|finding| finding.severity == "high" && finding.status == "open")
    {
        "fail"
    } else {
        "pass"
    };
    let mut output: LaneOutput = serde_json::from_value(json!({
        "verdict": raw_verdict,
        "acceptance": acceptance_out,
        "cases": Vec::<CaseResult>::new(),
        "findings": findings_out,
        "commands_run": Vec::<CommandRun>::new(),
        "environment": {"commit": now_source.commit},
    }))?;

    // Plan 0025 E2: the required name set, not just verify[] — a learning
    // activated after the seats ran still counts as declared.
    let declared_verify: Vec<String> = crate::kernel::verify::required_names(repo_root, record)?;
    let touches = touches_of(record);
    apply_seal_corrections(
        &mut output,
        SealContext {
            repo_root,
            ticket_id: id,
            role,
            actor,
            declared_verify: &declared_verify,
            touches: &touches,
            source: &now_source,
            receipts: &receipts,
            // Every seat's `pass` was held to D2 when that seat sealed; the
            // reconciliation actor is not a reviewer and ran no verify.
            verify_rule_satisfied: true,
        },
    );

    let mut by_finding = serde_json::Map::new();
    for finding in &input.findings {
        if root_of.contains_key(&finding.rid) {
            continue;
        }
        let mut tally =
            json!({"confirmed": 0, "refuted": 0, "duplicate": 0, "cannot_reproduce": 0});
        if let Some(votes) = by_rid_votes.get(&finding.rid) {
            for (_, vote) in votes {
                if let Some(count) = tally.get_mut(&vote.vote).and_then(|count| count.as_u64()) {
                    tally[&vote.vote] = json!(count + 1);
                }
            }
        }
        tally["support"] = json!(support.get(&finding.rid).map(BTreeSet::len).unwrap_or(0));
        tally["status"] = json!(statuses.get(&finding.rid).map(|(s, _)| s.clone()));
        by_finding.insert(finding.rid.clone(), tally);
    }
    let seats: Vec<String> = seat_receipts.values().map(|r| r.id.clone()).collect();
    let votes_summary = json!({
        "by_finding": by_finding,
        "missing": missing_votes,
        "invalid": invalid,
    });

    // Step 6: record under the store lock, then release before anything that
    // takes its own lock (`bounce_to_active`, event writes).
    let receipt = {
        let guard = WriteGuard::acquire(repo_root)?;
        let records = issues::read_all(repo_root)?;
        let _ = require(&records, id)?;
        let receipt = record_receipt(
            repo_root,
            None,
            NewReceipt {
                kind: "lane".to_string(),
                subject: ReceiptSubject {
                    id: id.to_string(),
                    revision: None,
                },
                actor: actor.as_kind_id(),
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
                    "reconciled": true,
                    "seats": seats,
                    "handoff": map.handoff,
                    "votes_summary": votes_summary,
                }),
                artifact_paths,
            },
        )?;
        write_verdict(
            repo_root,
            &guard,
            id,
            role,
            &receipt.id,
            &output.verdict,
            &now_source.commit,
        )?;
        receipt
    };

    // Step 7: the round state is consumed — a rerun needs a fresh `--prepare`.
    let _ = fs::remove_file(reconcile_snapshot_path(repo_root, id, role));
    let _ = fs::remove_file(reconcile_map_path(repo_root, id, role));

    emit_event(
        repo_root,
        "run.completed",
        actor.as_kind_id(),
        id,
        json!({"role": role, "phase": "reconcile", "verdict": output.verdict, "receipt": receipt.id}),
        chrono::Utc::now(),
    )?;

    if output.verdict == "fail" {
        return bounce_to_active(repo_root, actor, id, role);
    }
    Ok(require(&issues::read_all(repo_root)?, id)?.clone())
}

fn find_ticket<'a>(records: &'a [Value], id: &str) -> Option<&'a Value> {
    require(records, id).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source;
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

    fn lane_actor(role: &str) -> ActorRef {
        ActorRef {
            kind: crate::identity::actor::ActorKind::Agent,
            id: role.to_string(),
        }
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

        let err = validate_and_seal(
            repo.path(),
            &lane_actor("review-correctness"),
            "TK-a3f9",
            "review-correctness",
            &before,
            None,
        )
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
        let receipt = validate_and_seal(
            repo.path(),
            &lane_actor("qa-ui"),
            "TK-a3f9",
            "qa-ui",
            &before,
            None,
        )
        .unwrap();
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
        let receipt = validate_and_seal(
            repo.path(),
            &lane_actor("qa-ui"),
            "TK-a3f9",
            "qa-ui",
            &before,
            None,
        )
        .unwrap();
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
        let receipt = validate_and_seal(
            repo.path(),
            &lane_actor("review-correctness"),
            "TK-a3f9",
            "review-correctness",
            &before,
            None,
        )
        .unwrap();
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
        let receipt = validate_and_seal(
            repo.path(),
            &lane_actor("review-correctness"),
            "TK-a3f9",
            "review-correctness",
            &before,
            None,
        )
        .unwrap();
        assert_eq!(receipt.payload["verdict"], "inconclusive");
    }

    /// [`git_repo`] with `TK-a3f9` declaring `verify[]`, which is what arms
    /// decision 0026 D2's review rule.
    fn git_repo_declaring_verify(names: &[&str]) -> tempfile::TempDir {
        let repo = git_repo();
        issues::mutate(repo.path(), |mut records| {
            let ticket = records
                .iter_mut()
                .find(|record| record["id"] == "TK-a3f9")
                .expect("git_repo pushed TK-a3f9");
            ticket["verify"] = json!(names
                .iter()
                .map(|name| json!({"name": name, "argv": ["true"]}))
                .collect::<Vec<Value>>());
            Ok(records)
        })
        .unwrap();
        repo
    }

    /// Write a `pass` lane output for `role` and seal it. The verdict the
    /// seal produced is what the caller asserts on.
    fn seal_passing_review(repo: &Path, role: &str, before: &Source) -> Value {
        write_lane_output(
            repo,
            "TK-a3f9",
            role,
            &json!({
                "verdict": "pass",
                "acceptance": [{"id": "AC-1", "status": "pass", "how": "reviewed"}],
                "cases": [], "findings": [], "commands_run": [],
                "environment": {"commit": before.commit},
            }),
        );
        validate_and_seal(repo, &lane_actor(role), "TK-a3f9", role, before, None)
            .unwrap()
            .payload
    }

    #[test]
    fn a_review_pass_without_its_own_verify_receipt_is_inconclusive() {
        // Decision 0026 D2: the worker's receipt is the worker's claim. A
        // review lane that reports `pass` on a ticket declaring verify[]
        // must have observed the commands itself, or it has nothing.
        let repo = git_repo_declaring_verify(&["unit"]);
        let before = source::snapshot(repo.path(), &[]).unwrap();
        let payload = seal_passing_review(repo.path(), "review-correctness", &before);
        assert_eq!(payload["verdict"], "inconclusive");
        // The correction is visible to an operator, not silent.
        let events = crate::event::read_events(repo.path()).unwrap();
        assert!(
            events
                .iter()
                .any(|event| event.payload["lane_verdict_corrected"] == true),
            "a corrected verdict must emit its event"
        );
    }

    #[test]
    fn a_review_pass_on_another_actors_verify_receipt_is_inconclusive() {
        // The worker ran the commands — that is exactly the claim under
        // review, so it cannot also be the review's evidence.
        let repo = git_repo_declaring_verify(&["unit"]);
        let before = source::snapshot(repo.path(), &[]).unwrap();
        crate::kernel::verify::verify(
            repo.path(),
            &lane_actor("worker"),
            "TK-a3f9",
            std::time::Duration::from_secs(30),
        )
        .unwrap();
        let payload = seal_passing_review(repo.path(), "review-correctness", &before);
        assert_eq!(payload["verdict"], "inconclusive");
    }

    #[test]
    fn a_review_pass_on_its_own_verify_receipt_stays_pass() {
        let repo = git_repo_declaring_verify(&["unit"]);
        let before = source::snapshot(repo.path(), &[]).unwrap();
        crate::kernel::verify::verify(
            repo.path(),
            &lane_actor("review-correctness"),
            "TK-a3f9",
            std::time::Duration::from_secs(30),
        )
        .unwrap();
        let payload = seal_passing_review(repo.path(), "review-correctness", &before);
        assert_eq!(payload["verdict"], "pass");
    }

    #[test]
    fn a_review_fail_is_untouched_by_the_verify_rule() {
        // Only a claim of `pass` needs backing; a lane that reports failure
        // without its own verify receipt is not silently upgraded.
        let repo = git_repo_declaring_verify(&["unit"]);
        let before = source::snapshot(repo.path(), &[]).unwrap();
        write_lane_output(
            repo.path(),
            "TK-a3f9",
            "review-correctness",
            &json!({
                "verdict": "fail",
                "acceptance": [],
                "cases": [],
                "findings": [{"id": "F-1", "summary": "misses AC-1", "owner": "src/x.rs",
                              "check": {"argv": ["true"], "exit": 0},
                              "severity": "high", "status": "open"}],
                "commands_run": [],
                "environment": {"commit": before.commit},
            }),
        );
        let receipt = validate_and_seal(
            repo.path(),
            &lane_actor("review-correctness"),
            "TK-a3f9",
            "review-correctness",
            &before,
            None,
        )
        .unwrap();
        assert_eq!(receipt.payload["verdict"], "fail");
    }

    #[test]
    fn a_qa_pass_is_not_held_to_the_verify_rule() {
        // qa-* evidence is a case result, not a command output: the rule is
        // review-only (decision 0026 D2).
        let repo = git_repo_declaring_verify(&["unit"]);
        let before = source::snapshot(repo.path(), &[]).unwrap();
        let payload = seal_passing_review(repo.path(), "qa-api", &before);
        assert_eq!(payload["verdict"], "pass");
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
        let err = validate_and_seal(
            repo.path(),
            &lane_actor("review-correctness"),
            "TK-a3f9",
            "review-correctness",
            &before,
            None,
        )
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
        validate_and_seal(
            repo.path(),
            &lane_actor("review-correctness"),
            "TK-a3f9",
            "review-correctness",
            &before,
            None,
        )
        .unwrap();
        let records = issues::read_all(repo.path()).unwrap();
        let ticket = require(&records, "TK-a3f9").unwrap();
        assert_eq!(ticket["verdicts"]["review-correctness"]["verdict"], "pass");
    }

    // --- Plan 0025 B6: the fence is the ticket's own scope ---

    /// Committed `src/` and `web/` baselines plus a `TK-a3f9` scoped to
    /// `src/**`, so tests can mutate each side independently.
    fn repo_with_scoped_ticket() -> (tempfile::TempDir, Value) {
        let repo = git_repo();
        let run = |args: &[&str]| {
            assert!(StdCommand::new("git")
                .arg("-C")
                .arg(repo.path())
                .args(args)
                .status()
                .unwrap()
                .success());
        };
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        std::fs::create_dir_all(repo.path().join("web")).unwrap();
        std::fs::write(repo.path().join("src/lib.rs"), "fn a()\n").unwrap();
        std::fs::write(repo.path().join("web/nav.js"), "// nav\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "src and web"]);
        issues::mutate(repo.path(), |mut records| {
            if let Some(record) = records.iter_mut().find(|record| record["id"] == "TK-a3f9") {
                record["touches"] = json!(["src/**"]);
            }
            Ok(records)
        })
        .unwrap();
        let records = issues::read_all(repo.path()).unwrap();
        let ticket = require(&records, "TK-a3f9").unwrap().clone();
        (repo, ticket)
    }

    #[test]
    fn a_lane_mutation_outside_the_ticket_scope_is_not_a_workspace_mutation() {
        // Worker-2's in-progress web/ edit must not read as this lane
        // mutating the workspace, and the receipt carries the scoped fence.
        let (repo, ticket) = repo_with_scoped_ticket();
        std::fs::write(repo.path().join("src/lib.rs"), "fn a() + comment\n").unwrap();
        let before = profile::fence_for(repo.path(), &ticket).unwrap();
        write_lane_output(
            repo.path(),
            "TK-a3f9",
            "review-correctness",
            &json!({
                "verdict": "pass", "acceptance": [], "cases": [], "findings": [], "commands_run": [],
                "environment": {"commit": before.commit},
            }),
        );
        std::fs::write(repo.path().join("web/nav.js"), "// worker-2, mid-run\n").unwrap();

        let receipt = validate_and_seal(
            repo.path(),
            &lane_actor("review-correctness"),
            "TK-a3f9",
            "review-correctness",
            &before,
            None,
        )
        .unwrap();
        assert_eq!(receipt.source.dirty_hash, before.dirty_hash);
        assert!(receipt.source.dirty_hash.starts_with("scope:sha256:"));
    }

    #[test]
    fn a_scoped_ticket_lane_input_only_carries_its_scope() {
        // The reviewer of TK-a3f9 (src/**) must not chomp on worker-2's
        // web/ files: changed_files is scope-filtered (plan 0025 B6).
        let (repo, ticket) = repo_with_scoped_ticket();
        std::fs::write(repo.path().join("src/lib.rs"), "fn a() + comment\n").unwrap();
        std::fs::write(repo.path().join("web/nav.js"), "// nav, edited\n").unwrap();

        let input = lane_input(repo.path(), &ticket, None, "review-correctness").unwrap();
        let changed: Vec<&str> = input["changed_files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect();
        assert_eq!(changed, vec!["src/lib.rs"]);
    }

    // --- Plan 0025 F3: one changed-files source for the doc advisory ---

    #[test]
    fn changed_files_for_unions_worktree_and_handoff_diff_scope_filtered() {
        // Before any handoff, the dirty worktree is the source: a fenced-out
        // path and an out-of-scope path never appear.
        let (repo, ticket) = repo_with_scoped_ticket();
        let run = |args: &[&str]| {
            assert!(StdCommand::new("git")
                .arg("-C")
                .arg(repo.path())
                .args(args)
                .status()
                .unwrap()
                .success());
        };
        std::fs::write(repo.path().join("src/lib.rs"), "fn a() + comment\n").unwrap();
        std::fs::write(repo.path().join("web/nav.js"), "// worker-2\n").unwrap();
        std::fs::write(repo.path().join("PULSE.md"), "profiles:\n").unwrap();
        let changed = changed_files_for(repo.path(), &ticket).unwrap();
        assert_eq!(changed, vec!["src/lib.rs".to_string()]);

        // After a handoff receipt exists, the diff since its commit counts
        // even when the change is already committed (worktree clean).
        let commit = source::head_commit(repo.path()).unwrap();
        let fence = profile::fence_for(repo.path(), &ticket).unwrap();
        record_receipt(
            repo.path(),
            None,
            NewReceipt {
                kind: "handoff".to_string(),
                subject: ReceiptSubject {
                    id: "TK-a3f9".to_string(),
                    revision: None,
                },
                actor: "agent:worker".to_string(),
                source: ReceiptSource {
                    commit,
                    dirty_hash: fence.dirty_hash,
                },
                run_id: None,
                payload: json!({}),
                artifact_paths: Vec::new(),
            },
        )
        .unwrap();
        run(&["add", "src/lib.rs", "web/nav.js", "PULSE.md"]);
        run(&["commit", "-q", "-m", "landed"]);
        let changed = changed_files_for(repo.path(), &ticket).unwrap();
        assert_eq!(changed, vec!["src/lib.rs".to_string()]);
    }

    // --- Decision 0027 C2: panel seats ---

    fn write_seat_output(repo: &Path, ticket_id: &str, role: &str, seat: u32, output: &Value) {
        let dir = evidence_dir(repo, ticket_id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(format!("{role}.{seat}.json")),
            serde_json::to_vec(output).unwrap(),
        )
        .unwrap();
    }

    fn passing_seat_output(before: &Source) -> Value {
        json!({
            "verdict": "pass", "acceptance": [], "cases": [], "findings": [], "commands_run": [],
            "environment": {"commit": before.commit},
        })
    }

    #[test]
    fn seat_required_when_the_profile_declares_a_panel() {
        let panel = profile::Panel {
            count: 3,
            quorum: 2,
        };
        let err = check_seat("review-correctness", Some(&panel), None).unwrap_err();
        assert_eq!(err.code(), "lane_seat_required");
        assert!(err.hint().is_some());
        assert!(err.to_string().contains('3'), "{err}");
    }

    #[test]
    fn seat_refused_without_a_panel() {
        let err = check_seat("review-correctness", None, Some(1)).unwrap_err();
        assert_eq!(err.code(), "lane_seat_invalid");
        assert!(err.hint().is_some());
    }

    #[test]
    fn seat_out_of_range_is_refused() {
        let panel = profile::Panel {
            count: 3,
            quorum: 2,
        };
        for seat in [0, 4] {
            let err = check_seat("review-correctness", Some(&panel), Some(seat)).unwrap_err();
            assert_eq!(err.code(), "lane_seat_invalid");
        }
        assert!(check_seat("review-correctness", Some(&panel), Some(1)).is_ok());
    }

    #[test]
    fn seat_receipt_is_lane_seat_and_leaves_verdicts_untouched() {
        let repo = git_repo();
        let before = source::snapshot(repo.path(), &[]).unwrap();
        write_seat_output(
            repo.path(),
            "TK-a3f9",
            "review-correctness",
            1,
            &passing_seat_output(&before),
        );
        let receipt = validate_and_seal(
            repo.path(),
            &lane_actor("review-correctness-1"),
            "TK-a3f9",
            "review-correctness",
            &before,
            Some(1),
        )
        .unwrap();
        assert_eq!(receipt.kind, "lane_seat");
        assert_eq!(receipt.payload["seat"], 1);
        assert_eq!(receipt.payload["role"], "review-correctness");
        assert!(receipt.payload["handoff"]
            .as_str()
            .unwrap()
            .starts_with("head:"));
        // The close gate reads `verdicts[role]`; a seat must not write it.
        let records = issues::read_all(repo.path()).unwrap();
        let ticket = require(&records, "TK-a3f9").unwrap();
        assert!(ticket.get("verdicts").is_none(), "{ticket}");
    }

    #[test]
    fn one_actor_cannot_hold_two_seats() {
        let repo = git_repo();
        let before = source::snapshot(repo.path(), &[]).unwrap();
        let actor = lane_actor("review-correctness-1");
        write_seat_output(
            repo.path(),
            "TK-a3f9",
            "review-correctness",
            1,
            &passing_seat_output(&before),
        );
        validate_and_seal(
            repo.path(),
            &actor,
            "TK-a3f9",
            "review-correctness",
            &before,
            Some(1),
        )
        .unwrap();
        write_seat_output(
            repo.path(),
            "TK-a3f9",
            "review-correctness",
            2,
            &passing_seat_output(&before),
        );
        let err = validate_and_seal(
            repo.path(),
            &actor,
            "TK-a3f9",
            "review-correctness",
            &before,
            Some(2),
        )
        .unwrap_err();
        assert_eq!(err.code(), "lane_seat_actor_reused");
        assert!(err.hint().is_some());
    }

    #[test]
    fn resealing_the_same_seat_is_allowed() {
        // A seat that fixes its output and seals again is a rerun, not a
        // second seat.
        let repo = git_repo();
        let before = source::snapshot(repo.path(), &[]).unwrap();
        let actor = lane_actor("review-correctness-1");
        for _ in 0..2 {
            write_seat_output(
                repo.path(),
                "TK-a3f9",
                "review-correctness",
                1,
                &passing_seat_output(&before),
            );
            validate_and_seal(
                repo.path(),
                &actor,
                "TK-a3f9",
                "review-correctness",
                &before,
                Some(1),
            )
            .unwrap();
        }
        assert_eq!(record_receipt_count(repo.path()), 2);
    }
}
