//! Atomic claim pipeline (P2S2-I8).
//!
//! Implements `JsonGraphStore::claim_work` — the full atomic pipeline that
//! revalidates packet preconditions, acquires an exclusive lease, binds a
//! workspace, matches capabilities, evaluates the prepared-assignment
//! lifecycle gate and commits one multi-target transaction containing all
//! runtime records + graph node mutation + event.
//!
//! Ownership: this module is the sanctioned cross-domain composition layer.
//! It imports graph store, policy, docs, evidence, event and storage
//! transaction primitives. Graph store MUST NOT import docs/policy/evidence
//! services directly (architecture guards enforce this boundary).
//!
//! See `proposals/phase2-slice2-atomic-reservation-workspace-binding.md`
//! for the full design contract.

use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde_json::json;

use crate::assignment::{
    self, AssignmentLeaseAssignee, AssignmentLeaseRecordV1, AssignmentLeaseSource,
    AssignmentLeaseSubject, AssignmentLeaseSummary, AssignmentLifecycle, AssignmentSubjectSnapshot,
    AssignmentTombstoneV1, AssignmentTransaction, AssignmentWorkspaceRecordV1,
    AssignmentWorkspaceSummary, CapabilityInventoryV1, CapabilityMatchReport,
    PreparedAssignmentRecordV1, PreparedAssignmentV1, RevalidatedSnapshot, WorkspaceCleanupPolicy,
    WorkspaceSubjectRef, ASSIGNMENT_SCHEMA_VERSION, LEASE_KIND_IMPLEMENTATION,
    LEASE_SCHEMA_VERSION, LEASE_STATE_PREPARED, LIFECYCLE_GATE_PROFILE, LIFECYCLE_READY_TO_ACTIVE,
    MAX_TTL_SECONDS, MIN_TTL_SECONDS, PREPARED_ASSIGNMENT_PROFILE, TOMBSTONE_SCHEMA_VERSION,
    TOMBSTONE_STATE_EXPIRED, TOMBSTONE_STATE_RELEASED, TOMBSTONE_STATE_STALE,
    WORKSPACE_MODE_IN_PLACE, WORKSPACE_MODE_ISOLATED, WORKSPACE_SCHEMA_VERSION,
    WORKSPACE_STATE_BOUND, WORKSPACE_STATE_RELEASED, WORKSPACE_STATE_STALE,
};
use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::event::{new_event_id, EventEnvelope};
use crate::graph::node::{Node, NodeStatus};
use crate::graph::store::JsonGraphStore;
use crate::kernel::assignment_store;
use crate::kernel::lifecycle::PreparedAssignmentGateContext;
use crate::storage::transaction::{
    commit_prepared_multi_target_transaction, new_transaction_id, prepare_multi_target_transaction,
    recover_prepared_transactions, FileState, MultiTargetTransactionIntent, TransactionTarget,
};
use crate::storage::WriteGuard;
use crate::{PulseError, PulseResult};

// ---------------------------------------------------------------------------
// ClaimArgs
// ---------------------------------------------------------------------------

/// Arguments for `JsonGraphStore::claim_work`.
#[derive(Debug, Clone)]
pub struct ClaimArgs {
    /// Ticket ID to claim.
    pub ticket_id: String,
    /// Authorized principal performing the Pulse mutation. Checked for
    /// `work.assignment.prepare`.
    pub actor: String,
    /// Local principal string that will own the lease.
    pub assignee: String,
    /// Pre-loaded bytes of the capability inventory JSON file.
    pub capability_inventory_bytes: Vec<u8>,
    /// TTL for the lease in seconds. Defaults to `DEFAULT_TTL_SECONDS`.
    /// Values outside [`MIN_TTL_SECONDS`, `MAX_TTL_SECONDS`] are rejected.
    pub ttl_seconds: u64,
    /// Optional workspace mode override. `None` means `auto`:
    /// isolated when packet requires isolated, otherwise in-place.
    pub workspace_mode: Option<String>,
}

// ---------------------------------------------------------------------------
// ClaimWorkOutcome
// ---------------------------------------------------------------------------

/// Outcome of a successful claim operation.
#[derive(Debug, Clone)]
pub struct ClaimWorkOutcome {
    /// The committed `PreparedAssignmentV1` (matches committed bytes).
    pub prepared_assignment: PreparedAssignmentV1,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Allowed workspace mode values for claim.
const VALID_WORKSPACE_MODES: &[&str] = &["auto", "in_place", "isolated_worktree"];

// ---------------------------------------------------------------------------
// claim_work implementation
// ---------------------------------------------------------------------------

impl JsonGraphStore {
    /// Atomic claim pipeline: revalidate, acquire lease, bind workspace, match
    /// capabilities, transition ready -> active, commit runtime records + node
    /// + event, return `PreparedAssignmentV1`.
    pub fn claim_work(&self, args: ClaimArgs) -> PulseResult<ClaimWorkOutcome> {
        // Step 0: Validate enrollment before any lock or runtime directory
        // creation (preserve/no-bootstrap).
        assignment_store::check_enrolled(&self.repo_root)?;

        // Step 1: Validate TTL. The claim contract treats the published
        // bounds as hard preconditions rather than silently changing the
        // caller's requested lease duration.
        if !(MIN_TTL_SECONDS..=MAX_TTL_SECONDS).contains(&args.ttl_seconds) {
            return Err(PulseError::validation(
                "assignment_ttl_out_of_range",
                format!(
                    "ttl_seconds {} is outside allowed range {}..={}",
                    args.ttl_seconds, MIN_TTL_SECONDS, MAX_TTL_SECONDS
                ),
            ));
        }
        let ttl_seconds = args.ttl_seconds;

        // Step 1b: Validate workspace mode.
        if let Some(ref mode) = args.workspace_mode {
            if !VALID_WORKSPACE_MODES.contains(&mode.as_str()) {
                return Err(PulseError::validation(
                    "assignment_workspace_mode_unsupported",
                    format!(
                        "unsupported workspace mode {mode:?}; valid values: {}",
                        VALID_WORKSPACE_MODES.join(", ")
                    ),
                ));
            }
        }

        // Step 1c: Parse and validate capability inventory bytes.
        let inventory = CapabilityInventoryV1::from_json_bytes(&args.capability_inventory_bytes)?;

        let impl_args = ClaimImplArgs {
            ticket_id: args.ticket_id,
            actor: args.actor,
            assignee: args.assignee,
            inventory,
            ttl_seconds,
            workspace_mode: args.workspace_mode,
        };

        // Attempt the claim with at most one docs-cache refresh retry.
        let max_attempts = 2;
        for attempt in 1..=max_attempts {
            // Step 2: Acquire repository WriteGuard.
            let guard = WriteGuard::acquire(&self.repo_root)?;

            // Step 3: Run the inner pipeline under the fence.
            match self.claim_work_inner(&impl_args) {
                Ok(outcome) => {
                    drop(guard);
                    return Ok(outcome);
                }
                Err(error) => {
                    let needs_refresh = error.code() == "work_packet_docs_cache_needs_refresh";
                    drop(guard);

                    if needs_refresh && attempt < max_attempts {
                        // Release fence, refresh docs cache, retry.
                        let index_opts = crate::docs::index::IndexOptions {
                            changed: false,
                            rebuild: false,
                            check: false,
                            include_draft: false,
                            include_stale: false,
                        };
                        if let Err(refresh_err) =
                            crate::docs::index::build_search_cache(&self.repo_root, index_opts)
                        {
                            return Err(PulseError::validation(
                                "assignment_docs_cache_refresh_failed",
                                format!("docs cache refresh failed: {refresh_err}",),
                            ));
                        }
                        // Continue loop to reacquire and retry.
                        continue;
                    }
                    return Err(error);
                }
            }
        }

        // Should not reach here.
        Err(PulseError::validation(
            "assignment_internal_error",
            "claim pipeline exhausted retry attempts",
        ))
    }

    /// Full claim pipeline under the held repository fence.
    /// Caller must hold the WriteGuard.
    fn claim_work_inner(&self, args: &ClaimImplArgs) -> PulseResult<ClaimWorkOutcome> {
        // ------------------------------------------------------------------
        // 1. Recovery
        // ------------------------------------------------------------------
        recover_prepared_transactions(&self.repo_root)?;
        self.require_existing_workgraph_unlocked()?;

        // ------------------------------------------------------------------
        // 2. Authorize the claim actor for work.assignment.prepare
        // ------------------------------------------------------------------
        let policy_report = crate::policy::load_authority_policy(&self.repo_root)?;
        let caller = crate::policy::parse_actor(&args.actor);
        crate::policy::authorize(&policy_report, &caller, &["work.assignment.prepare"])?;

        // ------------------------------------------------------------------
        // 3. Reject live exclusive lease for subject
        // ------------------------------------------------------------------
        let live_lease =
            assignment_store::find_live_lease_for_subject(&self.repo_root, &args.ticket_id)?;
        if let Some(lease_id) = live_lease {
            return Err(PulseError::validation(
                "assignment_live_lease_exists",
                format!(
                    "live exclusive lease {lease_id} exists for {}",
                    args.ticket_id
                ),
            ));
        }

        // ------------------------------------------------------------------
        // 4. Build fresh WorkPacketV1 under the held fence
        // ------------------------------------------------------------------
        let packet = self.work_packet_under_fence(&args.ticket_id)?;

        // Verify packet is a reservation candidate with dispatch_authorized=false.
        if !packet.dispatch.reservation_candidate {
            return Err(PulseError::validation(
                "assignment_packet_invalid",
                format!(
                    "packet for {} has reservation_candidate=false; cannot claim",
                    args.ticket_id
                ),
            ));
        }
        if packet.dispatch.dispatch_authorized {
            return Err(PulseError::validation(
                "assignment_packet_invalid",
                format!(
                    "packet for {} already has dispatch_authorized=true; cannot claim fresh",
                    args.ticket_id
                ),
            ));
        }

        // ------------------------------------------------------------------
        // 5. Load subject node for revision/status verification
        // ------------------------------------------------------------------
        let node_path = self.node_path(&args.ticket_id);
        let before_bytes = fs::read(&node_path).map_err(|e| PulseError::io(&node_path, e))?;
        let node: Node =
            serde_json::from_slice(&before_bytes).map_err(|e| PulseError::json(&node_path, e))?;

        if node.status != NodeStatus::Ready {
            return Err(PulseError::validation(
                "assignment_subject_not_ready",
                format!("subject {} status is {:?}, not Ready", node.id, node.status),
            ));
        }

        let expected_revision = node.revision;

        // ------------------------------------------------------------------
        // 6. Load and match capability inventory
        // ------------------------------------------------------------------
        // The inventory was already parsed in the outer method. Now validate
        // principal and match required capabilities.
        args.inventory.validate_principal(&args.assignee)?;

        let workspace_mode_for_cap = self.resolve_workspace_mode_lookup(
            &packet.workspace.required_strategy,
            args.workspace_mode.as_deref(),
        )?;

        let capability_match = args.inventory.match_required(
            &args.assignee,
            &packet.capabilities.required,
            Some(&workspace_mode_for_cap),
        )?;

        if capability_match.status != assignment::CAP_MATCH_MATCHED {
            return Err(PulseError::validation(
                "assignment_capability_missing",
                format!(
                    "required capabilities missing: {}",
                    capability_match.missing.join(", ")
                ),
            ));
        }

        // ------------------------------------------------------------------
        // 7. Resolve workspace mode and bind/revalidate source workspace
        // ------------------------------------------------------------------
        let workspace_mode = self.resolve_workspace_mode_lookup(
            &packet.workspace.required_strategy,
            args.workspace_mode.as_deref(),
        )?;

        // Build source & repository info from packet.
        let repository_id = packet.source.repository_id.clone();
        let source_commit = packet.source.commit.clone();

        // Generate stable IDs before binding so all runtime records, event and
        // response share one identity set. Use sanitized workspace IDs for a
        // safe repository-relative worktree path.
        let now: DateTime<Utc> = Utc::now();
        let lease_id = format!("lease_{}", ulid::Ulid::new());
        let workspace_id = crate::workspace::generate_workspace_id(&args.ticket_id);
        let prepared_assignment_id = format!("pa_{}", ulid::Ulid::new());
        let transaction_id = new_transaction_id();

        // Bind the workspace before writing any runtime records. In-place mode
        // revalidates HEAD/cleanliness at the root; isolated mode creates and
        // validates the linked worktree, with cleanup owned by workspace.rs.
        let runtime_workspaces_root = self.repo_root.join(".pulse/runtime/workspaces");
        let workspace_binding = match workspace_mode.as_str() {
            WORKSPACE_MODE_IN_PLACE => {
                let binding = crate::workspace::bind_in_place(
                    &self.repo_root,
                    &source_commit,
                    &repository_id,
                )?;
                WorkspaceBindingSnapshot {
                    path: PathBuf::from("."),
                    repository_id: binding.repository_id,
                    base_commit: binding.base_commit,
                    head_commit: binding.head_commit,
                    cleanliness: binding.cleanliness,
                }
            }
            WORKSPACE_MODE_ISOLATED => {
                let binding = crate::workspace::create_isolated_worktree(
                    &self.repo_root,
                    &runtime_workspaces_root,
                    &workspace_id,
                    &source_commit,
                    &repository_id,
                )?;
                WorkspaceBindingSnapshot {
                    path: binding.path,
                    repository_id: binding.repository_id,
                    base_commit: binding.base_commit,
                    head_commit: binding.head_commit,
                    cleanliness: binding.cleanliness,
                }
            }
            other => {
                return Err(PulseError::validation(
                    "assignment_workspace_mode_unsupported",
                    format!("unsupported workspace mode {other:?}"),
                ));
            }
        };
        let workspace_path = self.workspace_record_path_string(&workspace_binding.path)?;

        // Re-scan lease/source immediately before transaction planning so a
        // stale preflight cannot authorize over a changed repository state.
        if let Some(lease_id) =
            assignment_store::find_live_lease_for_subject(&self.repo_root, &args.ticket_id)?
        {
            self.cleanup_bound_workspace_on_error(&workspace_mode, &workspace_binding.path)?;
            return Err(PulseError::validation(
                "assignment_live_lease_exists",
                format!(
                    "live exclusive lease {lease_id} exists for {}",
                    args.ticket_id
                ),
            ));
        }
        if let Err(error) =
            crate::workspace::bind_in_place(&self.repo_root, &source_commit, &repository_id)
        {
            self.cleanup_bound_workspace_on_error(&workspace_mode, &workspace_binding.path)?;
            return Err(error);
        }

        // Compute timestamps.
        let issued_at = now.to_rfc3339();
        let expires_at = (now + chrono::Duration::seconds(args.ttl_seconds as i64)).to_rfc3339();

        // ------------------------------------------------------------------
        // 8. Create lease, workspace, prepared records in memory
        // ------------------------------------------------------------------
        let lease_record = self.build_lease_record(
            &lease_id,
            &args.ticket_id,
            &node,
            &args.assignee,
            &args.actor,
            &issued_at,
            &expires_at,
            args.ttl_seconds,
            &workspace_id,
            &prepared_assignment_id,
            &packet.packet_fingerprint,
            &packet.snapshot.readiness_fingerprint,
            &capability_match.inventory_identity,
            &repository_id,
            &source_commit,
        )?;

        let workspace_record = self.build_workspace_record(
            &workspace_id,
            &lease_id,
            &prepared_assignment_id,
            &args.ticket_id,
            &node,
            &workspace_mode,
            &workspace_path,
            &workspace_binding,
            &now,
        )?;

        // Generate event/transaction IDs early so every record, event and
        // response share one final identity/fingerprint set.
        let event_id = new_event_id();
        let event_file_path = self
            .repo_root
            .join(".pulse/events")
            .join(now.format("%Y-%m-%d").to_string())
            .join(format!("{event_id}.json"));
        let mut committed_targets = vec![
            format!(".pulse/runtime/assignment/leases/{lease_id}.json"),
            format!(".pulse/runtime/assignment/workspaces/{workspace_id}.json"),
            format!(".pulse/runtime/assignment/prepared/{prepared_assignment_id}.json"),
            self.rel_path(&node_path).to_string_lossy().to_string(),
        ];
        committed_targets.sort();
        let event_path_rel = event_file_path
            .strip_prefix(&self.repo_root)
            .unwrap_or(&event_file_path)
            .to_string_lossy()
            .to_string();
        let final_transaction = AssignmentTransaction {
            transaction_id: transaction_id.clone(),
            committed_targets,
            event_path: event_path_rel,
            recovery_state: "complete".to_string(),
        };

        // ------------------------------------------------------------------
        // 9. Build the PreparedAssignmentV1 response wrapper
        // ------------------------------------------------------------------

        let lifecycle = AssignmentLifecycle {
            transition: LIFECYCLE_READY_TO_ACTIVE.to_string(),
            gate_profile: LIFECYCLE_GATE_PROFILE.to_string(),
            gate_status: "passed".to_string(),
            expected_revision,
            new_revision: expected_revision + 1,
            event_id: event_id.clone(),
        };

        let graph_fingerprint_before = self.graph_fingerprint_current_unlocked()?;
        let lease_summary = build_lease_summary(&lease_record);
        let workspace_summary = build_workspace_summary(&workspace_record, &lease_id);

        let revalidated_snapshot = RevalidatedSnapshot {
            graph_fingerprint: graph_fingerprint_before.clone(),
            readiness_profile: packet.snapshot.readiness_profile.clone(),
            readiness_fingerprint: packet.snapshot.readiness_fingerprint.clone(),
            authority_policy_fingerprint: packet.snapshot.authority_policy_fingerprint.clone(),
            docs_registry_fingerprint: packet.snapshot.docs_registry_fingerprint.clone(),
            docs_index_fingerprint: packet.snapshot.docs_index_fingerprint.clone(),
            source_commit: packet.snapshot.source_commit.clone(),
            source_cleanliness: packet.source.cleanliness.clone(),
            repository_id: repository_id.clone(),
        };

        let mut prepared_v1 = PreparedAssignmentV1 {
            schema_version: ASSIGNMENT_SCHEMA_VERSION,
            profile: PREPARED_ASSIGNMENT_PROFILE.to_string(),
            code: "prepared_assignment".to_string(),
            prepared_assignment_id: prepared_assignment_id.clone(),
            subject: AssignmentSubjectSnapshot {
                id: args.ticket_id.clone(),
                kind: node.kind.as_str().to_string(),
                revision_before: expected_revision,
                revision_after: expected_revision + 1,
                contract_revision: node.contract_revision,
                status_before: "ready".to_string(),
                status_after: "active".to_string(),
            },
            packet: packet.clone(),
            packet_fingerprint: packet.packet_fingerprint.clone(),
            revalidated_snapshot,
            lease: lease_summary,
            workspace: workspace_summary,
            capability_match: capability_match.clone(),
            lifecycle: lifecycle.clone(),
            dispatch: assignment::AssignmentDispatch::default(),
            transaction: final_transaction.clone(),
            prepared_assignment_fingerprint: String::new(),
            reason_codes: vec![],
        };
        prepared_v1.prepared_assignment_fingerprint = prepared_v1.compute_fingerprint()?;

        let mut prepared_record = self.build_prepared_assignment_record(
            &prepared_assignment_id,
            &args.ticket_id,
            &node,
            &packet,
            &lease_record,
            &workspace_record,
            &capability_match,
            &packet.snapshot,
            expected_revision,
            &event_id,
        )?;
        prepared_record.transaction = final_transaction.clone();
        prepared_record.prepared_assignment_fingerprint = prepared_record.compute_fingerprint()?;

        // ------------------------------------------------------------------
        // 10. Evaluate prepared-assignment lifecycle gate (no commit)
        // ------------------------------------------------------------------
        let gate_ctx = PreparedAssignmentGateContext {
            now,
            prepared: &prepared_record,
            lease: &lease_record,
            workspace: &workspace_record,
        };
        let gate_plan = self.evaluate_prepared_assignment_gate_for_active(
            &args.ticket_id,
            expected_revision,
            gate_ctx,
        )?;
        prepared_v1.revalidated_snapshot.graph_fingerprint =
            gate_plan.graph_fingerprint_before.clone();

        // ------------------------------------------------------------------
        // 11. Build event payload
        // ------------------------------------------------------------------
        let event_payload = json!({
            "from": "ready",
            "to": "active",
            "expected_revision": expected_revision,
            "lease_id": lease_id,
            "workspace_id": workspace_id,
            "prepared_assignment_id": prepared_assignment_id,
            "packet_fingerprint": packet.packet_fingerprint,
            "readiness_fingerprint": packet.snapshot.readiness_fingerprint,
            "source_commit": source_commit,
            "capability_inventory_identity": capability_match.inventory_identity,
            "graph_fingerprint_before": gate_plan.graph_fingerprint_before,
            "graph_fingerprint_after": gate_plan.graph_fingerprint_after,
            "gate_profile": LIFECYCLE_GATE_PROFILE,
            "gate_status": "passed",
        });

        // ------------------------------------------------------------------
        // 12. Prepare multi-target transaction
        // ------------------------------------------------------------------
        let event = EventEnvelope::new(
            event_id.clone(),
            "work.assignment.prepared",
            &args.actor,
            &args.ticket_id,
            event_payload.clone(),
            now,
        );

        // Recompute fingerprints after the gate confirms the final graph
        // fingerprint and before committing the prepared runtime bytes.
        prepared_record.prepared_assignment_fingerprint = prepared_record.compute_fingerprint()?;
        prepared_v1.prepared_assignment_fingerprint = prepared_v1.compute_fingerprint()?;
        if prepared_record.prepared_assignment_fingerprint
            != prepared_v1.prepared_assignment_fingerprint
        {
            self.cleanup_bound_workspace_on_error(&workspace_mode, &workspace_binding.path)?;
            return Err(PulseError::validation(
                "prepared_assignment_fingerprint_mismatch",
                "persisted prepared record and response fingerprint differ",
            ));
        }

        // Re-scan lease/source after all planning and before writing the
        // transaction intent. This is redundant under the repository fence but
        // documents/enforces the claim choreography boundary.
        if let Some(lease_id) =
            assignment_store::find_live_lease_for_subject(&self.repo_root, &args.ticket_id)?
        {
            self.cleanup_bound_workspace_on_error(&workspace_mode, &workspace_binding.path)?;
            return Err(PulseError::validation(
                "assignment_live_lease_exists",
                format!(
                    "live exclusive lease {lease_id} exists for {}",
                    args.ticket_id
                ),
            ));
        }
        if let Err(error) =
            crate::workspace::bind_in_place(&self.repo_root, &source_commit, &repository_id)
        {
            self.cleanup_bound_workspace_on_error(&workspace_mode, &workspace_binding.path)?;
            return Err(error);
        }

        // Canonical bytes for each target.
        let lease_bytes = to_canonical_bytes(&lease_record)?;
        let workspace_bytes = to_canonical_bytes(&workspace_record)?;
        let prepared_bytes = to_canonical_bytes(&prepared_record)?;
        let node_after_bytes = gate_plan.after_bytes.clone();

        // Build file states.
        let lease_file_path = assignment_store::lease_path(&self.repo_root, &lease_id)?;
        let workspace_file_path = assignment_store::workspace_path(&self.repo_root, &workspace_id)?;
        let prepared_file_path =
            assignment_store::prepared_assignment_path(&self.repo_root, &prepared_assignment_id)?;

        let lease_before = FileState::Absent;
        let lease_after = FileState::Present {
            hash: hash_bytes(&lease_bytes),
            revision: 0,
        };
        let workspace_before = FileState::Absent;
        let workspace_after = FileState::Present {
            hash: hash_bytes(&workspace_bytes),
            revision: 0,
        };
        let prepared_before = FileState::Absent;
        let prepared_after = FileState::Present {
            hash: hash_bytes(&prepared_bytes),
            revision: 0,
        };
        let node_before = FileState::Present {
            hash: hash_bytes(&before_bytes),
            revision: expected_revision,
        };
        let node_after = FileState::Present {
            hash: hash_bytes(&node_after_bytes),
            revision: expected_revision + 1,
        };

        let targets: Vec<TransactionTarget> = vec![
            TransactionTarget::new(lease_file_path, lease_before, lease_after, &lease_bytes),
            TransactionTarget::new(
                workspace_file_path,
                workspace_before,
                workspace_after,
                &workspace_bytes,
            ),
            TransactionTarget::new(
                prepared_file_path,
                prepared_before,
                prepared_after,
                &prepared_bytes,
            ),
            TransactionTarget::new(
                node_path.clone(),
                node_before,
                node_after,
                &node_after_bytes,
            ),
        ];

        let intent = MultiTargetTransactionIntent::prepared_with_transaction_id(
            transaction_id,
            event_id,
            "work.assignment.prepared",
            &args.actor,
            targets,
            event_file_path.clone(),
            serde_json::to_value(&event)?,
        )?;

        let prepared_txn = prepare_multi_target_transaction(&self.repo_root, intent)?;

        // ------------------------------------------------------------------
        // 13. Commit the multi-target transaction
        // ------------------------------------------------------------------
        commit_prepared_multi_target_transaction(&prepared_txn, self.failpoint)?;

        // ------------------------------------------------------------------
        // 14. Reload and validate committed runtime record before returning
        // ------------------------------------------------------------------
        let committed_record =
            assignment_store::load_prepared(&self.repo_root, &prepared_assignment_id)?;
        if committed_record != prepared_record {
            return Err(PulseError::validation(
                "prepared_assignment_committed_record_mismatch",
                "committed prepared assignment record does not match planned bytes",
            ));
        }
        if committed_record.prepared_assignment_fingerprint
            != prepared_v1.prepared_assignment_fingerprint
        {
            return Err(PulseError::validation(
                "prepared_assignment_fingerprint_mismatch",
                "committed prepared assignment fingerprint does not match response",
            ));
        }

        Ok(ClaimWorkOutcome {
            prepared_assignment: prepared_v1,
        })
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Resolve the effective workspace mode from packet requirement + CLI request.
fn resolve_workspace_mode(required_strategy: &str, requested: Option<&str>) -> PulseResult<String> {
    match requested {
        Some("auto") | None => {
            // Auto: follow packet requirement.
            if required_strategy == "isolated_worktree_required" {
                Ok(WORKSPACE_MODE_ISOLATED.to_string())
            } else {
                Ok(WORKSPACE_MODE_IN_PLACE.to_string())
            }
        }
        Some("in_place") => {
            if required_strategy == "isolated_worktree_required" {
                Err(PulseError::validation(
                    "assignment_workspace_worktree_required",
                    "packet requires isolated worktree; in_place is not allowed",
                ))
            } else {
                Ok(WORKSPACE_MODE_IN_PLACE.to_string())
            }
        }
        Some("isolated_worktree") => Ok(WORKSPACE_MODE_ISOLATED.to_string()),
        Some(other) => Err(PulseError::validation(
            "assignment_workspace_mode_unsupported",
            format!("unsupported workspace mode {other:?}"),
        )),
    }
}

#[derive(Debug, Clone)]
struct WorkspaceBindingSnapshot {
    path: PathBuf,
    repository_id: String,
    base_commit: String,
    head_commit: String,
    cleanliness: String,
}

// ---------------------------------------------------------------------------
// Record builders
// ---------------------------------------------------------------------------

impl JsonGraphStore {
    #[allow(clippy::too_many_arguments)]
    fn build_lease_record(
        &self,
        lease_id: &str,
        ticket_id: &str,
        node: &Node,
        assignee: &str,
        actor: &str,
        issued_at: &str,
        expires_at: &str,
        ttl_seconds: u64,
        workspace_id: &str,
        prepared_assignment_id: &str,
        packet_fingerprint: &str,
        readiness_fingerprint: &str,
        capability_inventory_identity: &str,
        repository_id: &str,
        source_commit: &str,
    ) -> PulseResult<AssignmentLeaseRecordV1> {
        let record = AssignmentLeaseRecordV1 {
            schema_version: LEASE_SCHEMA_VERSION,
            lease_id: lease_id.to_string(),
            kind: LEASE_KIND_IMPLEMENTATION.to_string(),
            subject: AssignmentLeaseSubject {
                kind: node.kind.as_str().to_string(),
                id: ticket_id.to_string(),
                revision: node.revision,
                contract_revision: node.contract_revision,
                status_at_claim: "ready".to_string(),
            },
            assignee: AssignmentLeaseAssignee {
                principal: assignee.to_string(),
            },
            issued_by: actor.to_string(),
            issued_at: issued_at.to_string(),
            expires_at: expires_at.to_string(),
            ttl_seconds,
            state: LEASE_STATE_PREPARED.to_string(),
            packet_fingerprint: packet_fingerprint.to_string(),
            readiness_fingerprint: readiness_fingerprint.to_string(),
            workspace_id: workspace_id.to_string(),
            prepared_assignment_id: prepared_assignment_id.to_string(),
            capability_inventory_identity: capability_inventory_identity.to_string(),
            source: AssignmentLeaseSource {
                repository_id: repository_id.to_string(),
                base_commit: source_commit.to_string(),
            },
        };
        Ok(record)
    }

    #[allow(clippy::too_many_arguments)]
    fn build_workspace_record(
        &self,
        workspace_id: &str,
        lease_id: &str,
        prepared_assignment_id: &str,
        ticket_id: &str,
        node: &Node,
        mode: &str,
        path: &str,
        binding: &WorkspaceBindingSnapshot,
        now: &DateTime<Utc>,
    ) -> PulseResult<AssignmentWorkspaceRecordV1> {
        let record = AssignmentWorkspaceRecordV1 {
            schema_version: WORKSPACE_SCHEMA_VERSION,
            workspace_id: workspace_id.to_string(),
            lease_id: lease_id.to_string(),
            prepared_assignment_id: prepared_assignment_id.to_string(),
            subject: WorkspaceSubjectRef {
                kind: node.kind.as_str().to_string(),
                id: ticket_id.to_string(),
                revision: node.revision,
            },
            mode: mode.to_string(),
            path: path.to_string(),
            repository_id: binding.repository_id.clone(),
            base_commit: binding.base_commit.clone(),
            head_commit_at_bind: binding.head_commit.clone(),
            cleanliness_at_bind: binding.cleanliness.clone(),
            state: WORKSPACE_STATE_BOUND.to_string(),
            created_at: now.to_rfc3339(),
            released_at: None,
            cleanup: WorkspaceCleanupPolicy {
                policy: "safe_remove_if_clean_at_base".to_string(),
                status: "not_requested".to_string(),
            },
        };
        Ok(record)
    }

    #[allow(clippy::too_many_arguments)]
    fn build_prepared_assignment_record(
        &self,
        prepared_assignment_id: &str,
        ticket_id: &str,
        node: &Node,
        packet: &crate::work_packet::WorkPacketV1,
        lease: &AssignmentLeaseRecordV1,
        workspace: &AssignmentWorkspaceRecordV1,
        cap_match: &CapabilityMatchReport,
        packet_snapshot: &crate::work_packet::SnapshotReport,
        expected_revision: u64,
        event_id: &str,
    ) -> PulseResult<PreparedAssignmentRecordV1> {
        let revalidated = RevalidatedSnapshot {
            graph_fingerprint: packet_snapshot.graph_fingerprint.clone(),
            readiness_profile: packet_snapshot.readiness_profile.clone(),
            readiness_fingerprint: packet_snapshot.readiness_fingerprint.clone(),
            authority_policy_fingerprint: packet_snapshot.authority_policy_fingerprint.clone(),
            docs_registry_fingerprint: packet_snapshot.docs_registry_fingerprint.clone(),
            docs_index_fingerprint: packet_snapshot.docs_index_fingerprint.clone(),
            source_commit: packet_snapshot.source_commit.clone(),
            source_cleanliness: packet.source.cleanliness.clone(),
            repository_id: packet.source.repository_id.clone(),
        };
        let lease_summary = build_lease_summary(lease);
        let workspace_summary = build_workspace_summary(workspace, &lease.lease_id);
        let lifecycle = AssignmentLifecycle {
            transition: LIFECYCLE_READY_TO_ACTIVE.to_string(),
            gate_profile: LIFECYCLE_GATE_PROFILE.to_string(),
            gate_status: "passed".to_string(),
            expected_revision,
            new_revision: expected_revision + 1,
            event_id: event_id.to_string(),
        };
        let record = PreparedAssignmentRecordV1 {
            schema_version: ASSIGNMENT_SCHEMA_VERSION,
            profile: PREPARED_ASSIGNMENT_PROFILE.to_string(),
            code: "prepared_assignment".to_string(),
            prepared_assignment_id: prepared_assignment_id.to_string(),
            subject: AssignmentSubjectSnapshot {
                id: ticket_id.to_string(),
                kind: node.kind.as_str().to_string(),
                revision_before: expected_revision,
                revision_after: expected_revision + 1,
                contract_revision: node.contract_revision,
                status_before: "ready".to_string(),
                status_after: "active".to_string(),
            },
            packet_fingerprint: packet.packet_fingerprint.clone(),
            revalidated_snapshot: revalidated,
            lease: lease_summary,
            workspace: workspace_summary,
            capability_match: cap_match.clone(),
            lifecycle,
            dispatch: assignment::AssignmentDispatch::default(),
            transaction: AssignmentTransaction {
                transaction_id: String::new(),
                committed_targets: vec![],
                event_path: String::new(),
                recovery_state: "prepare".to_string(),
            },
            prepared_assignment_fingerprint: String::new(),
            reason_codes: vec![],
        };
        Ok(record)
    }

    fn resolve_workspace_mode_lookup(
        &self,
        required_strategy: &str,
        requested: Option<&str>,
    ) -> PulseResult<String> {
        resolve_workspace_mode(required_strategy, requested)
    }

    fn workspace_record_path_string(&self, path: &std::path::Path) -> PulseResult<String> {
        if path == std::path::Path::new(".") {
            return Ok(".".to_string());
        }
        let relative = path.strip_prefix(&self.repo_root).map_err(|_| {
            PulseError::validation(
                "assignment_workspace_path_invalid",
                format!(
                    "workspace path {} is not under repository root {}",
                    path.display(),
                    self.repo_root.display()
                ),
            )
        })?;
        let relative = relative.to_string_lossy().to_string();
        crate::storage::safe_repo_relative(&relative).map_err(|error| {
            PulseError::validation(
                "assignment_workspace_path_invalid",
                format!("workspace path must be safe repository-relative: {error}"),
            )
        })?;
        Ok(relative)
    }

    fn cleanup_bound_workspace_on_error(
        &self,
        mode: &str,
        path: &std::path::Path,
    ) -> PulseResult<()> {
        if mode == WORKSPACE_MODE_ISOLATED && path.exists() {
            crate::workspace::cleanup_worktree(&self.repo_root, path)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Free helpers
// ---------------------------------------------------------------------------

fn build_lease_summary(lease: &AssignmentLeaseRecordV1) -> AssignmentLeaseSummary {
    AssignmentLeaseSummary {
        lease_id: lease.lease_id.clone(),
        state: lease.state.clone(),
        assignee: lease.assignee.principal.clone(),
        issued_by: lease.issued_by.clone(),
        issued_at: lease.issued_at.clone(),
        expires_at: lease.expires_at.clone(),
        ttl_seconds: lease.ttl_seconds,
        exclusive: true,
    }
}

fn build_workspace_summary(
    ws: &AssignmentWorkspaceRecordV1,
    lease_id: &str,
) -> AssignmentWorkspaceSummary {
    AssignmentWorkspaceSummary {
        workspace_id: ws.workspace_id.clone(),
        binding_status: ws.state.clone(),
        mode: ws.mode.clone(),
        path: ws.path.clone(),
        repository_id: ws.repository_id.clone(),
        base_commit: ws.base_commit.clone(),
        cleanliness: "clean".to_string(),
        owner_lease_id: lease_id.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Internal args struct
// ---------------------------------------------------------------------------

struct ClaimImplArgs {
    ticket_id: String,
    actor: String,
    assignee: String,
    inventory: CapabilityInventoryV1,
    ttl_seconds: u64,
    workspace_mode: Option<String>,
}

// ===========================================================================
// Release work (P2S2-I9)
// ===========================================================================

/// Arguments for `JsonGraphStore::release_work`.
#[derive(Debug, Clone)]
pub struct ReleaseArgs {
    /// Ticket ID whose lease to release.
    pub ticket_id: String,
    /// The exact lease ID to release.
    pub lease_id: String,
    /// Expected active revision of the Ticket node (CAS guard).
    pub expected_revision: u64,
    /// Human-readable reason for releasing.
    pub reason: String,
    /// Authorized principal performing the release.
    pub actor: String,
}

/// Outcome of a successful release operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ReleaseWorkOutcome {
    /// The ticket ID that was released.
    pub ticket_id: String,
    /// The released lease ID.
    pub lease_id: String,
    /// The workspace ID that was released.
    pub workspace_id: String,
    /// The prepared assignment ID associated with the release.
    pub prepared_assignment_id: String,
    /// Final node revision after active -> ready transition.
    pub new_revision: u64,
    /// Final workspace state after release.
    pub workspace_final_state: String,
    /// Whether the workspace was physically cleaned up after the transaction.
    pub workspace_cleaned_up: bool,
    /// The release event ID.
    pub event_id: String,
    /// Name of the transaction used to commit this release.
    pub transaction_id: String,
}

impl JsonGraphStore {
    /// Release a prepared/no-run assignment (P2S2-I9).
    ///
    /// One recoverable transaction:
    /// 1. Remove live lease record
    /// 2. Write terminal tombstone
    /// 3. Update workspace record to released/stale
    /// 4. Transition node active -> ready if exact claim revision
    ///
    /// After durable commit: attempt safe cleanup of isolated worktree
    /// only when clean at base.
    ///
    /// Requires `work.assignment.release` authority grant.
    /// Never deletes the in-place root. Preserves dirty/unknown workspaces.
    pub fn release_work(&self, args: ReleaseArgs) -> PulseResult<ReleaseWorkOutcome> {
        // Step 0: Validate enrollment before any lock or runtime directory creation.
        assignment_store::check_enrolled(&self.repo_root)?;

        // Step 1: Acquire repository WriteGuard.
        let guard = WriteGuard::acquire(&self.repo_root)?;

        // Step 2: Recover prepared transactions.
        recover_prepared_transactions(&self.repo_root)?;

        // Step 3: Authorize the release actor for work.assignment.release.
        let policy_report = crate::policy::load_authority_policy(&self.repo_root)?;
        let caller = crate::policy::parse_actor(&args.actor);
        crate::policy::authorize(&policy_report, &caller, &["work.assignment.release"])?;

        // Step 4: Load the lease record and validate.
        let lease = assignment_store::load_lease(&self.repo_root, &args.lease_id).map_err(|e| {
            if e.code() == "io_error" {
                PulseError::validation(
                    "assignment_lease_not_found",
                    format!("lease not found: {}", args.lease_id),
                )
            } else {
                e
            }
        })?;
        if lease.subject.id != args.ticket_id {
            return Err(PulseError::validation(
                "assignment_lease_not_found",
                format!(
                    "lease {} belongs to subject {} not {}",
                    args.lease_id, lease.subject.id, args.ticket_id
                ),
            ));
        }
        if lease.state != LEASE_STATE_PREPARED {
            return Err(PulseError::validation(
                "assignment_lease_not_releasable",
                format!(
                    "lease {} state is {}; only prepared leases are releasable",
                    args.lease_id, lease.state
                ),
            ));
        }
        if !args.lease_id.starts_with("lease_") {
            return Err(PulseError::validation(
                "assignment_lease_not_found",
                format!("invalid lease id format: {}", args.lease_id),
            ));
        }

        // Step 5: Load workspace record.
        let workspace_record =
            assignment_store::load_workspace(&self.repo_root, &lease.workspace_id)?;

        // Step 6: Load node and validate active status at expected revision.
        let node_path = self.node_path(&args.ticket_id);
        let before_bytes = fs::read(&node_path).map_err(|e| PulseError::io(&node_path, e))?;
        let node: Node =
            serde_json::from_slice(&before_bytes).map_err(|e| PulseError::json(&node_path, e))?;

        if node.status != NodeStatus::Active {
            return Err(PulseError::validation(
                "assignment_release_revision_mismatch",
                format!(
                    "subject {} status is {:?}, not Active",
                    node.id, node.status
                ),
            ));
        }
        if node.revision != args.expected_revision {
            return Err(PulseError::CasConflict {
                subject: args.ticket_id.clone(),
                expected_revision: args.expected_revision,
                current_revision: node.revision,
            });
        }

        // Step 7: Determine final workspace state.
        // In-place: never delete, just mark released.
        // Isolated clean at base: mark released (cleanup after commit).
        // Isolated dirty/unknown: mark stale_needs_operator, preserve path.
        let is_in_place = workspace_record.mode == WORKSPACE_MODE_IN_PLACE;
        let final_workspace_state = if is_in_place {
            WORKSPACE_STATE_RELEASED
        } else {
            // Check if workspace is clean at base for safe cleanup after commit.
            let ws_check_path = if workspace_record.path == "." {
                self.repo_root.clone()
            } else {
                self.repo_root.join(&workspace_record.path)
            };
            let cleanliness = crate::source::check_cleanliness(&ws_check_path);
            let is_clean = matches!(cleanliness, Ok(crate::source::SourceCleanliness::Clean));
            if is_clean {
                WORKSPACE_STATE_RELEASED
            } else {
                WORKSPACE_STATE_STALE
            }
        };

        // Step 8: Build the tombstone record.
        let now: DateTime<Utc> = Utc::now();
        let tombstone = AssignmentTombstoneV1 {
            schema_version: TOMBSTONE_SCHEMA_VERSION,
            lease_id: args.lease_id.clone(),
            subject_id: args.ticket_id.clone(),
            state: if final_workspace_state == WORKSPACE_STATE_STALE {
                TOMBSTONE_STATE_STALE
            } else {
                TOMBSTONE_STATE_RELEASED
            }
            .to_string(),
            recorded_at: now.to_rfc3339(),
            actor: args.actor.clone(),
            reason: Some(args.reason.clone()),
            reason_codes: vec![],
        };

        // Step 9: Build updated workspace record (released/stale).
        let mut updated_workspace = workspace_record.clone();
        updated_workspace.state = final_workspace_state.to_string();
        updated_workspace.released_at = Some(now.to_rfc3339());
        updated_workspace.cleanup.status = "released".to_string();

        // Step 10: Build node after state (active -> ready).
        // Ready status must not persist a status_reason per graph validation.
        let mut updated_node = node.clone();
        updated_node.status = NodeStatus::Ready;
        updated_node.status_reason = None;
        updated_node.revision += 1;
        updated_node.updated_at = now;

        // Step 11: Compute fingerprints and transaction IDs.
        let event_id = new_event_id();
        let transaction_id = new_transaction_id();
        let lease_path = assignment_store::lease_path(&self.repo_root, &args.lease_id)?;
        let tombstone_path = assignment_store::tombstone_path(&self.repo_root, &args.lease_id)?;
        let workspace_file_path =
            assignment_store::workspace_path(&self.repo_root, &workspace_record.workspace_id)?;

        // Event file path.
        let event_file_path = self
            .repo_root
            .join(".pulse/events")
            .join(now.format("%Y-%m-%d").to_string())
            .join(format!("{event_id}.json"));
        // Build canonical after bytes.
        let tombstone_bytes = to_canonical_bytes(&tombstone)?;
        let workspace_bytes = to_canonical_bytes(&updated_workspace)?;
        let node_after_bytes = to_canonical_bytes(&updated_node)?;

        // Build event payload.
        let event_payload = json!({
            "from": "active",
            "to": "ready",
            "expected_revision": args.expected_revision,
            "new_revision": updated_node.revision,
            "lease_id": args.lease_id,
            "workspace_id": workspace_record.workspace_id,
            "prepared_assignment_id": lease.prepared_assignment_id,
            "reason": args.reason,
            "workspace_final_state": final_workspace_state,
            "cleanup_policy": if is_in_place { "none" } else { "safe_remove_if_clean_at_base" },
        });

        let event = EventEnvelope::new(
            event_id.clone(),
            "work.assignment.released",
            &args.actor,
            &args.ticket_id,
            event_payload.clone(),
            now,
        );

        // Build file states for the transaction.
        let lease_before = FileState::Present {
            hash: hash_bytes(&to_canonical_bytes(&lease).expect("lease canonical")),
            revision: 0,
        };
        let lease_after = FileState::Absent;

        let tombstone_before = FileState::Absent;
        let tombstone_after = FileState::Present {
            hash: hash_bytes(&tombstone_bytes),
            revision: 0,
        };

        let workspace_before = FileState::Present {
            hash: hash_bytes(
                &to_canonical_bytes(&workspace_record).expect("workspace canonical before"),
            ),
            revision: 0,
        };
        let workspace_after = FileState::Present {
            hash: hash_bytes(&workspace_bytes),
            revision: 1,
        };

        let node_before = FileState::Present {
            hash: hash_bytes(&before_bytes),
            revision: args.expected_revision,
        };
        let node_after = FileState::Present {
            hash: hash_bytes(&node_after_bytes),
            revision: args.expected_revision + 1,
        };

        let targets: Vec<TransactionTarget> = vec![
            TransactionTarget::new(
                lease_path,
                lease_before,
                lease_after,
                &[], // empty bytes since target is Absent
            ),
            TransactionTarget::new(
                tombstone_path,
                tombstone_before,
                tombstone_after,
                &tombstone_bytes,
            ),
            TransactionTarget::new(
                workspace_file_path,
                workspace_before,
                workspace_after,
                &workspace_bytes,
            ),
            TransactionTarget::new(
                node_path.clone(),
                node_before,
                node_after,
                &node_after_bytes,
            ),
        ];

        let intent = MultiTargetTransactionIntent::prepared_with_transaction_id(
            transaction_id.clone(),
            event_id.clone(),
            "work.assignment.released",
            &args.actor,
            targets,
            event_file_path.clone(),
            serde_json::to_value(&event)?,
        )?;

        let prepared_txn = prepare_multi_target_transaction(&self.repo_root, intent)?;

        // Step 12: Commit the multi-target transaction.
        commit_prepared_multi_target_transaction(&prepared_txn, self.failpoint)?;

        // Step 13: Post-commit workspace physical cleanup (only for safe isolated).
        let workspace_cleaned_up =
            if !is_in_place && final_workspace_state == WORKSPACE_STATE_RELEASED {
                // Attempt safe cleanup of the isolated worktree after durable commit.
                let ws_path = self.repo_root.join(&workspace_record.path);
                if ws_path.exists() {
                    crate::workspace::cleanup_worktree(&self.repo_root, &ws_path).is_ok()
                } else {
                    false
                }
            } else {
                false
            };

        drop(guard);

        Ok(ReleaseWorkOutcome {
            ticket_id: args.ticket_id,
            lease_id: args.lease_id,
            workspace_id: workspace_record.workspace_id,
            prepared_assignment_id: lease.prepared_assignment_id,
            new_revision: updated_node.revision,
            workspace_final_state: final_workspace_state.to_string(),
            workspace_cleaned_up,
            event_id,
            transaction_id,
        })
    }
}

// ===========================================================================
// Leases listing (P2S2-I9, read-only)
// ===========================================================================

/// A single lease entry in the leases report, joining runtime lease state
/// with current graph node state.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LeaseEntry {
    pub lease_id: String,
    pub subject_id: String,
    pub assignee: String,
    pub issued_by: String,
    pub issued_at: String,
    pub expires_at: String,
    pub state: String,
    pub workspace_id: String,
    pub prepared_assignment_id: String,
    pub node_status: String,
    pub node_revision: u64,
    pub classification: String,
    pub is_tombstoned: bool,
}

/// Read-only leases report.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct LeasesReport {
    pub schema_version: u32,
    pub code: String,
    pub count: usize,
    pub live_count: usize,
    pub tombstoned_count: usize,
    pub expired_count: usize,
    pub entries: Vec<LeaseEntry>,
    pub orphan_workspace_ids: Vec<String>,
}

impl JsonGraphStore {
    /// Read-only leases listing (P2S2-I9).
    ///
    /// Joins runtime lease records with current graph node state.
    /// This is a pure read operation: it never creates runtime directories,
    /// never acquires the repository lock, and never mutates state.
    ///
    /// When `ticket_id` is `Some`, only entries matching that subject are
    /// returned.
    pub fn list_leases(&self, ticket_id: Option<&str>) -> PulseResult<LeasesReport> {
        assignment_store::check_enrolled(&self.repo_root)?;

        // Guard: do not create runtime directories as a side effect of a
        // read-only listing. If no runtime assignment directory exists,
        // return an empty report.
        let runtime_root = self
            .repo_root
            .join(crate::kernel::assignment_store::ASSIGNMENT_RUNTIME_ROOT);
        if !runtime_root.exists() {
            return Ok(LeasesReport {
                schema_version: 1,
                code: "leases_report".to_string(),
                count: 0,
                live_count: 0,
                tombstoned_count: 0,
                expired_count: 0,
                entries: vec![],
                orphan_workspace_ids: vec![],
            });
        }

        // Use the read-only classification report from assignment_store.
        let recovery_report =
            assignment_store::classify_assignment_recovery_state(&self.repo_root)?;

        let mut entries: Vec<LeaseEntry> = Vec::new();
        let mut live_count = 0usize;
        let mut tombstoned_count = 0usize;
        let mut expired_count = 0usize;

        for entry in &recovery_report.entries {
            // Apply optional ticket_id filter.
            if let Some(tid) = ticket_id {
                if entry.subject_id != tid {
                    continue;
                }
            }

            let (assignee, issued_by, issued_at, expires_at, state, prepared_assignment_id) =
                match assignment_store::load_lease(&self.repo_root, &entry.lease_id) {
                    Ok(lease) => (
                        lease.assignee.principal.clone(),
                        lease.issued_by.clone(),
                        lease.issued_at.clone(),
                        lease.expires_at.clone(),
                        lease.state.clone(),
                        lease.prepared_assignment_id.clone(),
                    ),
                    Err(_) => (
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new(),
                        entry.state.clone(),
                        String::new(),
                    ),
                };

            // Load node status.
            let (node_status, node_revision) = match self.read_node_status(&entry.subject_id) {
                Ok((status, rev)) => (status, rev),
                Err(_) => ("unknown".to_string(), 0),
            };

            let classification = match &entry.classification {
                crate::kernel::assignment_store::LeaseClassification::Live => "live",
                crate::kernel::assignment_store::LeaseClassification::Expired => "expired",
                crate::kernel::assignment_store::LeaseClassification::Tombstoned => "tombstoned",
                crate::kernel::assignment_store::LeaseClassification::Ambiguous(_) => "ambiguous",
                crate::kernel::assignment_store::LeaseClassification::Invalid(_) => "invalid",
            };

            let is_tombstoned = matches!(
                entry.classification,
                crate::kernel::assignment_store::LeaseClassification::Tombstoned
            );

            match entry.classification {
                crate::kernel::assignment_store::LeaseClassification::Live => live_count += 1,
                crate::kernel::assignment_store::LeaseClassification::Expired => expired_count += 1,
                crate::kernel::assignment_store::LeaseClassification::Tombstoned => {
                    tombstoned_count += 1
                }
                _ => {}
            }

            entries.push(LeaseEntry {
                lease_id: entry.lease_id.clone(),
                subject_id: entry.subject_id.clone(),
                assignee,
                issued_by,
                issued_at,
                expires_at,
                state,
                workspace_id: entry.workspace_id.clone(),
                prepared_assignment_id,
                node_status,
                node_revision,
                classification: classification.to_string(),
                is_tombstoned,
            });
        }

        entries.sort_by(|a, b| a.lease_id.cmp(&b.lease_id));

        // Collect orphan workspace IDs from the recovery report.
        let orphan_workspace_ids = recovery_report.orphan_workspace_ids.clone();

        Ok(LeasesReport {
            schema_version: 1,
            code: "leases_report".to_string(),
            count: entries.len(),
            live_count,
            tombstoned_count,
            expired_count,
            entries,
            orphan_workspace_ids,
        })
    }

    /// Read node status string and revision, returning ("unknown", 0)
    /// on any error instead of propagating so lease listing is best-effort.
    fn read_node_status(&self, id: &str) -> PulseResult<(String, u64)> {
        let path = self.node_path(id);
        if !path.exists() {
            return Ok(("not_found".to_string(), 0));
        }
        let bytes = fs::read(&path).map_err(|e| PulseError::io(&path, e))?;
        let node: Node = serde_json::from_slice(&bytes).map_err(|e| PulseError::json(&path, e))?;
        Ok((format!("{:?}", node.status), node.revision))
    }
}

// ===========================================================================
// Leases recover (P2S2-I9, safe mutation under fence)
// ===========================================================================

/// Outcome of a safe recovery operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct RecoveryOutcome {
    pub schema_version: u32,
    pub code: String,
    pub actor: String,
    pub expired_count: usize,
    pub requeued_count: usize,
    pub stale_count: usize,
    pub ambiguous_count: usize,
    pub invalid_count: usize,
    pub errors: Vec<String>,
    pub report: crate::kernel::assignment_store::AssignmentRecoveryReport,
}

impl JsonGraphStore {
    /// Safe recovery of runtime assignments (P2S2-I9).
    ///
    /// Runs under the repository fence with authority check for
    /// `work.assignment.release`. After transaction recovery:
    ///
    /// 1. Expired no-run assignments are safely released:
    ///    - lease removed, tombstone written, workspace released,
    ///    - node transitioned active -> ready if exact claim revision.
    /// 2. Ambiguous/invalid/orphan state is reported without mutation.
    /// 3. Workspaces with dirty/unknown state are preserved as stale.
    ///
    /// This implements only safe deterministic fixes. Ambiguous records,
    /// orphan workspaces and invalid records are never silently repaired.
    pub fn recover_leases(&self, actor: &str) -> PulseResult<RecoveryOutcome> {
        assignment_store::check_enrolled(&self.repo_root)?;

        let guard = WriteGuard::acquire(&self.repo_root)?;
        recover_prepared_transactions(&self.repo_root)?;

        // Authorize for work.assignment.release (same authority as release).
        let policy_report = crate::policy::load_authority_policy(&self.repo_root)?;
        let caller = crate::policy::parse_actor(actor);
        crate::policy::authorize(&policy_report, &caller, &["work.assignment.release"])?;

        // Run read-only classification.
        let report = assignment_store::classify_assignment_recovery_state(&self.repo_root)?;

        let mut expired_count = 0usize;
        let mut requeued_count = 0usize;
        let mut stale_count = 0usize;
        let mut ambiguous_count = 0usize;
        let mut invalid_count = 0usize;
        let mut errors: Vec<String> = Vec::new();

        // Iterate expired entries and attempt safe requeue.
        for entry in &report.entries {
            if !matches!(
                entry.classification,
                crate::kernel::assignment_store::LeaseClassification::Expired
            ) {
                match &entry.classification {
                    crate::kernel::assignment_store::LeaseClassification::Ambiguous(_) => {
                        ambiguous_count += 1
                    }
                    crate::kernel::assignment_store::LeaseClassification::Invalid(_) => {
                        invalid_count += 1
                    }
                    _ => {}
                }
                continue;
            }

            expired_count += 1;

            // Attempt safe expiry/requeue. If anything goes wrong here,
            // report the error but continue with other entries.
            let release_result = self.recover_single_expired_lease(actor, entry, &report);
            match release_result {
                Ok(released) => {
                    if released {
                        requeued_count += 1;
                    } else {
                        stale_count += 1;
                    }
                }
                Err(error) => {
                    errors.push(format!("lease {}: {}", entry.lease_id, error));
                    stale_count += 1;
                }
            }
        }

        drop(guard);

        Ok(RecoveryOutcome {
            schema_version: 1,
            code: "leases_recovered".to_string(),
            actor: actor.to_string(),
            expired_count,
            requeued_count,
            stale_count,
            ambiguous_count,
            invalid_count,
            errors,
            report,
        })
    }

    /// Recover a single expired lease: tombstone + node transition if safe.
    /// Returns `Ok(true)` if the lease was successfully released/requeued,
    /// `Ok(false)` if the lease was released but workspace became stale,
    /// `Err(...)` if recovery was not possible.
    fn recover_single_expired_lease(
        &self,
        actor: &str,
        entry: &crate::kernel::assignment_store::RecoveryEntry,
        _report: &crate::kernel::assignment_store::AssignmentRecoveryReport,
    ) -> PulseResult<bool> {
        // Load the lease record.
        let lease = match assignment_store::load_lease(&self.repo_root, &entry.lease_id) {
            Ok(l) => l,
            Err(e) => {
                return Err(PulseError::validation(
                    "assignment_recovery_failed",
                    format!("cannot load expired lease {}: {e}", entry.lease_id),
                ));
            }
        };

        // Verify lease is prepared (no-run state).
        if lease.state != LEASE_STATE_PREPARED {
            return Err(PulseError::validation(
                "assignment_lease_not_releasable",
                format!(
                    "expired lease {} has non-prepared state {}",
                    entry.lease_id, lease.state
                ),
            ));
        }

        // Load workspace record.
        let workspace_record =
            match assignment_store::load_workspace(&self.repo_root, &lease.workspace_id) {
                Ok(ws) => ws,
                Err(e) => {
                    return Err(PulseError::validation(
                        "assignment_recovery_failed",
                        format!(
                            "cannot load workspace {} for expired lease {}: {e}",
                            lease.workspace_id, entry.lease_id
                        ),
                    ));
                }
            };

        // Check if workspace is dirty/unknown (preserve as stale).
        let is_in_place = workspace_record.mode == WORKSPACE_MODE_IN_PLACE;
        let workspace_path_str = &workspace_record.path;
        let workspace_abs_path = if workspace_path_str == "." {
            self.repo_root.clone()
        } else {
            self.repo_root.join(workspace_path_str)
        };

        let is_clean = if is_in_place {
            // For in-place, check root cleanliness.
            matches!(
                crate::source::check_cleanliness(&self.repo_root),
                Ok(crate::source::SourceCleanliness::Clean)
            )
        } else {
            let ws_check = if workspace_path_str == "." {
                self.repo_root.clone()
            } else {
                self.repo_root.join(workspace_path_str)
            };
            matches!(
                crate::source::check_cleanliness(&ws_check),
                Ok(crate::source::SourceCleanliness::Clean)
            )
        };

        let final_workspace_state = if is_clean {
            WORKSPACE_STATE_RELEASED
        } else {
            WORKSPACE_STATE_STALE
        };

        // Load node.
        let node_path = self.node_path(&lease.subject.id);
        let before_bytes = match fs::read(&node_path) {
            Ok(b) => b,
            Err(e) => {
                return Err(PulseError::validation(
                    "assignment_recovery_failed",
                    format!(
                        "cannot load node {} for expired lease {}: {e}",
                        lease.subject.id, entry.lease_id
                    ),
                ));
            }
        };
        let node: Node = match serde_json::from_slice(&before_bytes) {
            Ok(n) => n,
            Err(e) => {
                return Err(PulseError::validation(
                    "assignment_recovery_failed",
                    format!("cannot parse node {}: {e}", lease.subject.id),
                ));
            }
        };

        // Only transition if node is still Active at the claim revision.
        // If node has been modified since claim (different revision or
        // different status), preserve as stale instead.
        if node.status != NodeStatus::Active || node.revision != lease.subject.revision {
            return Err(PulseError::validation(
                "assignment_release_revision_mismatch",
                format!(
                    "node {} status {:?} revision {} does not match claim revision {}; cannot auto-requeue",
                    node.id, node.status, node.revision, lease.subject.revision
                ),
            ));
        }

        let now: DateTime<Utc> = Utc::now();

        // If workspace is stale, write tombstone as stale but don't transition node.
        if final_workspace_state == WORKSPACE_STATE_STALE {
            // Just write tombstone and mark workspace; don't touch node.
            let tombstone = AssignmentTombstoneV1 {
                schema_version: TOMBSTONE_SCHEMA_VERSION,
                lease_id: entry.lease_id.clone(),
                subject_id: lease.subject.id.clone(),
                state: TOMBSTONE_STATE_STALE.to_string(),
                recorded_at: now.to_rfc3339(),
                actor: actor.to_string(),
                reason: Some(
                    "recovery: expired lease with dirty workspace, preserved for operator"
                        .to_string(),
                ),
                reason_codes: vec![],
            };

            let tombstone_bytes = to_canonical_bytes(&tombstone)?;
            // Write tombstone + update workspace + remove lease in one transaction.
            self.commit_expired_stale_tombstone(
                actor,
                entry,
                &lease,
                &workspace_record,
                &tombstone_bytes,
                now,
            )?;
            return Ok(false);
        }

        // Full requeue: tombstone + release workspace + transition node.
        let tombstone = AssignmentTombstoneV1 {
            schema_version: TOMBSTONE_SCHEMA_VERSION,
            lease_id: entry.lease_id.clone(),
            subject_id: lease.subject.id.clone(),
            state: TOMBSTONE_STATE_EXPIRED.to_string(),
            recorded_at: now.to_rfc3339(),
            actor: actor.to_string(),
            reason: Some("recovery: expired lease requeued to ready".to_string()),
            reason_codes: vec![],
        };

        let mut updated_workspace = workspace_record.clone();
        updated_workspace.state = WORKSPACE_STATE_RELEASED.to_string();
        updated_workspace.released_at = Some(now.to_rfc3339());

        let mut updated_node = node.clone();
        updated_node.status = NodeStatus::Ready;
        // Ready status must not persist a status_reason per graph validation.
        updated_node.status_reason = None;
        updated_node.revision += 1;
        updated_node.updated_at = now;

        let event_id = new_event_id();
        let transaction_id = new_transaction_id();

        let lease_path = assignment_store::lease_path(&self.repo_root, &entry.lease_id)?;
        let tombstone_path = assignment_store::tombstone_path(&self.repo_root, &entry.lease_id)?;
        let workspace_file_path =
            assignment_store::workspace_path(&self.repo_root, &workspace_record.workspace_id)?;

        let event_file_path = self
            .repo_root
            .join(".pulse/events")
            .join(now.format("%Y-%m-%d").to_string())
            .join(format!("{event_id}.json"));

        let tombstone_bytes = to_canonical_bytes(&tombstone)?;
        let workspace_bytes = to_canonical_bytes(&updated_workspace)?;
        let node_after_bytes = to_canonical_bytes(&updated_node)?;

        let event_payload = json!({
            "from": "active",
            "to": "ready",
            "expected_revision": lease.subject.revision,
            "new_revision": lease.subject.revision + 1,
            "lease_id": entry.lease_id,
            "workspace_id": workspace_record.workspace_id,
            "prepared_assignment_id": lease.prepared_assignment_id,
            "reason": "recovery: expired lease requeued",
            "workspace_final_state": WORKSPACE_STATE_RELEASED,
            "recovery": true,
        });

        let event = EventEnvelope::new(
            event_id.clone(),
            "work.assignment.released",
            actor,
            &lease.subject.id,
            event_payload,
            now,
        );

        let targets: Vec<TransactionTarget> = vec![
            TransactionTarget::new(
                lease_path,
                FileState::Present {
                    hash: hash_bytes(&to_canonical_bytes(&lease).expect("canonical lease")),
                    revision: 0,
                },
                FileState::Absent,
                &[],
            ),
            TransactionTarget::new(
                tombstone_path,
                FileState::Absent,
                FileState::Present {
                    hash: hash_bytes(&tombstone_bytes),
                    revision: 0,
                },
                &tombstone_bytes,
            ),
            TransactionTarget::new(
                workspace_file_path,
                FileState::Present {
                    hash: hash_bytes(
                        &to_canonical_bytes(&workspace_record).expect("canonical workspace before"),
                    ),
                    revision: 0,
                },
                FileState::Present {
                    hash: hash_bytes(&workspace_bytes),
                    revision: 1,
                },
                &workspace_bytes,
            ),
            TransactionTarget::new(
                node_path,
                FileState::Present {
                    hash: hash_bytes(&before_bytes),
                    revision: lease.subject.revision,
                },
                FileState::Present {
                    hash: hash_bytes(&node_after_bytes),
                    revision: lease.subject.revision + 1,
                },
                &node_after_bytes,
            ),
        ];

        let intent = MultiTargetTransactionIntent::prepared_with_transaction_id(
            transaction_id,
            event_id,
            "work.assignment.released",
            actor,
            targets,
            event_file_path,
            serde_json::to_value(&event)?,
        )?;

        let prepared_txn = prepare_multi_target_transaction(&self.repo_root, intent)?;
        commit_prepared_multi_target_transaction(&prepared_txn, self.failpoint)?;

        // Can attempt workspace cleanup after commit.
        if !is_in_place && workspace_abs_path.exists() {
            let _ = crate::workspace::cleanup_worktree(&self.repo_root, &workspace_abs_path);
        }

        Ok(true)
    }

    /// Commit a tombstone + workspace update for a stale-expired lease
    /// (no node transition because workspace is dirty).
    fn commit_expired_stale_tombstone(
        &self,
        actor: &str,
        entry: &crate::kernel::assignment_store::RecoveryEntry,
        lease: &crate::assignment::AssignmentLeaseRecordV1,
        workspace_record: &crate::assignment::AssignmentWorkspaceRecordV1,
        tombstone_bytes: &[u8],
        now: DateTime<Utc>,
    ) -> PulseResult<()> {
        let mut updated_workspace = workspace_record.clone();
        updated_workspace.state = WORKSPACE_STATE_STALE.to_string();
        updated_workspace.released_at = Some(now.to_rfc3339());

        let event_id = new_event_id();
        let transaction_id = new_transaction_id();

        let lease_path = assignment_store::lease_path(&self.repo_root, &entry.lease_id)?;
        let tombstone_path = assignment_store::tombstone_path(&self.repo_root, &entry.lease_id)?;
        let workspace_file_path =
            assignment_store::workspace_path(&self.repo_root, &workspace_record.workspace_id)?;

        let event_file_path = self
            .repo_root
            .join(".pulse/events")
            .join(now.format("%Y-%m-%d").to_string())
            .join(format!("{event_id}.json"));

        let workspace_bytes = to_canonical_bytes(&updated_workspace)?;

        let event_payload = json!({
            "from": "active",
            "to": "active",
            "expected_revision": lease.subject.revision,
            "lease_id": entry.lease_id,
            "workspace_id": workspace_record.workspace_id,
            "prepared_assignment_id": lease.prepared_assignment_id,
            "reason": "recovery: expired lease with dirty workspace preserved for operator",
            "workspace_final_state": WORKSPACE_STATE_STALE,
            "recovery": true,
            "node_unchanged": true,
        });

        let event = EventEnvelope::new(
            event_id.clone(),
            "work.assignment.released",
            actor,
            &lease.subject.id,
            event_payload,
            now,
        );

        let targets: Vec<TransactionTarget> = vec![
            TransactionTarget::new(
                lease_path,
                FileState::Present {
                    hash: hash_bytes(&to_canonical_bytes(lease).expect("canonical lease")),
                    revision: 0,
                },
                FileState::Absent,
                &[],
            ),
            TransactionTarget::new(
                tombstone_path,
                FileState::Absent,
                FileState::Present {
                    hash: hash_bytes(tombstone_bytes),
                    revision: 0,
                },
                tombstone_bytes,
            ),
            TransactionTarget::new(
                workspace_file_path,
                FileState::Present {
                    hash: hash_bytes(
                        &to_canonical_bytes(workspace_record).expect("canonical workspace before"),
                    ),
                    revision: 0,
                },
                FileState::Present {
                    hash: hash_bytes(&workspace_bytes),
                    revision: 1,
                },
                &workspace_bytes,
            ),
        ];

        let intent = MultiTargetTransactionIntent::prepared_with_transaction_id(
            transaction_id,
            event_id,
            "work.assignment.released",
            actor,
            targets,
            event_file_path,
            serde_json::to_value(&event)?,
        )?;

        let prepared_txn = prepare_multi_target_transaction(&self.repo_root, intent)?;
        commit_prepared_multi_target_transaction(&prepared_txn, self.failpoint)?;
        Ok(())
    }
}
