//! `pulse run <role> --ticket <id>`: execute a configured runner role.
//!
//! The kernel composes the runner mechanics with graph truth: it validates the
//! lifecycle gate for the role class, takes the exclusive lease for worker
//! runs, commits the bounded input contract to `.pulse/runtime/run/`, spawns
//! the configured command, classifies the outcome and appends one event. It
//! never guesses: exit, timeout, cancellation, malformed output and unproven
//! success claims all collapse into a `handed_off`-less `inconclusive` run.
//!
//! Role classes are fixed by name: `worker` runs on `ready` Tickets under a
//! lease, `reviewer` and `qa` run on `verifying` Tickets without a lease.
//! Other role names are reserved for future check-style roles.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;

use std::process::Command;
use std::time::Duration;

use crate::event::{new_event_id, write_event, EventEnvelope};
use crate::graph::model::node::NodeStatus;
use crate::graph::store::JsonGraphStore;
use crate::identity::actor::ActorKind;
use crate::policy::authority::{authority_path, AuthorityPrincipal};
use crate::reservation::{
    ActivateReservationArgs, AssignmentAcknowledgement, CoreReservation, ReservationState,
    ReserveWorkArgs, RuntimeBinding,
};
use crate::runner::{self, CommandSpec};
use crate::{PulseError, PulseResult};

/// Default worker lease TTL when the caller does not pin one.
pub const DEFAULT_RUN_TTL_SECONDS: u64 = 3600;

/// Root of the per-Ticket run workspace (gitignored).
pub const RUN_DIR: &str = ".pulse/runtime/run";

/// Validate a role name and return its lifecycle class.
fn role_class(role: &str) -> PulseResult<&'static str> {
    let valid = !role.is_empty()
        && role.len() <= 40
        && role.starts_with(|c: char| c.is_ascii_lowercase())
        && role
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if !valid {
        return Err(PulseError::validation(
            "run_role_invalid",
            "role must be 1-40 ASCII lowercase letters, digits, hyphens or underscores",
        ));
    }
    match role {
        "worker" => Ok("worker"),
        "reviewer" => Ok("reviewer"),
        "qa" => Ok("qa"),
        _ => Err(PulseError::validation(
            "run_role_lifecycle_unsupported",
            format!(
                "role {role} has no lifecycle class yet; supported roles are worker, reviewer, qa"
            ),
        )),
    }
}

/// Full `kind:id` actor string for the runner role.
fn runner_actor(role: &str) -> String {
    format!("agent:runner:{role}")
}

/// Policy principal id for the runner role (Agent kind).
fn runner_principal_id(role: &str) -> String {
    format!("runner:{role}")
}

/// Grants provisioned for the `runner:<role>` actor, by role class. Fixed,
/// narrow, no wildcards.
fn role_grants(class: &str) -> &'static [&'static str] {
    match class {
        // The worker agent hands off, leaves notes and captures learnings; it
        // never verifies or closes.
        "worker" => &[
            "note",
            "work.assignment.handoff",
            "work.assignment.prepare",
            "work.assignment.release",
        ],
        "reviewer" => &["work.assignment.verify"],
        "qa" => &["evidence.record"],
        _ => &[],
    }
}

/// Parsed `.pulse/config/runners.json`: role commands plus the
/// `auto_isolation` policy (default true).
#[derive(Debug, Clone, Deserialize)]
pub struct RunnersConfig {
    /// When another Ticket holds a live lease, `pulse run` isolates the new
    /// Ticket in a worktree. `false` refuses the run instead.
    #[serde(default = "default_true")]
    pub auto_isolation: bool,
    #[serde(flatten)]
    pub roles: BTreeMap<String, CommandSpec>,
}

fn default_true() -> bool {
    true
}

/// Load and validate `.pulse/config/runners.json`.
pub fn load_runner_config(repo_root: &Path) -> PulseResult<RunnersConfig> {
    let path = repo_root.join(".pulse/config/runners.json");
    let bytes = fs::read(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            PulseError::validation(
                "run_config_missing",
                format!(
                    "runner config {} is missing; define at least one role before pulse run",
                    path.display()
                ),
            )
        } else {
            PulseError::io(&path, error)
        }
    })?;
    let config: RunnersConfig =
        serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))?;
    for (role, spec) in &config.roles {
        role_class(role)?;
        spec.validate().map_err(|error| {
            PulseError::validation("run_config_invalid", format!("role {role}: {error}"))
        })?;
    }
    Ok(config)
}

/// Ensure a `runner:<role>` principal exists with exactly the role grants.
/// Idempotent: the policy is rewritten only when grants are missing.
fn provision_runner_actor(repo_root: &Path, role: &str, class: &str) -> PulseResult<()> {
    let actor_id = runner_principal_id(role);
    let required = role_grants(class);
    let path = authority_path(repo_root);
    let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
    let mut policy: crate::policy::AuthorityPolicy =
        serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))?;
    policy.normalize();
    let findings = policy.validate();
    if !findings.is_empty() {
        return Err(PulseError::validation(
            "readiness_policy_invalid",
            format!("authority policy is invalid: {}", findings.join(",")),
        ));
    }
    let existing = policy
        .principals
        .iter_mut()
        .find(|principal| principal.kind == ActorKind::Agent && principal.id == actor_id);
    match existing {
        Some(principal) => {
            let missing: Vec<&str> = required
                .iter()
                .filter(|grant| !principal.grants.iter().any(|held| held == *grant))
                .copied()
                .collect();
            if missing.is_empty() {
                return Ok(());
            }
            principal
                .grants
                .extend(missing.iter().map(|grant| (*grant).to_string()));
            principal.grants.sort();
            principal.grants.dedup();
        }
        None => {
            policy.principals.push(AuthorityPrincipal {
                kind: ActorKind::Agent,
                id: actor_id,
                grants: required
                    .iter()
                    .map(|grant| (*grant).to_string())
                    .collect::<Vec<_>>(),
            });
            policy
                .principals
                .sort_by(|left, right| left.id.cmp(&right.id));
        }
    }
    policy.revision += 1;
    policy.normalize();
    crate::storage::atomic_write(&path, &crate::canonical_json::to_canonical_bytes(&policy)?)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct RunOutcome {
    pub schema_version: u32,
    pub code: String,
    pub role: String,
    pub ticket_id: String,
    pub lease_id: Option<String>,
    /// `handed_off`, `blocked` or `completed` on success; `inconclusive`
    /// otherwise.
    pub status: String,
    /// Why the run is inconclusive: timeout, cancelled, exit_nonzero,
    /// malformed_output or unproven_claim.
    pub inconclusive_reason: Option<String>,
    pub summary: Option<String>,
    pub run_record_path: String,
    pub event_id: String,
}

impl JsonGraphStore {
    /// Execute the configured command for `role` against `ticket_id`.
    ///
    /// `isolation` is `auto` (checkout by default, worktree when another
    /// Ticket holds a live lease and `auto_isolation` allows it) or `worktree`
    /// (forced). Only worker runs take leases and can be isolated.
    pub fn run_role(
        &self,
        role: &str,
        ticket_id: &str,
        ttl_seconds: u64,
        idempotency_key: &str,
        forced_worktree: bool,
        acknowledge_drift: bool,
    ) -> PulseResult<RunOutcome> {
        let class = role_class(role)?;
        let config = load_runner_config(&self.repo_root)?;
        let spec = config.roles.get(role).ok_or_else(|| {
            PulseError::validation(
                "run_role_missing",
                format!("role {role} is not defined in .pulse/config/runners.json"),
            )
        })?;
        provision_runner_actor(&self.repo_root, role, class)?;

        let node = self.show_node(ticket_id)?;
        let started = Utc::now();
        // Isolation decision happens before the lease so the runtime binding
        // records the exact workspace the worker will run in.
        let workspace: Option<String> = match class {
            "worker" => self.decide_workspace(ticket_id, forced_worktree, config.auto_isolation)?,
            _ => None,
        };
        let (lease_id, reservation) = match class {
            "worker" => {
                let outcome = self.take_worker_lease(
                    role,
                    ticket_id,
                    ttl_seconds,
                    idempotency_key,
                    workspace.as_deref(),
                    acknowledge_drift,
                )?;
                (Some(outcome.0), Some(outcome.1))
            }
            _ => {
                if node.status != NodeStatus::Verifying {
                    return Err(PulseError::validation(
                        "run_ticket_not_verifying",
                        format!(
                            "role {role} runs on verifying Tickets; {} is {:?}",
                            ticket_id, node.status
                        ),
                    ));
                }
                (None, None)
            }
        };
        // Worker command runs inside the ticket workspace; reviewer/qa run
        // sequentially after the worker in the same checkout.
        let command_dir = workspace
            .as_ref()
            .map(|relative| self.repo_root.join(relative))
            .unwrap_or_else(|| self.repo_root.clone());

        // Commit the bounded input contract into the run workspace.
        let run_dir = self.repo_root.join(RUN_DIR).join(ticket_id);
        let artifact_dir = run_dir.join("artifacts");
        fs::create_dir_all(&artifact_dir).map_err(|error| PulseError::io(&artifact_dir, error))?;
        let input_path = match class {
            "worker" => run_dir.join("worker-input.json"),
            "reviewer" => run_dir.join("reviewer-input.json"),
            _ => run_dir.join("qa-input.json"),
        };
        let input_json = self.build_role_input(class, ticket_id, &node, &reservation)?;
        fs::write(&input_path, &input_json).map_err(|error| PulseError::io(&input_path, error))?;
        if class == "worker" {
            let Some(reservation) = &reservation else {
                return Err(PulseError::validation(
                    "run_worker_lease_missing",
                    "worker run requires an active lease before writing its input",
                ));
            };
            let prompt_path = run_dir.join("worker-prompt.md");
            fs::write(
                &prompt_path,
                worker_prompt(
                    ticket_id,
                    &reservation.lease_id,
                    reservation
                        .runtime_binding
                        .as_ref()
                        .map(|binding| binding.session_id.as_str())
                        .unwrap_or_default(),
                    &reservation.source.commit,
                ),
            )
            .map_err(|error| PulseError::io(&prompt_path, error))?;
            // Shell-sourceable run facts the agent script needs to record
            // proofs through the CLI.
            let env_path = run_dir.join("worker-env");
            let env_text = format!(
                "TICKET_ID=\"{ticket_id}\"\nLEASE_ID=\"{}\"\nSESSION_ID=\"{}\"\nSOURCE_COMMIT=\"{}\"\nACTOR=\"{}\"\n",
                reservation.lease_id,
                reservation
                    .runtime_binding
                    .as_ref()
                    .map(|binding| binding.session_id.as_str())
                    .unwrap_or_default(),
                reservation.source.commit,
                reservation.assignee,
            );
            fs::write(&env_path, env_text).map_err(|error| PulseError::io(&env_path, error))?;
        }
        if class == "reviewer" {
            let prompt_path = run_dir.join("reviewer-prompt.md");
            fs::write(
                &prompt_path,
                reviewer_prompt(ticket_id, &crate::source::head_commit(&self.repo_root)?),
            )
            .map_err(|error| PulseError::io(&prompt_path, error))?;
        }

        // Materialize argv and execute under the configured bounds.
        let argv = runner::split_argv(&spec.command)?;
        let mut values = BTreeMap::new();
        values.insert("input".to_string(), input_path.display().to_string());
        values.insert("ticket".to_string(), ticket_id.to_string());
        values.insert("repo".to_string(), self.repo_root.display().to_string());
        values.insert(
            "artifact_dir".to_string(),
            artifact_dir.display().to_string(),
        );
        let argv = runner::materialize_argv(&argv, &values)?;
        let execution = runner::execute(
            &command_dir,
            &argv,
            Duration::from_secs(spec.timeout_seconds),
            spec.max_output_bytes,
            None,
        );

        let (run, stderr_tail) = match &execution {
            Ok(outcome) => {
                // A bounded stderr tail makes inconclusive runs diagnosable
                // without turning Pulse into a log store.
                let stderr_tail: Option<String> = if outcome.stderr.is_empty() {
                    None
                } else {
                    let tail = outcome.stderr.len().saturating_sub(512);
                    Some(String::from_utf8_lossy(&outcome.stderr[tail..]).to_string())
                };
                (
                    self.classify_outcome(class, ticket_id, lease_id.as_deref(), outcome),
                    stderr_tail,
                )
            }
            Err(error) => (
                RunClassification::inconclusive("spawn_failed", Some(error.to_string())),
                None,
            ),
        };
        let run_record = RunRecord {
            schema_version: 1,
            role: role.to_string(),
            ticket_id: ticket_id.to_string(),
            lease_id: lease_id.clone(),
            status: run.status.clone(),
            inconclusive_reason: run.inconclusive_reason.clone(),
            summary: run.summary.clone(),
            stderr_tail,
            exit_code: run.exit_code,
            timed_out: run.timed_out,
            cancelled: run.cancelled,
            started_at: started.to_rfc3339(),
            finished_at: Utc::now().to_rfc3339(),
        };
        let record_path = run_dir.join(format!("{role}-outcome.json"));
        fs::write(
            &record_path,
            crate::canonical_json::to_canonical_bytes(&run_record)?,
        )
        .map_err(|error| PulseError::io(&record_path, error))?;

        let event = EventEnvelope::new(
            new_event_id(),
            "run.completed",
            format!("agent:{}", runner_actor(role)),
            ticket_id,
            json!({
                "role": role,
                "ticket_id": ticket_id,
                "lease_id": lease_id,
                "status": run.status,
                "inconclusive_reason": run.inconclusive_reason,
                "run_record": record_path.strip_prefix(&self.repo_root).ok().map(|p| p.to_string_lossy().to_string()),
            }),
            Utc::now(),
        );
        write_event(&self.repo_root, &event)?;

        Ok(RunOutcome {
            schema_version: 1,
            code: format!("run_{}", run.status),
            role: role.to_string(),
            ticket_id: ticket_id.to_string(),
            lease_id,
            status: run.status,
            inconclusive_reason: run.inconclusive_reason,
            summary: run.summary,
            run_record_path: record_path
                .strip_prefix(&self.repo_root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| record_path.display().to_string()),
            event_id: event.id,
        })
    }

    /// Reserve and activate the exclusive worker lease, moving the Ticket to
    /// `active`. The acknowledgement binds the committed packet fingerprint so
    /// a later contract drift is detectable.
    fn take_worker_lease(
        &self,
        role: &str,
        ticket_id: &str,
        ttl_seconds: u64,
        idempotency_key: &str,
        workspace: Option<&str>,
        acknowledge_drift: bool,
    ) -> PulseResult<(String, CoreReservation)> {
        let actor = runner_actor(role);
        let node = self.show_node(ticket_id)?;
        let key = if idempotency_key.trim().is_empty() {
            format!("run:{ticket_id}:{role}")
        } else {
            format!("run:{ticket_id}:{role}:{idempotency_key}")
        };
        // Resume-with-verification: an interrupted run leaves its lease (and
        // its committed packet) behind, with the Ticket still `active`. The
        // worker's own edits are expected drift, so resume compares the
        // semantic bindings only — contract revision and source commit. Drift
        // without an explicit acknowledgment is refused instead of silently
        // retried. Fresh dispatch below still requires `ready`.
        if let Some(lease_id) = crate::kernel::reservation::find_live_reservation_for_ticket(
            &self.repo_root,
            ticket_id,
        )? {
            let existing =
                crate::kernel::reservation::load_reservation(&self.repo_root, &lease_id)?;
            if existing.subject.ticket_id == ticket_id {
                let source_now = crate::source::head_commit(&self.repo_root)?;
                let contract_drift = existing.subject.contract_revision != node.contract_revision
                    || existing.source.commit != source_now;
                if contract_drift && !acknowledge_drift {
                    return Err(PulseError::validation(
                        "run_resume_drift",
                        format!(
                            "interrupted lease {lease_id} binds contract revision {} at commit {} but the Ticket is now revision {} at commit {}; re-run with --acknowledge-drift to release it and start fresh",
                            existing.subject.contract_revision,
                            existing.source.commit,
                            node.contract_revision,
                            source_now
                        ),
                    ));
                }
                if contract_drift {
                    // The operator's explicit flag releases the stale lease
                    // and returns an active Ticket to ready so the fresh
                    // reservation below can proceed.
                    self.release_live_lease_for_ticket(
                        ticket_id,
                        &actor,
                        "contract drift acknowledged",
                    )?;
                    // Fall through to a fresh reservation below.
                } else {
                    // Same contract: resume under the existing lease.
                    if existing.state == ReservationState::Reserved {
                        let session_id = format!("run:{}", existing.reservation_id);
                        let acknowledged = self.activate_reservation(ActivateReservationArgs {
                            lease_id: existing.lease_id.clone(),
                            actor: actor.clone(),
                            runtime_binding: RuntimeBinding {
                                project_id: ticket_id.to_string(),
                                workspace_id: workspace.unwrap_or("checkout").to_string(),
                                session_id: session_id.clone(),
                                provider_id: format!("runner:{role}"),
                            },
                            acknowledgement: AssignmentAcknowledgement {
                                acknowledgement_id: format!("ack:{session_id}"),
                                delivery_id: format!("delivery:{session_id}"),
                                session_id,
                                packet_fingerprint: existing.packet_fingerprint.clone(),
                                acknowledged_at: Utc::now().to_rfc3339(),
                            },
                        })?;
                        return Ok((existing.lease_id, acknowledged));
                    }
                    // Already acknowledged/active: reuse as-is.
                    let lease = existing.lease_id.clone();
                    return Ok((lease, existing));
                }
            }
        }
        if node.status != NodeStatus::Ready {
            return Err(PulseError::validation(
                "run_ticket_not_ready",
                format!(
                    "worker runs on ready Tickets; {} is {:?}",
                    ticket_id, node.status
                ),
            ));
        }
        let reserved = self.reserve_work(ReserveWorkArgs {
            ticket_id: ticket_id.to_string(),
            actor: actor.clone(),
            assignee: actor.clone(),
            ttl_seconds,
            idempotency_key: key,
        })?;
        let session_id = format!("run:{}", reserved.reservation.reservation_id);
        let active = self.activate_reservation(ActivateReservationArgs {
            lease_id: reserved.reservation.lease_id.clone(),
            actor,
            runtime_binding: RuntimeBinding {
                project_id: reserved.reservation.subject.ticket_id.clone(),
                workspace_id: workspace.unwrap_or("checkout").to_string(),
                session_id: session_id.clone(),
                provider_id: format!("runner:{role}"),
            },
            acknowledgement: AssignmentAcknowledgement {
                acknowledgement_id: format!("ack:{session_id}"),
                delivery_id: format!("delivery:{session_id}"),
                session_id,
                packet_fingerprint: reserved.reservation.packet_fingerprint.clone(),
                acknowledged_at: Utc::now().to_rfc3339(),
            },
        })?;
        Ok((reserved.reservation.lease_id, active))
    }

    /// Build the role input contract written to the run workspace.
    fn build_role_input(
        &self,
        class: &str,
        ticket_id: &str,
        node: &crate::graph::model::node::Node,
        reservation: &Option<CoreReservation>,
    ) -> PulseResult<Vec<u8>> {
        match class {
            "worker" => {
                let packet = match reservation {
                    Some(reservation) => {
                        self.work_packet_for_reservation(ticket_id, &reservation.lease_id)?
                    }
                    None => self.work_packet(ticket_id)?,
                };
                let mut value = serde_json::to_value(&packet).map_err(|error| {
                    PulseError::validation("run_input_invalid", error.to_string())
                })?;
                // The packet already passed schema validation; the map write
                // below stays canonical because serde_json preserved order.
                value["run_context"] = json!({
                    "repo_root": self.repo_root.display().to_string(),
                    "input_file": "worker-input.json",
                    "artifact_dir": "artifacts",
                    "actor": reservation
                        .as_ref()
                        .map(|reservation| reservation.assignee.clone()),
                    "lease_id": reservation
                        .as_ref()
                        .map(|reservation| reservation.lease_id.clone()),
                    "session_id": reservation.as_ref().and_then(|reservation| {
                        reservation.runtime_binding.as_ref().map(|binding| binding.session_id.clone())
                    }),
                    "source_commit": reservation
                        .as_ref()
                        .map(|reservation| reservation.source.commit.clone()),
                });
                Ok(crate::canonical_json::to_canonical_bytes(&value)?)
            }
            "reviewer" => {
                let acceptance_ids =
                    crate::kernel::completion::ticket_acceptance_ids(&self.repo_root, node)?;
                let handoffs = verifying_handoffs(&self.repo_root, ticket_id, node.revision)?;
                Ok(crate::canonical_json::to_canonical_bytes(&json!({
                    "schema_version": 1,
                    "ticket_id": ticket_id,
                    "source_commit": crate::source::head_commit(&self.repo_root)?,
                    "acceptance": acceptance_ids.into_iter().map(|id| json!({"id": id})).collect::<Vec<_>>(),
                    "handoffs": handoffs.iter().map(|handoff| json!({
                        "handoff_id": handoff.handoff_id,
                        "summary": handoff.summary,
                        "changed_paths": handoff.changed_paths,
                        "recorded_by": handoff.recorded_by,
                        "source_commit": handoff.source_commit,
                    })).collect::<Vec<_>>(),
                    "artifact_dir": "artifacts",
                }))?)
            }
            _ => {
                // Required posture resolves the current Story baseline; other
                // postures run with an explicit empty case list so the role
                // can report not_applicable observations.
                use crate::graph::model::contract::QaImpactPosture;
                let posture = node
                    .qa
                    .as_ref()
                    .map(|qa| qa.impact.posture)
                    .unwrap_or(QaImpactPosture::Unknown);
                let resolution = (posture == QaImpactPosture::Required)
                    .then(|| crate::qa::resolve_ticket_cases(&self.repo_root, node))
                    .transpose()?;
                Ok(crate::canonical_json::to_canonical_bytes(&json!({
                    "schema_version": 1,
                    "ticket_id": ticket_id,
                    "story_id": resolution.as_ref().map(|r| r.owner_id.clone()),
                    "source_commit": crate::source::head_commit(&self.repo_root)?,
                    "baseline_revision": resolution.as_ref().map(|r| r.revision),
                    "baseline_content_hash": resolution.as_ref().map(|r| r.content_hash.clone()),
                    "qa_posture": qa_posture_str(posture),
                    "cases": resolution.as_ref().map(|r| {
                        r.cases.iter().map(|case| json!({
                            "id": case.id,
                            "revision": case.revision,
                        })).collect::<Vec<_>>()
                    }).unwrap_or_default(),
                    "artifact_dir": "artifacts",
                }))?)
            }
        }
    }

    /// Classify one finished execution. Success claims are verified against
    /// graph truth; everything else is inconclusive by construction.
    fn classify_outcome(
        &self,
        class: &str,
        ticket_id: &str,
        lease_id: Option<&str>,
        outcome: &runner::Outcome,
    ) -> RunClassification {
        let exit_code = outcome.exit_code;
        let timed_out = outcome.timed_out;
        let cancelled = outcome.cancelled;
        if outcome.cancelled {
            return RunClassification {
                exit_code,
                timed_out,
                cancelled,
                ..RunClassification::inconclusive("cancelled", None)
            };
        }
        if outcome.timed_out {
            return RunClassification {
                exit_code,
                timed_out,
                cancelled,
                ..RunClassification::inconclusive("timeout", None)
            };
        }
        if !outcome.exited_cleanly() {
            return RunClassification {
                exit_code,
                timed_out,
                cancelled,
                ..RunClassification::inconclusive(
                    "exit_nonzero",
                    Some(format!("runner exited with {:?}", outcome.exit_code)),
                )
            };
        }
        let value = match runner::parse_output_json(outcome) {
            Ok(value) => value,
            Err(error) => {
                return RunClassification {
                    exit_code,
                    timed_out,
                    cancelled,
                    ..RunClassification::inconclusive("malformed_output", Some(error.to_string()))
                }
            }
        };
        match class {
            "worker" => {
                let status = value.get("status").and_then(|v| v.as_str());
                match status {
                    Some("blocked") => RunClassification {
                        status: "blocked".to_string(),
                        inconclusive_reason: None,
                        summary: value
                            .get("reason")
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                        exit_code,
                        timed_out,
                        cancelled,
                    },
                    Some("handed_off") => {
                        // The claim must be backed by a handoff proof bound to
                        // this lease and by the Ticket actually being in
                        // `verifying`.
                        let proven = lease_id
                            .and_then(|lease| find_lease_handoff(&self.repo_root, lease).ok())
                            .is_some()
                            && self
                                .show_node(ticket_id)
                                .map(|node| node.status == NodeStatus::Verifying)
                                .unwrap_or(false);
                        if proven {
                            RunClassification {
                                status: "handed_off".to_string(),
                                inconclusive_reason: None,
                                summary: value
                                    .get("summary")
                                    .and_then(|v| v.as_str())
                                    .map(str::to_string),
                                exit_code,
                                timed_out,
                                cancelled,
                            }
                        } else {
                            RunClassification::inconclusive(
                                "unproven_claim",
                                Some(
                                    "handed_off reported without a lease-bound handoff proof"
                                        .to_string(),
                                ),
                            )
                        }
                    }
                    _ => RunClassification::inconclusive(
                        "malformed_output",
                        Some("worker output status must be handed_off or blocked".to_string()),
                    ),
                }
            }
            _ => RunClassification {
                status: "completed".to_string(),
                inconclusive_reason: None,
                summary: None,
                exit_code,
                timed_out,
                cancelled,
            },
        }
    }

    /// Decide where this worker run executes: the checkout by default, or a
    /// Pulse-created worktree when another Ticket holds a live lease (auto)
    /// or the operator forces worktree isolation.
    fn decide_workspace(
        &self,
        ticket_id: &str,
        forced_worktree: bool,
        auto_isolation: bool,
    ) -> PulseResult<Option<String>> {
        if !forced_worktree {
            let foreign_live = has_live_foreign_lease(&self.repo_root, ticket_id)?;
            if !foreign_live {
                return Ok(None);
            }
            if !auto_isolation {
                return Err(PulseError::validation(
                    "run_isolation_refused",
                    format!(
                        "another Ticket holds a live lease and auto_isolation is disabled; run {ticket_id} later or pass --isolation worktree"
                    ),
                ));
            }
        }
        let relative = ensure_ticket_worktree(&self.repo_root, ticket_id)?;
        Ok(Some(relative))
    }
}

struct RunClassification {
    status: String,
    inconclusive_reason: Option<String>,
    summary: Option<String>,
    exit_code: Option<i32>,
    timed_out: bool,
    cancelled: bool,
}

impl RunClassification {
    fn inconclusive(reason: &str, detail: Option<String>) -> Self {
        Self {
            status: "inconclusive".to_string(),
            inconclusive_reason: Some(reason.to_string()),
            summary: detail,
            exit_code: None,
            timed_out: false,
            cancelled: false,
        }
    }
}

/// Machine-readable summary of one run, persisted next to the inputs.
#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunRecord {
    pub schema_version: u32,
    pub role: String,
    pub ticket_id: String,
    pub lease_id: Option<String>,
    pub status: String,
    pub inconclusive_reason: Option<String>,
    pub summary: Option<String>,
    /// Last 512 bytes of captured stderr when the run was inconclusive.
    pub stderr_tail: Option<String>,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub cancelled: bool,
    pub started_at: String,
    pub finished_at: String,
}

/// Root of Pulse-owned per-Ticket worktrees (gitignored).
pub const WORKTREES_DIR: &str = ".pulse/runtime/worktrees";

/// Whether any Ticket other than `ticket_id` holds a live lease.
fn has_live_foreign_lease(repo_root: &Path, ticket_id: &str) -> PulseResult<bool> {
    let directory = repo_root.join(".pulse/runtime/assignment/reservations");
    if !directory.exists() {
        return Ok(false);
    }
    let entries = fs::read_dir(&directory).map_err(|error| PulseError::io(&directory, error))?;
    for entry in entries {
        let path = entry
            .map_err(|error| PulseError::io(&directory, error))?
            .path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        let reservation: CoreReservation =
            serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))?;
        if reservation.subject.ticket_id != ticket_id
            && matches!(
                reservation.state,
                ReservationState::Reserved
                    | ReservationState::Acknowledged
                    | ReservationState::Active
            )
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Relative workspace id for a Ticket's Pulse-owned worktree.
fn ticket_worktree_rel(ticket_id: &str) -> String {
    format!("{WORKTREES_DIR}/{ticket_id}")
}

/// Create (or reuse) the Pulse-owned detached worktree for a Ticket.
fn ensure_ticket_worktree(repo_root: &Path, ticket_id: &str) -> PulseResult<String> {
    let relative = ticket_worktree_rel(ticket_id);
    let absolute = repo_root.join(&relative);
    if absolute.exists() {
        // Reuse a worktree from an earlier run of the same Ticket.
        if git_ok(repo_root, &["worktree", "list", "--porcelain"])?
            .contains(&absolute.to_string_lossy().to_string())
        {
            return Ok(relative);
        }
        // A stale plain directory is Pulse-owned runtime state; replace it.
        fs::remove_dir_all(&absolute).map_err(|error| PulseError::io(&absolute, error))?;
    }
    if let Some(parent) = absolute.parent() {
        fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
    }
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["worktree", "add", "--detach"])
        .arg(&absolute)
        .output()
        .map_err(|error| PulseError::io(repo_root, error))?;
    if !output.status.success() {
        return Err(PulseError::validation(
            "run_worktree_unavailable",
            format!(
                "git worktree add failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }
    // Ownership marker: only worktrees Pulse created (and marked) are ever
    // removed by cleanup.
    let marker = absolute.join(".pulse-owned");
    fs::write(&marker, b"created by pulse run\n")
        .map_err(|error| PulseError::io(&marker, error))?;
    Ok(relative)
}

/// Remove a Pulse-owned worktree after its Ticket reached a terminal state.
/// Only worktrees that are (a) under the Pulse worktrees root for this
/// Ticket and (b) registered with Git are ever removed; foreign directories
/// and worktrees are never touched.
pub(crate) fn cleanup_ticket_worktree(repo_root: &Path, ticket_id: &str) -> PulseResult<()> {
    let relative = ticket_worktree_rel(ticket_id);
    let absolute = repo_root.join(&relative);
    if !absolute.exists() {
        return Ok(());
    }
    let registered = git_ok(repo_root, &["worktree", "list", "--porcelain"])?
        .contains(&absolute.to_string_lossy().to_string());
    let owned = absolute.join(".pulse-owned").exists();
    if !registered || !owned {
        // Not ours (foreign worktree, or no longer a Git worktree): leave it
        // alone.
        return Ok(());
    }
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["worktree", "remove", "--force"])
        .arg(&absolute)
        .output()
        .map_err(|error| PulseError::io(repo_root, error))?;
    if !output.status.success() {
        return Err(PulseError::validation(
            "run_worktree_cleanup_failed",
            format!(
                "git worktree remove failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }
    Ok(())
}

fn git_ok(repo_root: &Path, args: &[&str]) -> PulseResult<String> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .output()
        .map_err(|error| PulseError::io(repo_root, error))?;
    if !output.status.success() {
        return Err(PulseError::validation(
            "run_worktree_unavailable",
            format!(
                "git {} failed: {}",
                args.first().unwrap_or(&""),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Find the handoff proof bound to `lease_id`, if any.
pub(crate) fn find_lease_handoff(
    repo_root: &Path,
    lease_id: &str,
) -> PulseResult<crate::execution::HandoffReceipt> {
    let directory = repo_root.join(".pulse/evidence/execution/handoffs");
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(PulseError::validation(
                "run_handoff_missing",
                "no handoff proof exists for this run",
            ))
        }
        Err(error) => return Err(PulseError::io(&directory, error)),
    };
    let mut paths: Vec<PathBuf> = entries
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|error| PulseError::io(&directory, error))?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    paths.sort();
    for path in paths {
        let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        let receipt: crate::execution::HandoffReceipt =
            serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))?;
        if receipt.lease_id == lease_id {
            return crate::kernel::completion::load_handoff(repo_root, &receipt.handoff_id);
        }
    }
    Err(PulseError::validation(
        "run_handoff_missing",
        format!("no handoff proof is bound to lease {lease_id}"),
    ))
}

/// Handoff receipts binding a Ticket's current verifying revision, sorted
/// by handoff id. Empty when none match — the reviewer input reports the
/// truth instead of failing the run.
fn verifying_handoffs(
    repo_root: &Path,
    ticket_id: &str,
    verifying_revision: u64,
) -> PulseResult<Vec<crate::execution::HandoffReceipt>> {
    let directory = repo_root.join(".pulse/evidence/execution/handoffs");
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(PulseError::io(&directory, error)),
    };
    let mut handoffs: Vec<crate::execution::HandoffReceipt> = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| PulseError::io(&directory, error))?
            .path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        let handoff: crate::execution::HandoffReceipt =
            serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))?;
        if handoff.ticket_id == ticket_id && handoff.verifying_revision == verifying_revision {
            handoffs.push(handoff);
        }
    }
    handoffs.sort_by(|left, right| left.handoff_id.cmp(&right.handoff_id));
    Ok(handoffs)
}

fn qa_posture_str(posture: crate::graph::model::contract::QaImpactPosture) -> &'static str {
    use crate::graph::model::contract::QaImpactPosture;
    match posture {
        QaImpactPosture::Unknown => "unknown",
        QaImpactPosture::Required => "required",
        QaImpactPosture::CoveredByStoryClose => "covered_by_story_close",
        QaImpactPosture::None => "none",
    }
}

/// The bootstrap prompt points the agent at the packet and the CLI workflow;
/// it never copies the contract into the prompt. Commands are exact, with the
/// run's lease/session/source facts filled in, so the agent cannot fumble
/// the proof syntax.
fn worker_prompt(ticket_id: &str, lease_id: &str, session_id: &str, source_commit: &str) -> String {
    format!(
        "# Pulse worker run — {ticket_id}\n\
         You are `agent:runner:worker` implementing Ticket {ticket_id} in this\n\
         repository. The process runs at the repository root; the `pulse` CLI\n\
         is on PATH. Work only inside this Ticket's contract.\n\
         \n\
         1. Read `worker-input.json` in this directory. It is your complete\n\
         work packet (contract prose, QA cases, docs, source fence).\n\
         `worker-env` holds the same run facts as shell variables.\n\
         2. Read required docs with `pulse docs get <section-ref>`; do not guess.\n\
         3. Implement. Do not change acceptance criteria. Do not run\n\
         `git commit` — leave changes in the working tree; the developer\n\
         commits after close.\n\
         4. When done, record the handoff proof (edit the summary and paths):\n\
         \n\
         pulse --idempotency-key handoff:{ticket_id}:{lease_id} work handoff \\\n\
           --lease {lease_id} \\\n\
           --session {session_id} \\\n\
           --source-commit {source_commit} \\\n\
           --summary \"<what changed and how you verified it>\" \\\n\
           --changed-path src/example.ext\n\
         \n\
         5. Your last output line must be exactly one JSON object with\n\
         nothing after it (no code fence, no trailing prose):\n\
         `{{\"status\": \"handed_off\", \"summary\": \"<one line>\"}}`\n\
         6. If you cannot finish: `pulse note --ticket {ticket_id} --message\n\
         \"<why>\" --from agent:runner:worker`, then end with\n\
         `{{\"status\": \"blocked\", \"reason\": \"<why>\"}}`.\n\
         \n\
         Proof comes from the CLI receipt in step 4; the final JSON line is\n\
         only a summary. Never claim handed_off without a successful step 4.\n"
    )
}

/// Bootstrap prompt for the independent reviewer agent. Like the worker
/// prompt it carries workflow and identity only, plus the exact verify
/// syntax with the run's facts filled in.
fn reviewer_prompt(ticket_id: &str, source_commit: &str) -> String {
    format!(
        "# Pulse reviewer run — {ticket_id}\n\
         You are `agent:runner:reviewer` independently reviewing the handoff\n\
         of Ticket {ticket_id} in this repository. The process runs at the\n\
         repository root; the `pulse` CLI is on PATH.\n\
         \n\
         1. Read `reviewer-input.json` in this directory: the acceptance\n\
         ids, the handoff receipt(s) to review, and the artifact directory.\n\
         2. Review independently. Inspect the working-tree changes\n\
         (`git status --porcelain`, `git diff`), re-read the Ticket contract\n\
         (`pulse work show {ticket_id} --json`), and re-run this repository's\n\
         verification command yourself (see its AGENTS.md). Do not trust the\n\
         worker summary.\n\
         3. Collect the proof receipts recorded for this Ticket:\n\
         `pulse evidence receipt list --kind qa_checkpoint --json` and\n\
         `pulse evidence receipt list --kind documentation_validation --json`.\n\
         4. Record the verdict — one --check per command you ran, exactly\n\
         one --proof per acceptance id mapping it to checks and/or receipts:\n\
         \n\
         pulse --idempotency-key verify:{ticket_id}:<handoff_id> work verify \\\n\
           --handoff <handoff_id> \\\n\
           --actor agent:runner:reviewer \\\n\
           --source-commit {source_commit} \\\n\
           --disposition passed \\\n\
           --summary \"<evidence-based summary>\" \\\n\
           --check \"verify=<the command you ran>=0\" \\\n\
           --proof \"AC-1=verify=<receipt_id>,…\"\n\
         \n\
         `passed` requires every check to exit 0. Use `--disposition\n\
         rework` with findings when the handoff does not hold.\n\
         5. Your last output line must be exactly one JSON object with\n\
         nothing after it (no code fence):\n\
         `{{\"disposition\": \"pass\", \"acceptance\": {{…}}, \"findings\": []}}`\n"
    )
}
