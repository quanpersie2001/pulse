//! Coherent canonical packet snapshot builder (P2S1-I3).
//!
//! This module composes the cross-domain [`WorkPacketV1`] under a repository
//! fence, reusing readiness snapshots and preserving the Phase 1 readiness
//! contract.  No docs search/index or CLI rendering is owned here — only
//! graph/source/authority plane composition.
//!
//! See `proposals/phase2-slice1-work-packet-dispatch-foundation.md` § P2S1-I3.

use std::fs;

use crate::docs::applicability::ApplicableDocsReport;
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
    PacketResolution, PacketScope, PacketScopeHints, PacketShaping, PacketShapingDestination,
    PacketShapingMapSnapshot, PacketShapingWorkBinding, PacketSource, PacketSurfaceRef,
    PacketWorkspace, SubjectSnapshot, WorkPacketV1,
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
    pub fn work_packet(&self, id: &str) -> PulseResult<WorkPacketV1> {
        // ---- Step 1: Validate enrollment ---------------------------------
        let evidence = check_repository_identity(&self.repo_root)?;
        let repository_id = evidence.repository_id.clone();

        // ---- Step 2: Acquire fence, recover, load graph -------------------
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;

        let projection = self.export_unlocked()?;

        // ---- Step 3: Load subject and verify eligibility ------------------
        let node = self.load_subject(id, &projection)?;
        self.verify_packet_eligible(&node)?;

        // ---- Step 4: Build readiness snapshot -----------------------------
        let readiness = self.build_readiness_snapshot(&node)?;
        let inputs = readiness.as_inputs(&node);
        let readiness_report = evaluate_readiness(&inputs, EvalProfile::Ready)?;

        if readiness_report.code != "ready" {
            return self.readiness_error(&readiness_report);
        }

        // ---- Step 5: Extract every packet section -------------------------
        let subject = extract_subject(&node);
        let snapshot = extract_snapshot(&readiness, &projection);
        let contract = extract_contract_dto(&node)?;
        let context = extract_context(&node, &projection, &self.repo_root);
        let shaping = extract_shaping(&node, &readiness.shaping, &projection)?;
        let graph = extract_graph(&readiness, &projection)?;
        let documentation = extract_documentation(&readiness.docs);
        let source = extract_source_snapshot(&self.repo_root, &repository_id)?;
        let workspace = extract_workspace(&node, &source, &repository_id);
        let capabilities = extract_capabilities(&node);
        let scope = extract_scope(&node);
        let assurance = extract_assurance(&node);

        // ---- Step 6: Assemble, normalize, fingerprint, finalize -----------
        let mut packet = WorkPacketV1 {
            schema_version: work_packet::PACKET_SCHEMA_VERSION,
            profile: work_packet::PACKET_PROFILE.to_string(),
            code: "reservation_candidate".to_string(),
            subject,
            snapshot,
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
            dispatch: build_dispatch(&readiness_report),
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
) -> work_packet::SnapshotReport {
    work_packet::SnapshotReport {
        graph_fingerprint: projection.graph_fingerprint.clone(),
        readiness_profile: crate::graph::readiness::READINESS_PROFILE.to_string(),
        readiness_fingerprint: readiness.structural.graph_fingerprint.clone(),
        readiness_status: "ready".to_string(),
        authority_policy_revision: readiness.authority.policy_revision.unwrap_or(0),
        authority_policy_fingerprint: readiness.authority.fingerprint.clone().unwrap_or_default(),
        docs_registry_revision: readiness.docs.registry.revision,
        docs_registry_fingerprint: readiness.docs.registry.fingerprint.clone(),
        docs_index_fingerprint: String::new(),
        source_commit: String::new(),
    }
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
            .map(|e| format!("{e:?}"))
            .collect(),
        expected_handoff: contract
            .expected_handoff
            .iter()
            .map(|h| format!("{h:?}"))
            .collect(),
    })
}

fn extract_context(
    node: &Node,
    projection: &GraphProjection,
    repo_root: &std::path::Path,
) -> work_packet::PacketContext {
    let parents = extract_parents(node, projection);
    let decisions = extract_decisions(node, projection, repo_root);
    work_packet::PacketContext { parents, decisions }
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
        if let Some(parent) = projection.nodes.iter().find(|n| n.id == edge.to) {
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
                if let Some(ancestor) = projection.nodes.iter().find(|n| n.id == ancestor_edge.to) {
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
    repo_root: &std::path::Path,
) -> Vec<work_packet::PacketDecisionRef> {
    let Some(contract) = &node.implementation else {
        return Vec::new();
    };
    let mut decisions = Vec::new();
    for required in &contract.required_decisions {
        let decision_node = projection.nodes.iter().find(|n| n.id == required.id);
        let (status, revision, contract_revision, title) = match decision_node {
            Some(dn) => (
                node_status_str(dn.status),
                dn.revision,
                dn.contract_revision,
                dn.title.clone(),
            ),
            None => continue,
        };
        let receipt = match crate::evidence::receipt::load_receipt(
            repo_root,
            &required.acceptance_receipt.id,
        ) {
            Ok((receipt, hash)) if hash == required.acceptance_receipt.hash => {
                Some(work_packet::PacketReceiptRef {
                    id: receipt.id,
                    hash,
                })
            }
            _ => None,
        };
        decisions.push(work_packet::PacketDecisionRef {
            id: required.id.clone(),
            revision,
            contract_revision,
            status,
            title,
            acceptance_receipt: receipt,
            content_refs: vec![],
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

    // Decision frontier: max 16 items
    let frontier_items: Vec<PacketDecisionFrontierItem> = payload
        .resolution_pointers
        .iter()
        .filter(|rp| critical_branches.iter().any(|cb| cb.id == rp.id))
        .map(|rp| PacketDecisionFrontierItem {
            id: rp.id.clone(),
            revision: rp.revision,
            gap_kind: "resolved".to_string(),
            question: String::new(),
            status: "evaluated".to_string(),
        })
        .collect();

    if payload.resolution_pointers.len() > 16 {
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
            title: b.path.join(" -> "),
        })
        .collect();

    let soft_preferences: Vec<work_packet::PacketBlockerItem> = structural
        .soft_preferences
        .iter()
        .map(|p| work_packet::PacketBlockerItem {
            id: p.preferred_after.clone(),
            relation: "preferred_after".to_string(),
            title: String::new(),
        })
        .collect();

    let supersession = structural.supersession.as_ref().and_then(|s| {
        s.replacement
            .clone()
            .map(|repl| work_packet::PacketSupersessionRef {
                id: repl,
                revision: 0,
                status: String::new(),
                title: String::new(),
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
    if total > 128 {
        return Err(PulseError::validation(
            "work_packet_relation_overflow",
            "more than 128 incident edges",
        ));
    }

    for edge in &projection.edges {
        if edge.from == *subject_id {
            if let Some(opp) = projection.nodes.iter().find(|n| n.id == edge.to) {
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
            if let Some(opp) = projection.nodes.iter().find(|n| n.id == edge.from) {
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

fn extract_documentation(docs: &ApplicableDocsReport) -> PacketDocumentation {
    let required: Vec<PacketDocRef> = docs
        .required
        .iter()
        .map(|d| PacketDocRef {
            id: d.id.clone(),
            path: d.path.clone(),
            kind: format!("{:?}", d.kind),
            authority: format!("{:?}", d.authority),
            owner: d.owner.clone(),
            summary: d.summary.clone(),
            revision: d.document_revision,
            content_hash: d.content_hash.clone(),
            reasons: d.reasons.clone(),
        })
        .collect();

    let optional: Vec<PacketDocRef> = docs
        .optional
        .iter()
        .map(|d| PacketDocRef {
            id: d.id.clone(),
            path: d.path.clone(),
            kind: format!("{:?}", d.kind),
            authority: format!("{:?}", d.authority),
            owner: d.owner.clone(),
            summary: d.summary.clone(),
            revision: d.document_revision,
            content_hash: d.content_hash.clone(),
            reasons: d.reasons.clone(),
        })
        .collect();

    let write_candidates: Vec<PacketDocRef> = docs
        .write_candidates
        .iter()
        .map(|wc| PacketDocRef {
            id: wc.id.clone(),
            path: String::new(),
            kind: String::new(),
            authority: String::new(),
            owner: String::new(),
            summary: String::new(),
            revision: 0,
            content_hash: String::new(),
            reasons: wc.reasons.clone(),
        })
        .collect();

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

    PacketDocumentation {
        applicability: PacketDocsApplicability {
            status: docs.gate.status.clone(),
            required,
            optional,
            write_candidates,
            excluded,
        },
        suggestion_query: work_packet::PacketSuggestionQuery {
            text: String::new(),
            normalized_terms: vec![],
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
            state: "current".to_string(),
            fingerprint: docs.registry.fingerprint.clone(),
            mode: "lexical".to_string(),
        },
    }
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
            hints.source_paths.push(anchor.path.clone());
        }
        for anchor in &contract.documentation_anchors {
            hints.documentation_paths.push(anchor.path.clone());
        }
        for anchor in &contract.configuration_anchors {
            hints.configuration_paths.push(anchor.path.clone());
        }
        for anchor in &contract.data_anchors {
            hints.data_paths.push(anchor.path.clone());
        }
        hints.included = contract.scope.included.clone();
        hints.excluded = contract.scope.excluded.clone();
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
                .map(|e| format!("{e:?}"))
                .collect()
        })
        .unwrap_or_default();

    let expected_handoff: Vec<String> = node
        .implementation
        .as_ref()
        .map(|c| {
            c.expected_handoff
                .iter()
                .map(|h| format!("{h:?}"))
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

fn build_dispatch(report: &ReadinessReport) -> PacketDispatch {
    let mut dispatch = PacketDispatch::default();
    for gate in &mut dispatch.gate_families {
        match gate.family.as_str() {
            "readiness" => {
                gate.status = if report.transition_eligible {
                    "passed".to_string()
                } else {
                    "failed".to_string()
                };
            }
            "packet_completeness" => {
                gate.status = "passed".to_string();
            }
            "source_base" => {
                gate.status = "passed".to_string();
            }
            "documentation_context" => {
                gate.status = "passed".to_string();
            }
            _ => {}
        }
    }
    dispatch
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
