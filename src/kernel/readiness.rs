use std::fs;
use std::path::Path;

use crate::canonical_json::hash_bytes;
use crate::graph::model::brief::TicketBrief;
use crate::graph::model::contract::Materialization;
use crate::graph::model::node::{Node, NodeStatus};
use crate::graph::read::executability::{structural_executability, StructuralExecutabilityReport};
use crate::graph::read::readiness::{
    evaluate as evaluate_readiness, ContentHashBinding, EvalProfile, QaCaseResolutionSnapshot,
    ReadinessInputs, ReadinessReport,
};
use crate::graph::store::JsonGraphStore;
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
        self.build_readiness_snapshot_from_projection(node, &projection)
    }

    /// Build readiness from a caller-supplied graph projection. Packet assembly
    /// uses this to ensure subject, relations and readiness all observe the
    /// same canonical graph snapshot under the already-held repository fence.
    pub(crate) fn build_readiness_snapshot_from_projection(
        &self,
        node: &Node,
        projection: &crate::graph::read::projection::GraphProjection,
    ) -> PulseResult<ReadinessSnapshot> {
        let structural = structural_executability(projection, &node.id).or_else(|err| {
            if matches!(err, PulseError::NotFound { .. }) {
                Ok(self.empty_structural_report(&node.id))
            } else {
                Err(err)
            }
        })?;
        let (ticket_brief, ticket_brief_error) = self.build_ticket_brief_snapshot(node);
        let docs = self.build_docs_applicability(node)?;
        let qa_resolution = self.build_qa_resolution(node);
        let authority = crate::policy::load_authority_policy(&self.repo_root)?;
        let content_bindings = self.build_content_bindings(node);
        Ok(ReadinessSnapshot {
            graph_fingerprint: projection.graph_fingerprint.clone(),
            structural,
            ticket_brief,
            ticket_brief_error,
            docs,
            qa_resolution,
            authority,
            content_bindings,
        })
    }

    /// Parse the current ticket.md for the ambiguity gate without mutating the
    /// graph. Errors are carried into the pure report so `work readiness` can
    /// explain the failed gate instead of turning malformed prose into an I/O
    /// failure.
    fn build_ticket_brief_snapshot(&self, node: &Node) -> (Option<TicketBrief>, Option<String>) {
        if node.kind != crate::id::WorkKind::Ticket
            || node.role != Some(crate::graph::model::contract::TicketRole::Implementation)
        {
            return (None, None);
        }
        let path = self.repo_root.join(&node.content_dir).join("ticket.md");
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                return (
                    None,
                    Some(if error.kind() == std::io::ErrorKind::NotFound {
                        "ticket_brief_missing".to_string()
                    } else {
                        "ticket_brief_unreadable".to_string()
                    }),
                )
            }
        };
        let markdown = match std::str::from_utf8(&bytes) {
            Ok(markdown) => markdown,
            Err(_) => return (None, Some("ticket_brief_invalid".to_string())),
        };
        let brief = match crate::graph::model::brief::parse_ticket_brief(markdown) {
            Ok(brief) => brief,
            Err(error) => return (None, Some(error.code().to_string())),
        };
        let materialization = node.materialization.unwrap_or(Materialization::R0);
        if let Err(error) = brief.validate_for(materialization) {
            return (Some(brief), Some(error.code().to_string()));
        }
        (Some(brief), None)
    }

    fn build_qa_resolution(&self, node: &Node) -> Option<QaCaseResolutionSnapshot> {
        if node.qa.as_ref().map(|qa| qa.impact.posture)
            != Some(crate::graph::model::contract::QaImpactPosture::Required)
        {
            return None;
        }
        match crate::qa::resolve_ticket_cases(&self.repo_root, node) {
            Ok(resolution) => Some(QaCaseResolutionSnapshot {
                owner_id: resolution.owner_id,
                baseline_content_hash: resolution.content_hash,
                selected_cases: resolution
                    .cases
                    .into_iter()
                    .map(|case| (case.id, case.case_hash))
                    .collect(),
                error_code: None,
            }),
            Err(error) => Some(QaCaseResolutionSnapshot {
                owner_id: node
                    .qa
                    .as_ref()
                    .and_then(|qa| qa.impact.behavioral_owner.clone())
                    .unwrap_or_default(),
                baseline_content_hash: String::new(),
                selected_cases: Vec::new(),
                error_code: Some(error.code().to_string()),
            }),
        }
    }

    pub(crate) fn empty_structural_report(&self, id: &str) -> StructuralExecutabilityReport {
        StructuralExecutabilityReport {
            schema_version: 1,
            subject: id.to_string(),
            graph_fingerprint: String::new(),
            structural_state: crate::graph::read::executability::StructuralState::Invalid,
            dispatch_authorized: false,
            lifecycle: crate::graph::read::executability::LifecycleSummary {
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

    pub(crate) fn build_content_bindings(&self, node: &Node) -> Vec<ContentHashBinding> {
        let mut bindings = Vec::new();
        if let Some(brief_hash) = &node.brief_hash {
            bindings.push(ContentHashBinding {
                label: "brief".to_string(),
                path: format!("{}/ticket.md", node.content_dir),
                bound_hash: brief_hash.clone(),
                current_hash: content_hash_option(
                    &self.repo_root,
                    &format!("{}/ticket.md", node.content_dir),
                ),
            });
        }
        bindings
    }
}

pub(crate) struct ReadinessSnapshot {
    pub(crate) graph_fingerprint: String,
    pub(crate) structural: StructuralExecutabilityReport,
    pub(crate) ticket_brief: Option<TicketBrief>,
    pub(crate) ticket_brief_error: Option<String>,
    pub(crate) docs: crate::docs::applicability::ApplicableDocsReport,
    pub(crate) qa_resolution: Option<QaCaseResolutionSnapshot>,
    pub(crate) authority: crate::policy::AuthorityPolicyReport,
    pub(crate) content_bindings: Vec<ContentHashBinding>,
}

impl ReadinessSnapshot {
    pub(crate) fn as_inputs<'a>(&'a self, node: &'a Node) -> ReadinessInputs<'a> {
        ReadinessInputs {
            subject: node,
            graph_valid: true,
            structural: &self.structural,
            ticket_brief: self.ticket_brief.as_ref(),
            ticket_brief_error: self.ticket_brief_error.as_deref(),
            qa_resolution: self.qa_resolution.as_ref(),
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
