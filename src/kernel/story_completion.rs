//! Core-owned Story completion gate.
//!
//! This owner composes the current graph projection, frozen source, Story QA
//! baseline, immutable qualification evidence and authority policy. Story work
//! is not leased like a Ticket, so this explicit gate closes a `ready` Story
//! without fabricating an assignment lifecycle.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde_json::json;

use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::event::{new_event_id, EventEnvelope};
use crate::evidence::model::{ReceiptKind, ReceiptPayload, ReceiptResult};
use crate::execution::{CloseStoryArgs, StoryCloseReceipt};
use crate::graph::edge::EdgeType;
use crate::graph::node::{Node, NodeStatus};
use crate::graph::projection::GraphProjection;
use crate::graph::store::JsonGraphStore;
use crate::id::WorkKind;
use crate::qa::{QaCaseOutcome, QaExecutionScope};
use crate::storage::transaction::{
    commit_prepared_multi_target_transaction, new_transaction_id, prepare_multi_target_transaction,
    recover_prepared_transactions, FileState, MultiTargetTransactionIntent, TransactionTarget,
};
use crate::storage::WriteGuard;
use crate::{PulseError, Result};

impl JsonGraphStore {
    /// Close a ready Story after full qualification of its integrated outcome.
    ///
    /// # Errors
    ///
    /// Returns a typed error when authority, source, child outcomes, blockers,
    /// baseline coverage, executor evidence or receipt independence is invalid.
    pub fn close_story(&self, args: CloseStoryArgs) -> Result<StoryCloseReceipt> {
        validate_args(&args)?;
        let close_id = deterministic_id("story_close", &args.idempotency_key);
        let close_path = story_close_path(&self.repo_root, &close_id);
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        authorize(&self.repo_root, &args.actor)?;
        recover_prepared_transactions(&self.repo_root)?;
        if close_path.exists() {
            return replay_story_close(&close_path, &args);
        }
        if crate::source::head_commit(&self.repo_root)? != args.source_commit {
            return Err(PulseError::validation(
                "story_close_source_mismatch",
                "Story close must bind the current repository HEAD",
            ));
        }
        if crate::source::check_cleanliness(&self.repo_root)?
            != crate::source::SourceCleanliness::Clean
        {
            return Err(PulseError::validation(
                "story_close_source_dirty",
                "Story close requires a clean frozen repository snapshot",
            ));
        }

        let story_path = self.node_path(&args.story_id);
        let story_before_bytes = fs::read(&story_path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                PulseError::NotFound {
                    subject: args.story_id.clone(),
                }
            } else {
                PulseError::io(&story_path, error)
            }
        })?;
        let mut story: Node = serde_json::from_slice(&story_before_bytes)
            .map_err(|error| PulseError::json(&story_path, error))?;
        if story.kind != WorkKind::Story {
            return Err(PulseError::validation(
                "story_close_kind_invalid",
                "Story close requires a Story node",
            ));
        }
        if story.status != NodeStatus::Ready {
            return Err(PulseError::validation(
                "story_close_not_ready",
                "Story close requires the Story to remain at its exact ready revision",
            ));
        }

        let projection = self.export_unlocked()?;
        let outcomes = validate_descendant_outcomes(&projection, &args.story_id)?;
        validate_story_qualifications(
            &self.repo_root,
            &projection,
            &args.story_id,
            &args.qualification_receipt_ids,
            &args.source_commit,
            &args.actor,
        )?;

        story.status = NodeStatus::Done;
        story.status_reason = None;
        story.revision += 1;
        story.updated_at = Utc::now();
        let mut close = StoryCloseReceipt {
            schema_version: 1,
            close_id: close_id.clone(),
            idempotency_key_hash: hash_bytes(args.idempotency_key.as_bytes()),
            story_id: args.story_id.clone(),
            qualification_receipt_ids: args.qualification_receipt_ids.clone(),
            source_commit: args.source_commit.clone(),
            graph_fingerprint_observed: projection.graph_fingerprint,
            done_ticket_ids: outcomes.done,
            superseded_ticket_ids: outcomes.superseded,
            summary: args.summary.trim().to_string(),
            closed_by: args.actor.clone(),
            recorded_at: Utc::now().to_rfc3339(),
            resulting_revision: story.revision,
            close_fingerprint: String::new(),
        };
        close.close_fingerprint = close.compute_fingerprint()?;
        commit_story_close(
            &self.repo_root,
            &story_path,
            &story_before_bytes,
            &story,
            &close_path,
            &close,
            self.failpoint,
        )?;
        Ok(close)
    }
}

struct TicketOutcomes {
    done: Vec<String>,
    superseded: Vec<String>,
}

fn validate_descendant_outcomes(
    projection: &GraphProjection,
    story_id: &str,
) -> Result<TicketOutcomes> {
    let nodes = projection
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut children = BTreeMap::<&str, Vec<&str>>::new();
    for edge in projection
        .edges
        .iter()
        .filter(|edge| edge.edge_type == EdgeType::Parent)
    {
        children
            .entry(edge.to.as_str())
            .or_default()
            .push(edge.from.as_str());
    }
    let mut descendants = BTreeSet::new();
    let mut queue = VecDeque::from([story_id]);
    while let Some(parent) = queue.pop_front() {
        for child in children.get(parent).into_iter().flatten() {
            if descendants.insert(*child) {
                queue.push_back(child);
            }
        }
    }
    let mut done = Vec::new();
    let mut superseded = Vec::new();
    let mut incomplete = Vec::new();
    for id in &descendants {
        let node = nodes.get(id).ok_or_else(|| {
            PulseError::validation(
                "story_close_graph_invalid",
                format!("Story descendant {id} is missing"),
            )
        })?;
        match (node.kind, node.status) {
            (WorkKind::Ticket, NodeStatus::Done) => done.push((*id).to_string()),
            (WorkKind::Ticket, NodeStatus::Superseded) => {
                superseded.push((*id).to_string());
            }
            (WorkKind::Ticket, _) => {
                incomplete.push((*id).to_string());
            }
            (WorkKind::Story, NodeStatus::Done | NodeStatus::Superseded) => {}
            (WorkKind::Story, _) => {
                incomplete.push((*id).to_string());
            }
            _ => {}
        }
    }
    if !incomplete.is_empty() {
        return Err(PulseError::validation(
            "story_close_children_incomplete",
            format!("Story has nonterminal or cancelled descendants: {incomplete:?}"),
        ));
    }
    if done.is_empty() && superseded.is_empty() {
        return Err(PulseError::validation(
            "story_close_children_missing",
            "Story close requires at least one terminal descendant Ticket",
        ));
    }
    let affected = descendants
        .iter()
        .copied()
        .chain(std::iter::once(story_id))
        .collect::<BTreeSet<_>>();
    let blockers = projection
        .edges
        .iter()
        .filter(|edge| {
            edge.edge_type == EdgeType::BlockedBy && affected.contains(edge.from.as_str())
        })
        .filter(|edge| {
            nodes
                .get(edge.to.as_str())
                .map_or(true, |node| node.status != NodeStatus::Done)
        })
        .map(|edge| edge.to.clone())
        .collect::<BTreeSet<_>>();
    if !blockers.is_empty() {
        return Err(PulseError::validation(
            "story_close_blocked",
            format!("Story scope still has open hard blockers: {blockers:?}"),
        ));
    }
    done.sort();
    superseded.sort();
    Ok(TicketOutcomes { done, superseded })
}

fn validate_story_qualifications(
    repo_root: &Path,
    projection: &GraphProjection,
    story_id: &str,
    receipt_ids: &[String],
    source_commit: &str,
    closing_actor: &str,
) -> Result<()> {
    if receipt_ids.len() != 1 {
        return Err(PulseError::validation(
            "story_close_qualification_required",
            "Story close requires exactly one current qualification receipt",
        ));
    }
    let baseline = crate::qa::resolve_story_cases(repo_root, story_id)?;
    validate_story_qualification(
        repo_root,
        projection,
        story_id,
        &receipt_ids[0],
        source_commit,
        closing_actor,
        &baseline,
    )
}

fn validate_story_qualification(
    repo_root: &Path,
    projection: &GraphProjection,
    story_id: &str,
    receipt_id: &str,
    source_commit: &str,
    closing_actor: &str,
    baseline: &crate::qa::QaBaselineResolution,
) -> Result<()> {
    let report = crate::evidence::verify_receipt(repo_root, receipt_id, true, None)?;
    if report.integrity.status != "valid" || report.bindings.status != "current" {
        return Err(PulseError::validation(
            "story_close_qualification_stale",
            format!(
                "Story qualification integrity or bindings are not current: integrity={:?}, bindings={:?}",
                report.integrity.reason_codes, report.bindings.reason_codes
            ),
        ));
    }
    let receipt = crate::evidence::show_receipt(repo_root, receipt_id)?.receipt;
    let ReceiptPayload::QaCheckpoint(payload) = &receipt.payload else {
        return Err(PulseError::validation(
            "story_close_qualification_invalid",
            "Story close requires a QA qualification receipt",
        ));
    };
    if receipt.kind != ReceiptKind::QaCheckpoint
        || receipt.result != ReceiptResult::Passed
        || payload.qa_scope != QaExecutionScope::StoryClose
        || receipt.subject.kind != "work"
        || receipt.subject.id != story_id
        || payload.story_id != story_id
    {
        return Err(PulseError::validation(
            "story_close_qualification_invalid",
            "Story close requires a passed Story-subject qualification receipt",
        ));
    }
    if receipt.actor == crate::policy::parse_actor(closing_actor) {
        return Err(PulseError::validation(
            "story_close_independence_required",
            "the Story qualification actor cannot close their own assurance result",
        ));
    }
    let source = receipt.bindings.source.as_ref().ok_or_else(|| {
        PulseError::validation(
            "story_close_source_missing",
            "Story qualification lacks an exact source binding",
        )
    })?;
    if source.commit != source_commit
        || payload.baseline_revision != baseline.revision
        || payload.baseline_content_hash != baseline.content_hash
    {
        return Err(PulseError::validation(
            "story_close_qualification_stale",
            "Story qualification does not bind the current source and baseline",
        ));
    }
    let initiating_ticket = projection
        .nodes
        .iter()
        .find(|node| node.id == payload.ticket_id)
        .ok_or_else(|| PulseError::NotFound {
            subject: payload.ticket_id.clone(),
        })?;
    if initiating_ticket.kind != WorkKind::Ticket
        || initiating_ticket
            .qa
            .as_ref()
            .and_then(|qa| qa.impact.behavioral_owner.as_deref())
            != Some(story_id)
    {
        return Err(PulseError::validation(
            "story_close_assignment_mismatch",
            "Story qualification must originate from a Ticket owned by the Story",
        ));
    }
    let expected = baseline
        .cases
        .iter()
        .map(|case| (case.id.as_str(), case))
        .collect::<BTreeMap<_, _>>();
    let mut covered = BTreeSet::new();
    for observation in &payload.cases {
        let case = expected.get(observation.case_id.as_str()).ok_or_else(|| {
            PulseError::validation(
                "story_close_case_unexpected",
                format!(
                    "Story qualification contains unexpected case {}",
                    observation.case_id
                ),
            )
        })?;
        if observation.case_revision != case.revision
            || observation.outcome != QaCaseOutcome::Passed
            || !covered.insert(observation.case_id.clone())
        {
            return Err(PulseError::validation(
                "story_close_case_invalid",
                format!(
                    "Story qualification case {} is stale, incomplete, or duplicated",
                    observation.case_id
                ),
            ));
        }
    }
    let wanted = expected
        .keys()
        .map(|id| (*id).to_string())
        .collect::<Vec<_>>();
    let actual = covered.into_iter().collect::<Vec<_>>();
    if actual != wanted {
        return Err(PulseError::validation(
            "story_close_coverage_incomplete",
            format!("Story qualification must cover the full applicable baseline: expected={wanted:?}, actual={actual:?}"),
        ));
    }
    Ok(())
}

fn validate_args(args: &CloseStoryArgs) -> Result<()> {
    if args.idempotency_key.trim().is_empty() {
        return Err(PulseError::validation(
            "story_close_idempotency_key_required",
            "Story close requires an idempotency key",
        ));
    }
    if args.summary.trim().is_empty() {
        return Err(PulseError::validation(
            "story_close_summary_missing",
            "Story close summary must not be empty",
        ));
    }
    if args.qualification_receipt_ids.is_empty()
        || args
            .qualification_receipt_ids
            .iter()
            .any(|id| id.trim().is_empty())
    {
        return Err(PulseError::validation(
            "story_close_qualification_missing",
            "Story close requires a qualification receipt ID",
        ));
    }
    Ok(())
}

fn replay_story_close(path: &Path, args: &CloseStoryArgs) -> Result<StoryCloseReceipt> {
    let close = load_story_close_path(path)?;
    if close.story_id != args.story_id
        || close.qualification_receipt_ids != args.qualification_receipt_ids
        || close.closed_by != args.actor
        || close.source_commit != args.source_commit
        || close.summary != args.summary.trim()
    {
        return Err(PulseError::validation(
            "story_close_idempotency_conflict",
            "Story close idempotency key was already used with different inputs",
        ));
    }
    Ok(close)
}

fn authorize(repo_root: &Path, actor: &str) -> Result<()> {
    let report = crate::policy::load_authority_policy(repo_root)?;
    crate::policy::authorize(
        &report,
        &crate::policy::parse_actor(actor),
        &["work.story.close"],
    )
}

fn deterministic_id(prefix: &str, key: &str) -> String {
    let digest = hash_bytes(key.as_bytes());
    format!(
        "{prefix}_{}",
        digest
            .trim_start_matches("sha256:")
            .chars()
            .take(26)
            .collect::<String>()
    )
}

fn story_close_path(repo_root: &Path, close_id: &str) -> PathBuf {
    repo_root
        .join(".pulse/evidence/execution/story-closes")
        .join(format!("{close_id}.json"))
}

/// Load and verify an immutable Story close receipt.
///
/// # Errors
///
/// Returns a typed error when the receipt is missing, malformed or tampered.
pub fn load_story_close(repo_root: &Path, close_id: &str) -> Result<StoryCloseReceipt> {
    load_story_close_path(&story_close_path(repo_root, close_id))
}

fn load_story_close_path(path: &Path) -> Result<StoryCloseReceipt> {
    let bytes = fs::read(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            PulseError::NotFound {
                subject: format!("Story close proof {}", path.display()),
            }
        } else {
            PulseError::io(path, error)
        }
    })?;
    let close: StoryCloseReceipt =
        serde_json::from_slice(&bytes).map_err(|error| PulseError::json(path, error))?;
    if close.compute_fingerprint()? != close.close_fingerprint {
        return Err(PulseError::validation(
            "story_close_fingerprint_mismatch",
            "Story close fingerprint does not match canonical contents",
        ));
    }
    Ok(close)
}

fn commit_story_close(
    repo_root: &Path,
    story_path: &Path,
    story_before_bytes: &[u8],
    story_after: &Node,
    close_path: &Path,
    close: &StoryCloseReceipt,
    failpoint: Option<crate::storage::transaction::TransactionFailpoint>,
) -> Result<()> {
    let story_after_bytes = to_canonical_bytes(story_after)?;
    let close_bytes = to_canonical_bytes(close)?;
    let before: Node = serde_json::from_slice(story_before_bytes).map_err(PulseError::from)?;
    let event_id = new_event_id();
    let now = Utc::now();
    let event_path = repo_root
        .join(".pulse/events")
        .join(now.format("%Y-%m-%d").to_string())
        .join(format!("{event_id}.json"));
    let event = EventEnvelope::new(
        event_id.clone(),
        "work.story.closed",
        &close.closed_by,
        &close.story_id,
        json!({
            "close_id": close.close_id,
            "qualification_receipt_ids": close.qualification_receipt_ids,
            "source_commit": close.source_commit,
            "graph_fingerprint_observed": close.graph_fingerprint_observed,
            "from": "ready",
            "to": "done",
        }),
        now,
    );
    let targets = vec![
        TransactionTarget::new(
            story_path.to_path_buf(),
            FileState::Present {
                hash: hash_bytes(story_before_bytes),
                revision: before.revision,
            },
            FileState::Present {
                hash: hash_bytes(&story_after_bytes),
                revision: story_after.revision,
            },
            &story_after_bytes,
        ),
        TransactionTarget::new(
            close_path.to_path_buf(),
            FileState::Absent,
            FileState::Present {
                hash: hash_bytes(&close_bytes),
                revision: 0,
            },
            &close_bytes,
        ),
    ];
    let intent = MultiTargetTransactionIntent::prepared_with_transaction_id(
        new_transaction_id(),
        event_id,
        "work.story.closed",
        close.closed_by.clone(),
        targets,
        event_path,
        serde_json::to_value(event)?,
    )?;
    let transaction = prepare_multi_target_transaction(repo_root, intent)?;
    commit_prepared_multi_target_transaction(&transaction, failpoint)
}
