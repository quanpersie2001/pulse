use std::path::PathBuf;

use crate::execution::{AcceptanceProof, VerificationCheck, VerificationDisposition};
use crate::graph::model::contract::{Materialization, Risk, TicketRole};
use crate::graph::model::node::NodeStatus;
use crate::id::WorkKind;
use clap::{Subcommand, ValueEnum};

/// Verification disposition for `pulse work verify`.
#[derive(Clone, Copy, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum VerifyDispositionArg {
    Passed,
    Rework,
    Blocked,
}

impl From<VerifyDispositionArg> for VerificationDisposition {
    fn from(value: VerifyDispositionArg) -> Self {
        match value {
            VerifyDispositionArg::Passed => VerificationDisposition::Passed,
            VerifyDispositionArg::Rework => VerificationDisposition::Rework,
            VerifyDispositionArg::Blocked => VerificationDisposition::Blocked,
        }
    }
}

/// Parse `LRN-001=helpful|not_needed|misleading` for `--learning-used`.
fn parse_learning_used(
    value: &str,
) -> Result<(String, crate::execution::KnowledgeUsageOutcome), String> {
    use crate::execution::KnowledgeUsageOutcome;
    let (learning_id, outcome) = value
        .split_once('=')
        .ok_or_else(|| format!("expected <learning-id>=<outcome>, got {value:?}"))?;
    let learning_id = learning_id.trim();
    if learning_id.is_empty() {
        return Err(format!("empty learning id in {value:?}"));
    }
    let outcome = match outcome.trim() {
        "helpful" => KnowledgeUsageOutcome::Helpful,
        "not_needed" => KnowledgeUsageOutcome::NotNeeded,
        "misleading" => KnowledgeUsageOutcome::Misleading,
        other => {
            return Err(format!(
                "unknown usage outcome {other:?}; expected helpful, not_needed or misleading"
            ))
        }
    };
    Ok((learning_id.to_string(), outcome))
}

fn parse_check(value: &str) -> Result<VerificationCheck, String> {
    let parts: Vec<&str> = value.splitn(3, '=').collect();
    let [name, command, exit] = parts.as_slice() else {
        return Err("check must be name=command=exit_code".to_string());
    };
    let exit_code = exit
        .parse::<i32>()
        .map_err(|_| format!("check exit_code must be an integer: {exit}"))?;
    Ok(VerificationCheck {
        name: (*name).to_string(),
        command: (*command).to_string(),
        exit_code,
        artifact_ids: vec![],
    })
}

fn parse_proof(value: &str) -> Result<AcceptanceProof, String> {
    let parts: Vec<&str> = value.splitn(3, '=').collect();
    let [acceptance_id, checks, receipts] = parts.as_slice() else {
        return Err("proof must be AC-ID=checks=receipts".to_string());
    };
    let split_list = |value: &str| -> Vec<String> {
        value
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(str::to_string)
            .collect()
    };
    Ok(AcceptanceProof {
        acceptance_id: (*acceptance_id).to_string(),
        check_names: split_list(checks),
        evidence_receipt_ids: split_list(receipts),
    })
}

/// Parse one shaped finding for `--finding`, as
/// `<AC-ID|->|<summary>|<owner>|<check|->|<severity>` (Decision 0012 §3).
/// `-` drops the optional field: no acceptance id, or no check — the latter
/// marks the finding `unverifiable`. Severity is high|medium|low.
fn parse_finding(value: &str) -> Result<crate::execution::Finding, String> {
    use crate::execution::{Finding, FindingSeverity};
    let parts: Vec<&str> = value.splitn(5, '|').collect();
    let [acceptance_id, summary, owner, check, severity] = parts.as_slice() else {
        return Err(
            "finding must be AC-ID|summary|owner|check|severity (use - for a missing AC-ID or check)"
                .to_string(),
        );
    };
    let field = |raw: &str, name: &str| -> Result<String, String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(format!("finding {name} must not be empty"));
        }
        Ok(trimmed.to_string())
    };
    let acceptance_id = if acceptance_id.trim() == "-" {
        None
    } else {
        Some(field(acceptance_id, "acceptance id")?)
    };
    let check = if check.trim() == "-" {
        None
    } else {
        Some(field(check, "check")?)
    };
    let severity = match severity.trim() {
        "high" => FindingSeverity::High,
        "medium" => FindingSeverity::Medium,
        "low" => FindingSeverity::Low,
        other => {
            return Err(format!(
                "finding severity must be high, medium or low, got {other:?}"
            ))
        }
    };
    let mut finding = Finding {
        summary: field(summary, "summary")?,
        owner: field(owner, "owner")?,
        check,
        severity,
        acceptance_id,
        case_id: None,
        unverifiable: false,
    };
    finding.normalize();
    Ok(finding)
}

#[derive(Subcommand)]
pub(crate) enum WorkCommand {
    Create {
        #[arg(long)]
        kind: KindArg,
        #[arg(long)]
        title: String,
        #[arg(long)]
        role: Option<TicketRoleArg>,
        #[arg(long)]
        risk: Option<RiskArg>,
        #[arg(long)]
        materialization: Option<MaterializationArg>,
        #[arg(long)]
        parent: Option<String>,
        #[arg(long = "tag")]
        tags: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    List {
        #[arg(long)]
        kind: Option<KindArg>,
        #[arg(long)]
        json: bool,
    },
    Edit {
        id: String,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        title: String,
        #[arg(long)]
        json: bool,
    },
    /// Synchronize the graph contract binding from works/<id>/ticket.md.
    Sync {
        id: String,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long, default_value = "human:unknown")]
        actor: String,
        #[arg(long)]
        json: bool,
    },
    Supersede {
        old_id: String,
        #[arg(long = "by", conflicts_with = "decision")]
        by: Option<String>,
        #[arg(long, conflicts_with = "by")]
        decision: Option<String>,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        reason: String,
        #[arg(long, hide = true)]
        assertion: Option<PathBuf>,
        #[arg(long)]
        reconciliation_receipt: Option<String>,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        json: bool,
    },
    Transition {
        id: String,
        #[arg(long = "to")]
        to: StatusArg,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        reason_code: Option<String>,
        #[arg(long = "reason")]
        reason: Option<String>,
        #[arg(long)]
        reference: Option<String>,
        #[arg(long)]
        expected_readiness_fingerprint: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Executability {
        id: String,
        #[arg(long)]
        json: bool,
    },
    Ready {
        id: String,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Rollup {
        id: String,
        #[arg(long)]
        json: bool,
    },
    Close {
        /// Ticket ID whose current passed verification should be closed.
        id: String,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        source_commit: String,
        #[arg(long)]
        summary: String,
        #[arg(long)]
        json: bool,
    },
    /// Free the live lease a Ticket holds and return an active Ticket to
    /// ready (recovery for a stuck, crashed or expired run).
    Release {
        ticket_id: String,
        #[arg(long)]
        actor: String,
        #[arg(long, default_value = "released by operator")]
        reason: String,
        #[arg(long)]
        json: bool,
    },
    /// Submit the worker handoff proof for an active assignment.
    Handoff {
        /// Lease ID binding this handoff to its assignment (from the run input).
        #[arg(long)]
        lease: String,
        #[arg(long)]
        session: String,
        #[arg(long, default_value = "agent:runner:worker")]
        actor: String,
        #[arg(long)]
        source_commit: String,
        #[arg(long)]
        summary: String,
        #[arg(long = "changed-path")]
        changed_paths: Vec<String>,
        #[arg(long = "evidence-receipt")]
        evidence_receipt_ids: Vec<String>,
        /// Claimed check as `name=command=exit_code`; the reviewer re-runs it.
        #[arg(long = "check", value_parser = parse_check)]
        checks: Vec<VerificationCheck>,
        /// Claimed acceptance coverage as `AC-ID=check1,check2=receipt1,receipt2`.
        #[arg(long = "proof", value_parser = parse_proof)]
        proofs: Vec<AcceptanceProof>,
        /// Learning usage feedback as `LRN-001=helpful|not_needed|misleading`.
        #[arg(long = "learning-used", value_parser = parse_learning_used)]
        learning_used: Vec<(String, crate::execution::KnowledgeUsageOutcome)>,
        #[arg(long)]
        json: bool,
    },
    /// Record an independent verification observation for a handoff.
    Verify {
        ticket_id: String,
        #[arg(long)]
        handoff: String,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        source_commit: String,
        #[arg(long, default_value = "passed")]
        disposition: VerifyDispositionArg,
        #[arg(long)]
        summary: String,
        /// Passing/failed check as `name=command=exit_code`.
        #[arg(long = "check", value_parser = parse_check)]
        checks: Vec<VerificationCheck>,
        /// Acceptance proof as `AC-ID=check1,check2=receipt1,receipt2`.
        #[arg(long = "proof", value_parser = parse_proof)]
        proofs: Vec<AcceptanceProof>,
        /// Shaped finding as `AC-ID|summary|owner|check|severity` (use - for
        /// a missing AC-ID or check; severity is high|medium|low). A rework
        /// needs at least one finding whose check is present.
        #[arg(long = "finding", value_parser = parse_finding)]
        findings: Vec<crate::execution::Finding>,
        #[arg(long)]
        json: bool,
    },
    CloseStory {
        story_id: String,
        #[arg(long, required = true, value_delimiter = ',')]
        qualification_receipt: Vec<String>,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        source_commit: String,
        #[arg(long)]
        summary: String,
        #[arg(long)]
        json: bool,
    },
    Frontier {
        #[arg(long)]
        for_: Option<String>,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        include_excluded: bool,
        #[arg(long)]
        json: bool,
    },
    /// Build a preview work packet for a ready implementation Ticket.
    ///
    /// Output is a bounded, deterministic context snapshot. The packet does
    /// not acquire a lease, create a workspace or change lifecycle.
    Packet {
        /// Ticket ID (must be an implementation Ticket with lifecycle ready).
        id: String,
        /// Load the exact packet committed for this Core reservation lease.
        #[arg(long)]
        lease: Option<String>,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, ValueEnum)]
pub(crate) enum KindArg {
    Epic,
    Story,
    Ticket,
    Decision,
}

#[derive(Clone, Copy, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum TicketRoleArg {
    Implementation,
    DecisionWork,
}

#[derive(Clone, Copy, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum RiskArg {
    Unassessed,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Copy, ValueEnum)]
#[value(rename_all = "verbatim")]
pub(crate) enum MaterializationArg {
    Unassessed,
    R0,
    R1,
    R2,
    R3,
}

#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum StatusArg {
    Draft,
    Shaped,
    Ready,
    Active,
    Verifying,
    Done,
    Rework,
    Blocked,
    Cancelled,
    Superseded,
}

impl From<StatusArg> for NodeStatus {
    fn from(value: StatusArg) -> Self {
        match value {
            StatusArg::Draft => NodeStatus::Draft,
            StatusArg::Shaped => NodeStatus::Shaped,
            StatusArg::Ready => NodeStatus::Ready,
            StatusArg::Active => NodeStatus::Active,
            StatusArg::Verifying => NodeStatus::Verifying,
            StatusArg::Done => NodeStatus::Done,
            StatusArg::Rework => NodeStatus::Rework,
            StatusArg::Blocked => NodeStatus::Blocked,
            StatusArg::Cancelled => NodeStatus::Cancelled,
            StatusArg::Superseded => NodeStatus::Superseded,
        }
    }
}

impl From<KindArg> for WorkKind {
    fn from(value: KindArg) -> Self {
        match value {
            KindArg::Epic => WorkKind::Epic,
            KindArg::Story => WorkKind::Story,
            KindArg::Ticket => WorkKind::Ticket,
            KindArg::Decision => WorkKind::Decision,
        }
    }
}

impl From<TicketRoleArg> for TicketRole {
    fn from(value: TicketRoleArg) -> Self {
        match value {
            TicketRoleArg::Implementation => TicketRole::Implementation,
            TicketRoleArg::DecisionWork => TicketRole::DecisionWork,
        }
    }
}

impl From<RiskArg> for Risk {
    fn from(value: RiskArg) -> Self {
        match value {
            RiskArg::Unassessed => Risk::Unassessed,
            RiskArg::Low => Risk::Low,
            RiskArg::Medium => Risk::Medium,
            RiskArg::High => Risk::High,
            RiskArg::Critical => Risk::Critical,
        }
    }
}

impl From<MaterializationArg> for Materialization {
    fn from(value: MaterializationArg) -> Self {
        match value {
            MaterializationArg::Unassessed => Materialization::Unassessed,
            MaterializationArg::R0 => Materialization::R0,
            MaterializationArg::R1 => Materialization::R1,
            MaterializationArg::R2 => Materialization::R2,
            MaterializationArg::R3 => Materialization::R3,
        }
    }
}

use serde_json::json;

use crate::cli::output::render;
use crate::graph::model::contract::PublicCreateClassification;
use crate::graph::model::lifecycle::TransitionReason;
use crate::graph::store::SupersessionTarget;
use crate::{JsonGraphStore, PulseError};

pub(crate) fn handle(
    store: &JsonGraphStore,
    command: WorkCommand,
    explicit_key: Option<&str>,
) -> Result<(), PulseError> {
    match command {
        WorkCommand::Create {
            kind,
            title,
            role,
            risk,
            materialization,
            parent,
            tags,
            json,
        } => {
            let classification = PublicCreateClassification {
                role: role.map(Into::into),
                risk: risk.map(Into::into),
                materialization: materialization.map(Into::into),
            };
            let out = store.create_node_public_with_context_and_tags(
                kind.into(),
                title,
                classification,
                tags,
                crate::graph::store::OperationContext::default(),
            )?;
            if let Some(parent) = parent {
                store.add_edge(
                    crate::graph::model::edge::EdgeType::Parent,
                    out.value.id.clone(),
                    parent,
                    "human:unknown".to_string(),
                )?;
            }
            render(json, &out, format!("created {}", out.value.id))
        }
        WorkCommand::Sync {
            id,
            expected_revision,
            actor,
            json,
        } => {
            let out = store.sync_ticket(&id, expected_revision, actor)?;
            render(json, &out, format!("synchronized {}", out.value.id))
        }
        WorkCommand::Show { id, json } => {
            let node = store.show_node(&id)?;
            let human = node.title.clone();
            let brief = if node.kind == WorkKind::Ticket {
                Some(store.read_ticket_brief(&id)?)
            } else {
                None
            };
            render(
                json,
                &json!({"schema_version": 1, "code": "ok", "node": node, "brief": brief}),
                human,
            )
        }
        WorkCommand::List { kind, json } => {
            let out = store.list_nodes(kind.map(Into::into))?;
            render(json, &out, format!("{} work items", out.items.len()))
        }
        WorkCommand::Edit {
            id,
            expected_revision,
            title,
            json,
        } => {
            let out = store.edit_title(&id, expected_revision, title)?;
            render(json, &out, format!("updated {}", out.value.id))
        }
        WorkCommand::Supersede {
            old_id,
            by,
            decision,
            expected_revision,
            reason,
            assertion,
            reconciliation_receipt,
            actor,
            json,
        } => {
            let target = match (by, decision) {
                (Some(id), None) => SupersessionTarget::Replacement { id },
                (None, Some(id)) => SupersessionTarget::Decision { id },
                _ => {
                    return Err(PulseError::validation(
                        "invalid_supersession_target_form",
                        "choose exactly one of --by or --decision",
                    ));
                }
            };
            if assertion.is_some() {
                return Err(PulseError::validation(
                "inline_supersession_assertion_unsupported",
                "new supersession CLI requires --reconciliation-receipt; inline --assertion is retained only for historical/library compatibility",
            ));
            }
            let Some(receipt_id) = reconciliation_receipt else {
                return Err(PulseError::validation(
                    "supersession_receipt_required",
                    "new supersession CLI requires --reconciliation-receipt",
                ));
            };
            let out = store.supersede_work_with_receipt(
                &old_id,
                target,
                expected_revision,
                reason,
                receipt_id,
                actor,
            )?;
            render(json, &out, format!("{} {}", out.code, out.value.node.id))
        }
        WorkCommand::Transition {
            id,
            to,
            expected_revision,
            actor,
            reason_code,
            reason,
            reference,
            expected_readiness_fingerprint,
            json,
        } => {
            let transition_reason = match (reason_code, reason, reference) {
                (None, None, None) => None,
                (Some(code), Some(summary), reference) => Some(TransitionReason {
                    code,
                    summary,
                    reference,
                }),
                _ => {
                    return Err(PulseError::validation(
                        "missing_status_reason",
                        "transition reason requires --reason-code and --reason together",
                    ));
                }
            };
            let out = store.transition_node_gated_with_context(
                &id,
                to.into(),
                expected_revision,
                transition_reason,
                expected_readiness_fingerprint.as_deref(),
                crate::graph::store::OperationContext {
                    actor: actor.clone(),
                    now: chrono::Utc::now(),
                },
            )?;
            render(json, &out, format!("transitioned {}", out.value.id))
        }
        WorkCommand::Executability { id, json } => {
            let out = store.executability(&id)?;
            render(
                json,
                &out,
                format!("{:?} {}", out.structural_state, out.subject),
            )
        }
        WorkCommand::Ready { id, profile, json } => {
            if profile.is_some()
                && profile.as_deref() != Some(crate::graph::read::readiness::READINESS_PROFILE)
            {
                return Err(PulseError::validation(
                    "readiness_profile_unsupported",
                    format!(
                        "unsupported readiness profile; only {} is available in this release",
                        crate::graph::read::readiness::READINESS_PROFILE
                    ),
                ));
            }
            let out = store.readiness(&id)?;
            let human = format!(
                "{} {} ({} families passing)",
                out.subject.id,
                out.status_as_word(),
                out.gate_families
                    .iter()
                    .filter(|family| {
                        matches!(
                            family.status,
                            crate::graph::read::readiness::GateStatus::Passed
                                | crate::graph::read::readiness::GateStatus::NotApplicable
                        )
                    })
                    .count()
            );
            render(json, &out, human)?;
            if out.status == crate::graph::read::readiness::ReadinessStatus::Ready {
                Ok(())
            } else {
                Err(PulseError::validation(
                    "readiness_not_ready",
                    format!(
                        "work {} is {} under {}",
                        out.subject.id,
                        out.status_as_word(),
                        out.profile
                    ),
                ))
            }
        }
        WorkCommand::Rollup { id, json } => {
            let out = store.rollup(&id)?;
            render(json, &out, format!("rollup {}", out.subject))
        }
        WorkCommand::Close {
            id,
            actor,
            source_commit,
            summary,
            json,
        } => {
            let out = store.close_execution_ticket_for_ticket(
                &id,
                actor,
                source_commit,
                summary,
                explicit_key.unwrap_or_default().to_string(),
            )?;
            render(json, &out, format!("closed Ticket {}", out.ticket_id))
        }
        WorkCommand::Release {
            ticket_id,
            actor,
            reason,
            json,
        } => {
            let out = store.release_live_lease_for_ticket(&ticket_id, &actor, &reason)?;
            render(json, &out, format!("released lease {}", out.lease_id))
        }
        WorkCommand::Handoff {
            lease,
            session,
            actor,
            source_commit,
            summary,
            changed_paths,
            evidence_receipt_ids,
            checks,
            proofs,
            learning_used,
            json,
        } => {
            let out = store.submit_execution_handoff(crate::execution::SubmitHandoffArgs {
                lease_id: lease,
                actor,
                session_id: session,
                source_commit,
                summary,
                changed_paths,
                evidence_receipt_ids,
                checks,
                acceptance_proofs: proofs,
                learning_usage: learning_used
                    .into_iter()
                    .map(
                        |(learning_id, outcome)| crate::execution::KnowledgeUsageClaim {
                            learning_id,
                            outcome,
                        },
                    )
                    .collect(),
                idempotency_key: explicit_key.unwrap_or_default().to_string(),
            })?;
            render(
                json,
                &out,
                format!("handed off Ticket {} ({})", out.ticket_id, out.handoff_id),
            )
        }
        WorkCommand::Verify {
            ticket_id: _,
            handoff,
            actor,
            source_commit,
            disposition,
            summary,
            checks,
            proofs,
            findings,
            json,
        } => {
            let out = store.complete_execution_verification(
                crate::execution::CompleteVerificationArgs {
                    handoff_id: handoff,
                    actor,
                    source_commit,
                    disposition: disposition.into(),
                    summary,
                    checks,
                    acceptance_proofs: proofs,
                    findings,
                    idempotency_key: explicit_key.unwrap_or_default().to_string(),
                },
            )?;
            render(
                json,
                &out,
                format!(
                    "verification {} -> {}",
                    out.verification_id, out.resulting_status
                ),
            )
        }
        WorkCommand::CloseStory {
            story_id,
            qualification_receipt,
            actor,
            source_commit,
            summary,
            json,
        } => {
            let out = store.close_story(crate::execution::CloseStoryArgs {
                story_id,
                qualification_receipt_ids: qualification_receipt,
                actor,
                source_commit,
                summary,
                idempotency_key: explicit_key.unwrap_or_default().to_string(),
            })?;
            render(json, &out, format!("closed Story {}", out.story_id))
        }
        WorkCommand::Frontier {
            for_,
            profile,
            include_excluded,
            json,
        } => {
            let out = store.frontier(for_.as_deref(), profile.as_deref(), include_excluded)?;
            let human = format!(
                "execution frontier: {} item(s){}",
                out.items.len(),
                out.for_
                    .as_ref()
                    .map(|owner| format!(" for {owner}"))
                    .unwrap_or_default()
            );
            render(json, &out, human)
        }
        WorkCommand::Packet { id, lease, json } => {
            let packet = match lease {
                Some(lease_id) => store.work_packet_for_lease(&id, &lease_id)?,
                None => store.work_packet(&id)?,
            };
            packet.validate_schema_contract()?;
            let human = packet_human(&packet);
            render(json, &packet, human)
        }
    }
}

fn packet_human(packet: &crate::work_packet::WorkPacket) -> String {
    format!(
        "{} packet: {}\nsource: {} ({})\nrequired docs: {}\nsuggested sections: {}\nblockers: {}\npacket fingerprint: {}",
        packet.ticket.node.id,
        packet.code,
        packet.source.commit,
        if packet.source.dirty { "dirty" } else { "clean" },
        packet.docs.required.len(),
        packet.docs.suggested.len(),
        packet.blockers.len(),
        packet.packet_fingerprint,
    )
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::WorkCommand;

    #[test]
    fn cli_parses_ticket_close_request_by_ticket_id() {
        let cli = crate::cli::Cli::try_parse_from([
            "pulse",
            "--idempotency-key",
            "ticket-close-test",
            "work",
            "close",
            "TK-01J00000000000000000000000",
            "--actor",
            "human:reviewer",
            "--source-commit",
            "0123456789012345678901234567890123456789",
            "--summary",
            "All close gates passed.",
        ])
        .expect("Ticket close CLI should parse");
        assert!(matches!(
            cli.command,
            crate::cli::args::Command::Work {
                command: WorkCommand::Close { id, .. }
            } if id == "TK-01J00000000000000000000000"
        ));
    }

    #[test]
    fn cli_parses_story_close_request_without_versioned_naming() {
        let cli = crate::cli::Cli::try_parse_from([
            "pulse",
            "--idempotency-key",
            "story-close-test",
            "work",
            "close-story",
            "ST-01J00000000000000000000000",
            "--qualification-receipt",
            "rcpt_01J00000000000000000000000",
            "--actor",
            "human:conductor",
            "--source-commit",
            "0123456789012345678901234567890123456789",
            "--summary",
            "Integrated outcome qualified.",
        ])
        .expect("Story close CLI should parse");
        assert!(matches!(
            cli.command,
            crate::cli::args::Command::Work {
                command: WorkCommand::CloseStory {
                    story_id,
                    qualification_receipt,
                    ..
                }
            } if story_id == "ST-01J00000000000000000000000"
                && qualification_receipt == vec!["rcpt_01J00000000000000000000000"]
        ));
    }
}
