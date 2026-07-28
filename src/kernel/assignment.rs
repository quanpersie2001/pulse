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

use chrono::{DateTime, Utc};
use serde_json::json;

use crate::assignment::{
    self, AssignmentLeaseAssignee, AssignmentLeaseRecordV1, AssignmentLeaseSource,
    AssignmentLeaseSubject, AssignmentLeaseSummary, AssignmentLifecycle, AssignmentSubjectSnapshot,
    AssignmentTransaction, AssignmentWorkspaceRecordV1, AssignmentWorkspaceSummary,
    CapabilityInventoryV1, CapabilityMatchReport, PreparedAssignmentRecordV1, PreparedAssignmentV1,
    RevalidatedSnapshot, WorkspaceCleanupPolicy, WorkspaceSubjectRef, ASSIGNMENT_SCHEMA_VERSION,
    LEASE_KIND_IMPLEMENTATION, LEASE_SCHEMA_VERSION, LEASE_STATE_PREPARED, LIFECYCLE_GATE_PROFILE,
    LIFECYCLE_READY_TO_ACTIVE, MAX_TTL_SECONDS, MIN_TTL_SECONDS, PREPARED_ASSIGNMENT_PROFILE,
    WORKSPACE_MODE_IN_PLACE, WORKSPACE_MODE_ISOLATED, WORKSPACE_SCHEMA_VERSION,
    WORKSPACE_STATE_BOUND,
};
use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::event::{event_path, new_event_id, EventEnvelope};
use crate::graph::node::{Node, NodeStatus};
use crate::graph::store::JsonGraphStore;
use crate::kernel::assignment_store;
use crate::kernel::lifecycle::PreparedAssignmentGateContext;
use crate::storage::transaction::{
    commit_prepared_multi_target_transaction, prepare_multi_target_transaction,
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
    /// TTL for the lease in seconds. Defaults to `DEFAULT_TTL_SECONDS`,
    /// clamped to [`MIN_TTL_SECONDS`, `MAX_TTL_SECONDS`].
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

        // Step 1: Validate TTL.
        let ttl_seconds = args.ttl_seconds.clamp(MIN_TTL_SECONDS, MAX_TTL_SECONDS);

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
        // 7. Resolve workspace mode and create workspace binding
        // ------------------------------------------------------------------
        let workspace_mode = self.resolve_workspace_mode_lookup(
            &packet.workspace.required_strategy,
            args.workspace_mode.as_deref(),
        )?;

        // Generate IDs.
        let now: DateTime<Utc> = Utc::now();
        let lease_id = format!("lease_{}", ulid::Ulid::new());
        let workspace_id = format!("wt_{}_{}", args.ticket_id, ulid::Ulid::new());
        let prepared_assignment_id = format!("pa_{}", ulid::Ulid::new());

        // Determine workspace path.
        let workspace_path = match workspace_mode.as_str() {
            WORKSPACE_MODE_IN_PLACE => ".".to_string(),
            _ => format!(".pulse/runtime/workspaces/{workspace_id}"),
        };

        // Compute timestamps.
        let issued_at = now.to_rfc3339();
        let expires_at = (now + chrono::Duration::seconds(args.ttl_seconds as i64)).to_rfc3339();

        // Build source & repository info from packet.
        let repository_id = packet.source.repository_id.clone();
        let source_commit = packet.source.commit.clone();

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
            &repository_id,
            &source_commit,
            &now,
        )?;

        // Generate event_id early so the prepared record's lifecycle has a
        // non-empty event_id for the gate fingerprint projection.
        let event_id = new_event_id();

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
        // Compute fingerprint for the prepared record so the gate validator
        // can verify its integrity before commit.
        prepared_record.prepared_assignment_fingerprint = prepared_record.compute_fingerprint()?;

        // ------------------------------------------------------------------
        // 9. Evaluate prepared-assignment lifecycle gate (no commit)
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

        // ------------------------------------------------------------------
        // 10. Build the PreparedAssignmentV1 response wrapper
        // ------------------------------------------------------------------

        let lifecycle = AssignmentLifecycle {
            transition: LIFECYCLE_READY_TO_ACTIVE.to_string(),
            gate_profile: LIFECYCLE_GATE_PROFILE.to_string(),
            gate_status: "passed".to_string(),
            expected_revision,
            new_revision: expected_revision + 1,
            event_id: event_id.clone(),
        };

        let lease_summary = build_lease_summary(&lease_record);
        let workspace_summary = build_workspace_summary(&workspace_record, &lease_id);

        let revalidated_snapshot = RevalidatedSnapshot {
            graph_fingerprint: gate_plan.graph_fingerprint_before.clone(),
            readiness_profile: packet.snapshot.readiness_profile.clone(),
            readiness_fingerprint: packet.snapshot.readiness_fingerprint.clone(),
            authority_policy_fingerprint: packet.snapshot.authority_policy_fingerprint.clone(),
            docs_registry_fingerprint: packet.snapshot.docs_registry_fingerprint.clone(),
            docs_index_fingerprint: packet.snapshot.docs_index_fingerprint.clone(),
            source_commit: packet.snapshot.source_commit.clone(),
            source_cleanliness: packet.source.cleanliness.clone(),
            repository_id: repository_id.clone(),
        };

        let transaction_id = format!("txn_{}", ulid::Ulid::new());

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
            transaction: AssignmentTransaction {
                transaction_id: transaction_id.clone(),
                committed_targets: vec![],
                event_path: String::new(),
                recovery_state: "complete".to_string(),
            },
            prepared_assignment_fingerprint: String::new(),
            reason_codes: vec![],
        };
        prepared_v1.prepared_assignment_fingerprint = prepared_v1.compute_fingerprint()?;

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
        let event_file_path = event_path(&self.repo_root, &event);

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

        let intent = MultiTargetTransactionIntent::prepared(
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
        // 14. Build the final PreparedAssignmentV1 with transaction fields
        // ------------------------------------------------------------------
        prepared_v1.transaction = AssignmentTransaction {
            transaction_id: prepared_txn.intent.transaction_id.clone(),
            committed_targets: vec![
                format!(".pulse/runtime/assignment/leases/{lease_id}.json",),
                format!(".pulse/runtime/assignment/workspaces/{workspace_id}.json",),
                format!(".pulse/runtime/assignment/prepared/{prepared_assignment_id}.json",),
                self.rel_path(&node_path).to_string_lossy().to_string(),
            ],
            event_path: event_file_path
                .strip_prefix(&self.repo_root)
                .unwrap_or(&event_file_path)
                .to_string_lossy()
                .to_string(),
            recovery_state: "complete".to_string(),
        };
        prepared_v1.prepared_assignment_fingerprint = prepared_v1.compute_fingerprint()?;

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
        repository_id: &str,
        base_commit: &str,
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
            repository_id: repository_id.to_string(),
            base_commit: base_commit.to_string(),
            head_commit_at_bind: base_commit.to_string(),
            cleanliness_at_bind: "clean".to_string(),
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
