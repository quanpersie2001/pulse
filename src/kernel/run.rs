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
use serde::Serialize;
use serde_json::json;

use std::time::Duration;

use crate::event::{new_event_id, write_event, EventEnvelope};
use crate::graph::model::node::NodeStatus;
use crate::graph::store::JsonGraphStore;
use crate::identity::actor::ActorKind;
use crate::policy::authority::{authority_path, AuthorityPrincipal};
use crate::reservation::{
    ActivateReservationArgs, AssignmentAcknowledgement, CoreReservation, ReserveWorkArgs,
    RuntimeBinding,
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

/// Load and validate `.pulse/config/runners.json`.
pub fn load_runner_roles(repo_root: &Path) -> PulseResult<BTreeMap<String, CommandSpec>> {
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
    let roles: BTreeMap<String, CommandSpec> =
        serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))?;
    for (role, spec) in &roles {
        role_class(role)?;
        spec.validate().map_err(|error| {
            PulseError::validation("run_config_invalid", format!("role {role}: {error}"))
        })?;
    }
    Ok(roles)
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
    pub fn run_role(
        &self,
        role: &str,
        ticket_id: &str,
        ttl_seconds: u64,
        idempotency_key: &str,
    ) -> PulseResult<RunOutcome> {
        let class = role_class(role)?;
        let roles = load_runner_roles(&self.repo_root)?;
        let spec = roles.get(role).ok_or_else(|| {
            PulseError::validation(
                "run_role_missing",
                format!("role {role} is not defined in .pulse/config/runners.json"),
            )
        })?;
        provision_runner_actor(&self.repo_root, role, class)?;

        let node = self.show_node(ticket_id)?;
        let started = Utc::now();
        let (lease_id, reservation) = match class {
            "worker" => {
                let outcome =
                    self.take_worker_lease(role, ticket_id, ttl_seconds, idempotency_key)?;
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
            let prompt_path = run_dir.join("worker-prompt.md");
            fs::write(&prompt_path, worker_prompt(ticket_id))
                .map_err(|error| PulseError::io(&prompt_path, error))?;
            // Shell-sourceable run facts the agent script needs to record
            // proofs through the CLI.
            let env_path = run_dir.join("worker-env");
            let env_text = match &reservation {
                Some(reservation) => format!(
                    "TICKET_ID=\"{ticket_id}\"\nLEASE_ID=\"{}\"\nSESSION_ID=\"{}\"\nSOURCE_COMMIT=\"{}\"\nACTOR=\"{}\"\n",
                    reservation.lease_id,
                    reservation
                        .runtime_binding
                        .as_ref()
                        .map(|binding| binding.session_id.as_str())
                        .unwrap_or_default(),
                    reservation.source.commit,
                    reservation.assignee,
                ),
                None => format!("TICKET_ID=\"{ticket_id}\"\n"),
            };
            fs::write(&env_path, env_text).map_err(|error| PulseError::io(&env_path, error))?;
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
        let outcome = runner::execute(
            &self.repo_root,
            &argv,
            Duration::from_secs(spec.timeout_seconds),
            spec.max_output_bytes,
            None,
        );

        let run = match outcome {
            Ok(outcome) => self.classify_outcome(class, ticket_id, lease_id.as_deref(), &outcome),
            Err(error) => RunClassification {
                status: "inconclusive".to_string(),
                inconclusive_reason: Some("spawn_failed".to_string()),
                summary: Some(error.to_string()),
                exit_code: None,
                timed_out: false,
                cancelled: false,
            },
        };

        let run_record = RunRecord {
            schema_version: 1,
            role: role.to_string(),
            ticket_id: ticket_id.to_string(),
            lease_id: lease_id.clone(),
            status: run.status.clone(),
            inconclusive_reason: run.inconclusive_reason.clone(),
            summary: run.summary.clone(),
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
    ) -> PulseResult<(String, CoreReservation)> {
        let actor = runner_actor(role);
        let node = self.show_node(ticket_id)?;
        if node.status != NodeStatus::Ready {
            return Err(PulseError::validation(
                "run_ticket_not_ready",
                format!(
                    "worker runs on ready Tickets; {} is {:?}",
                    ticket_id, node.status
                ),
            ));
        }
        let key = if idempotency_key.trim().is_empty() {
            format!("run:{ticket_id}:{role}")
        } else {
            format!("run:{ticket_id}:{role}:{idempotency_key}")
        };
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
                workspace_id: "checkout".to_string(),
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
                Ok(crate::canonical_json::to_canonical_bytes(&json!({
                    "schema_version": 1,
                    "ticket_id": ticket_id,
                    "source_commit": crate::source::head_commit(&self.repo_root)?,
                    "acceptance": acceptance_ids.into_iter().map(|id| json!({"id": id})).collect::<Vec<_>>(),
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
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub cancelled: bool,
    pub started_at: String,
    pub finished_at: String,
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
/// it never copies the contract into the prompt.
fn worker_prompt(ticket_id: &str) -> String {
    format!(
        "# Pulse worker run — {ticket_id}\n\n\
         1. Read `worker-input.json` in this directory. It is your complete work packet.\n\
         2. Read required docs with `pulse docs get <section-ref>`; do not guess.\n\
         3. Implement inside the contract. Do not change acceptance criteria.\n\
         4. When done, call `pulse work handoff {ticket_id} --actor runner:worker ...` with an\n\
         acceptance-to-check mapping, then print one final JSON line:\n\
         `{{\"status\": \"handed_off\", \"summary\": \"...\"}}`\n\
         5. If blocked, call `pulse work transition {ticket_id} blocked --reason ...` (or leave a\n\
         `pulse note --ticket {ticket_id}`) and print `{{\"status\": \"blocked\", \"reason\": \"...\"}}`.\n\n\
         The final JSON line is a summary only; proof comes from the CLI receipts you record.\n"
    )
}
