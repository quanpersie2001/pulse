//! Coherent canonical packet snapshot builder (P2S1-I3 / P2S1-I4).
//!
//! This module composes the cross-domain [`WorkPacket`] using a two-fence
//! algorithm: the first fence validates graph/source/authority state and builds
//! the deterministic query; the fence is released for a cache-only docs index
//! refresh; the second fence revalidates all preconditions before searching
//! suggestions and assembling the packet.
//!
//! P2S1-I4 adds the deterministic suggestion query builder, integer micro-score
//! conversion, cache-only refresh that never writes `docs/**/_index.md`, and
//! two-fence revalidation with no internal retry.
//!
//! See `proposals/phase2-slice1-work-packet-dispatch-foundation.md` § P2S1-I3/I4.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::docs::applicability::{ApplicableDocsReport, ApplicableDocument};
use crate::docs::model::{DocumentKind, WorkDocumentationContext};
use crate::graph::model::brief::TicketBrief;
use crate::graph::model::contract::{Risk, TicketRole};
use crate::graph::model::edge::EdgeType;
use crate::graph::model::node::{Node, NodeStatus};
use crate::graph::read::executability::StructuralState;
use crate::graph::read::projection::GraphProjection;
use crate::graph::read::readiness::{evaluate as evaluate_readiness, EvalProfile};
use crate::graph::store::JsonGraphStore;
use crate::id::WorkKind;
use crate::kernel::readiness::ReadinessSnapshot;
use crate::source::{check_repository_identity, packet_base_snapshot};
use crate::storage::safe_repo_relative;
use crate::storage::transaction::recover_prepared_transactions;
use crate::storage::WriteGuard;
use crate::work_packet;
use crate::work_packet::{
    PacketDecisionSummary, PacketDocRef, PacketDocs, PacketDocsApplicability, PacketDocumentation,
    PacketExcludedDocRef, PacketGraph, PacketHandoff, PacketParentRef, PacketParentSummary,
    PacketQa, PacketRawFile, PacketReadBudget, PacketRelationBundle, PacketRelationItem,
    PacketSource, PacketTicket, SubjectSnapshot, WorkPacket,
};
use crate::{PulseError, PulseResult};

// ---------------------------------------------------------------------------
// PacketPhase1State — intermediate state shared across fence phases
// ---------------------------------------------------------------------------

/// Intermediate snapshot extracted under the first repository fence.
///
/// Carries all preconditions that must be revalidated under the second
/// fence, plus the extracted-but-not-yet-assembled packet sections that
/// are invariant across the docs-cache refresh between fences.
///
/// See [`JsonGraphStore::work_packet`] and the two-fence algorithm
/// described in P2S1-D9.
pub(crate) struct PacketPhase1State {
    // -- Preconditions for phase 2 revalidation --
    pub pre_graph_fingerprint: String,
    pub pre_subject_id: String,
    pub pre_subject_revision: u64,
    pub pre_subject_status: NodeStatus,
    pub pre_readiness_fingerprint: String,
    pub pre_authority_fingerprint: Option<String>,
    pub pre_docs_registry_fingerprint: String,
    pub pre_docs_content_fingerprints: BTreeMap<String, String>,
    pub pre_source: crate::source::PacketSourceSnapshot,
    pub pre_suggestion_query: crate::work_packet::PacketSuggestionQuery,
    pub pre_excluded_doc_ids: Vec<String>,

    // -- Real packet content extracted under the first fence
    pub packet_ticket: PacketTicket,
    pub packet_parents: Vec<PacketParentSummary>,
    pub packet_decisions: Vec<PacketDecisionSummary>,
    pub packet_qa: PacketQa,
    pub notes: Vec<String>,

    // -- Reusable extracted data (invariant across fence drop) --
    pub graph: PacketGraph,
    pub documentation_base: PacketDocumentation,
    pub source: PacketSource,
    pub docs_work_context: WorkDocumentationContext,
}

// ---------------------------------------------------------------------------
// Public entrypoint
// ---------------------------------------------------------------------------

impl JsonGraphStore {
    /// Build a coherent preview work packet for a `ready` implementation
    /// Ticket.
    ///
    /// Follows a single-fence snapshot algorithm: acquire fence, recover,
    /// load projection + readiness snapshot, extract all packet fields,
    /// normalize, compute fingerprint, finalize size.
    /// Build a coherent preview work packet for a `ready` implementation
    /// Ticket.
    ///
    /// Uses a two-fence algorithm (P2S1-D9):
    ///   1. First fence: validate, load subject, build query, save preconditions.
    ///   2. Release fence, build/search cache-only docs index (never writes
    ///      tracked `docs/**/_index.md`).
    ///   3. Second fence: revalidate all preconditions, search suggestions,
    ///      assemble packet, compute fingerprint, enforce budget.
    ///
    /// No internal retry: if state changed during docs search the caller must
    /// retry.
    pub fn work_packet(&self, id: &str) -> PulseResult<WorkPacket> {
        // ==================================================================
        // Phase 1 — First fence: validate, load, extract query, snapshot
        // ==================================================================
        let evidence = check_repository_identity(&self.repo_root)?;
        let repository_id = evidence.repository_id.clone();
        self.require_existing_workgraph_unlocked()?;
        validate_packet_operational_paths(&self.repo_root)?;

        let _guard = acquire_packet_fence(&self.repo_root)?;
        let phase1 = self.packet_phase1_under_fence(id, &repository_id)?;

        // Phase 1 complete: drop first fence.
        drop(_guard);
        self.work_packet_test_barrier_after_first_fence()?;

        // ==================================================================
        // Between fences — cache-only docs index refresh
        // ==================================================================
        let (suggested_sections, docs_cache_fp) =
            packet_refresh_and_search(&self.repo_root, &phase1)?;
        let pre_suggestion_fingerprints = suggestion_fingerprints(&suggested_sections);

        // ==================================================================
        // Phase 2 — Second fence: revalidate, search, assemble
        // ==================================================================
        let _guard2 = acquire_packet_fence(&self.repo_root)?;
        let packet = self.packet_phase2_under_fence(
            phase1,
            suggested_sections,
            docs_cache_fp,
            pre_suggestion_fingerprints,
        )?;
        drop(_guard2);
        Ok(packet)
    }

    /// Load the immutable packet committed atomically with a Core reservation.
    pub fn work_packet_for_lease(&self, id: &str, lease_id: &str) -> PulseResult<WorkPacket> {
        self.work_packet_for_reservation(id, lease_id)
    }

    /// Build a work packet assuming the repository fence is already held by
    /// the caller (P2S2-I6).
    ///
    /// This is the fence-aware entry point for the claim pipeline so it can
    /// revalidate packet preconditions without WriteGuard self-deadlock.
    ///
    /// Algorithm (single-fence for claim):
    ///   1. Pre-check (repository identity, workgraph, operational paths).
    ///   2. Phase 1 under the caller-held fence.
    ///   3. If the docs cache is NOT current, tell the caller to
    ///      release/reacquire and retry.
    ///   4. If the docs cache IS current, skip the refresh and complete
    ///      under the same fence.
    ///
    /// The caller is responsible for:
    ///   - holding WriteGuard before calling;
    ///   - having run `recover_prepared_transactions` already;
    ///   - releasing/reacquiring if this returns
    ///     `work_packet_docs_cache_needs_refresh`.
    pub(crate) fn work_packet_under_fence(&self, id: &str) -> PulseResult<WorkPacket> {
        let evidence = check_repository_identity(&self.repo_root)?;
        let repository_id = evidence.repository_id.clone();
        self.require_existing_workgraph_unlocked()?;
        validate_packet_operational_paths(&self.repo_root)?;

        // Phase 1 — under the caller-held fence, extract everything.
        let phase1 = self.packet_phase1_under_fence(id, &repository_id)?;

        // Check whether the docs cache is current against the live registry and
        // document content using a read-only path. If not fresh, tell the
        // caller to release/reacquire and retry instead of entering docs search
        // under the held non-reentrant repository fence.
        let cache_fp = current_docs_cache_fingerprint_under_fence(&self.repo_root)?;

        if cache_fp.is_none() {
            return Err(PulseError::validation(
                "work_packet_docs_cache_needs_refresh",
                "docs cache is stale under held fence; caller must release fence, \
                 run build_search_cache, reacquire fence, and call again",
            ));
        }

        // Cache is current: search suggestions (no fence needed — cache is
        // read-after-write consistent without the repository lock).
        let suggested_sections = search_suggestions(
            &self.repo_root,
            &phase1.pre_suggestion_query,
            &phase1.pre_excluded_doc_ids,
            &phase1.docs_work_context,
            true,
        )?;
        let pre_suggestion_fingerprints = suggestion_fingerprints(&suggested_sections);

        // Phase 2 — same fence, just revalidate and complete.
        self.packet_phase2_under_fence(
            phase1,
            suggested_sections,
            cache_fp,
            pre_suggestion_fingerprints,
        )
    }
}

// ---------------------------------------------------------------------------
// Phase 1 + Phase 2 helpers (fence-aware, P2S2-I6)
// ---------------------------------------------------------------------------

impl JsonGraphStore {
    // -----------------------------------------------------------------------
    // Phase 1 — extract everything that doesn't need docs cache
    // -----------------------------------------------------------------------
    //
    // PRECONDITION: caller holds the repository fence AND has run
    // `recover_prepared_transactions`.

    fn packet_phase1_under_fence(
        &self,
        id: &str,
        repository_id: &str,
    ) -> PulseResult<PacketPhase1State> {
        recover_prepared_transactions(&self.repo_root)?;
        let projection = self.export_unlocked()?;

        let node = self.load_subject(id, &projection)?;
        self.verify_packet_eligible(&node)?;

        let readiness = self.build_readiness_snapshot_from_projection(&node, &projection)?;
        let inputs = readiness.as_inputs(&node);
        let readiness_report = evaluate_readiness(&inputs, EvalProfile::Ready)?;
        if readiness_report.code != "ready" {
            return Err(PulseError::validation(
                "work_packet_readiness_failed",
                format!(
                    "readiness check did not pass (code={})",
                    readiness_report.code
                ),
            ));
        }

        // Extract sections that do NOT depend on docs search or source.
        let packet_ticket = extract_packet_ticket(&self.repo_root, &node)?;
        let packet_parents = extract_packet_parents(&self.repo_root, &node, &projection)?;
        let packet_decisions = extract_packet_decisions(&self.repo_root, &node, &projection);
        let packet_qa = extract_packet_qa(&self.repo_root, &packet_parents);
        let notes = crate::kernel::communication::list_notes_for_ticket(&self.repo_root, &node.id);
        let graph = extract_graph(&readiness, &projection)?;
        let documentation_base = extract_documentation(&readiness.docs)?;
        let pre_source = packet_base_snapshot(&self.repo_root, repository_id)?;
        let source: PacketSource = pre_source.clone().into();

        // Build deterministic suggestion query.
        let docs_work_context = work_documentation_context(&node);
        let suggestion_query = build_suggestion_query(
            &node,
            readiness.ticket_brief.as_ref(),
            &packet_decisions,
            &docs_work_context,
        )?;
        let excluded_doc_ids: Vec<String> = documentation_base
            .applicability
            .excluded
            .iter()
            .map(|e| e.id.clone())
            .collect();

        // Save preconditions for revalidation after docs search.
        let pre_graph_fingerprint = projection.graph_fingerprint.clone();
        let pre_subject_revision = node.revision;
        let pre_subject_status = node.status;
        let pre_readiness_fingerprint = readiness_report.readiness_fingerprint.clone();
        let pre_authority_fingerprint = readiness.authority.fingerprint.clone();
        let pre_docs_registry_fingerprint = readiness.docs.registry.fingerprint.clone();
        let pre_docs_content_fingerprints = docs_content_fingerprints(&documentation_base);
        let pre_subject_id = node.id.clone();
        let pre_suggestion_query = suggestion_query.clone();
        let pre_excluded_doc_ids = excluded_doc_ids.clone();
        Ok(PacketPhase1State {
            pre_graph_fingerprint,
            pre_subject_id,
            pre_subject_revision,
            pre_subject_status,
            pre_readiness_fingerprint,
            pre_authority_fingerprint,
            pre_docs_registry_fingerprint,
            pre_docs_content_fingerprints,
            pre_source,
            pre_suggestion_query,
            pre_excluded_doc_ids,
            packet_ticket,
            packet_parents,
            packet_decisions,
            packet_qa,
            notes,
            graph,
            documentation_base,
            source,
            docs_work_context,
        })
    }

    // -----------------------------------------------------------------------
    // Phase 2 — revalidate preconditions and complete the packet
    // -----------------------------------------------------------------------
    //
    // PRECONDITION: caller holds the repository fence AND has run
    // `recover_prepared_transactions`.

    fn packet_phase2_under_fence(
        &self,
        phase1: PacketPhase1State,
        suggested_sections: Vec<crate::work_packet::PacketSuggestedSection>,
        docs_cache_fp: Option<String>,
        pre_suggestion_fingerprints: Vec<(u64, String, String, String, u64, u64)>,
    ) -> PulseResult<WorkPacket> {
        self.require_existing_workgraph_unlocked()?;
        validate_packet_operational_paths(&self.repo_root)?;
        recover_prepared_transactions(&self.repo_root)?;
        let projection = self.export_unlocked()?;

        // Revalidate graph fingerprint.
        if projection.graph_fingerprint != phase1.pre_graph_fingerprint {
            return Err(PulseError::validation(
                "work_packet_snapshot_changed",
                "graph fingerprint changed during work packet build",
            ));
        }

        // Revalidate subject.
        let node = self.load_subject(&phase1.pre_subject_id, &projection)?;
        if node.revision != phase1.pre_subject_revision || node.status != phase1.pre_subject_status
        {
            return Err(PulseError::validation(
                "work_packet_snapshot_changed",
                "subject revision or status changed during work packet build",
            ));
        }

        // Revalidate authority and docs registry fingerprints.
        let readiness = self.build_readiness_snapshot_from_projection(&node, &projection)?;
        if readiness.authority.fingerprint != phase1.pre_authority_fingerprint {
            return Err(PulseError::validation(
                "work_packet_snapshot_changed",
                "authority policy fingerprint changed during work packet build",
            ));
        }
        if readiness.docs.registry.fingerprint != phase1.pre_docs_registry_fingerprint {
            return Err(PulseError::validation(
                "work_packet_snapshot_changed",
                "docs registry fingerprint changed during work packet build",
            ));
        }
        let readiness_report = evaluate_readiness(&readiness.as_inputs(&node), EvalProfile::Ready)?;
        if readiness_report.readiness_fingerprint != phase1.pre_readiness_fingerprint {
            return Err(PulseError::validation(
                "work_packet_snapshot_changed",
                "readiness fingerprint changed during work packet build",
            ));
        }
        if docs_content_fingerprints(&extract_documentation(&readiness.docs)?)
            != phase1.pre_docs_content_fingerprints
        {
            return Err(PulseError::validation(
                "work_packet_snapshot_changed",
                "applicable document content changed during work packet build",
            ));
        }
        let current_docs_cache_fp = crate::docs::index::current_cache_fingerprint(&self.repo_root)?;
        if current_docs_cache_fp != docs_cache_fp {
            return Err(PulseError::validation(
                "work_packet_snapshot_changed",
                "docs search cache fingerprint changed during work packet build",
            ));
        }
        if suggestion_fingerprints(&suggested_sections) != pre_suggestion_fingerprints {
            return Err(PulseError::validation(
                "work_packet_snapshot_changed",
                "selected documentation suggestion identity changed during work packet build",
            ));
        }

        // Revalidate source (HEAD, cleanliness, operation state).
        crate::source::revalidate_packet_base(&self.repo_root, &phase1.pre_source)?;

        // Permission check: no retry on snapshot/source change. Keep the
        // second repository fence held through final packet assembly,
        // normalization, fingerprinting and fixed-point budget enforcement so
        // the returned bytes are produced under the revalidated snapshot.

        // Build fully integrated documentation section before completing the
        // snapshot.
        let docs_index_fingerprint = docs_cache_fp
            .clone()
            .unwrap_or_else(|| readiness.docs.registry.fingerprint.clone());
        let documentation = PacketDocumentation {
            applicability: phase1.documentation_base.applicability,
            suggestion_query: phase1.pre_suggestion_query,
            suggested_sections,
            read_budget: phase1.documentation_base.read_budget,
            index: work_packet::PacketDocsIndex {
                state: if docs_cache_fp.is_some() {
                    "current".to_string()
                } else {
                    "not_installed".to_string()
                },
                fingerprint: docs_index_fingerprint,
                mode: "lexical".to_string(),
            },
        };

        // ---- Assemble only the context that actually exists today. Runtime
        // assignment and capability/workspace policy are reservation concerns,
        // not packet content.
        let mut packet = WorkPacket {
            schema_version: work_packet::PACKET_SCHEMA_VERSION,
            profile: "work_packet".to_string(),
            code: "ready_ticket".to_string(),
            ticket: phase1.packet_ticket.clone(),
            parents: phase1.packet_parents,
            decisions: phase1.packet_decisions,
            blockers: phase1.graph.hard_blockers,
            related: phase1
                .graph
                .relations
                .outgoing
                .into_iter()
                .chain(phase1.graph.relations.incoming)
                .collect(),
            docs: packet_docs_from_legacy(documentation),
            qa: phase1.packet_qa,
            knowledge: applicable_knowledge(
                &self.repo_root,
                readiness
                    .ticket_brief
                    .as_ref()
                    .map(|brief| brief.code_anchors.clone())
                    .unwrap_or_default(),
                &phase1.packet_ticket.tags,
            ),
            notes: phase1.notes,
            // Every reviewer's rework findings for this Ticket, with the
            // recording actor, so the next run fixes what was actually shown
            // broken instead of guessing (Decision 0012 §5).
            rework: rework_observations(&self.repo_root, &node.id)?,
            source: phase1.source,
            tags_vocabulary: vec![],
            handoff: PacketHandoff {
                commands: vec![
                    "pulse --idempotency-key handoff:<ticket>:<lease> work handoff --lease <lease> --session <session> --source-commit <commit> --summary <summary> --changed-path <path>".to_string(),
                    "pulse --idempotency-key verify:<ticket>:<handoff> work verify --handoff <handoff> --actor <actor> --source-commit <commit> --disposition passed --summary <summary> --check <name=command=exit> --proof <AC=checks=receipts>".to_string(),
                ],
            },
            packet_fingerprint: String::new(),
        };
        packet.normalize();
        packet.finalize_size()?;
        Ok(packet)
    }
}

/// Upper bound of knowledge items injected into one packet.
pub(crate) const MAX_KNOWLEDGE_ITEMS: usize = 5;

/// Applicable validated or promoted learnings for the subject Ticket:
/// required and recommended feed the packet; suggested and excluded carry
/// the rest of the recall decision. Bounded and sorted by learning id for
/// deterministic output.
fn applicable_knowledge(
    repo_root: &Path,
    code_anchors: Vec<String>,
    tags: &[String],
) -> Vec<work_packet::PacketKnowledgeItem> {
    let buckets = applicable_knowledge_buckets(repo_root, &code_anchors, tags);
    let mut items: Vec<work_packet::PacketKnowledgeItem> = buckets
        .required
        .into_iter()
        .chain(buckets.recommended)
        .collect();
    items.sort_by(|left, right| left.detail_ref.cmp(&right.detail_ref));
    items.truncate(MAX_KNOWLEDGE_ITEMS);
    items
}

/// One applicable learning with its identity, for `knowledge applicable`.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ApplicableKnowledgeItem {
    pub id: String,
    pub summary: String,
    pub why_applicable: String,
    pub required_checks: Vec<String>,
}

/// One learning left out of injection, with the mechanical reason.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ExcludedKnowledgeItem {
    pub id: String,
    pub reason: String,
}

/// The four recall buckets for one Ticket (Decision 13.3: one shared logic
/// for the packet and `knowledge applicable`). Bucket rules:
/// - excluded: candidate/disputed/superseded/retired statuses, harness scope;
/// - required: matched and confidence `enforced` (the ratchet demands it);
/// - recommended: matched validated/promoted learnings;
/// - suggested: enforced-confidence learnings without a path/tag match, as
///   a reference only.
#[derive(Debug, Clone, Default)]
pub(crate) struct KnowledgeBuckets {
    pub required: Vec<work_packet::PacketKnowledgeItem>,
    pub recommended: Vec<work_packet::PacketKnowledgeItem>,
    pub suggested: Vec<work_packet::PacketKnowledgeItem>,
    pub excluded: Vec<ExcludedKnowledgeItem>,
}

impl KnowledgeBuckets {
    /// Move an item into `required` or `recommended` by confidence.
    fn push_matched(&mut self, enforced: bool, item: work_packet::PacketKnowledgeItem) {
        if enforced {
            self.required.push(item);
        } else {
            self.recommended.push(item);
        }
    }
}

/// Compute the applicability buckets for a Ticket's code anchors and tags.
pub(crate) fn applicable_knowledge_buckets(
    repo_root: &Path,
    code_anchors: &[String],
    tags: &[String],
) -> KnowledgeBuckets {
    let mut buckets = KnowledgeBuckets::default();
    let Ok((entries, _)) = crate::knowledge::validate::load_records(repo_root) else {
        return buckets;
    };
    use crate::knowledge::model::{Confidence, LearningScope, LearningStatus};
    for (id, learning) in &entries {
        if learning.scope == LearningScope::Harness {
            buckets.excluded.push(ExcludedKnowledgeItem {
                id: id.clone(),
                reason: "harness scope injects via the runner bootstrap prompt, not by path"
                    .to_string(),
            });
            continue;
        }
        if !matches!(
            learning.status,
            LearningStatus::Validated | LearningStatus::Promoted
        ) {
            buckets.excluded.push(ExcludedKnowledgeItem {
                id: id.clone(),
                reason: format!("status {:?} is not injected", learning.status),
            });
            continue;
        }
        let mut reasons = Vec::new();
        for pattern in &learning.applicability.paths {
            for anchor in code_anchors {
                if knowledge_path_matches(anchor, pattern) {
                    reasons.push(format!("anchor {anchor} matches path {pattern}"));
                    break;
                }
            }
        }
        for label in &learning.applicability.work_labels {
            if tags.iter().any(|tag| tag == label) {
                reasons.push(format!("ticket tag {label}"));
                break;
            }
        }
        let enforced = learning.validation.confidence == Confidence::Enforced;
        if reasons.is_empty() {
            if enforced {
                buckets.suggested.push(work_packet::PacketKnowledgeItem {
                    summary: learning.summary.clone(),
                    why_applicable: "enforced ratchet check; no path or tag match".to_string(),
                    required_checks: learning.guidance.required_checks.clone(),
                    detail_ref: Some(id.clone()),
                });
            }
            continue;
        }
        buckets.push_matched(
            enforced,
            work_packet::PacketKnowledgeItem {
                summary: learning.summary.clone(),
                why_applicable: reasons.join("; "),
                required_checks: learning.guidance.required_checks.clone(),
                detail_ref: Some(id.clone()),
            },
        );
    }
    buckets
}

/// `knowledge applicable --work <id>`: the recall decision for one Ticket,
/// shared with packet injection by construction.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct KnowledgeApplicableReport {
    pub schema_version: u32,
    pub code: String,
    pub work: String,
    pub required: Vec<ApplicableKnowledgeItem>,
    pub recommended: Vec<ApplicableKnowledgeItem>,
    pub suggested: Vec<ApplicableKnowledgeItem>,
    pub excluded: Vec<ExcludedKnowledgeItem>,
}

impl JsonGraphStore {
    /// Build the applicability report for one Ticket.
    pub(crate) fn knowledge_applicable(
        &self,
        work_id: &str,
    ) -> PulseResult<KnowledgeApplicableReport> {
        let node = self.show_node(work_id)?;
        let anchors = if node.kind == crate::id::WorkKind::Ticket {
            self.read_ticket_brief(work_id)
                .map(|brief| brief.code_anchors)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let buckets = applicable_knowledge_buckets(&self.repo_root, &anchors, &node.tags);
        let item = |value: work_packet::PacketKnowledgeItem| ApplicableKnowledgeItem {
            id: value.detail_ref.unwrap_or_default(),
            summary: value.summary,
            why_applicable: value.why_applicable,
            required_checks: value.required_checks,
        };
        Ok(KnowledgeApplicableReport {
            schema_version: 1,
            code: "ok".to_string(),
            work: work_id.to_string(),
            required: buckets.required.into_iter().map(item).collect(),
            recommended: buckets.recommended.into_iter().map(item).collect(),
            suggested: buckets.suggested.into_iter().map(item).collect(),
            excluded: buckets.excluded,
        })
    }
}

fn knowledge_path_matches(anchor: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return anchor.starts_with(prefix);
    }
    if let Some(prefix) = pattern.strip_suffix("/*") {
        return anchor.starts_with(prefix) && !anchor[prefix.len()..].contains('/');
    }
    anchor == pattern
}

// ---------------------------------------------------------------------------
// Subject loading and eligibility
// ---------------------------------------------------------------------------

impl JsonGraphStore {
    fn load_subject(&self, id: &str, projection: &GraphProjection) -> PulseResult<Node> {
        let path = self.node_path(id);
        if !path.exists() {
            return Err(PulseError::validation(
                "work_packet_subject_not_found",
                format!("subject {id} not found"),
            ));
        }
        let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        let node: Node =
            serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))?;
        if !projection.nodes.iter().any(|n| n.id == id) {
            return Err(PulseError::validation(
                "work_packet_graph_invalid",
                format!("subject {id} not found in current graph projection"),
            ));
        }
        Ok(node)
    }

    fn verify_packet_eligible(&self, node: &Node) -> PulseResult<()> {
        if node.kind != WorkKind::Ticket {
            return Err(PulseError::validation(
                "work_packet_subject_not_ticket",
                format!("subject {} is not a Ticket", node.id),
            ));
        }
        if node.role != Some(TicketRole::Implementation) {
            return Err(PulseError::validation(
                "work_packet_role_unsupported",
                format!("subject {} role is not implementation", node.id),
            ));
        }
        if node.status != NodeStatus::Ready {
            return Err(PulseError::validation(
                "work_packet_status_not_ready",
                format!(
                    "subject {} status is {:?}, expected ready",
                    node.id, node.status
                ),
            ));
        }
        let risk = node.risk.ok_or_else(|| {
            PulseError::validation(
                "work_packet_readiness_failed",
                format!("subject {} has unassessed risk", node.id),
            )
        })?;
        if risk == Risk::Unassessed {
            return Err(PulseError::validation(
                "work_packet_readiness_failed",
                format!("subject {} risk is unassessed", node.id),
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Section extractors (free functions)
// ---------------------------------------------------------------------------

fn extract_subject(node: &Node) -> SubjectSnapshot {
    SubjectSnapshot {
        id: node.id.clone(),
        kind: "ticket".to_string(),
        role: "implementation".to_string(),
        title: node.title.clone(),
        revision: node.revision,
        contract_revision: node.contract_revision,
        status: node_status_str(node.status),
        risk: risk_str(node.risk),
        materialization: materialization_str(node.materialization),
        content_dir: node.content_dir.clone(),
    }
}

fn read_packet_file(repo_root: &Path, relative: &str) -> PulseResult<PacketRawFile> {
    let path = safe_repo_relative(relative)?;
    let full = repo_root.join(&path);
    let bytes = fs::read(&full).map_err(|error| PulseError::io(&full, error))?;
    let content = String::from_utf8(bytes.clone()).map_err(|_| {
        PulseError::validation(
            "work_packet_content_invalid",
            format!("packet content is not UTF-8: {relative}"),
        )
    })?;
    Ok(PacketRawFile {
        path: path.to_string_lossy().replace('\\', "/"),
        content_hash: crate::canonical_json::hash_bytes(&bytes),
        content,
    })
}

fn read_optional_packet_file(
    repo_root: &Path,
    relative: &str,
) -> PulseResult<Option<PacketRawFile>> {
    if repo_root.join(safe_repo_relative(relative)?).exists() {
        read_packet_file(repo_root, relative).map(Some)
    } else {
        Ok(None)
    }
}

fn extract_packet_ticket(repo_root: &Path, node: &Node) -> PulseResult<PacketTicket> {
    let ticket_path = format!("{}/ticket.md", node.content_dir);
    Ok(PacketTicket {
        node: extract_subject(node),
        brief_hash: node.brief_hash.clone(),
        tags: node.tags.clone(),
        ticket_md: read_packet_file(repo_root, &ticket_path)?,
        plan_md: read_optional_packet_file(repo_root, &format!("{}/plan.md", node.content_dir))?,
    })
}

fn extract_packet_parents(
    repo_root: &Path,
    node: &Node,
    projection: &GraphProjection,
) -> PulseResult<Vec<PacketParentSummary>> {
    extract_parents(node, projection)
        .into_iter()
        .map(|parent| {
            let summary =
                read_optional_packet_file(repo_root, &format!("{}/story.md", parent.content_dir))?
                    .or(read_optional_packet_file(
                        repo_root,
                        &format!("{}/brief.md", parent.content_dir),
                    )?);
            let approach_md = if parent.kind == "story" {
                read_optional_packet_file(
                    repo_root,
                    &format!("{}/approach.md", parent.content_dir),
                )?
            } else {
                None
            };
            Ok(PacketParentSummary {
                node: parent,
                summary: summary.map(|file| file.content).unwrap_or_default(),
                approach_md,
            })
        })
        .collect()
}

fn extract_packet_decisions(
    repo_root: &Path,
    node: &Node,
    projection: &GraphProjection,
) -> Vec<PacketDecisionSummary> {
    // Decision context is graph-derived: Decision nodes connected to this
    // Ticket through `related` edges in either direction. The linked
    // decision.md prose is inlined as the bounded contract context.
    let mut decision_ids: Vec<String> = Vec::new();
    for edge in &projection.edges {
        if edge.edge_type != EdgeType::Related {
            continue;
        }
        if edge.to == node.id {
            decision_ids.push(edge.from.clone());
        } else if edge.from == node.id {
            decision_ids.push(edge.to.clone());
        }
    }
    decision_ids.sort();
    decision_ids.dedup();
    decision_ids
        .into_iter()
        .filter_map(|id| {
            let decision = node_by_id(projection, &id)?;
            if decision.kind != WorkKind::Decision {
                return None;
            }
            Some(PacketDecisionSummary {
                id: decision.id.clone(),
                title: decision.title.clone(),
                status: node_status_str(decision.status),
                decision_md: read_optional_packet_file(
                    repo_root,
                    &format!("{}/decision.md", decision.content_dir),
                )
                .ok()
                .flatten(),
            })
        })
        .collect()
}

fn extract_packet_qa(repo_root: &Path, parents: &[PacketParentSummary]) -> PacketQa {
    let story = parents.iter().find(|parent| parent.node.kind == "story");
    let Some(story) = story else {
        return PacketQa {
            posture: "none".to_string(),
            cases: vec![],
        };
    };
    let cases = read_optional_packet_file(repo_root, &format!("{}/qa.md", story.node.content_dir))
        .ok()
        .flatten()
        .into_iter()
        .collect();
    PacketQa {
        posture: "available".to_string(),
        cases,
    }
}

fn packet_docs_from_legacy(documentation: PacketDocumentation) -> PacketDocs {
    PacketDocs {
        required: documentation.applicability.required,
        suggested: documentation.suggested_sections,
        write_candidates: documentation.applicability.write_candidates,
        excluded: documentation.applicability.excluded,
        read_budget: documentation.read_budget,
    }
}

fn node_by_id<'a>(projection: &'a GraphProjection, id: &str) -> Option<&'a Node> {
    projection.nodes.iter().find(|node| node.id == id)
}

fn extract_parents(node: &Node, projection: &GraphProjection) -> Vec<PacketParentRef> {
    let mut parents = Vec::new();
    for edge in &projection.edges {
        if edge.edge_type != EdgeType::Parent {
            continue;
        }
        if edge.from != node.id {
            continue;
        }
        if let Some(parent) = node_by_id(projection, &edge.to) {
            parents.push(PacketParentRef {
                relation: "parent_of".to_string(),
                id: parent.id.clone(),
                kind: parent.kind.as_str().to_string(),
                revision: parent.revision,
                contract_revision: parent.contract_revision,
                status: node_status_str(parent.status),
                title: parent.title.clone(),
                content_dir: parent.content_dir.clone(),
            });
            // Walk one more level (max 2 edges from Ticket)
            for ancestor_edge in &projection.edges {
                if ancestor_edge.edge_type != EdgeType::Parent {
                    continue;
                }
                if ancestor_edge.from != parent.id {
                    continue;
                }
                if let Some(ancestor) = node_by_id(projection, &ancestor_edge.to) {
                    parents.push(PacketParentRef {
                        relation: "parent_of".to_string(),
                        id: ancestor.id.clone(),
                        kind: ancestor.kind.as_str().to_string(),
                        revision: ancestor.revision,
                        contract_revision: ancestor.contract_revision,
                        status: node_status_str(ancestor.status),
                        title: ancestor.title.clone(),
                        content_dir: ancestor.content_dir.clone(),
                    });
                }
            }
        }
    }
    parents.sort_by(|a, b| a.id.cmp(&b.id));
    parents
}

fn extract_graph(
    readiness: &ReadinessSnapshot,
    projection: &GraphProjection,
) -> PulseResult<PacketGraph> {
    let structural = &readiness.structural;
    let subject_id = &structural.subject;

    let hard_blockers: Vec<work_packet::PacketBlockerItem> = structural
        .hard_blockers
        .iter()
        .map(|b| work_packet::PacketBlockerItem {
            id: b.id.clone(),
            relation: "blocked_by".to_string(),
            title: node_by_id(projection, &b.id)
                .map(|node| node.title.clone())
                .unwrap_or_else(|| b.path.join(" -> ")),
        })
        .collect();

    let soft_preferences: Vec<work_packet::PacketBlockerItem> = structural
        .soft_preferences
        .iter()
        .map(|p| work_packet::PacketBlockerItem {
            id: p.preferred_after.clone(),
            relation: "preferred_after".to_string(),
            title: node_by_id(projection, &p.preferred_after)
                .map(|node| node.title.clone())
                .unwrap_or_default(),
        })
        .collect();

    let supersession = structural.supersession.as_ref().and_then(|s| {
        s.replacement.clone().map(|repl| {
            let node = node_by_id(projection, &repl);
            work_packet::PacketSupersessionRef {
                id: repl,
                revision: node.map(|n| n.revision).unwrap_or(0),
                status: node.map(|n| node_status_str(n.status)).unwrap_or_default(),
                title: node.map(|n| n.title.clone()).unwrap_or_default(),
            }
        })
    });

    let structural_state = match structural.structural_state {
        StructuralState::Candidate => "executable",
        StructuralState::Blocked => "blocked",
        StructuralState::Paused => "paused",
        StructuralState::Terminal => "terminal",
        StructuralState::NotExecutableKind => "not_executable",
        StructuralState::Invalid => "invalid",
    };

    // Incident relation projection: max 128 edges total
    let mut outgoing = Vec::new();
    let mut incoming = Vec::new();
    let mut total = 0usize;
    for edge in &projection.edges {
        if edge.from == *subject_id || edge.to == *subject_id {
            total += 1;
        }
    }
    if total > work_packet::MAX_INCIDENT_RELATIONS {
        return Err(PulseError::validation(
            "work_packet_relation_overflow",
            "more than 128 incident edges",
        ));
    }

    for edge in &projection.edges {
        if edge.from == *subject_id {
            if let Some(opp) = node_by_id(projection, &edge.to) {
                outgoing.push(PacketRelationItem {
                    edge_id: edge.id.clone(),
                    edge_type: edge_type_str(edge.edge_type),
                    from: edge.from.clone(),
                    to: edge.to.clone(),
                    edge_revision: edge.revision,
                    opposite_id: opp.id.clone(),
                    opposite_kind: opp.kind.as_str().to_string(),
                    opposite_status: node_status_str(opp.status),
                    opposite_revision: opp.revision,
                    opposite_title: opp.title.clone(),
                });
            }
        } else if edge.to == *subject_id {
            if let Some(opp) = node_by_id(projection, &edge.from) {
                incoming.push(PacketRelationItem {
                    edge_id: edge.id.clone(),
                    edge_type: edge_type_str(edge.edge_type),
                    from: edge.from.clone(),
                    to: edge.to.clone(),
                    edge_revision: edge.revision,
                    opposite_id: opp.id.clone(),
                    opposite_kind: opp.kind.as_str().to_string(),
                    opposite_status: node_status_str(opp.status),
                    opposite_revision: opp.revision,
                    opposite_title: opp.title.clone(),
                });
            }
        }
    }

    Ok(PacketGraph {
        structural_state: structural_state.to_string(),
        hard_blockers,
        soft_preferences,
        supersession,
        relations: PacketRelationBundle { outgoing, incoming },
    })
}

fn extract_documentation(docs: &ApplicableDocsReport) -> PulseResult<PacketDocumentation> {
    if docs.registry.fingerprint.is_empty() {
        return Err(PulseError::validation(
            "work_packet_docs_registry_missing",
            "packet requires existing docs manifest/registry",
        ));
    }
    if docs.gate.status != "complete" {
        return Err(PulseError::validation(
            "work_packet_docs_context_incomplete",
            format!(
                "documentation applicability gate status is {}",
                docs.gate.status
            ),
        ));
    }
    let required: Vec<PacketDocRef> = docs.required.iter().map(packet_doc_ref).collect();
    let optional: Vec<PacketDocRef> = docs.optional.iter().map(packet_doc_ref).collect();

    let by_id: BTreeMap<&str, &ApplicableDocument> = docs
        .required
        .iter()
        .chain(docs.optional.iter())
        .map(|doc| (doc.id.as_str(), doc))
        .collect();
    let mut write_candidates = Vec::new();
    for wc in &docs.write_candidates {
        let doc = by_id.get(wc.id.as_str()).ok_or_else(|| {
            PulseError::validation(
                "work_packet_docs_context_incomplete",
                format!(
                    "write candidate {} has no current applicable document metadata",
                    wc.id
                ),
            )
        })?;
        let mut dto = packet_doc_ref(doc);
        dto.reasons = wc.reasons.clone();
        write_candidates.push(dto);
    }

    let excluded: Vec<PacketExcludedDocRef> = docs
        .excluded
        .iter()
        .map(|e| PacketExcludedDocRef {
            id: e.id.clone(),
            path: e.path.clone(),
            reason_codes: e.reason_codes.clone(),
            replacement: e.replacement.clone(),
        })
        .collect();

    Ok(PacketDocumentation {
        applicability: PacketDocsApplicability {
            status: docs.gate.status.clone(),
            required,
            optional,
            write_candidates,
            excluded,
        },
        suggestion_query: work_packet::PacketSuggestionQuery {
            text: docs.work.id.clone(),
            normalized_terms: vec![docs.work.id.clone()],
        },
        suggested_sections: vec![],
        read_budget: PacketReadBudget {
            required_sections: docs.required.len() as u64,
            recommended_initial_sections: work_packet::RECOMMENDED_INITIAL_SECTIONS as u64,
            max_initial_lines: work_packet::MAX_INITIAL_LINES as u64,
            suggestion_limit: work_packet::MAX_SUGGESTED_SECTIONS as u64,
            snippet_max_bytes_each: work_packet::MAX_SNIPPET_BYTES_EACH as u64,
        },
        index: work_packet::PacketDocsIndex {
            state: "not_installed".to_string(),
            fingerprint: docs.registry.fingerprint.clone(),
            mode: "lexical".to_string(),
        },
    })
}

fn packet_doc_ref(document: &ApplicableDocument) -> PacketDocRef {
    PacketDocRef {
        id: document.id.clone(),
        path: document.path.clone(),
        kind: document_kind_str(document.kind).to_string(),
        status: serde_json::to_string(&document.status)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string(),
        owner: document.owner.clone(),
        summary: document.summary.clone(),
        revision: document.document_revision,
        content_hash: document.content_hash.clone(),
        reasons: document.reasons.clone(),
    }
}

fn docs_content_fingerprints(documentation: &PacketDocumentation) -> BTreeMap<String, String> {
    documentation
        .applicability
        .required
        .iter()
        .chain(documentation.applicability.optional.iter())
        .chain(documentation.applicability.write_candidates.iter())
        .map(|doc| (doc.id.clone(), doc.content_hash.clone()))
        .collect()
}

fn suggestion_fingerprints(
    suggestions: &[work_packet::PacketSuggestedSection],
) -> Vec<(u64, String, String, String, u64, u64)> {
    suggestions
        .iter()
        .map(|section| {
            (
                section.rank,
                section.section_ref.clone(),
                section.document_hash.clone(),
                section.section_hash.clone(),
                section.score_micros,
                section.lexical_score_micros,
            )
        })
        .collect()
}

/// Refresh the cache-only docs index and search suggestions between fences.
///
/// Returns (suggested_sections, docs_cache_fingerprint).
fn packet_refresh_and_search(
    repo_root: &Path,
    phase1: &PacketPhase1State,
) -> PulseResult<(
    Vec<crate::work_packet::PacketSuggestedSection>,
    Option<String>,
)> {
    let index_opts = crate::docs::index::IndexOptions {
        changed: false,
        rebuild: false,
        check: false,
        include_draft: false,
        include_stale: false,
    };
    // Build/search cache-only disposable docs index (never writes
    // projections). If the cache is already current this is a no-op.
    crate::docs::index::build_search_cache(repo_root, index_opts)
        .map_err(|error| map_docs_index_error(error, "docs search cache refresh failed"))?;

    let docs_cache_fp = crate::docs::index::current_cache_fingerprint(repo_root)?;
    let suggested_sections = search_suggestions(
        repo_root,
        &phase1.pre_suggestion_query,
        &phase1.pre_excluded_doc_ids,
        &phase1.docs_work_context,
        false,
    )?;
    Ok((suggested_sections, docs_cache_fp))
}

fn work_documentation_context(node: &Node) -> WorkDocumentationContext {
    node.documentation
        .as_ref()
        .map(|documentation| {
            WorkDocumentationContext::from((node.id.as_str(), node.revision, documentation))
        })
        .unwrap_or_else(|| WorkDocumentationContext::unknown(node.id.clone(), node.revision))
}

fn current_docs_cache_fingerprint_under_fence(repo_root: &Path) -> PulseResult<Option<String>> {
    let status = crate::docs::index::index_status_with_options(
        repo_root,
        crate::docs::policy::RetrievalEligibilityOptions::default(),
    )?;
    if status.index.state == "current" {
        Ok(status.index.fingerprint)
    } else {
        Ok(None)
    }
}

fn map_docs_index_error(error: PulseError, context: &str) -> PulseError {
    match error.code() {
        "work_packet_docs_score_invalid" | "work_packet_snapshot_changed" => error,
        "lock_timeout" => PulseError::validation(
            "work_packet_lock_timeout",
            format!("docs search cache lock timed out; cause_code=lock_timeout: {error}"),
        ),
        code if code.starts_with("docs_") => PulseError::validation(
            "work_packet_docs_index_unavailable",
            format!("{context}; cause_code={code}: {error}"),
        ),
        _ => error,
    }
}

fn acquire_packet_fence(repo_root: &Path) -> PulseResult<WriteGuard> {
    WriteGuard::acquire(repo_root).map_err(map_packet_lock_error)
}

fn map_packet_lock_error(error: PulseError) -> PulseError {
    match error {
        PulseError::LockTimeout { .. } => PulseError::validation(
            "work_packet_lock_timeout",
            format!("repository fence lock timed out; cause_code=lock_timeout: {error}"),
        ),
        other => other,
    }
}

fn validate_packet_operational_paths(repo_root: &Path) -> PulseResult<()> {
    for relative in [
        ".pulse/runtime/locks/workgraph.lock",
        ".pulse/runtime/locks/docs-search.lock",
        ".pulse/cache/workgraph.snapshot.json",
        ".pulse/cache/docs-search/CURRENT",
    ] {
        validate_packet_operational_path(repo_root, relative)?;
    }
    Ok(())
}

fn validate_packet_operational_path(repo_root: &Path, relative: &str) -> PulseResult<()> {
    let tracked = Command::new("git")
        .current_dir(repo_root)
        .args(["ls-files", "--error-unmatch", "--", relative])
        .output()
        .map_err(|error| PulseError::io(repo_root, error))?;
    if tracked.status.success() {
        return Err(packet_operational_path_not_ignored(relative));
    }
    if tracked.status.code() != Some(1) {
        return Err(PulseError::validation(
            "work_packet_source_unavailable",
            format!(
                "git ls-files failed for {relative}: {}",
                String::from_utf8_lossy(&tracked.stderr).trim()
            ),
        ));
    }

    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["check-ignore", "-q", "--", relative])
        .output()
        .map_err(|error| PulseError::io(repo_root, error))?;
    if output.status.success() {
        return Ok(());
    }
    if output.status.code() != Some(1) {
        return Err(PulseError::validation(
            "work_packet_source_unavailable",
            format!(
                "git check-ignore failed for {relative}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }

    if repo_root.join(relative).exists() {
        let status = Command::new("git")
            .current_dir(repo_root)
            .args([
                "status",
                "--porcelain=v1",
                "--untracked-files=all",
                "--",
                relative,
            ])
            .output()
            .map_err(|error| PulseError::io(repo_root, error))?;
        if !status.status.success() {
            return Err(PulseError::validation(
                "work_packet_source_unavailable",
                format!(
                    "git status failed for {relative}: {}",
                    String::from_utf8_lossy(&status.stderr).trim()
                ),
            ));
        }
        if String::from_utf8_lossy(&status.stdout).trim().is_empty() {
            return Ok(());
        }
    }

    Err(packet_operational_path_not_ignored(relative))
}

fn packet_operational_path_not_ignored(relative: &str) -> PulseError {
    PulseError::validation(
        "work_packet_operational_path_not_ignored",
        format!(
            "packet operational path {relative} would dirty the source tree; ignore .pulse/runtime/ and .pulse/cache/ before running work packet"
        ),
    )
}

impl JsonGraphStore {
    fn work_packet_test_barrier_after_first_fence(&self) -> PulseResult<()> {
        if !self.work_packet_after_first_fence_failpoint {
            return Ok(());
        }
        test_only_work_packet_barrier_after_first_fence()
    }
}

#[cfg(any(test, debug_assertions))]
fn test_only_work_packet_barrier_after_first_fence() -> PulseResult<()> {
    use std::io::Write;
    use std::time::{Duration, Instant};

    let Some(signal_path) = std::env::var_os("PULSE_WORK_PACKET_AFTER_FIRST_FENCE_SIGNAL") else {
        return Err(PulseError::validation(
            "work_packet_test_failpoint_missing",
            "--test-work-packet-after-first-fence requires PULSE_WORK_PACKET_AFTER_FIRST_FENCE_SIGNAL",
        ));
    };
    let signal_path = PathBuf::from(signal_path);
    if let Some(parent) = signal_path.parent() {
        fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
    }
    let mut file =
        fs::File::create(&signal_path).map_err(|error| PulseError::io(&signal_path, error))?;
    file.write_all(b"after_first_fence\n")
        .map_err(|error| PulseError::io(&signal_path, error))?;
    file.sync_all()
        .map_err(|error| PulseError::io(&signal_path, error))?;

    if let Some(wait_path) = std::env::var_os("PULSE_WORK_PACKET_AFTER_FIRST_FENCE_WAIT") {
        let wait_path = PathBuf::from(wait_path);
        let start = Instant::now();
        while !wait_path.exists() {
            if start.elapsed() > Duration::from_secs(10) {
                return Err(PulseError::validation(
                    "work_packet_snapshot_changed",
                    format!(
                        "timed out waiting for test barrier release at {}",
                        wait_path.display()
                    ),
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    Ok(())
}

#[cfg(not(any(test, debug_assertions)))]
fn test_only_work_packet_barrier_after_first_fence() -> PulseResult<()> {
    Ok(())
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn node_status_str(status: NodeStatus) -> String {
    match status {
        NodeStatus::Draft => "draft",
        NodeStatus::Shaped => "shaped",
        NodeStatus::Ready => "ready",
        NodeStatus::Active => "active",
        NodeStatus::Verifying => "verifying",
        NodeStatus::Done => "done",
        NodeStatus::Rework => "rework",
        NodeStatus::Blocked => "blocked",
        NodeStatus::Cancelled => "cancelled",
        NodeStatus::Superseded => "superseded",
    }
    .to_string()
}

fn risk_str(risk: Option<Risk>) -> String {
    match risk {
        None | Some(Risk::Unassessed) => "unassessed",
        Some(Risk::Low) => "low",
        Some(Risk::Medium) => "medium",
        Some(Risk::High) => "high",
        Some(Risk::Critical) => "critical",
    }
    .to_string()
}

fn materialization_str(mat: Option<crate::graph::model::contract::Materialization>) -> String {
    match mat {
        None | Some(crate::graph::model::contract::Materialization::Unassessed) => "unassessed",
        Some(crate::graph::model::contract::Materialization::R0) => "R0",
        Some(crate::graph::model::contract::Materialization::R1) => "R1",
        Some(crate::graph::model::contract::Materialization::R2) => "R2",
        Some(crate::graph::model::contract::Materialization::R3) => "R3",
    }
    .to_string()
}

fn edge_type_str(edge_type: EdgeType) -> String {
    match edge_type {
        EdgeType::Parent => "parent",
        EdgeType::BlockedBy => "blocked_by",
        EdgeType::PreferredAfter => "preferred_after",
        EdgeType::SupersededBy => "superseded_by",
        EdgeType::Duplicates => "duplicates",
        EdgeType::Related => "related",
    }
    .to_string()
}

fn document_kind_str(kind: DocumentKind) -> &'static str {
    match kind {
        DocumentKind::Policy => "policy",
        DocumentKind::Product => "product",
        DocumentKind::Architecture => "architecture",
        DocumentKind::Domain => "domain",
        DocumentKind::Operations => "operations",
        DocumentKind::Reference => "reference",
        DocumentKind::Generated => "generated",
    }
}

// ===================================================================
// P2S1-I4: Deterministic suggestion query, score conversion, search
// ===================================================================

/// Build the deterministic suggestion query string.
///
/// Order of fragments:
/// 1. Ticket title
/// 2. objective
/// 3. target_behavior
/// 4. Each acceptance summary (by acceptance ID)
/// 5. Anchor path basenames
/// 6. Documentation routing domains, then labels (lexical sort)
/// 7. Packet Decision titles (sort by Decision ID)
///
/// Normalized and truncated to at most 32 terms / 256 UTF-8 bytes.
pub fn build_suggestion_query(
    node: &Node,
    brief: Option<&TicketBrief>,
    decisions: &[PacketDecisionSummary],
    docs_work: &WorkDocumentationContext,
) -> PulseResult<work_packet::PacketSuggestionQuery> {
    let mut fragments: Vec<String> = Vec::new();

    // 1. Ticket title
    if !node.title.is_empty() {
        fragments.push(node.title.clone());
    }

    if let Some(brief) = brief {
        // 2. objective
        if let Some(objective) = &brief.objective {
            if !objective.is_empty() {
                fragments.push(objective.clone());
            }
        }
        // 3. target_behavior
        if let Some(target) = &brief.target_behavior {
            if !target.is_empty() {
                fragments.push(target.clone());
            }
        }
        // 4. Each acceptance summary (by acceptance ID)
        for item in &brief.acceptance {
            if !item.summary.is_empty() {
                fragments.push(item.summary.clone());
            }
        }
        // 5. Anchor basenames
        let mut anchor_fragments: Vec<String> = Vec::new();
        for anchor in &brief.code_anchors {
            if let Some(basename) = std::path::Path::new(anchor).file_name() {
                let frag = basename.to_string_lossy().to_string();
                if !frag.is_empty() {
                    anchor_fragments.push(frag);
                }
            }
        }
        anchor_fragments.sort();
        anchor_fragments.dedup();
        fragments.extend(anchor_fragments);
    }

    // 6. Documentation tags (lexical sort).
    for tag in &docs_work.tags {
        fragments.push(tag.clone());
    }

    // 7. Packet Decision titles (sort by Decision ID).
    let mut decision_titles: Vec<(&str, &str)> = decisions
        .iter()
        .map(|decision| (decision.id.as_str(), decision.title.as_str()))
        .collect();
    decision_titles.sort_by(|a, b| a.0.cmp(b.0));
    for (_, title) in decision_titles {
        if !title.is_empty() {
            fragments.push(title.to_string());
        }
    }

    // Normalize fragments
    let normalized = normalize_query_fragments(&fragments);

    // Tokenize and build prefix
    let all_terms: Vec<String> = normalized
        .iter()
        .flat_map(|frag| crate::docs::lexical::tokenize_query_text(frag))
        .collect();

    // Deduplicate preserving order
    let mut seen = std::collections::BTreeSet::new();
    let unique_terms: Vec<String> = all_terms
        .into_iter()
        .filter(|t| seen.insert(t.clone()))
        .collect();

    // Take prefix: at most 32 terms, total byte length <= 256 when joined by space
    let final_terms = truncate_query_terms(&unique_terms, 32, 256);

    if final_terms.is_empty() {
        return Err(PulseError::validation(
            "work_packet_docs_query_empty",
            "deterministic suggestion query produced no terms",
        ));
    }

    let query_text = final_terms.join(" ");
    Ok(work_packet::PacketSuggestionQuery {
        text: query_text,
        normalized_terms: final_terms,
    })
}

/// Normalize query fragments: trim, collapse whitespace, dedup, cap at 280
/// Unicode scalar values per fragment.
fn normalize_query_fragments(fragments: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for frag in fragments {
        let trimmed: String = frag.split_whitespace().collect::<Vec<_>>().join(" ");
        if trimmed.is_empty() {
            continue;
        }
        let capped: String = trimmed.chars().take(280).collect();
        if seen.insert(capped.clone()) {
            out.push(capped);
        }
    }
    out
}

/// Truncate terms to at most `max_terms` and total byte length at most
/// `max_bytes` when joined by one ASCII space. Never cuts a term.
fn truncate_query_terms(terms: &[String], max_terms: usize, max_bytes: usize) -> Vec<String> {
    let mut result = Vec::new();
    let mut byte_len = 0usize;
    for term in terms {
        if result.len() >= max_terms {
            break;
        }
        let add_bytes = if result.is_empty() {
            term.len()
        } else {
            1 + term.len() // preceding space + term
        };
        if byte_len + add_bytes > max_bytes {
            break;
        }
        byte_len += add_bytes;
        result.push(term.clone());
    }
    result
}

/// Convert an f64 search score to a non-negative integer micro-score.
///
/// Per P2S1-D13:
///   score_micros = round_nonnegative(score * 1_000_000)
/// Rules:
///   - input must be finite and >= 0
///   - rounding uses f64::round
///   - checked conversion to u64
///   - violation => work_packet_docs_score_invalid
pub fn score_to_micros(score: f64) -> PulseResult<u64> {
    if !score.is_finite() || score < 0.0 {
        return Err(PulseError::validation(
            "work_packet_docs_score_invalid",
            format!("search score must be finite and non-negative: {}", score),
        ));
    }
    let product = score * 1_000_000.0;
    let rounded = product.round();
    if rounded < 0.0 || rounded > (u64::MAX as f64) {
        return Err(PulseError::validation(
            "work_packet_docs_score_invalid",
            format!("score micros out of range: {}", rounded),
        ));
    }
    // Safe: checked above
    Ok(rounded as u64)
}

/// Search the cache-only docs index for suggested sections, excluding any
/// sections belonging to excluded documents.
fn search_suggestions(
    repo_root: &std::path::Path,
    query: &work_packet::PacketSuggestionQuery,
    excluded_doc_ids: &[String],
    docs_work: &WorkDocumentationContext,
    under_repository_fence: bool,
) -> PulseResult<Vec<work_packet::PacketSuggestedSection>> {
    if query.normalized_terms.is_empty() {
        return Ok(Vec::new());
    }

    let report = crate::docs::search_docs(
        repo_root,
        &query.text,
        crate::docs::SearchOptions {
            kind: None,
            tag: None,
            status: None,
            limit: Some(work_packet::MAX_SUGGESTED_SECTIONS),
            no_refresh: false,
            explain: true,
            include_draft: false,
            include_stale: false,
            work: Some(docs_work.clone()),
            under_repository_fence,
        },
    )
    .map_err(|error| map_docs_index_error(error, "docs suggestion search failed"))?;

    let excluded: std::collections::BTreeSet<&str> =
        excluded_doc_ids.iter().map(|id| id.as_str()).collect();
    let mut results = Vec::new();
    for hit in report.results {
        if excluded.contains(hit.document_id.as_str()) {
            continue;
        }
        let score_micros = score_to_micros(hit.score)?;
        let lexical_score_micros = score_to_micros(hit.lexical_score)?;
        results.push(work_packet::PacketSuggestedSection {
            rank: 0,
            score_micros,
            lexical_score_micros,
            section_ref: hit.section_ref,
            heading_path: hit.heading_path.join(" > "),
            line_range: work_packet::PacketLineRange {
                start: hit.range.start_line as u64,
                end: hit.range.end_line as u64,
            },
            document_id: hit.document_id,
            document_hash: hit.document_content_hash,
            section_hash: hit.section_content_hash,
            summary: hit.summary,
            snippet: hit.snippet,
            status: hit.status,
            owner: hit.owner,
            kind: hit.kind,
            matched_fields: hit.matched_fields,
            applicability_reasons: hit.applicability_reasons,
        });
    }

    results.sort_by(|a, b| {
        b.score_micros
            .cmp(&a.score_micros)
            .then_with(|| b.lexical_score_micros.cmp(&a.lexical_score_micros))
            .then_with(|| a.section_ref.cmp(&b.section_ref))
    });
    results.truncate(work_packet::MAX_SUGGESTED_SECTIONS);
    for (idx, result) in results.iter_mut().enumerate() {
        result.rank = (idx as u64) + 1;
    }
    Ok(results)
}

/// Shaped findings from every rework verification recorded for `ticket_id`,
/// each carrying the actor that recorded it. Receipt order is deterministic
/// (sorted by verification id); the packet normalize pass sorts again by
/// content. Read failures collapse to an empty list — rework observations
/// must never block a packet rebuild.
fn rework_observations(
    repo_root: &Path,
    ticket_id: &str,
) -> PulseResult<Vec<crate::work_packet::PacketReworkObservation>> {
    let mut out = Vec::new();
    for verification in crate::kernel::completion::list_verifications(repo_root)? {
        if verification.ticket_id != ticket_id
            || verification.disposition != crate::execution::VerificationDisposition::Rework
        {
            continue;
        }
        for finding in &verification.findings {
            out.push(crate::work_packet::PacketReworkObservation {
                actor: verification.verified_by.clone(),
                acceptance_id: finding.acceptance_id.clone(),
                summary: finding.summary.clone(),
                owner: finding.owner.clone(),
                check: finding.check.clone(),
                severity: finding.severity,
                unverifiable: finding.unverifiable,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod rework_observation_tests {
    use crate::execution::{
        Finding, FindingSeverity, VerificationCheck, VerificationDisposition, VerificationReceipt,
    };

    fn rework_receipt(ticket_id: &str, actor: &str, findings: Vec<Finding>) -> VerificationReceipt {
        let mut receipt = VerificationReceipt {
            schema_version: 1,
            verification_id: format!("verify_{ticket_id}_{actor}"),
            idempotency_key_hash: "sha256:00".to_string(),
            handoff_id: "handoff_x".to_string(),
            ticket_id: ticket_id.to_string(),
            lease_id: "lease_x".to_string(),
            source_commit: "c".repeat(40),
            source_dirty_hash: "d".repeat(64),
            disposition: VerificationDisposition::Rework,
            summary: "rework".to_string(),
            checks: vec![VerificationCheck {
                name: "focused".to_string(),
                command: "node scripts/verify.mjs".to_string(),
                exit_code: 1,
                artifact_ids: vec![],
            }],
            acceptance_proofs: vec![],
            findings,
            verified_by: actor.to_string(),
            recorded_at: "2026-09-06T00:00:00Z".to_string(),
            resulting_status: "rework".to_string(),
            resulting_revision: 9,
            verification_fingerprint: String::new(),
        };
        receipt.verification_fingerprint = receipt.compute_fingerprint().unwrap();
        receipt
    }

    /// Decision 0012 §5: the packet's rework observations carry every
    /// reviewer's findings, each with the actor that recorded it, so the
    /// next worker run knows exactly what was shown broken and how.
    #[test]
    fn rework_observations_carry_findings_with_actor() {
        let repo = tempfile::tempdir().unwrap();
        let verifications = repo.path().join(".pulse/evidence/execution/verifications");
        std::fs::create_dir_all(&verifications).unwrap();
        for (actor, summary, check) in [
            (
                "agent:runner:reviewer",
                "expired branch missing",
                Some("node scripts/verify.mjs --grep expired"),
            ),
            ("agent:human:second", "gut feeling, ran nothing", None),
        ] {
            let receipt = rework_receipt(
                "TK-1",
                actor,
                vec![Finding {
                    summary: summary.to_string(),
                    owner: "src/token.mjs".to_string(),
                    check: check.map(str::to_string),
                    severity: FindingSeverity::High,
                    acceptance_id: Some("AC-1".to_string()),
                    case_id: None,
                    // `work verify` derives this from the presence of a
                    // check before recording; mirror that here.
                    unverifiable: check.is_none(),
                }],
            );
            std::fs::write(
                verifications.join(format!("{}.json", receipt.verification_id)),
                serde_json::to_vec(&receipt).unwrap(),
            )
            .unwrap();
        }
        let observations = super::rework_observations(repo.path(), "TK-1").unwrap();
        assert_eq!(observations.len(), 2);
        let by_actor = |actor: &str| {
            observations
                .iter()
                .find(|observation| observation.actor == actor)
                .unwrap_or_else(|| panic!("no observation for {actor}"))
        };
        let recorded = by_actor("agent:runner:reviewer");
        assert_eq!(recorded.summary, "expired branch missing");
        assert!(!recorded.unverifiable);
        let gut = by_actor("agent:human:second");
        assert_eq!(gut.check, None);
        assert!(gut.unverifiable);
        // Findings of other Tickets never leak into this packet.
        let other = super::rework_observations(repo.path(), "TK-2").unwrap();
        assert!(other.is_empty());
    }
}
