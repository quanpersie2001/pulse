//! Coherent canonical packet snapshot builder (P2S1-I3 / P2S1-I4).
//!
//! This module composes the cross-domain [`WorkPacketV1`] using a two-fence
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

use crate::docs::applicability::{ApplicableDocsReport, ApplicableDocument};
use crate::docs::model::{DocumentAuthority, DocumentKind, WorkDocumentationContext};
use crate::evidence::model::{BranchCriticality, BranchDisposition, ShapeMode};
use crate::graph::contract::{
    ExpectedEvidence, ImplementationMode, PlanPolicy, QaImpactPosture, Risk, TicketRole,
    WorkSurface,
};
use crate::graph::edge::EdgeType;
use crate::graph::executability::StructuralState;
use crate::graph::node::{DocumentationImpactPosture, Node, NodeStatus};
use crate::graph::projection::GraphProjection;
use crate::graph::readiness::{
    evaluate as evaluate_readiness, EvalProfile, ReadinessReport, ReadinessStatus,
    ShapingReceiptSnapshot,
};
use crate::graph::store::JsonGraphStore;
use crate::id::WorkKind;
use crate::kernel::readiness::ReadinessSnapshot;
use crate::source::{check_repository_identity, packet_base_snapshot};
use crate::storage::safe_repo_relative;
use crate::storage::transaction::recover_prepared_transactions;
use crate::storage::WriteGuard;
use crate::work_packet;
use crate::work_packet::{
    PacketAssurance, PacketBoundedFog, PacketBranchDisposition, PacketBudget, PacketCapabilities,
    PacketContentRef, PacketContractItem, PacketContractScope, PacketCriticalBranch,
    PacketDecisionFrontier, PacketDecisionFrontierItem, PacketDispatch, PacketDocRef,
    PacketDocsApplicability, PacketDocumentation, PacketDocumentationImpact, PacketExcludedDocRef,
    PacketFutureGate, PacketGraph, PacketKnowledge, PacketParentRef, PacketQaStatus,
    PacketReadBudget, PacketRelationBundle, PacketRelationItem, PacketRemainingUncertainty,
    PacketResolution, PacketRevalidationPrecondition, PacketScope, PacketScopeHints, PacketShaping,
    PacketShapingDestination, PacketShapingMapSnapshot, PacketShapingWorkBinding, PacketSource,
    PacketSurfaceRef, PacketWorkspace, SubjectSnapshot, WorkPacketV1,
};
use crate::{PulseError, PulseResult};

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
    pub fn work_packet(&self, id: &str) -> PulseResult<WorkPacketV1> {
        // ==================================================================
        // Phase 1 — First fence: validate, load, extract query, snapshot
        // ==================================================================
        let evidence = check_repository_identity(&self.repo_root)?;
        let repository_id = evidence.repository_id.clone();

        let guard = WriteGuard::acquire(&self.repo_root)?;
        self.require_existing_workgraph_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let projection = self.export_unlocked()?;

        let node = self.load_subject(id, &projection)?;
        self.verify_packet_eligible(&node)?;

        let readiness = self.build_readiness_snapshot_from_projection(&node, &projection)?;
        let inputs = readiness.as_inputs(&node);
        let readiness_report = evaluate_readiness(&inputs, EvalProfile::Ready)?;
        if readiness_report.code != "ready" {
            return self.readiness_error(&readiness_report);
        }

        // Extract sections that do NOT depend on docs search or source.
        let subject = extract_subject(&node);
        let snapshot_pre = extract_snapshot(&readiness, &projection, &readiness_report);
        let contract = extract_contract_dto(&node)?;
        let context = extract_context(&node, &projection, &readiness);
        let shaping = extract_shaping(&node, &readiness.shaping, &projection)?;
        let graph = extract_graph(&readiness, &projection)?;
        let documentation_base = extract_documentation(&readiness.docs)?;
        let source = extract_source_snapshot(&self.repo_root, &repository_id)?;
        let workspace = extract_workspace(&node, &source, &repository_id);
        let capabilities = extract_capabilities(&node);
        let scope = extract_scope(&node);
        let assurance = extract_assurance(&node);

        // Build deterministic suggestion query (P2S1-D14).
        let docs_work_context = work_documentation_context(&node);
        let suggestion_query =
            build_suggestion_query(&node, &shaping, &context.decisions, &docs_work_context)?;
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
        let pre_source = packet_source_snapshot_from_packet(&source);

        // Phase 1 complete: drop first fence.
        drop(guard);
        work_packet_test_barrier_after_first_fence()?;

        // ==================================================================
        // Between fences — cache-only docs index refresh
        // ==================================================================
        let index_opts = crate::docs::index::IndexOptions {
            changed: false,
            rebuild: false,
            check: false,
            include_draft: false,
            include_stale: false,
        };
        // Build/search cache-only disposable docs index (never writes
        // projections). If the cache is already current this is a no-op.
        crate::docs::index::build_search_cache(&self.repo_root, index_opts)
            .map_err(|error| map_docs_index_error(error, "docs search cache refresh failed"))?;

        let docs_cache_fp = crate::docs::index::current_cache_fingerprint(&self.repo_root)?;
        let suggested_sections = search_suggestions(
            &self.repo_root,
            &pre_suggestion_query,
            &pre_excluded_doc_ids,
            &docs_work_context,
        )?;
        let selected_suggestion_fingerprints = suggestion_fingerprints(&suggested_sections);

        // ==================================================================
        // Phase 2 — Second fence: revalidate, search, assemble
        // ==================================================================
        let guard2 = WriteGuard::acquire(&self.repo_root)?;
        self.require_existing_workgraph_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let projection2 = self.export_unlocked()?;

        // Revalidate graph fingerprint.
        if projection2.graph_fingerprint != pre_graph_fingerprint {
            return Err(PulseError::validation(
                "work_packet_snapshot_changed",
                "graph fingerprint changed during work packet build",
            ));
        }

        // Revalidate subject.
        let node2 = self.load_subject(&pre_subject_id, &projection2)?;
        if node2.revision != pre_subject_revision || node2.status != pre_subject_status {
            return Err(PulseError::validation(
                "work_packet_snapshot_changed",
                "subject revision or status changed during work packet build",
            ));
        }

        // Revalidate authority and docs registry fingerprints.
        let readiness2 = self.build_readiness_snapshot_from_projection(&node2, &projection2)?;
        if readiness2.authority.fingerprint != pre_authority_fingerprint {
            return Err(PulseError::validation(
                "work_packet_snapshot_changed",
                "authority policy fingerprint changed during work packet build",
            ));
        }
        if readiness2.docs.registry.fingerprint != pre_docs_registry_fingerprint {
            return Err(PulseError::validation(
                "work_packet_snapshot_changed",
                "docs registry fingerprint changed during work packet build",
            ));
        }
        let readiness_report2 =
            evaluate_readiness(&readiness2.as_inputs(&node2), EvalProfile::Ready)?;
        if readiness_report2.readiness_fingerprint != pre_readiness_fingerprint {
            return Err(PulseError::validation(
                "work_packet_snapshot_changed",
                "readiness fingerprint changed during work packet build",
            ));
        }
        if docs_content_fingerprints(&extract_documentation(&readiness2.docs)?)
            != pre_docs_content_fingerprints
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
        if suggestion_fingerprints(&suggested_sections) != selected_suggestion_fingerprints {
            return Err(PulseError::validation(
                "work_packet_snapshot_changed",
                "selected documentation suggestion identity changed during work packet build",
            ));
        }

        // Revalidate source (HEAD, cleanliness, operation state).
        crate::source::revalidate_packet_base(&self.repo_root, &pre_source)?;

        // Permission check: no retry on snapshot/source change.
        drop(guard2);

        // Build fully integrated documentation section before completing the
        // snapshot, so the snapshot/revalidation precondition binds the same
        // docs-index fingerprint exposed in the packet documentation block.
        let docs_index_fingerprint = docs_cache_fp
            .clone()
            .unwrap_or_else(|| readiness2.docs.registry.fingerprint.clone());
        let documentation = PacketDocumentation {
            applicability: documentation_base.applicability,
            suggestion_query: pre_suggestion_query,
            suggested_sections,
            read_budget: documentation_base.read_budget,
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

        let snapshot_done = complete_snapshot(snapshot_pre, &documentation, &source);
        let dispatch = build_dispatch(&readiness_report, &snapshot_done, &source)?;

        // ---- Assemble ----
        let mut packet = WorkPacketV1 {
            schema_version: work_packet::PACKET_SCHEMA_VERSION,
            profile: work_packet::PACKET_PROFILE.to_string(),
            code: "reservation_candidate".to_string(),
            subject,
            snapshot: snapshot_done,
            contract,
            context,
            shaping,
            graph,
            documentation,
            knowledge: PacketKnowledge {
                status: "not_installed".to_string(),
                owner_phase: 4,
                knowledge_fingerprint: None,
                required: vec![],
                recommended: vec![],
                suggested: vec![],
                excluded: vec![],
            },
            source,
            workspace,
            capabilities,
            scope,
            assurance,
            dispatch,
            budget: PacketBudget::default(),
            packet_fingerprint: String::new(),
            reason_codes: vec![],
        };
        packet.normalize();
        packet.finalize_size()?;
        Ok(packet)
    }
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

    fn readiness_error(&self, report: &ReadinessReport) -> PulseResult<WorkPacketV1> {
        let code = match report.status {
            ReadinessStatus::Stale => "work_packet_readiness_stale",
            _ => {
                let qa_blocked = report.gate_families.iter().any(|g| {
                    g.family == "qa_baseline_and_cases"
                        && matches!(
                            g.status,
                            crate::graph::readiness::GateStatus::Failed
                                | crate::graph::readiness::GateStatus::Unavailable
                        )
                });
                if qa_blocked {
                    "work_packet_qa_resolver_unavailable"
                } else {
                    "work_packet_readiness_failed"
                }
            }
        };
        Err(PulseError::validation(code, "readiness check did not pass"))
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

fn extract_snapshot(
    readiness: &ReadinessSnapshot,
    projection: &GraphProjection,
    report: &ReadinessReport,
) -> work_packet::SnapshotReport {
    work_packet::SnapshotReport {
        graph_fingerprint: projection.graph_fingerprint.clone(),
        readiness_profile: report.profile.clone(),
        readiness_fingerprint: report.readiness_fingerprint.clone(),
        readiness_status: report.status_as_word().to_string(),
        authority_policy_revision: readiness.authority.policy_revision.unwrap_or(0),
        authority_policy_fingerprint: readiness.authority.fingerprint.clone().unwrap_or_default(),
        docs_registry_revision: readiness.docs.registry.revision,
        docs_registry_fingerprint: readiness.docs.registry.fingerprint.clone(),
        docs_index_fingerprint: String::new(),
        source_commit: String::new(),
    }
}

fn complete_snapshot(
    mut snapshot: work_packet::SnapshotReport,
    documentation: &PacketDocumentation,
    source: &PacketSource,
) -> work_packet::SnapshotReport {
    snapshot.docs_index_fingerprint = documentation.index.fingerprint.clone();
    snapshot.source_commit = source.commit.clone();
    snapshot
}

fn extract_contract_dto(node: &Node) -> PulseResult<work_packet::PacketImplementationContractV1> {
    let contract = node.implementation.as_ref().ok_or_else(|| {
        PulseError::validation(
            "work_packet_readiness_failed",
            format!("subject {} has no implementation contract", node.id),
        )
    })?;

    Ok(work_packet::PacketImplementationContractV1 {
        mode: pkt_mode_str(contract.mode),
        work_surface: pkt_surface_str(contract.work_surface),
        plan_policy: pkt_plan_policy_str(contract.plan_policy),
        semantic_impact: pkt_semantic_impact_str(contract.semantic_impact),
        effort: work_packet::PacketEffortMetadata {
            multi_session: contract.effort.multi_session,
            multiple_dependent_decisions: contract.effort.multiple_dependent_decisions,
            resume_or_audit_continuity: contract.effort.resume_or_audit_continuity,
        },
        verification_profile: contract.verification_profile.clone(),
        brief: contract.brief.as_ref().map(|b| PacketContentRef {
            path: b.path.clone(),
            content_hash: b.content_hash.clone(),
        }),
        objective: contract.objective.clone(),
        current_behavior: contract.current_behavior.clone(),
        target_behavior: contract.target_behavior.clone(),
        code_anchors: contract.code_anchors.iter().map(pkt_surface_ref).collect(),
        documentation_anchors: contract
            .documentation_anchors
            .iter()
            .map(pkt_surface_ref)
            .collect(),
        configuration_anchors: contract
            .configuration_anchors
            .iter()
            .map(pkt_surface_ref)
            .collect(),
        data_anchors: contract.data_anchors.iter().map(pkt_surface_ref).collect(),
        research_refs: contract.research_refs.iter().map(pkt_surface_ref).collect(),
        required_changes: contract
            .required_changes
            .iter()
            .map(pkt_contract_item)
            .collect(),
        invariants: contract.invariants.iter().map(pkt_contract_item).collect(),
        acceptance: contract.acceptance.iter().map(pkt_contract_item).collect(),
        scope: PacketContractScope {
            included: contract.scope.included.clone(),
            excluded: contract.scope.excluded.clone(),
        },
        implementation_freedom: contract
            .implementation_freedom
            .iter()
            .map(pkt_contract_item)
            .collect(),
        required_decisions: contract
            .required_decisions
            .iter()
            .map(|d| work_packet::PacketRequiredDecisionRef {
                id: d.id.clone(),
                contract_revision: d.contract_revision,
                acceptance_receipt: Some(work_packet::PacketReceiptRef {
                    id: d.acceptance_receipt.id.clone(),
                    hash: d.acceptance_receipt.hash.clone(),
                }),
            })
            .collect(),
        shared_approach_refs: contract
            .shared_approach_refs
            .iter()
            .map(|s| work_packet::PacketSharedApproachRef {
                owner: work_packet::PacketSharedApproachOwner {
                    id: s.owner.id.clone(),
                    contract_revision: s.owner.contract_revision,
                },
                path: s.path.clone(),
                content_hash: s.content_hash.clone(),
            })
            .collect(),
        expected_evidence: contract
            .expected_evidence
            .iter()
            .map(|e| expected_evidence_str(*e).to_string())
            .collect(),
        expected_handoff: contract
            .expected_handoff
            .iter()
            .map(|h| expected_handoff_str(*h).to_string())
            .collect(),
    })
}

fn extract_context(
    node: &Node,
    projection: &GraphProjection,
    readiness: &ReadinessSnapshot,
) -> work_packet::PacketContext {
    let parents = extract_parents(node, projection);
    let decisions = extract_decisions(node, projection, readiness);
    work_packet::PacketContext { parents, decisions }
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

fn extract_decisions(
    node: &Node,
    projection: &GraphProjection,
    readiness: &ReadinessSnapshot,
) -> Vec<work_packet::PacketDecisionRef> {
    let Some(contract) = &node.implementation else {
        return Vec::new();
    };
    let proofs: BTreeMap<&str, _> = readiness
        .decision_proofs
        .iter()
        .map(|proof| (proof.decision_id.as_str(), proof))
        .collect();
    let mut decisions = Vec::new();
    for required in &contract.required_decisions {
        let decision_node = node_by_id(projection, &required.id);
        let (status, revision, contract_revision, title) = match decision_node {
            Some(dn) => (
                node_status_str(dn.status),
                dn.revision,
                dn.contract_revision,
                dn.title.clone(),
            ),
            None => continue,
        };
        let proof = proofs.get(required.id.as_str());
        decisions.push(work_packet::PacketDecisionRef {
            id: required.id.clone(),
            revision,
            contract_revision,
            status,
            title,
            acceptance_receipt: proof.map(|proof| work_packet::PacketReceiptRef {
                id: proof.receipt_id.clone(),
                hash: proof.receipt_hash.clone(),
            }),
            content_refs: proof
                .map(|proof| {
                    vec![PacketContentRef {
                        path: proof.payload.decision.content.path.clone(),
                        content_hash: proof.payload.decision.content.content_hash.clone(),
                    }]
                })
                .unwrap_or_default(),
        });
    }
    decisions.sort_by(|a, b| a.id.cmp(&b.id));
    decisions
}

fn extract_shaping(
    node: &Node,
    shaping_snapshot: &Option<ShapingReceiptSnapshot>,
    _projection: &GraphProjection,
) -> PulseResult<PacketShaping> {
    let Some(shaping) = shaping_snapshot else {
        return Ok(PacketShaping {
            status: "absent".to_string(),
            receipt_id: String::new(),
            receipt_hash: String::new(),
            owning_work: PacketShapingWorkBinding {
                id: String::new(),
                revision_observed: 0,
                contract_revision: 0,
            },
            shape_mode: "unknown".to_string(),
            destination: None,
            map: None,
            critical_branches: vec![],
            bounded_fog: vec![],
            remaining_uncertainty: vec![],
            decision_frontier: PacketDecisionFrontier {
                status: "not_applicable".to_string(),
                items: vec![],
            },
        });
    };

    let payload = &shaping.payload;
    let shape_mode_str = match payload.shape_mode {
        ShapeMode::ConciseSelfCheck => "concise_self_check",
        ShapeMode::FocusedBranches => "focused_branches",
        ShapeMode::PersistedMap => "persisted_map",
    };

    let subject_id = &node.id;
    let owning_id = &payload.owning_work.id;
    let is_owning = subject_id == owning_id;

    // Only include critical branches where affected_work contains subject
    let critical_branches: Vec<PacketCriticalBranch> = payload
        .branches
        .iter()
        .filter(|b| {
            b.criticality == BranchCriticality::Critical
                && (is_owning || b.affected_work.contains(subject_id))
        })
        .map(|b| PacketCriticalBranch {
            id: b.id.clone(),
            question: b.question.clone(),
            gap_kind: b.gap_kind.clone(),
            affected_work: b.affected_work.clone(),
            disposition: branch_disposition_to_packet(&b.disposition),
        })
        .collect();

    // Only include fog where affected_work contains subject or subject is owning
    let bounded_fog: Vec<PacketBoundedFog> = payload
        .fog
        .iter()
        .filter(|f| is_owning || f.affected_work.contains(subject_id))
        .map(|f| PacketBoundedFog {
            id: f.id.clone(),
            statement: f.statement.clone(),
            bounds: f.bounds.clone(),
            why_not_precise: f.why_not_precise.clone(),
            trigger: f.trigger.clone(),
            affected_work: f.affected_work.clone(),
        })
        .collect();

    let remaining_uncertainty: Vec<PacketRemainingUncertainty> = payload
        .remaining_uncertainty
        .iter()
        .map(|r| PacketRemainingUncertainty {
            summary: r.summary.clone(),
            trigger: r.trigger.clone(),
        })
        .collect();

    let included_branch_by_resolution: BTreeMap<&str, &PacketCriticalBranch> = critical_branches
        .iter()
        .filter_map(|branch| {
            branch
                .disposition
                .resolution
                .as_ref()
                .map(|resolution| (resolution.id.as_str(), branch))
        })
        .collect();
    let frontier_items: Vec<PacketDecisionFrontierItem> = payload
        .resolution_pointers
        .iter()
        .filter_map(|rp| {
            let branch = included_branch_by_resolution.get(rp.id.as_str())?;
            Some(PacketDecisionFrontierItem {
                id: rp.id.clone(),
                revision: rp.revision,
                gap_kind: branch.gap_kind.clone(),
                question: branch.question.clone(),
                status: "evaluated".to_string(),
            })
        })
        .collect();

    if frontier_items.len() > work_packet::MAX_DECISION_FRONTIER_ITEMS {
        return Err(PulseError::validation(
            "work_packet_decision_frontier_overflow",
            "more than 16 relevant decision-frontier items",
        ));
    }

    let destination = payload
        .destination
        .as_ref()
        .map(|d| PacketShapingDestination {
            summary: d.summary.clone(),
            scope_boundary: d.scope_boundary.clone(),
            exit_conditions: d.exit_conditions.clone(),
        });

    let map = payload.map.as_ref().map(|m| PacketShapingMapSnapshot {
        path: m.path.clone(),
        revision: m.revision,
        content_hash: m.content_hash.clone(),
    });

    Ok(PacketShaping {
        status: if shaping.integrity_valid {
            "current".to_string()
        } else {
            "invalid".to_string()
        },
        receipt_id: shaping.receipt_id.clone(),
        receipt_hash: shaping.receipt_hash.clone(),
        owning_work: PacketShapingWorkBinding {
            id: payload.owning_work.id.clone(),
            revision_observed: payload.owning_work.revision_observed,
            contract_revision: payload.owning_work.contract_revision,
        },
        shape_mode: shape_mode_str.to_string(),
        destination,
        map,
        critical_branches,
        bounded_fog,
        remaining_uncertainty,
        decision_frontier: PacketDecisionFrontier {
            status: "evaluated".to_string(),
            items: frontier_items,
        },
    })
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
        authority: document_authority_str(document.authority).to_string(),
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

fn packet_source_snapshot_from_packet(
    source: &PacketSource,
) -> crate::source::PacketSourceSnapshot {
    crate::source::PacketSourceSnapshot {
        repository_id: source.repository_id.clone(),
        kind: source.kind.clone(),
        commit: source.commit.clone(),
        head_ref: source.head_ref.clone(),
        worktree_root_kind: match source.worktree_root_kind.as_str() {
            "linked_worktree" => crate::source::WorktreeRootKind::LinkedWorktree,
            _ => crate::source::WorktreeRootKind::PrimaryOrExistingWorktree,
        },
        cleanliness: match source.cleanliness.as_str() {
            "dirty" => crate::source::SourceCleanliness::Dirty,
            _ => crate::source::SourceCleanliness::Clean,
        },
        operation_state: match source.operation_state.as_str() {
            "merge_in_progress" => crate::source::RepositoryOperationState::MergeInProgress,
            "rebase_in_progress" => crate::source::RepositoryOperationState::RebaseInProgress,
            "cherry_pick_in_progress" => {
                crate::source::RepositoryOperationState::CherryPickInProgress
            }
            "revert_in_progress" => crate::source::RepositoryOperationState::RevertInProgress,
            "bisect_in_progress" => crate::source::RepositoryOperationState::BisectInProgress,
            _ => crate::source::RepositoryOperationState::Normal,
        },
        currentness: source.currentness.clone(),
    }
}

fn work_documentation_context(node: &Node) -> WorkDocumentationContext {
    node.documentation
        .as_ref()
        .map(|documentation| {
            WorkDocumentationContext::from((node.id.as_str(), node.revision, documentation))
        })
        .unwrap_or_else(|| WorkDocumentationContext::unknown(node.id.clone(), node.revision))
}

fn map_docs_index_error(error: PulseError, context: &str) -> PulseError {
    match error.code() {
        "work_packet_docs_score_invalid" | "work_packet_snapshot_changed" => error,
        code if code.starts_with("docs_") => PulseError::validation(
            "work_packet_docs_index_unavailable",
            format!("{context}; cause_code={code}: {error}"),
        ),
        _ => error,
    }
}

fn work_packet_test_barrier_after_first_fence() -> PulseResult<()> {
    #[cfg(debug_assertions)]
    {
        use std::io::Write;
        use std::time::{Duration, Instant};

        let Some(signal_path) = std::env::var_os("PULSE_WORK_PACKET_AFTER_FIRST_FENCE_SIGNAL")
        else {
            return Ok(());
        };
        let signal_path = std::path::PathBuf::from(signal_path);
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
            let wait_path = std::path::PathBuf::from(wait_path);
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
    }
    Ok(())
}

fn extract_source_snapshot(
    repo_root: &std::path::Path,
    repository_id: &str,
) -> PulseResult<PacketSource> {
    let snapshot = packet_base_snapshot(repo_root, repository_id)?;
    Ok(snapshot.into())
}

fn extract_workspace(node: &Node, source: &PacketSource, repository_id: &str) -> PacketWorkspace {
    let risk = node.risk.unwrap_or(Risk::Unassessed);
    let required_strategy = match risk {
        Risk::Low => "in_place_allowed",
        Risk::Medium | Risk::High | Risk::Critical => "isolated_worktree_required",
        Risk::Unassessed => "unassessed",
    };

    PacketWorkspace {
        binding_status: "not_allocated".to_string(),
        workspace_id: None,
        required_strategy: required_strategy.to_string(),
        base_repository_id: repository_id.to_string(),
        base_commit: source.commit.clone(),
        requirements: vec![
            "same_repository_identity".to_string(),
            "exact_base_commit".to_string(),
            "clean_at_reservation".to_string(),
            "scope_policy_revalidation".to_string(),
        ],
    }
}

fn extract_capabilities(node: &Node) -> PacketCapabilities {
    let mut required: Vec<String> =
        vec!["repository.inspect".to_string(), "source.read".to_string()];
    if let Some(contract) = &node.implementation {
        match contract.work_surface {
            WorkSurface::Code | WorkSurface::Configuration | WorkSurface::Data => {
                required.push("source.write".to_string());
            }
            WorkSurface::Documentation => {
                required.push("docs.write".to_string());
            }
            WorkSurface::Research => {}
        }
        for evidence in &contract.expected_evidence {
            match evidence {
                ExpectedEvidence::FocusedTestOutput => {
                    required.push("test.run".to_string());
                }
                ExpectedEvidence::DocumentationDiff => {
                    required.push("docs.write".to_string());
                }
                ExpectedEvidence::DecisionRecord => {
                    required.push("decision.propose".to_string());
                }
                _ => {}
            }
        }
        if contract.plan_policy == PlanPolicy::RequiredBeforeExecution {
            required.push("plan.materialize".to_string());
        }
    }
    // Isolated workspace requirement
    if node.risk != Some(Risk::Low) && node.risk.is_some() {
        required.push("workspace.worktree".to_string());
    }
    required.sort();
    required.dedup();

    PacketCapabilities {
        evaluation_status: "not_evaluated".to_string(),
        required,
        optional: vec![],
        missing: vec![],
        inventory_identity: None,
    }
}

fn extract_scope(node: &Node) -> PacketScope {
    let mut hints = PacketScopeHints::default();
    if let Some(contract) = &node.implementation {
        for anchor in &contract.code_anchors {
            if let Some(path) = validated_hint_path(&anchor.path) {
                hints.source_paths.push(path);
            }
        }
        for anchor in &contract.documentation_anchors {
            if let Some(path) = validated_hint_path(&anchor.path) {
                hints.documentation_paths.push(path);
            }
        }
        for anchor in &contract.configuration_anchors {
            if let Some(path) = validated_hint_path(&anchor.path) {
                hints.configuration_paths.push(path);
            }
        }
        for anchor in &contract.data_anchors {
            if let Some(path) = validated_hint_path(&anchor.path) {
                hints.data_paths.push(path);
            }
        }
        hints.included = contract
            .scope
            .included
            .iter()
            .filter_map(|path| validated_hint_path(path))
            .collect();
        hints.excluded = contract
            .scope
            .excluded
            .iter()
            .filter_map(|path| validated_hint_path(path))
            .collect();
    }
    hints.source_paths.sort();
    hints.source_paths.dedup();
    hints.documentation_paths.sort();
    hints.documentation_paths.dedup();
    hints.configuration_paths.sort();
    hints.configuration_paths.dedup();
    hints.data_paths.sort();
    hints.data_paths.dedup();
    hints.included.sort();
    hints.included.dedup();
    hints.excluded.sort();
    hints.excluded.dedup();

    let implementation_freedom: Vec<PacketContractItem> = node
        .implementation
        .as_ref()
        .map(|c| {
            c.implementation_freedom
                .iter()
                .map(|item| PacketContractItem {
                    id: item.id.clone(),
                    summary: item.summary.clone(),
                })
                .collect()
        })
        .unwrap_or_default();

    PacketScope {
        scope_hints: hints,
        implementation_freedom,
        hard_stops: vec![
            "do_not_change_acceptance_without_authority".to_string(),
            "do_not_override_accepted_decision".to_string(),
            "stop_on_objective_or_invariant_ambiguity".to_string(),
            "stop_on_source_or_contract_drift".to_string(),
        ],
        enforcement: work_packet::PacketScopeEnforcement {
            status: "not_installed".to_string(),
            owner_phase: 2,
        },
    }
}

fn extract_assurance(node: &Node) -> PacketAssurance {
    let verification_profile = node
        .implementation
        .as_ref()
        .map(|c| c.verification_profile.clone())
        .unwrap_or_default();

    let expected_evidence: Vec<String> = node
        .implementation
        .as_ref()
        .map(|c| {
            c.expected_evidence
                .iter()
                .map(|e| expected_evidence_str(*e).to_string())
                .collect()
        })
        .unwrap_or_default();

    let expected_handoff: Vec<String> = node
        .implementation
        .as_ref()
        .map(|c| {
            c.expected_handoff
                .iter()
                .map(|h| expected_handoff_str(*h).to_string())
                .collect()
        })
        .unwrap_or_default();

    let qa = node
        .qa
        .as_ref()
        .map(|q| &q.impact)
        .cloned()
        .unwrap_or_default();
    let qa_posture = match qa.posture {
        QaImpactPosture::Unknown => "unknown",
        QaImpactPosture::Required => "required",
        QaImpactPosture::CoveredByStoryClose => "covered_by_story_close",
        QaImpactPosture::None => "none",
    }
    .to_string();
    let qa_status = match qa.posture {
        QaImpactPosture::None | QaImpactPosture::CoveredByStoryClose => "ready_gate_satisfied",
        QaImpactPosture::Required => "resolver_unavailable",
        QaImpactPosture::Unknown => "not_assessed",
    }
    .to_string();

    let docs_impact = node.documentation.as_ref().map(|d| &d.impact);
    let doc_posture = match docs_impact.map(|d| d.posture) {
        Some(DocumentationImpactPosture::Required) => "required",
        Some(DocumentationImpactPosture::None) => "none",
        Some(DocumentationImpactPosture::Deferred) => "deferred",
        None | Some(DocumentationImpactPosture::Unknown) => "unknown",
    }
    .to_string();
    let doc_status = match docs_impact {
        Some(d) if d.posture == DocumentationImpactPosture::Required => "gate_required",
        Some(d) if d.posture == DocumentationImpactPosture::None => "not_required",
        Some(d) if d.posture == DocumentationImpactPosture::Deferred => "deferred",
        _ => "not_assessed",
    }
    .to_string();
    let doc_required_ids = docs_impact
        .map(|d| d.required_documents.clone())
        .unwrap_or_default();

    PacketAssurance {
        verification_profile,
        expected_evidence,
        expected_handoff,
        documentation_impact: PacketDocumentationImpact {
            posture: doc_posture,
            status: doc_status,
            required_doc_ids: doc_required_ids,
        },
        qa: PacketQaStatus {
            posture: qa_posture,
            status: qa_status,
            affected_case_ids: qa.affected_case_ids,
        },
        promotion_policy: PacketFutureGate {
            status: "not_installed".to_string(),
            owner_phase: 2,
        },
        close_gate: PacketFutureGate {
            status: "not_installed".to_string(),
            owner_phase: 2,
        },
    }
}

fn build_dispatch(
    report: &ReadinessReport,
    snapshot: &work_packet::SnapshotReport,
    source: &PacketSource,
) -> PulseResult<PacketDispatch> {
    if report.code != "ready"
        || report.status != ReadinessStatus::Ready
        || !report.transition_eligible
    {
        return Err(PulseError::validation(
            "work_packet_readiness_failed",
            "work packet preview requires current transition-eligible readiness; failed pre-reservation gates do not produce partial packets",
        ));
    }

    let mut dispatch = PacketDispatch {
        revalidation_preconditions: vec![
            PacketRevalidationPrecondition {
                field: "snapshot.graph_fingerprint".to_string(),
                value: snapshot.graph_fingerprint.clone(),
            },
            PacketRevalidationPrecondition {
                field: "snapshot.readiness_profile".to_string(),
                value: snapshot.readiness_profile.clone(),
            },
            PacketRevalidationPrecondition {
                field: "snapshot.readiness_fingerprint".to_string(),
                value: snapshot.readiness_fingerprint.clone(),
            },
            PacketRevalidationPrecondition {
                field: "snapshot.readiness_status".to_string(),
                value: snapshot.readiness_status.clone(),
            },
            PacketRevalidationPrecondition {
                field: "snapshot.authority_policy_revision".to_string(),
                value: snapshot.authority_policy_revision.to_string(),
            },
            PacketRevalidationPrecondition {
                field: "snapshot.authority_policy_fingerprint".to_string(),
                value: snapshot.authority_policy_fingerprint.clone(),
            },
            PacketRevalidationPrecondition {
                field: "snapshot.docs_registry_revision".to_string(),
                value: snapshot.docs_registry_revision.to_string(),
            },
            PacketRevalidationPrecondition {
                field: "snapshot.docs_registry_fingerprint".to_string(),
                value: snapshot.docs_registry_fingerprint.clone(),
            },
            PacketRevalidationPrecondition {
                field: "snapshot.docs_index_fingerprint".to_string(),
                value: snapshot.docs_index_fingerprint.clone(),
            },
            PacketRevalidationPrecondition {
                field: "snapshot.source_commit".to_string(),
                value: snapshot.source_commit.clone(),
            },
            PacketRevalidationPrecondition {
                field: "source.cleanliness".to_string(),
                value: source.cleanliness.clone(),
            },
        ],
        ..PacketDispatch::default()
    };
    for gate in &mut dispatch.gate_families {
        match gate.family.as_str() {
            "readiness" => {
                gate.status = "passed".to_string();
            }
            "packet_completeness" => {
                gate.status = "passed".to_string();
            }
            "source_base" => {
                gate.status = "passed".to_string();
            }
            "documentation_context" => {
                gate.status = "passed".to_string();
                gate.reason_codes = vec![];
            }
            _ => {}
        }
    }
    Ok(dispatch)
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

fn materialization_str(mat: Option<crate::graph::contract::Materialization>) -> String {
    match mat {
        None | Some(crate::graph::contract::Materialization::Unassessed) => "unassessed",
        Some(crate::graph::contract::Materialization::R0) => "R0",
        Some(crate::graph::contract::Materialization::R1) => "R1",
        Some(crate::graph::contract::Materialization::R2) => "R2",
        Some(crate::graph::contract::Materialization::R3) => "R3",
    }
    .to_string()
}

fn pkt_mode_str(mode: ImplementationMode) -> String {
    match mode {
        ImplementationMode::Locked => "locked",
        ImplementationMode::Guided => "guided",
        ImplementationMode::Open => "open",
    }
    .to_string()
}

fn pkt_surface_str(surface: WorkSurface) -> String {
    match surface {
        WorkSurface::Code => "code",
        WorkSurface::Documentation => "documentation",
        WorkSurface::Configuration => "configuration",
        WorkSurface::Data => "data",
        WorkSurface::Research => "research",
    }
    .to_string()
}

fn pkt_plan_policy_str(policy: PlanPolicy) -> String {
    match policy {
        PlanPolicy::None => "none",
        PlanPolicy::WorkerOptional => "worker_optional",
        PlanPolicy::RequiredBeforeExecution => "required_before_execution",
    }
    .to_string()
}

fn pkt_semantic_impact_str(impact: crate::graph::contract::ImplementationSemanticImpact) -> String {
    use crate::graph::contract::ImplementationSemanticImpact;
    match impact {
        ImplementationSemanticImpact::NoBehaviorOrPublicRiskChange => {
            "no_behavior_or_public_risk_change"
        }
        ImplementationSemanticImpact::BehaviorOrPublicRiskChange => {
            "behavior_or_public_risk_change"
        }
    }
    .to_string()
}

fn pkt_surface_ref(ref_: &crate::graph::contract::SurfaceRef) -> PacketSurfaceRef {
    PacketSurfaceRef {
        path: ref_.path.clone(),
        symbol: ref_.symbol.clone(),
        content_hash: ref_.content_hash.clone(),
    }
}

fn pkt_contract_item(item: &crate::graph::contract::ContractItem) -> PacketContractItem {
    PacketContractItem {
        id: item.id.clone(),
        summary: item.summary.clone(),
    }
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
        DocumentKind::RepositoryMap => "repository_map",
        DocumentKind::Policy => "policy",
        DocumentKind::Product => "product",
        DocumentKind::Architecture => "architecture",
        DocumentKind::Domain => "domain",
        DocumentKind::Operations => "operations",
        DocumentKind::Reference => "reference",
        DocumentKind::DecisionProjection => "decision_projection",
        DocumentKind::Generated => "generated",
        DocumentKind::Informational => "informational",
    }
}

fn document_authority_str(authority: DocumentAuthority) -> &'static str {
    match authority {
        DocumentAuthority::Draft => "draft",
        DocumentAuthority::Approved => "approved",
        DocumentAuthority::Informational => "informational",
        DocumentAuthority::Generated => "generated",
    }
}

fn expected_evidence_str(evidence: ExpectedEvidence) -> &'static str {
    match evidence {
        ExpectedEvidence::FocusedTestOutput => "focused_test_output",
        ExpectedEvidence::AcceptanceMapping => "acceptance_mapping",
        ExpectedEvidence::ClientContractInventory => "client_contract_inventory",
        ExpectedEvidence::PrototypeEvidence => "prototype_evidence",
        ExpectedEvidence::ResearchNotes => "research_notes",
        ExpectedEvidence::DecisionRecord => "decision_record",
        ExpectedEvidence::DocumentationDiff => "documentation_diff",
        ExpectedEvidence::ConfigurationDiff => "configuration_diff",
        ExpectedEvidence::DataSample => "data_sample",
    }
}

fn expected_handoff_str(handoff: crate::graph::contract::ExpectedHandoff) -> &'static str {
    use crate::graph::contract::ExpectedHandoff;
    match handoff {
        ExpectedHandoff::SourceSnapshot => "source_snapshot",
        ExpectedHandoff::AcceptanceToEvidence => "acceptance_to_evidence",
        ExpectedHandoff::RemainingRisks => "remaining_risks",
        ExpectedHandoff::DocumentationFindings => "documentation_findings",
        ExpectedHandoff::DecisionSummary => "decision_summary",
        ExpectedHandoff::FollowUpWork => "follow_up_work",
    }
}

fn validated_hint_path(path: &str) -> Option<String> {
    safe_repo_relative(path)
        .ok()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
}

// ===================================================================
// P2S1-I4: Deterministic suggestion query, score conversion, search
// ===================================================================

/// Build the deterministic suggestion query string per P2S1-D14.
///
/// Order of fragments:
/// 1. Ticket title
/// 2. objective
/// 3. target_behavior
/// 4. Each acceptance summary (by acceptance ID)
/// 5. Basename + optional symbol of code/config/data/docs anchors
/// 6. Documentation routing domains, then labels (lexical sort)
/// 7. Required Decision titles (sort by Decision ID)
/// 8. Shaping destination summary, then exit_conditions by order
/// 9. Critical branch question (sort by branch ID)
///
/// Normalized and truncated to at most 32 terms / 256 UTF-8 bytes.
pub fn build_suggestion_query(
    node: &Node,
    shaping: &PacketShaping,
    decisions: &[work_packet::PacketDecisionRef],
    docs_work: &WorkDocumentationContext,
) -> PulseResult<work_packet::PacketSuggestionQuery> {
    let mut fragments: Vec<String> = Vec::new();

    // 1. Ticket title
    if !node.title.is_empty() {
        fragments.push(node.title.clone());
    }

    if let Some(contract) = &node.implementation {
        // 2. objective
        if !contract.objective.is_empty() {
            fragments.push(contract.objective.clone());
        }
        // 3. target_behavior
        if !contract.target_behavior.is_empty() {
            fragments.push(contract.target_behavior.clone());
        }
        // 4. Each acceptance summary (by acceptance ID)
        if !contract.acceptance.is_empty() {
            for item in &contract.acceptance {
                if !item.summary.is_empty() {
                    fragments.push(item.summary.clone());
                }
            }
        }
        // 5. Anchor basename + optional symbol
        let mut anchor_fragments: Vec<(String, Option<String>)> = Vec::new();
        for anchor in &contract.code_anchors {
            anchor_fragments.push((anchor.path.clone(), anchor.symbol.clone()));
        }
        for anchor in &contract.configuration_anchors {
            anchor_fragments.push((anchor.path.clone(), anchor.symbol.clone()));
        }
        for anchor in &contract.data_anchors {
            anchor_fragments.push((anchor.path.clone(), anchor.symbol.clone()));
        }
        for anchor in &contract.documentation_anchors {
            anchor_fragments.push((anchor.path.clone(), anchor.symbol.clone()));
        }
        anchor_fragments.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        for (path, symbol) in anchor_fragments {
            if let Some(basename) = std::path::Path::new(&path).file_name() {
                let mut frag = basename.to_string_lossy().to_string();
                if let Some(sym) = &symbol {
                    frag.push(' ');
                    frag.push_str(sym);
                }
                if !frag.is_empty() {
                    fragments.push(frag);
                }
            }
        }
    }

    // 6. Documentation routing domains, then labels (lexical sort).
    for domain in &docs_work.domains {
        fragments.push(domain.clone());
    }
    for label in &docs_work.labels {
        fragments.push(label.clone());
    }

    // 7. Required Decision titles (sort by Decision ID).
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

    // 8. Shaping destination summary + exit_conditions
    if let Some(dest) = &shaping.destination {
        if !dest.summary.is_empty() {
            fragments.push(dest.summary.clone());
        }
        for ec in &dest.exit_conditions {
            if !ec.is_empty() {
                fragments.push(ec.clone());
            }
        }
    }

    // 9. Critical branch question (sort by branch ID)
    if !shaping.critical_branches.is_empty() {
        let mut branches = shaping.critical_branches.clone();
        branches.sort_by(|a, b| a.id.cmp(&b.id));
        for branch in &branches {
            if !branch.question.is_empty() {
                fragments.push(branch.question.clone());
            }
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
) -> PulseResult<Vec<work_packet::PacketSuggestedSection>> {
    if query.normalized_terms.is_empty() {
        return Ok(Vec::new());
    }

    let report = crate::docs::search_docs(
        repo_root,
        &query.text,
        crate::docs::SearchOptions {
            kind: None,
            domain: None,
            authority: None,
            limit: Some(work_packet::MAX_SUGGESTED_SECTIONS),
            no_refresh: false,
            explain: true,
            include_draft: false,
            include_stale: false,
            work: Some(docs_work.clone()),
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
            authority: hit.authority,
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

fn branch_disposition_to_packet(disposition: &BranchDisposition) -> PacketBranchDisposition {
    match disposition {
        BranchDisposition::Resolved { resolution } => PacketBranchDisposition {
            kind: "resolved".to_string(),
            resolution: Some(PacketResolution {
                kind: resolution.kind.clone(),
                id: resolution.id.clone(),
                revision: resolution.revision,
                gist: resolution.gist.clone(),
            }),
            non_blocking_context: None,
        },
        BranchDisposition::Rejected { reason, reference } => PacketBranchDisposition {
            kind: "rejected".to_string(),
            resolution: None,
            non_blocking_context: reference.clone().or_else(|| Some(reason.clone())),
        },
        BranchDisposition::Delegated { freedom_id, reason } => PacketBranchDisposition {
            kind: "delegated".to_string(),
            resolution: None,
            non_blocking_context: Some(format!("freedom_id={freedom_id}: {reason}")),
        },
        BranchDisposition::Deferred {
            reason,
            owner,
            target_work,
            trigger,
            ..
        } => PacketBranchDisposition {
            kind: "deferred".to_string(),
            resolution: None,
            non_blocking_context: Some(format!(
                "owner={owner}, target={target_work}, trigger={trigger}: {reason}"
            )),
        },
        BranchDisposition::Blocking {
            linked_decision_work,
        } => PacketBranchDisposition {
            kind: "blocking".to_string(),
            resolution: None,
            non_blocking_context: linked_decision_work.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docs::applicability::{
        ApplicabilityGate, ApplicableRegistry, ApplicableWork, WriteCandidate,
    };
    use crate::docs::model::DocumentationPosture;
    use crate::evidence::model::{
        FogReview, RemainingUncertainty, ShapingApproval, ShapingBranch, ShapingDestination,
        ShapingFog, ShapingResolutionPointer, ShapingValidationPayload, ShapingWorkBinding,
        SourcePosture,
    };
    use crate::graph::edge::Edge;
    use crate::graph::executability::{LifecycleSummary, StructuralExecutabilityReport};
    use crate::graph::projection::{self, GraphProjection};
    use crate::graph::readiness::{GateFamilyReport, GateStatus};
    use crate::graph::{contract, node};
    use crate::identity::ActorKind;
    use crate::policy::AuthorityPolicyReport;
    use chrono::Utc;
    use std::path::PathBuf;

    fn base_node(id: &str, kind: WorkKind, title: &str) -> Node {
        Node {
            schema_version: contract::NODE_SCHEMA_VERSION,
            id: id.to_string(),
            kind,
            revision: 1,
            contract_revision: 1,
            title: title.to_string(),
            status: NodeStatus::Ready,
            status_reason: None,
            documentation: Some(node::DocumentationMetadata {
                impact: node::DocumentationImpact {
                    posture: DocumentationImpactPosture::None,
                    rationale: Some("No docs change.".to_string()),
                    required_documents: vec![],
                    deferred_to: vec![],
                },
                routing: node::DocumentationRouting {
                    paths: vec![],
                    domains: vec!["authentication".to_string()],
                    labels: vec!["tokens".to_string()],
                },
            }),
            role: Some(TicketRole::Implementation),
            risk: Some(Risk::Low),
            materialization: Some(contract::Materialization::R1),
            qa: Some(contract::QaMetadata {
                impact: contract::QaImpact {
                    posture: QaImpactPosture::None,
                    rationale: Some("No behavior change.".to_string()),
                    behavioral_owner: None,
                    affected_case_ids: vec![],
                },
            }),
            implementation: Some(contract::ImplementationContract {
                mode: ImplementationMode::Guided,
                work_surface: WorkSurface::Code,
                plan_policy: PlanPolicy::None,
                semantic_impact:
                    contract::ImplementationSemanticImpact::NoBehaviorOrPublicRiskChange,
                effort: contract::EffortMetadata::default(),
                verification_profile: "service-change".to_string(),
                brief: None,
                objective: "Objective".to_string(),
                current_behavior: "Current".to_string(),
                target_behavior: "Target".to_string(),
                code_anchors: vec![contract::SurfaceRef::path("src/token.rs")],
                documentation_anchors: vec![],
                configuration_anchors: vec![],
                data_anchors: vec![],
                research_refs: vec![],
                required_changes: vec![],
                invariants: vec![],
                acceptance: vec![],
                scope: contract::ContractScope::default(),
                implementation_freedom: vec![],
                required_decisions: vec![],
                shared_approach_refs: vec![],
                expected_evidence: vec![],
                expected_handoff: vec![],
            }),
            decision_work: None,
            shaping: None,
            content_dir: format!("works/{id}"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn edge(edge_type: EdgeType, from: &str, to: &str) -> Edge {
        Edge::new(
            edge_type,
            from.to_string(),
            to.to_string(),
            "human:tester".to_string(),
            Utc::now(),
        )
        .unwrap()
    }

    fn projection(nodes: Vec<Node>, edges: Vec<Edge>) -> GraphProjection {
        let node_files = nodes
            .into_iter()
            .map(|node| (PathBuf::from(format!("nodes/{}.json", node.id)), node))
            .collect::<Vec<_>>();
        let edge_files = edges
            .into_iter()
            .map(|edge| (PathBuf::from(format!("edges/{}.json", edge.id)), edge))
            .collect::<Vec<_>>();
        projection::build_projection("sha256:graph".to_string(), &node_files, &edge_files)
    }

    fn readiness_snapshot(projection: &GraphProjection, subject: &str) -> ReadinessSnapshot {
        ReadinessSnapshot {
            graph_fingerprint: projection.graph_fingerprint.clone(),
            structural: StructuralExecutabilityReport {
                schema_version: 1,
                subject: subject.to_string(),
                graph_fingerprint: projection.graph_fingerprint.clone(),
                structural_state: StructuralState::Candidate,
                dispatch_authorized: false,
                lifecycle: LifecycleSummary {
                    status: NodeStatus::Ready,
                    revision: 1,
                },
                hard_blockers: vec![],
                soft_preferences: vec![],
                supersession: None,
                gate_coverage: vec![],
                missing_gate_families: vec![],
                reason_codes: vec![],
            },
            shaping: None,
            decision_proofs: vec![],
            docs: docs_report_complete(),
            authority: AuthorityPolicyReport {
                schema_version: 1,
                code: "ok".to_string(),
                available: true,
                valid: true,
                policy_revision: Some(1),
                fingerprint: Some("sha256:policy".to_string()),
                principals: vec![],
                reason_codes: vec![],
            },
            content_bindings: vec![],
        }
    }

    fn docs_report_complete() -> ApplicableDocsReport {
        ApplicableDocsReport {
            schema_version: 1,
            work: ApplicableWork {
                id: "TK-001".to_string(),
                revision: 1,
                documentation_posture: DocumentationPosture::None,
            },
            registry: ApplicableRegistry {
                revision: 3,
                fingerprint: "sha256:registry".to_string(),
            },
            required: vec![],
            optional: vec![],
            write_candidates: vec![],
            excluded: vec![],
            gate: ApplicabilityGate {
                status: "complete".to_string(),
                reason_codes: vec![],
                policy_status: "not_evaluated".to_string(),
            },
        }
    }

    fn packet_doc(id: &str) -> ApplicableDocument {
        ApplicableDocument {
            id: id.to_string(),
            path: format!("docs/{id}.md"),
            kind: DocumentKind::Architecture,
            authority: DocumentAuthority::Approved,
            owner: "docs-team".to_string(),
            summary: format!("{id} summary"),
            content_hash: format!("sha256:{id}"),
            document_revision: 4,
            reasons: vec!["route".to_string()],
        }
    }

    fn shaping_payload(
        branches: Vec<ShapingBranch>,
        pointers: Vec<ShapingResolutionPointer>,
    ) -> ShapingValidationPayload {
        ShapingValidationPayload {
            payload_version: 1,
            owning_work: ShapingWorkBinding {
                id: "ST-001".to_string(),
                revision_observed: 2,
                contract_revision: 1,
            },
            materialization: "R1".to_string(),
            shape_mode: ShapeMode::FocusedBranches,
            source_posture: SourcePosture::NotRequiredContentBound,
            destination: Some(ShapingDestination {
                summary: "Deliver stable packet context".to_string(),
                scope_boundary: vec!["No dispatch".to_string()],
                exit_conditions: vec!["Packet coherent".to_string()],
            }),
            map: None,
            affected_work: vec![],
            branches,
            fog: vec![
                ShapingFog {
                    id: "FOG-B".to_string(),
                    statement: "B".to_string(),
                    bounds: vec!["bounded".to_string()],
                    why_not_precise: "later".to_string(),
                    review: FogReview::BoundedNonBlocking,
                    trigger: "when needed".to_string(),
                    affected_work: vec!["TK-001".to_string()],
                },
                ShapingFog {
                    id: "FOG-A".to_string(),
                    statement: "A".to_string(),
                    bounds: vec!["bounded".to_string()],
                    why_not_precise: "later".to_string(),
                    review: FogReview::BoundedNonBlocking,
                    trigger: "when needed".to_string(),
                    affected_work: vec!["TK-001".to_string()],
                },
            ],
            out_of_scope: vec![],
            resolution_pointers: pointers,
            approval: ShapingApproval {
                approved_by: crate::identity::ActorRef {
                    kind: ActorKind::Human,
                    id: "tester".to_string(),
                },
                reference: "approval".to_string(),
            },
            reconciliation: None,
            remaining_uncertainty: vec![
                RemainingUncertainty {
                    summary: "Z uncertainty".to_string(),
                    trigger: "later".to_string(),
                },
                RemainingUncertainty {
                    summary: "A uncertainty".to_string(),
                    trigger: "later".to_string(),
                },
            ],
        }
    }

    #[test]
    fn packet_uses_same_projection_for_readiness_snapshot() {
        let node = base_node("TK-001", WorkKind::Ticket, "Ticket");
        let parent = base_node("ST-001", WorkKind::Story, "Story");
        let projection = projection(
            vec![node.clone(), parent],
            vec![edge(EdgeType::Parent, "TK-001", "ST-001")],
        );
        let snapshot = readiness_snapshot(&projection, "TK-001");
        let report = evaluate_readiness(&snapshot.as_inputs(&node), EvalProfile::Ready).unwrap();
        let extracted = extract_snapshot(&snapshot, &projection, &report);
        assert_eq!(extracted.graph_fingerprint, projection.graph_fingerprint);
        assert_eq!(snapshot.graph_fingerprint, projection.graph_fingerprint);
        assert_eq!(
            extracted.readiness_fingerprint,
            report.readiness_fingerprint
        );
    }

    #[test]
    fn relation_projection_orders_and_includes_opposite_endpoint_metadata() {
        let subject = base_node("TK-001", WorkKind::Ticket, "Subject");
        let a = base_node("TK-002", WorkKind::Ticket, "Alpha");
        let b = base_node("TK-003", WorkKind::Ticket, "Beta");
        let c = base_node("TK-004", WorkKind::Ticket, "Gamma");
        let edges = vec![
            edge(EdgeType::Related, "TK-004", "TK-001"),
            edge(EdgeType::PreferredAfter, "TK-001", "TK-003"),
            edge(EdgeType::BlockedBy, "TK-001", "TK-002"),
        ];
        let projection = projection(vec![subject, a, b, c], edges);
        let snapshot = readiness_snapshot(&projection, "TK-001");

        let mut graph = extract_graph(&snapshot, &projection).unwrap();
        graph.normalize();
        let outgoing = graph
            .relations
            .outgoing
            .iter()
            .map(|item| {
                (
                    item.edge_type.as_str(),
                    item.to.as_str(),
                    item.opposite_title.as_str(),
                )
            })
            .collect::<Vec<_>>();
        let incoming = graph
            .relations
            .incoming
            .iter()
            .map(|item| {
                (
                    item.edge_type.as_str(),
                    item.from.as_str(),
                    item.opposite_title.as_str(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            outgoing,
            vec![
                ("blocked_by", "TK-002", "Alpha"),
                ("preferred_after", "TK-003", "Beta"),
                ("related", "TK-004", "Gamma"),
            ]
        );
        assert!(incoming.is_empty());
    }

    #[test]
    fn relation_projection_rejects_more_than_128_incident_edges() {
        let subject = base_node("TK-001", WorkKind::Ticket, "Subject");
        let mut nodes = vec![subject];
        let mut edges = Vec::new();
        for i in 0..=work_packet::MAX_INCIDENT_RELATIONS {
            let id = format!("TK-X{i:03}");
            nodes.push(base_node(&id, WorkKind::Ticket, &format!("Node {i}")));
            edges.push(edge(EdgeType::Related, "TK-001", &id));
        }
        let projection = projection(nodes, edges);
        let snapshot = readiness_snapshot(&projection, "TK-001");

        let err = extract_graph(&snapshot, &projection).unwrap_err();
        assert_eq!(err.code(), "work_packet_relation_overflow");
    }

    #[test]
    fn decision_frontier_filters_relevant_resolved_critical_branches_and_sorts() {
        let node = base_node("TK-001", WorkKind::Ticket, "Ticket");
        let branch_z = ShapingBranch {
            id: "BR-Z".to_string(),
            question: "Z question".to_string(),
            gap_kind: "tradeoff_gap".to_string(),
            criticality: BranchCriticality::Critical,
            affected_work: vec!["TK-001".to_string()],
            disposition: BranchDisposition::Resolved {
                resolution: ShapingResolutionPointer {
                    kind: "decision".to_string(),
                    id: "DEC-Z".to_string(),
                    revision: 7,
                    gist: "Z".to_string(),
                },
            },
        };
        let branch_a = ShapingBranch {
            id: "BR-A".to_string(),
            question: "A question".to_string(),
            gap_kind: "architecture_gap".to_string(),
            criticality: BranchCriticality::Critical,
            affected_work: vec!["TK-001".to_string()],
            disposition: BranchDisposition::Resolved {
                resolution: ShapingResolutionPointer {
                    kind: "decision".to_string(),
                    id: "DEC-A".to_string(),
                    revision: 3,
                    gist: "A".to_string(),
                },
            },
        };
        let irrelevant = ShapingBranch {
            id: "BR-OTHER".to_string(),
            question: "Other".to_string(),
            gap_kind: "tradeoff_gap".to_string(),
            criticality: BranchCriticality::Critical,
            affected_work: vec!["TK-OTHER".to_string()],
            disposition: BranchDisposition::Resolved {
                resolution: ShapingResolutionPointer {
                    kind: "decision".to_string(),
                    id: "DEC-OTHER".to_string(),
                    revision: 1,
                    gist: "Other".to_string(),
                },
            },
        };
        let payload = shaping_payload(
            vec![branch_z, irrelevant, branch_a],
            vec![
                ShapingResolutionPointer {
                    kind: "decision".to_string(),
                    id: "DEC-Z".to_string(),
                    revision: 7,
                    gist: "Z".to_string(),
                },
                ShapingResolutionPointer {
                    kind: "decision".to_string(),
                    id: "DEC-OTHER".to_string(),
                    revision: 1,
                    gist: "Other".to_string(),
                },
                ShapingResolutionPointer {
                    kind: "decision".to_string(),
                    id: "DEC-A".to_string(),
                    revision: 3,
                    gist: "A".to_string(),
                },
            ],
        );
        let shaping = Some(ShapingReceiptSnapshot {
            receipt_id: "rcpt_shape".to_string(),
            receipt_hash: "sha256:shape".to_string(),
            payload,
            integrity_valid: true,
            binding_codes: vec![],
            map_current: true,
        });

        let mut packet_shaping =
            extract_shaping(&node, &shaping, &projection(vec![node.clone()], vec![])).unwrap();
        packet_shaping.normalize();
        assert_eq!(
            packet_shaping
                .decision_frontier
                .items
                .iter()
                .map(|item| (
                    item.id.as_str(),
                    item.question.as_str(),
                    item.gap_kind.as_str()
                ))
                .collect::<Vec<_>>(),
            vec![
                ("DEC-A", "A question", "architecture_gap"),
                ("DEC-Z", "Z question", "tradeoff_gap"),
            ]
        );
        assert_eq!(
            packet_shaping
                .bounded_fog
                .iter()
                .map(|fog| fog.id.as_str())
                .collect::<Vec<_>>(),
            vec!["FOG-A", "FOG-B"]
        );
        assert_eq!(
            packet_shaping
                .remaining_uncertainty
                .iter()
                .map(|u| u.summary.as_str())
                .collect::<Vec<_>>(),
            vec!["A uncertainty", "Z uncertainty"]
        );
    }

    #[test]
    fn decision_frontier_rejects_more_than_16_relevant_items() {
        let node = base_node("TK-001", WorkKind::Ticket, "Ticket");
        let mut branches = Vec::new();
        let mut pointers = Vec::new();
        for i in 0..=work_packet::MAX_DECISION_FRONTIER_ITEMS {
            let decision_id = format!("DEC-{i:03}");
            branches.push(ShapingBranch {
                id: format!("BR-{i:03}"),
                question: format!("Question {i}"),
                gap_kind: "tradeoff_gap".to_string(),
                criticality: BranchCriticality::Critical,
                affected_work: vec!["TK-001".to_string()],
                disposition: BranchDisposition::Resolved {
                    resolution: ShapingResolutionPointer {
                        kind: "decision".to_string(),
                        id: decision_id.clone(),
                        revision: 1,
                        gist: "gist".to_string(),
                    },
                },
            });
            pointers.push(ShapingResolutionPointer {
                kind: "decision".to_string(),
                id: decision_id,
                revision: 1,
                gist: "gist".to_string(),
            });
        }
        let shaping = Some(ShapingReceiptSnapshot {
            receipt_id: "rcpt_shape".to_string(),
            receipt_hash: "sha256:shape".to_string(),
            payload: shaping_payload(branches, pointers),
            integrity_valid: true,
            binding_codes: vec![],
            map_current: true,
        });

        let err =
            extract_shaping(&node, &shaping, &projection(vec![node.clone()], vec![])).unwrap_err();
        assert_eq!(err.code(), "work_packet_decision_frontier_overflow");
    }

    #[test]
    fn documentation_gate_incomplete_rejects_and_write_candidates_join_metadata() {
        let mut docs = docs_report_complete();
        docs.required = vec![packet_doc("DOC-A")];
        docs.optional = vec![packet_doc("DOC-B")];
        docs.write_candidates = vec![WriteCandidate {
            id: "DOC-A".to_string(),
            reasons: vec!["impact_required".to_string()],
        }];
        let packet_docs = extract_documentation(&docs).unwrap();
        assert_eq!(
            packet_docs.applicability.write_candidates[0].path,
            "docs/DOC-A.md"
        );
        assert_eq!(
            packet_docs.applicability.write_candidates[0].authority,
            "approved"
        );
        assert_eq!(
            packet_docs.applicability.write_candidates[0].reasons,
            vec!["impact_required"]
        );
        assert_eq!(packet_docs.index.state, "not_installed");
        assert_eq!(packet_docs.suggested_sections.len(), 0);

        docs.gate.status = "incomplete".to_string();
        let err = extract_documentation(&docs).unwrap_err();
        assert_eq!(err.code(), "work_packet_docs_context_incomplete");
    }

    #[test]
    fn scope_hints_reject_traversal_without_fabricating_enforcement() {
        let mut node = base_node("TK-001", WorkKind::Ticket, "Ticket");
        let contract = node.implementation.as_mut().unwrap();
        contract.code_anchors = vec![
            contract::SurfaceRef::path("src/z.rs"),
            contract::SurfaceRef::path("../escape.rs"),
            contract::SurfaceRef::path("src/a.rs"),
        ];
        contract.scope.included = vec!["src".to_string(), "/tmp/abs".to_string()];
        let scope = extract_scope(&node);
        assert_eq!(scope.scope_hints.source_paths, vec!["src/a.rs", "src/z.rs"]);
        assert_eq!(scope.scope_hints.included, vec!["src"]);
        assert_eq!(scope.enforcement.status, "not_installed");
    }

    // ===================================================================
    // P2S1-I4: Deterministic suggestion query
    // ===================================================================

    #[test]
    fn build_suggestion_query_uses_all_fragments() {
        let mut node = base_node("TK-001", WorkKind::Ticket, "Token Rotation");
        if let Some(contract) = &mut node.implementation {
            contract.objective = "Rotate refresh tokens atomically".to_string();
            contract.target_behavior = "Concurrent rotation is serialized".to_string();
            contract.acceptance = vec![
                contract::ContractItem {
                    id: "A1".to_string(),
                    summary: "Tokens rotate without race".to_string(),
                },
                contract::ContractItem {
                    id: "A2".to_string(),
                    summary: "Old token remains valid briefly".to_string(),
                },
            ];
            contract.code_anchors = vec![
                contract::SurfaceRef::path("src/z_last.rs"),
                contract::SurfaceRef::path("src/a_first.rs"),
            ];
        }
        let shaping = PacketShaping {
            destination: Some(PacketShapingDestination {
                summary: "Deliver rotation".to_string(),
                scope_boundary: vec![],
                exit_conditions: vec!["Concurrent pass".to_string()],
            }),
            critical_branches: vec![PacketCriticalBranch {
                id: "BR-C".to_string(),
                question: "How to serialize?".to_string(),
                gap_kind: "tradeoff_gap".to_string(),
                affected_work: vec!["TK-001".to_string()],
                disposition: PacketBranchDisposition {
                    kind: "resolved".to_string(),
                    resolution: Some(PacketResolution {
                        kind: "decision".to_string(),
                        id: "DEC-001".to_string(),
                        revision: 1,
                        gist: "Use single-use".to_string(),
                    }),
                    non_blocking_context: None,
                },
            }],
            ..Default::default()
        };

        let docs_work = work_documentation_context(&node);
        let decisions = vec![work_packet::PacketDecisionRef {
            id: "DEC-001".to_string(),
            revision: 1,
            contract_revision: 1,
            status: "done".to_string(),
            title: "Atomic rotation decision".to_string(),
            acceptance_receipt: None,
            content_refs: vec![],
        }];
        let query = build_suggestion_query(&node, &shaping, &decisions, &docs_work).unwrap();
        assert!(!query.text.is_empty(), "query must not be empty");
        assert!(!query.normalized_terms.is_empty(), "must have terms");
        assert!(
            query.text.len() <= 256,
            "query text must not exceed 256 bytes"
        );
        assert!(query.normalized_terms.len() <= 32, "at most 32 terms");
        // Verify title, objective, target_behavior are present
        assert!(
            query.text.contains("token") || query.text.contains("Token"),
            "query should contain rotation-related terms"
        );
        assert!(
            query.text.contains("rotate") || query.text.contains("rotation"),
            "query should include objective"
        );
        assert!(
            query.text.contains("authentication"),
            "query should include documentation routing domains"
        );
        assert!(
            query.text.contains("tokens"),
            "query should include documentation routing labels"
        );
        assert!(
            query.text.contains("atomic"),
            "query should include required Decision titles"
        );
    }

    #[test]
    fn build_suggestion_query_empty_rejects() {
        let node = Node {
            schema_version: 1,
            id: "TK-001".to_string(),
            kind: WorkKind::Ticket,
            revision: 1,
            contract_revision: 1,
            title: String::new(),
            status: NodeStatus::Ready,
            status_reason: None,
            documentation: None,
            role: Some(TicketRole::Implementation),
            risk: Some(Risk::Low),
            materialization: Some(contract::Materialization::R1),
            qa: None,
            implementation: None,
            decision_work: None,
            shaping: None,
            content_dir: String::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let shaping = PacketShaping::default();
        let docs_work = work_documentation_context(&node);
        let result = build_suggestion_query(&node, &shaping, &[], &docs_work);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), "work_packet_docs_query_empty");
    }

    #[test]
    fn build_suggestion_query_truncates_long_fragments() {
        let mut node = base_node("TK-001", WorkKind::Ticket, &"a ".repeat(200));
        if let Some(contract) = &mut node.implementation {
            contract.objective = "b ".repeat(200);
        }
        let shaping = PacketShaping::default();
        let docs_work = work_documentation_context(&node);
        let query = build_suggestion_query(&node, &shaping, &[], &docs_work).unwrap();
        assert!(!query.text.is_empty());
        assert!(query.normalized_terms.len() <= 32);
        assert!(query.text.len() <= 256);
    }

    // ===================================================================
    // P2S1-I4: Score micro conversion
    // ===================================================================

    #[test]
    fn score_to_micros_converts_positive_scores() {
        assert_eq!(score_to_micros(0.0).unwrap(), 0);
        assert_eq!(score_to_micros(0.5).unwrap(), 500_000);
        assert_eq!(score_to_micros(1.0).unwrap(), 1_000_000);
        assert_eq!(score_to_micros(0.000001).unwrap(), 1);
        assert_eq!(score_to_micros(1.5).unwrap(), 1_500_000);
    }

    #[test]
    fn score_to_micros_rounds_correctly() {
        // Values that require rounding
        assert_eq!(score_to_micros(0.1234567).unwrap(), 123_457);
        assert_eq!(score_to_micros(0.9999999).unwrap(), 1_000_000);
    }

    #[test]
    fn score_to_micros_rejects_negative_scores() {
        let result = score_to_micros(-0.5);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), "work_packet_docs_score_invalid");
    }

    #[test]
    fn score_to_micros_rejects_nan() {
        let result = score_to_micros(f64::NAN);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), "work_packet_docs_score_invalid");
    }

    #[test]
    fn score_to_micros_rejects_infinity() {
        let result = score_to_micros(f64::INFINITY);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), "work_packet_docs_score_invalid");
        let result = score_to_micros(f64::NEG_INFINITY);
        assert!(result.is_err());
    }

    // ===================================================================
    // P2S1-I4: Query term truncation
    // ===================================================================

    #[test]
    fn truncate_query_terms_respects_max_terms() {
        let terms: Vec<String> = (0..50).map(|i| format!("t{i}")).collect();
        let result = truncate_query_terms(&terms, 10, 9999);
        assert_eq!(result.len(), 10);
    }

    #[test]
    fn truncate_query_terms_respects_max_bytes() {
        let terms: Vec<String> = vec![
            "aaa".to_string(),
            "bbb".to_string(),
            "ccc".to_string(),
            "ddd".to_string(),
        ];
        // "aaa bbb ccc ddd" = 15 bytes. With limit 10, only first 2 fit:
        // "aaa" (3) + " bbb" (4) = 7 <= 10, " ccc" (4) would make 11 > 10.
        let result = truncate_query_terms(&terms, 10, 10);
        assert_eq!(result, vec!["aaa", "bbb"]);
    }

    #[test]
    fn truncate_query_terms_empty_input() {
        let result = truncate_query_terms(&[], 10, 100);
        assert!(result.is_empty());
    }

    #[test]
    fn truncate_query_terms_never_cuts_term() {
        let terms: Vec<String> = vec!["verylongterm".to_string(), "short".to_string()];
        // "verylongterm" is 12 chars, " verylongterm short" would be 24 bytes
        // With limit 5, even the first term doesn't fit.
        let result = truncate_query_terms(&terms, 10, 5);
        assert!(result.is_empty());
    }

    #[test]
    fn dispatch_remains_preview_and_revalidation_omits_packet_fingerprint() {
        let report = ReadinessReport {
            schema_version: 1,
            code: "ready".to_string(),
            subject: crate::graph::readiness::ReadinessSubject {
                id: "TK-001".to_string(),
                revision: 1,
                contract_revision: 1,
                status: NodeStatus::Ready,
            },
            profile: "phase1_contract_readiness_v1".to_string(),
            status: ReadinessStatus::Ready,
            transition_eligible: true,
            dispatch_authorized: false,
            readiness_fingerprint: "sha256:ready".to_string(),
            graph_fingerprint_observed: "sha256:graph".to_string(),
            gate_families: vec![GateFamilyReport {
                family: "readiness".to_string(),
                status: GateStatus::Passed,
                reason_codes: vec![],
            }],
            destination: None,
            remaining_non_blocking_uncertainty: vec![],
            future_gate_families: vec![],
            reason_codes: vec![],
        };
        let snapshot = work_packet::SnapshotReport {
            graph_fingerprint: "sha256:graph".to_string(),
            readiness_profile: "phase1_contract_readiness_v1".to_string(),
            readiness_fingerprint: "sha256:ready".to_string(),
            readiness_status: "ready".to_string(),
            authority_policy_revision: 1,
            authority_policy_fingerprint: "sha256:policy".to_string(),
            docs_registry_revision: 1,
            docs_registry_fingerprint: "sha256:registry".to_string(),
            docs_index_fingerprint: "sha256:registry".to_string(),
            source_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
        };
        let source = PacketSource {
            repository_id: "repo".to_string(),
            kind: "git_commit".to_string(),
            commit: snapshot.source_commit.clone(),
            head_ref: Some("refs/heads/main".to_string()),
            worktree_root_kind: "primary_or_existing_worktree".to_string(),
            cleanliness: "clean".to_string(),
            operation_state: "normal".to_string(),
            currentness: "current".to_string(),
        };
        let dispatch = build_dispatch(&report, &snapshot, &source).unwrap();
        assert!(dispatch.reservation_candidate);
        assert!(!dispatch.dispatch_authorized);
        assert_eq!(dispatch.authorization_status, "not_reserved");

        // Every snapshot field plus source.cleanliness must have a
        // revalidation precondition.  No packet_fingerprint precondition.
        let pre_fields: Vec<&str> = dispatch
            .revalidation_preconditions
            .iter()
            .map(|p| p.field.as_str())
            .collect();
        for field in [
            "snapshot.graph_fingerprint",
            "snapshot.readiness_profile",
            "snapshot.readiness_fingerprint",
            "snapshot.readiness_status",
            "snapshot.authority_policy_revision",
            "snapshot.authority_policy_fingerprint",
            "snapshot.docs_registry_revision",
            "snapshot.docs_registry_fingerprint",
            "snapshot.docs_index_fingerprint",
            "snapshot.source_commit",
            "source.cleanliness",
        ] {
            assert!(
                pre_fields.contains(&field),
                "missing revalidation precondition: {}",
                field
            );
        }
        assert!(
            !pre_fields.contains(&"packet_fingerprint"),
            "packet_fingerprint must not be a revalidation precondition"
        );

        // Verify preconditions carry the expected values
        let lookup = |field: &str| -> Option<&str> {
            dispatch
                .revalidation_preconditions
                .iter()
                .find(|p| p.field == field)
                .map(|p| p.value.as_str())
        };
        assert_eq!(lookup("snapshot.graph_fingerprint"), Some("sha256:graph"));
        assert_eq!(
            lookup("snapshot.readiness_profile"),
            Some("phase1_contract_readiness_v1")
        );
        assert_eq!(lookup("snapshot.readiness_status"), Some("ready"));
        assert_eq!(lookup("snapshot.authority_policy_revision"), Some("1"));
        assert_eq!(lookup("snapshot.docs_registry_revision"), Some("1"));
        assert_eq!(lookup("source.cleanliness"), Some("clean"));

        // Pre-reservation gate families are correctly set
        let documentation_context = dispatch
            .gate_families
            .iter()
            .find(|gate| gate.family == "documentation_context")
            .unwrap();
        assert_eq!(documentation_context.status, "passed");
        assert!(documentation_context.reason_codes.is_empty());

        // Runtime future gates stay not_evaluated with appropriate reason codes
        assert_eq!(
            dispatch
                .gate_families
                .iter()
                .find(|gate| gate.family == "lease")
                .unwrap()
                .status,
            "not_evaluated"
        );
        assert_eq!(
            dispatch
                .gate_families
                .iter()
                .find(|gate| gate.family == "workspace_binding")
                .unwrap()
                .status,
            "not_evaluated"
        );
        assert_eq!(
            dispatch
                .gate_families
                .iter()
                .find(|gate| gate.family == "capability_match")
                .unwrap()
                .status,
            "not_evaluated"
        );
        assert_eq!(
            dispatch
                .gate_families
                .iter()
                .find(|gate| gate.family == "qa_baseline_and_cases")
                .unwrap()
                .status,
            "not_applicable"
        );
    }

    // ===================================================================
    // P2S1-I5: No fabricated pass / empty meaning
    // ===================================================================

    #[test]
    fn no_gate_family_claims_passed_when_not_evaluated() {
        let report = ReadinessReport {
            schema_version: 1,
            code: "ready".to_string(),
            subject: crate::graph::readiness::ReadinessSubject {
                id: "TK-001".to_string(),
                revision: 1,
                contract_revision: 1,
                status: NodeStatus::Ready,
            },
            profile: "phase1_contract_readiness_v1".to_string(),
            status: ReadinessStatus::Ready,
            transition_eligible: true,
            dispatch_authorized: false,
            readiness_fingerprint: "sha256:ready".to_string(),
            graph_fingerprint_observed: "sha256:graph".to_string(),
            gate_families: vec![GateFamilyReport {
                family: "readiness".to_string(),
                status: GateStatus::Passed,
                reason_codes: vec![],
            }],
            destination: None,
            remaining_non_blocking_uncertainty: vec![],
            future_gate_families: vec![],
            reason_codes: vec![],
        };
        let snapshot = work_packet::SnapshotReport {
            graph_fingerprint: "sha256:graph".to_string(),
            readiness_profile: "phase1_contract_readiness_v1".to_string(),
            readiness_fingerprint: "sha256:ready".to_string(),
            readiness_status: "ready".to_string(),
            authority_policy_revision: 1,
            authority_policy_fingerprint: "sha256:policy".to_string(),
            docs_registry_revision: 1,
            docs_registry_fingerprint: "sha256:registry".to_string(),
            docs_index_fingerprint: "sha256:registry".to_string(),
            source_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
        };
        let source = PacketSource {
            repository_id: "repo".to_string(),
            kind: "git_commit".to_string(),
            commit: snapshot.source_commit.clone(),
            head_ref: Some("refs/heads/main".to_string()),
            worktree_root_kind: "primary_or_existing_worktree".to_string(),
            cleanliness: "clean".to_string(),
            operation_state: "normal".to_string(),
            currentness: "current".to_string(),
        };
        let dispatch = build_dispatch(&report, &snapshot, &source).unwrap();

        // Only the four pre-reservation gate families that Slice 1 owns
        // should appear as "passed".  Every other gate must stay at its
        // default typed state — never fabricated empty/pass.
        for gate in &dispatch.gate_families {
            match gate.family.as_str() {
                "readiness" | "packet_completeness" | "source_base" | "documentation_context" => {
                    assert_eq!(
                        gate.status, "passed",
                        "gate {} should be 'passed' but is '{}'",
                        gate.family, gate.status
                    );
                }
                "qa_baseline_and_cases" => {
                    assert_eq!(
                        gate.status, "not_applicable",
                        "gate {} should be 'not_applicable' but is '{}'",
                        gate.family, gate.status
                    );
                }
                "lease" | "workspace_binding" | "capability_match" => {
                    assert_eq!(
                        gate.status, "not_evaluated",
                        "runtime gate {} should be 'not_evaluated' but is '{}'",
                        gate.family, gate.status
                    );
                }
                other => {
                    panic!("unexpected gate family: {}", other);
                }
            }
        }

        // Runtime gates must carry typed reason codes, not empty strings
        let lease = dispatch
            .gate_families
            .iter()
            .find(|g| g.family == "lease")
            .unwrap();
        assert!(
            !lease.reason_codes.is_empty(),
            "lease gate must have reason codes"
        );
        assert_eq!(lease.reason_codes[0], "lease_resolver_not_installed");

        let workspace = dispatch
            .gate_families
            .iter()
            .find(|g| g.family == "workspace_binding")
            .unwrap();
        assert!(
            !workspace.reason_codes.is_empty(),
            "workspace_binding gate must have reason codes"
        );
        assert_eq!(workspace.reason_codes[0], "workspace_not_allocated");

        let capability = dispatch
            .gate_families
            .iter()
            .find(|g| g.family == "capability_match")
            .unwrap();
        assert!(
            !capability.reason_codes.is_empty(),
            "capability_match gate must have reason codes"
        );
        assert_eq!(capability.reason_codes[0], "capability_inventory_not_bound");
    }

    // ===================================================================
    // P2S1-I5: No authorization path — dispatch_authorized is always false
    // ===================================================================

    #[test]
    fn no_authorization_path_in_preview_packet() {
        // Verify that every possible path through build_dispatch produces
        // dispatch_authorized=false and authorization_status=not_reserved.
        let base_report = ReadinessReport {
            schema_version: 1,
            code: "ready".to_string(),
            subject: crate::graph::readiness::ReadinessSubject {
                id: "TK-001".to_string(),
                revision: 1,
                contract_revision: 1,
                status: NodeStatus::Ready,
            },
            profile: "phase1_contract_readiness_v1".to_string(),
            status: ReadinessStatus::Ready,
            transition_eligible: true,
            dispatch_authorized: false,
            readiness_fingerprint: "sha256:ready".to_string(),
            graph_fingerprint_observed: "sha256:graph".to_string(),
            gate_families: vec![GateFamilyReport {
                family: "readiness".to_string(),
                status: GateStatus::Passed,
                reason_codes: vec![],
            }],
            destination: None,
            remaining_non_blocking_uncertainty: vec![],
            future_gate_families: vec![],
            reason_codes: vec![],
        };
        let snapshot = work_packet::SnapshotReport {
            graph_fingerprint: "sha256:graph".to_string(),
            readiness_profile: "phase1_contract_readiness_v1".to_string(),
            readiness_fingerprint: "sha256:ready".to_string(),
            readiness_status: "ready".to_string(),
            authority_policy_revision: 1,
            authority_policy_fingerprint: "sha256:policy".to_string(),
            docs_registry_revision: 1,
            docs_registry_fingerprint: "sha256:registry".to_string(),
            docs_index_fingerprint: "sha256:registry".to_string(),
            source_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
        };
        let source = PacketSource {
            repository_id: "repo".to_string(),
            kind: "git_commit".to_string(),
            commit: snapshot.source_commit.clone(),
            head_ref: Some("refs/heads/main".to_string()),
            worktree_root_kind: "primary_or_existing_worktree".to_string(),
            cleanliness: "clean".to_string(),
            operation_state: "normal".to_string(),
            currentness: "current".to_string(),
        };

        // Case 1: transition_eligible=true (normal happy path)
        let dispatch = build_dispatch(&base_report, &snapshot, &source).unwrap();
        assert!(
            !dispatch.dispatch_authorized,
            "dispatch_authorized must be false when transition_eligible=true"
        );
        assert_eq!(
            dispatch.authorization_status, "not_reserved",
            "authorization_status must be not_reserved when transition_eligible=true"
        );

        // Case 2: transition_eligible=false (e.g. stale readiness)
        let stale_report = ReadinessReport {
            transition_eligible: false,
            ..base_report.clone()
        };
        let err = build_dispatch(&stale_report, &snapshot, &source).unwrap_err();
        assert!(
            format!("{}", err).contains("pre-reservation gates"),
            "failed pre-reservation gates must fail non-zero, not return partial packets: {}",
            err
        );

        // Case 3: a non-ready report code/status is also an error, never a
        // reservation_candidate=false packet with failed gates.
        let failed_report = ReadinessReport {
            code: "not_ready".to_string(),
            status: ReadinessStatus::NotReady,
            transition_eligible: false,
            ..base_report
        };
        let err = build_dispatch(&failed_report, &snapshot, &source).unwrap_err();
        assert!(
            format!("{}", err).contains("pre-reservation gates"),
            "non-ready report must fail non-zero, not return partial packets: {}",
            err
        );
    }
}
