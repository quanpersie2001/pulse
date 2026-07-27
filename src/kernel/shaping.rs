use std::fs;
use std::path::Path;

use chrono::Utc;
use serde_json::json;

use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::event::EventEnvelope;
use crate::graph::contract::{
    validate_node_contract_result, ContractValidationMode, ReceiptRef, ShapingMapRef,
    ShapingPointer,
};
use crate::graph::node::Node;
use crate::graph::store::{
    JsonGraphStore, MutationOutcome, MutationStatus, OperationContext, ShapingView,
};
use crate::graph::validate::validate_graph;
use crate::id::WorkKind;
use crate::storage::transaction::{recover_prepared_transactions, FileState};
use crate::storage::{self, WriteGuard};
use crate::{PulseError, PulseResult};

impl JsonGraphStore {
    pub fn apply_shaping_with_context(
        &self,
        owner_id: &str,
        expected_revision: u64,
        receipt_id: &str,
        expected_current_receipt: Option<&str>,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let path = self.node_path(owner_id);
        if !path.exists() {
            return Err(PulseError::NotFound {
                subject: owner_id.to_string(),
            });
        }
        let before_bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        let mut node: Node = serde_json::from_slice(&before_bytes)
            .map_err(|error| PulseError::json(&path, error))?;
        if !matches!(
            node.kind,
            WorkKind::Epic | WorkKind::Story | WorkKind::Ticket
        ) {
            return Err(PulseError::validation(
                "shaping_receipt_subject_mismatch",
                format!("shaping can only be applied to epic/story/ticket: {owner_id}"),
            ));
        }

        let (receipt, receipt_hash) =
            crate::evidence::receipt::load_receipt(&self.repo_root, receipt_id)?;
        let payload = shaping_payload_for_apply(&receipt)?;
        if receipt.id != receipt_id {
            return Err(PulseError::validation(
                "shaping_receipt_hash_mismatch",
                "loaded receipt id does not match requested id",
            ));
        }
        if receipt.subject.id != owner_id || payload.owning_work.id != owner_id {
            return Err(PulseError::validation(
                "shaping_receipt_subject_mismatch",
                "shaping receipt subject must match the owning work",
            ));
        }

        // Idempotency: a pointer to the same receipt id+hash is unchanged even
        // when the expected revision is stale, after verifying a prior apply
        // event. Same id with a different hash is corruption.
        if let Some(current) = &node.shaping {
            if current.receipt.id == receipt_id {
                if current.receipt.hash != receipt_hash {
                    return Err(PulseError::validation(
                        "shaping_receipt_hash_mismatch",
                        "current shaping pointer references the same receipt id with a different hash",
                    ));
                }
                if self.has_shaping_applied_event(owner_id, receipt_id, &receipt_hash)? {
                    return Ok(MutationOutcome {
                        schema_version: 1,
                        code: "unchanged".to_string(),
                        status: MutationStatus::Unchanged,
                        value: node,
                    });
                }
            }
        }

        if node.revision != expected_revision {
            return Err(PulseError::CasConflict {
                subject: owner_id.to_string(),
                expected_revision,
                current_revision: node.revision,
            });
        }

        if let Some(expected_current) = expected_current_receipt {
            let current_id = node.shaping.as_ref().map(|s| s.receipt.id.as_str());
            if current_id != Some(expected_current) {
                return Err(PulseError::validation(
                    "shaping_expected_current_receipt_conflict",
                    "expected-current receipt does not match the node's current shaping receipt",
                ));
            }
        }

        // Shaping currentness is by contract_revision: the generic work
        // normal-revision binding is intentionally not a staleness signal here.
        if payload.owning_work.contract_revision != node.contract_revision {
            return Err(PulseError::validation(
                "shaping_receipt_stale",
                format!(
                    "shaping receipt binds contract_revision {} but owner is at {}",
                    payload.owning_work.contract_revision, node.contract_revision
                ),
            ));
        }
        let binding_codes = crate::evidence::receipt::content_source_binding_codes(
            &self.repo_root,
            &receipt.bindings,
            None,
        )?;
        if let Some(code) = binding_codes.first() {
            return Err(PulseError::validation(
                crate::evidence::receipt::code_to_static(code),
                "shaping receipt content/source bindings are not current",
            ));
        }
        if let Some(map) = &payload.map {
            verify_map_current(&self.repo_root, map)?;
        }

        let policy_report = crate::policy::load_authority_policy(&self.repo_root)?;
        let caller = crate::policy::parse_actor(&ctx.actor);
        crate::policy::authorize(&policy_report, &caller, &["shape.apply"])?;
        let approve_grant =
            crate::graph::shaping::materialization_approve_grant(&payload.materialization)?;
        crate::policy::authorize(
            &policy_report,
            &payload.approval.approved_by,
            &[approve_grant.as_str()],
        )?;

        let previous = node.shaping.clone();
        let pointer = ShapingPointer {
            receipt: ReceiptRef {
                id: receipt_id.to_string(),
                hash: receipt_hash.clone(),
            },
            map: payload.map.as_ref().map(|map| ShapingMapRef {
                path: map.path.clone(),
                revision: map.revision,
                content_hash: map.content_hash.clone(),
            }),
            applied_at: ctx.now,
            applied_by: ctx.actor.clone(),
        };
        node.shaping = Some(pointer.clone());
        node.revision += 1;
        node.updated_at = ctx.now;
        validate_node_contract_result(&node, ContractValidationMode::CanonicalStorage)?;
        let node_values = self
            .load_nodes_with_override(node.clone())?
            .into_values()
            .collect::<Vec<_>>();
        let edge_values = self
            .load_edges()?
            .iter()
            .map(|(_, e)| e.clone())
            .collect::<Vec<_>>();
        validate_graph(
            &self.repo_root,
            &self.manifest()?,
            &node_values,
            &edge_values,
        )
        .into_result()?;
        let after_bytes = to_canonical_bytes(&node)?;
        let affected_work: Vec<String> = std::iter::once(owner_id.to_string())
            .chain(payload.affected_work.iter().map(|w| w.id.clone()))
            .collect::<std::collections::BTreeSet<String>>()
            .into_iter()
            .collect();
        self.commit_mutation(
            "work.shaping.applied",
            ctx.actor,
            owner_id,
            json!({
                "owner_id": owner_id,
                "expected_revision": expected_revision,
                "new_revision": node.revision,
                "contract_revision": node.contract_revision,
                "previous_receipt": previous.as_ref().map(|p| &p.receipt),
                "receipt": {"id": receipt_id, "hash": receipt_hash},
                "map_revision": payload.map.as_ref().map(|m| m.revision),
                "materialization": payload.materialization,
                "affected_work": affected_work,
                "gate_coverage": ["owner_kind", "node_revision_cas", "expected_current_receipt", "receipt_identity", "receipt_integrity", "contract_revision_binding", "content_source_bindings", "map_currentness", "authority", "graph_integrity"]
            }),
            &path,
            FileState::Present {
                hash: hash_bytes(&before_bytes),
                revision: expected_revision,
            },
            FileState::Present {
                hash: hash_bytes(&after_bytes),
                revision: expected_revision + 1,
            },
            &after_bytes,
            ctx.now,
        )?;
        Ok(MutationOutcome {
            schema_version: 1,
            code: "applied".to_string(),
            status: MutationStatus::Updated,
            value: node,
        })
    }

    pub fn apply_shaping(
        &self,
        owner_id: &str,
        expected_revision: u64,
        receipt_id: &str,
        expected_current_receipt: Option<&str>,
        actor: String,
    ) -> PulseResult<MutationOutcome<Node>> {
        self.apply_shaping_with_context(
            owner_id,
            expected_revision,
            receipt_id,
            expected_current_receipt,
            OperationContext {
                actor,
                now: Utc::now(),
            },
        )
    }

    /// Clear the current shaping pointer with authority.
    ///
    /// Only the pointer is cleared; the historical receipt and any map content
    /// remain. Pointer-only mutation bumps the normal `revision` only, not the
    /// semantic `contract_revision`. Lifecycle status is not auto-mutated.
    pub fn invalidate_shaping_with_context(
        &self,
        owner_id: &str,
        expected_revision: u64,
        reason: String,
        ctx: OperationContext,
    ) -> PulseResult<MutationOutcome<Node>> {
        if reason.trim().is_empty() {
            return Err(PulseError::validation(
                "reason_required",
                "shaping invalidate requires a non-empty reason",
            ));
        }
        let _guard = WriteGuard::acquire(&self.repo_root)?;
        self.bootstrap_unlocked()?;
        recover_prepared_transactions(&self.repo_root)?;
        let path = self.node_path(owner_id);
        if !path.exists() {
            return Err(PulseError::NotFound {
                subject: owner_id.to_string(),
            });
        }
        let before_bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        let mut node: Node = serde_json::from_slice(&before_bytes)
            .map_err(|error| PulseError::json(&path, error))?;
        if node.revision != expected_revision {
            return Err(PulseError::CasConflict {
                subject: owner_id.to_string(),
                expected_revision,
                current_revision: node.revision,
            });
        }
        let previous = node.shaping.clone();
        if previous.is_none() {
            return Err(PulseError::validation(
                "shaping_receipt_missing",
                format!("work has no current shaping pointer to invalidate: {owner_id}"),
            ));
        }

        let policy_report = crate::policy::load_authority_policy(&self.repo_root)?;
        let caller = crate::policy::parse_actor(&ctx.actor);
        crate::policy::authorize(&policy_report, &caller, &["shape.invalidate"])?;

        node.shaping = None;
        node.revision += 1;
        node.updated_at = ctx.now;
        let node_values = self
            .load_nodes_with_override(node.clone())?
            .into_values()
            .collect::<Vec<_>>();
        let edge_values = self
            .load_edges()?
            .iter()
            .map(|(_, e)| e.clone())
            .collect::<Vec<_>>();
        validate_graph(
            &self.repo_root,
            &self.manifest()?,
            &node_values,
            &edge_values,
        )
        .into_result()?;
        let after_bytes = to_canonical_bytes(&node)?;
        self.commit_mutation(
            "work.shaping.invalidated",
            ctx.actor,
            owner_id,
            json!({
                "owner_id": owner_id,
                "expected_revision": expected_revision,
                "new_revision": node.revision,
                "contract_revision": node.contract_revision,
                "previous_receipt": previous.as_ref().map(|p| &p.receipt),
                "reason": reason,
                "gate_coverage": ["node_revision_cas", "authority", "graph_integrity"]
            }),
            &path,
            FileState::Present {
                hash: hash_bytes(&before_bytes),
                revision: expected_revision,
            },
            FileState::Present {
                hash: hash_bytes(&after_bytes),
                revision: expected_revision + 1,
            },
            &after_bytes,
            ctx.now,
        )?;
        Ok(MutationOutcome {
            schema_version: 1,
            code: "invalidated".to_string(),
            status: MutationStatus::Updated,
            value: node,
        })
    }

    pub fn invalidate_shaping(
        &self,
        owner_id: &str,
        expected_revision: u64,
        reason: String,
        actor: String,
    ) -> PulseResult<MutationOutcome<Node>> {
        self.invalidate_shaping_with_context(
            owner_id,
            expected_revision,
            reason,
            OperationContext {
                actor,
                now: Utc::now(),
            },
        )
    }

    pub fn show_shaping(&self, owner_id: &str) -> PulseResult<ShapingView> {
        let node = self.show_node(owner_id)?;
        Ok(ShapingView {
            schema_version: 1,
            code: "ok".to_string(),
            owner_id: node.id,
            revision: node.revision,
            contract_revision: node.contract_revision,
            shaping: node.shaping,
        })
    }

    fn has_shaping_applied_event(
        &self,
        owner_id: &str,
        receipt_id: &str,
        receipt_hash: &str,
    ) -> PulseResult<bool> {
        let events_dir = self.repo_root.join(".pulse/events");
        let Ok(date_dirs) = fs::read_dir(events_dir) else {
            return Ok(false);
        };
        for date_dir in date_dirs.flatten() {
            let Ok(entries) = fs::read_dir(date_dir.path()) else {
                continue;
            };
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let Ok(event) = storage::read_json::<EventEnvelope>(&entry.path()) else {
                    continue;
                };
                if event.event_type == "work.shaping.applied"
                    && event.subject.id == owner_id
                    && event
                        .payload
                        .get("receipt")
                        .and_then(|v| v.get("id"))
                        .and_then(|v| v.as_str())
                        == Some(receipt_id)
                    && event
                        .payload
                        .get("receipt")
                        .and_then(|v| v.get("hash"))
                        .and_then(|v| v.as_str())
                        == Some(receipt_hash)
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

pub(crate) fn shaping_payload_for_apply(
    receipt: &crate::evidence::model::ReceiptEnvelope,
) -> PulseResult<crate::evidence::model::ShapingValidationPayload> {
    use crate::evidence::model::{ReceiptKind, ReceiptPayload, ReceiptResult};
    if receipt.kind != ReceiptKind::ShapingValidation {
        return Err(PulseError::validation(
            "shaping_receipt_version_ineligible",
            "shaping apply requires a shaping_validation receipt",
        ));
    }
    if receipt.result != ReceiptResult::Passed {
        return Err(PulseError::validation(
            "shaping_receipt_version_ineligible",
            "only a passed shaping receipt can support a current shaping pointer",
        ));
    }
    match &receipt.payload {
        ReceiptPayload::ShapingValidation(payload) => {
            if payload.payload_version != 1 {
                return Err(PulseError::validation(
                    "shaping_receipt_version_ineligible",
                    "shaping apply requires current payload version 1",
                ));
            }
            Ok(payload.clone())
        }
        _ => Err(PulseError::validation(
            "shaping_receipt_version_ineligible",
            "receipt payload does not match shaping_validation",
        )),
    }
}

/// Verify a shaping map snapshot's content hash is current under the repository
/// fence. The map is an optional human-facing index bound by path/revision/hash.
pub(crate) fn verify_map_current(
    repo_root: &Path,
    map: &crate::evidence::model::ShapingMapSnapshot,
) -> PulseResult<()> {
    let rel = crate::storage::safe_repo_relative(&map.path)?;
    let path = repo_root.join(rel);
    match fs::read(&path) {
        Ok(bytes) if hash_bytes(&bytes) == map.content_hash => Ok(()),
        Ok(_) => Err(PulseError::validation(
            "shaping_map_content_stale",
            "shaping map content no longer matches the receipt binding",
        )),
        Err(_) => Err(PulseError::validation(
            "shaping_map_missing",
            "shaping receipt references a map that no longer exists",
        )),
    }
}
