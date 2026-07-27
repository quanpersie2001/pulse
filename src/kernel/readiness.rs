use std::fs;
use std::path::Path;

use crate::canonical_json::hash_bytes;
use crate::graph::executability::{structural_executability, StructuralExecutabilityReport};
use crate::graph::node::{Node, NodeStatus};
use crate::graph::readiness::{
    evaluate as evaluate_readiness, ContentHashBinding, DecisionProofSnapshot, EvalProfile,
    ReadinessInputs, ReadinessReport, ShapingReceiptSnapshot,
};
use crate::graph::store::JsonGraphStore;
use crate::kernel::shaping::verify_map_current;
use crate::storage::transaction::recover_prepared_transactions;
use crate::storage::WriteGuard;
use crate::{PulseError, PulseResult};

impl JsonGraphStore {
    pub fn readiness(&self, id: &str) -> PulseResult<ReadinessReport> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        // Read the node directly (not via `show_node`, which re-acquires the
        // fence and would deadlock the guard we already hold).
        let path = self.node_path(id);
        if !path.exists() {
            return Err(PulseError::NotFound {
                subject: id.to_string(),
            });
        }
        let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        let node: Node =
            serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))?;
        let snapshot = self.build_readiness_snapshot(&node)?;
        let inputs = snapshot.as_inputs(&node);
        evaluate_readiness(&inputs, EvalProfile::Ready)
    }

    /// Build a coherent readiness snapshot for a subject node. Acquires the
    /// repository fence so multi-plane reads (graph, docs, evidence, policy,
    /// bound content) are consistent. Caller is expected to have recovered.
    pub(crate) fn build_readiness_snapshot(&self, node: &Node) -> PulseResult<ReadinessSnapshot> {
        let projection = self.export_unlocked()?;
        let structural = structural_executability(&projection, &node.id).or_else(|err| {
            if matches!(err, PulseError::NotFound { .. }) {
                Ok(self.empty_structural_report(&node.id))
            } else {
                Err(err)
            }
        })?;
        let shaping = self.build_shaping_snapshot(node)?;
        let decision_proofs = self.build_decision_proofs(node)?;
        let docs = self.build_docs_applicability(node)?;
        let authority = crate::policy::load_authority_policy(&self.repo_root)?;
        let content_bindings = self.build_content_bindings(node, &shaping, &decision_proofs)?;
        Ok(ReadinessSnapshot {
            graph_fingerprint: projection.graph_fingerprint.clone(),
            structural,
            shaping,
            decision_proofs,
            docs,
            authority,
            content_bindings,
        })
    }

    pub(crate) fn empty_structural_report(&self, id: &str) -> StructuralExecutabilityReport {
        StructuralExecutabilityReport {
            schema_version: 1,
            subject: id.to_string(),
            graph_fingerprint: String::new(),
            structural_state: crate::graph::executability::StructuralState::Invalid,
            dispatch_authorized: false,
            lifecycle: crate::graph::executability::LifecycleSummary {
                status: NodeStatus::Draft,
                revision: 0,
            },
            hard_blockers: vec![],
            soft_preferences: vec![],
            supersession: None,
            gate_coverage: vec![],
            missing_gate_families: vec![],
            reason_codes: vec!["structural_invalid".to_string()],
        }
    }

    pub(crate) fn build_shaping_snapshot(
        &self,
        node: &Node,
    ) -> PulseResult<Option<ShapingReceiptSnapshot>> {
        let Some(pointer) = &node.shaping else {
            return Ok(None);
        };
        let snapshot =
            match crate::evidence::receipt::load_receipt(&self.repo_root, &pointer.receipt.id) {
                Ok((receipt, hash)) => {
                    let payload = match &receipt.payload {
                        crate::evidence::model::ReceiptPayload::ShapingValidation(payload) => {
                            Some(payload.clone())
                        }
                        _ => None,
                    };
                    let integrity_valid = hash == pointer.receipt.hash
                        && receipt.kind == crate::evidence::model::ReceiptKind::ShapingValidation
                        && receipt.result == crate::evidence::model::ReceiptResult::Passed
                        && payload.as_ref().is_some_and(|p| p.payload_version == 1);
                    let payload = payload.unwrap_or_else(default_shaping_payload);
                    let binding_codes = crate::evidence::receipt::content_source_binding_codes(
                        &self.repo_root,
                        &receipt.bindings,
                        None,
                    )
                    .unwrap_or_default();
                    let map_current = payload
                        .map
                        .as_ref()
                        .map(|map| verify_map_current(&self.repo_root, map).is_ok())
                        .unwrap_or(true);
                    ShapingReceiptSnapshot {
                        receipt_id: pointer.receipt.id.clone(),
                        receipt_hash: hash,
                        payload,
                        integrity_valid,
                        binding_codes,
                        map_current,
                    }
                }
                Err(_) => ShapingReceiptSnapshot {
                    receipt_id: pointer.receipt.id.clone(),
                    receipt_hash: pointer.receipt.hash.clone(),
                    payload: default_shaping_payload(),
                    integrity_valid: false,
                    binding_codes: vec!["shaping_receipt_missing".to_string()],
                    map_current: true,
                },
            };
        Ok(Some(snapshot))
    }

    pub(crate) fn build_decision_proofs(
        &self,
        node: &Node,
    ) -> PulseResult<Vec<DecisionProofSnapshot>> {
        let Some(contract) = &node.implementation else {
            return Ok(Vec::new());
        };
        if contract.required_decisions.is_empty() {
            return Ok(Vec::new());
        }
        let nodes = self.load_nodes()?;
        let mut proofs = Vec::new();
        for decision in &contract.required_decisions {
            let snapshot = match crate::evidence::receipt::load_receipt(
                &self.repo_root,
                &decision.acceptance_receipt.id,
            ) {
                Ok((receipt, hash)) => {
                    let payload = match &receipt.payload {
                        crate::evidence::model::ReceiptPayload::DecisionAcceptance(p) => {
                            Some(p.clone())
                        }
                        _ => None,
                    };
                    let integrity_valid = hash == decision.acceptance_receipt.hash
                        && receipt.kind == crate::evidence::model::ReceiptKind::DecisionAcceptance
                        && payload.is_some();
                    let payload = payload.unwrap_or_else(default_decision_payload);
                    let content_current =
                        content_hash_option(&self.repo_root, &payload.decision.content.path)
                            .map(|h| h == payload.decision.content.content_hash)
                            .unwrap_or(false);
                    let decision_node = nodes.get(&decision.id);
                    DecisionProofSnapshot {
                        decision_id: decision.id.clone(),
                        required_contract_revision: decision.contract_revision,
                        receipt_id: decision.acceptance_receipt.id.clone(),
                        receipt_hash: hash,
                        payload,
                        integrity_valid,
                        decision_node_present: decision_node.is_some(),
                        decision_terminal: decision_node
                            .map(|n| {
                                matches!(n.status, NodeStatus::Cancelled | NodeStatus::Superseded)
                            })
                            .unwrap_or(false),
                        decision_contract_revision: decision_node
                            .map(|n| n.contract_revision)
                            .unwrap_or(0),
                        content_current,
                    }
                }
                Err(error) if error.code() == "receipt_not_found" => {
                    // The acceptance proof does not exist yet. Per the proposal,
                    // work depending on a not-yet-accepted Decision is
                    // `unavailable` (decision_acceptance_missing), not stale. We
                    // omit the proof so the readiness gate sees `None`.
                    continue;
                }
                Err(_) => DecisionProofSnapshot {
                    decision_id: decision.id.clone(),
                    required_contract_revision: decision.contract_revision,
                    receipt_id: decision.acceptance_receipt.id.clone(),
                    receipt_hash: decision.acceptance_receipt.hash.clone(),
                    payload: default_decision_payload(),
                    integrity_valid: false,
                    decision_node_present: nodes.contains_key(&decision.id),
                    decision_terminal: false,
                    decision_contract_revision: nodes
                        .get(&decision.id)
                        .map(|n| n.contract_revision)
                        .unwrap_or(0),
                    content_current: false,
                },
            };
            proofs.push(snapshot);
        }
        Ok(proofs)
    }

    pub(crate) fn build_docs_applicability(
        &self,
        node: &Node,
    ) -> PulseResult<crate::docs::applicability::ApplicableDocsReport> {
        let work = node
            .documentation
            .as_ref()
            .map(|documentation| {
                crate::docs::WorkDocumentationContext::from((
                    node.id.as_str(),
                    node.revision,
                    documentation,
                ))
            })
            .unwrap_or_else(|| {
                crate::docs::WorkDocumentationContext::unknown(node.id.clone(), node.revision)
            });
        // Read-only readiness/frontier projections must never bootstrap the
        // docs registry (or, transitively, the evidence manifest) as a side
        // effect of a query. When the registry is absent, compute applicability
        // against an empty docs set instead of materializing canonical state.
        let registry = crate::docs::manifest::load_unlocked_preserve(&self.repo_root)?
            .unwrap_or_else(|| crate::docs::model::DocsRegistry::empty(String::new()));
        let resolver = crate::docs::FsContentResolver::new(&self.repo_root);
        crate::docs::applicable_docs(
            &work,
            &registry,
            &resolver,
            crate::docs::ApplicabilityOptions::default(),
        )
    }

    pub(crate) fn build_content_bindings(
        &self,
        node: &Node,
        shaping: &Option<ShapingReceiptSnapshot>,
        decision_proofs: &[DecisionProofSnapshot],
    ) -> PulseResult<Vec<ContentHashBinding>> {
        let mut bindings = Vec::new();
        if let Some(contract) = &node.implementation {
            if let Some(brief) = &contract.brief {
                bindings.push(ContentHashBinding {
                    label: "brief".to_string(),
                    path: brief.path.clone(),
                    bound_hash: brief.content_hash.clone(),
                    current_hash: content_hash_option(&self.repo_root, &brief.path),
                });
            }
            for approach in &contract.shared_approach_refs {
                bindings.push(ContentHashBinding {
                    label: format!("shared_approach:{}", approach.path),
                    path: approach.path.clone(),
                    bound_hash: approach.content_hash.clone(),
                    current_hash: content_hash_option(&self.repo_root, &approach.path),
                });
            }
        }
        if let Some(shaping) = shaping {
            if let Some(map) = &shaping.payload.map {
                bindings.push(ContentHashBinding {
                    label: "map".to_string(),
                    path: map.path.clone(),
                    bound_hash: map.content_hash.clone(),
                    current_hash: content_hash_option(&self.repo_root, &map.path),
                });
            }
        }
        for proof in decision_proofs {
            bindings.push(ContentHashBinding {
                label: format!("decision:{}", proof.decision_id),
                path: proof.payload.decision.content.path.clone(),
                bound_hash: proof.payload.decision.content.content_hash.clone(),
                current_hash: content_hash_option(
                    &self.repo_root,
                    &proof.payload.decision.content.path,
                ),
            });
        }
        Ok(bindings)
    }
}

pub(crate) struct ReadinessSnapshot {
    pub(crate) graph_fingerprint: String,
    pub(crate) structural: StructuralExecutabilityReport,
    pub(crate) shaping: Option<ShapingReceiptSnapshot>,
    pub(crate) decision_proofs: Vec<DecisionProofSnapshot>,
    pub(crate) docs: crate::docs::applicability::ApplicableDocsReport,
    pub(crate) authority: crate::policy::AuthorityPolicyReport,
    pub(crate) content_bindings: Vec<ContentHashBinding>,
}

impl ReadinessSnapshot {
    pub(crate) fn as_inputs<'a>(&'a self, node: &'a Node) -> ReadinessInputs<'a> {
        ReadinessInputs {
            subject: node,
            graph_valid: true,
            structural: &self.structural,
            shaping: self.shaping.as_ref(),
            decision_proofs: self.decision_proofs.clone(),
            docs: &self.docs,
            authority: &self.authority,
            content_bindings: self.content_bindings.clone(),
            graph_fingerprint: self.graph_fingerprint.clone(),
        }
    }
}

/// Current canonical content hash of a repository-relative file, or `None` when
/// the file is missing/unreadable. Used for content-reference currentness.
fn content_hash_option(repo_root: &Path, path: &str) -> Option<String> {
    let rel = crate::storage::safe_repo_relative(path).ok()?;
    let bytes = fs::read(repo_root.join(rel)).ok()?;
    Some(hash_bytes(&bytes))
}

/// Placeholder shaping payload used when a current shaping receipt cannot be
/// loaded as a valid v1 passed payload. The evaluator flags the family via
/// `integrity_valid=false`; payload contents are irrelevant in that case.
fn default_shaping_payload() -> crate::evidence::model::ShapingValidationPayload {
    use crate::evidence::model::{
        ShapeMode, ShapingApproval, ShapingValidationPayload, ShapingWorkBinding, SourcePosture,
    };
    use crate::identity::actor::{ActorKind, ActorRef};
    ShapingValidationPayload {
        payload_version: 1,
        owning_work: ShapingWorkBinding {
            id: String::new(),
            revision_observed: 0,
            contract_revision: 0,
        },
        materialization: "R0".to_string(),
        shape_mode: ShapeMode::ConciseSelfCheck,
        source_posture: SourcePosture::NotRequiredContentBound,
        destination: None,
        map: None,
        affected_work: vec![],
        branches: vec![],
        fog: vec![],
        out_of_scope: vec![],
        resolution_pointers: vec![],
        approval: ShapingApproval {
            approved_by: ActorRef {
                kind: ActorKind::System,
                id: String::new(),
            },
            reference: String::new(),
        },
        reconciliation: None,
        remaining_uncertainty: vec![],
    }
}

/// Placeholder Decision acceptance payload used when the referenced receipt
/// cannot be loaded as a valid Decision acceptance proof.
fn default_decision_payload() -> crate::evidence::model::DecisionAcceptancePayload {
    use crate::evidence::model::{
        DecisionAcceptanceDecision, DecisionAcceptancePayload, DecisionContentSnapshot,
        SourcePosture,
    };
    use crate::identity::actor::{ActorKind, ActorRef};
    DecisionAcceptancePayload {
        payload_version: 1,
        decision: DecisionAcceptanceDecision {
            id: String::new(),
            revision_observed: 0,
            contract_revision: 0,
            content: DecisionContentSnapshot {
                path: String::new(),
                content_hash: String::new(),
            },
        },
        accepted_outcome: String::new(),
        approver: ActorRef {
            kind: ActorKind::System,
            id: String::new(),
        },
        source_posture: SourcePosture::NotRequiredContentBound,
    }
}
