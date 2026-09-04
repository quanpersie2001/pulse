use std::path::PathBuf;

use crate::graph::model::contract::{Materialization, QaImpactPosture, Risk, TicketRole};
use crate::graph::model::node::NodeStatus;
use crate::id::WorkKind;
use clap::{Subcommand, ValueEnum};

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
        #[arg(long, value_enum)]
        kind: FrontierKindArg,
        #[arg(long)]
        for_: Option<String>,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        include_excluded: bool,
        #[arg(long)]
        json: bool,
    },
    ReadinessPolicy {
        #[command(subcommand)]
        command: ReadinessPolicyCommand,
    },
    Contract {
        #[command(subcommand)]
        command: ContractCommand,
    },
    QaImpact {
        #[command(subcommand)]
        command: QaImpactCommand,
    },
    Shaping {
        #[command(subcommand)]
        command: ShapingCommand,
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

#[derive(Subcommand)]
pub(crate) enum ContractCommand {
    Set {
        ticket_id: String,
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        json: bool,
    },
    Show {
        ticket_id: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum QaImpactCommand {
    Set {
        ticket_id: String,
        #[arg(long)]
        posture: QaImpactPostureArg,
        #[arg(long)]
        rationale: Option<String>,
        #[arg(long)]
        behavioral_owner: Option<String>,
        #[arg(long = "case")]
        cases: Vec<String>,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        json: bool,
    },
    Show {
        ticket_id: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum ShapingCommand {
    Apply {
        owner_id: String,
        #[arg(long)]
        receipt: String,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        expected_current_receipt: Option<String>,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        json: bool,
    },
    Show {
        owner_id: String,
        #[arg(long)]
        json: bool,
    },
    Invalidate {
        owner_id: String,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum ReadinessPolicyCommand {
    Show {
        #[arg(long)]
        json: bool,
    },
    Validate {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum QaImpactPostureArg {
    Unknown,
    Required,
    CoveredByStoryClose,
    None,
}

impl From<QaImpactPostureArg> for QaImpactPosture {
    fn from(value: QaImpactPostureArg) -> Self {
        match value {
            QaImpactPostureArg::Unknown => QaImpactPosture::Unknown,
            QaImpactPostureArg::Required => QaImpactPosture::Required,
            QaImpactPostureArg::CoveredByStoryClose => QaImpactPosture::CoveredByStoryClose,
            QaImpactPostureArg::None => QaImpactPosture::None,
        }
    }
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
pub(crate) enum FrontierKindArg {
    Decision,
    Execution,
}

impl From<FrontierKindArg> for crate::graph::read::frontier::FrontierKind {
    fn from(value: FrontierKindArg) -> Self {
        match value {
            FrontierKindArg::Decision => crate::graph::read::frontier::FrontierKind::Decision,
            FrontierKindArg::Execution => crate::graph::read::frontier::FrontierKind::Execution,
        }
    }
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
use crate::graph::store::{ContractSetRequest, QaImpactUpdate, SupersessionTarget};
use crate::{policy, JsonGraphStore, PulseError};

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
            kind,
            for_,
            profile,
            include_excluded,
            json,
        } => {
            let out = store.frontier(
                kind.into(),
                for_.as_deref(),
                profile.as_deref(),
                include_excluded,
            )?;
            match out {
                crate::graph::read::frontier::FrontierReport::Decision(report) => {
                    let human = format!(
                        "decision frontier: {} item(s){}",
                        report.items.len(),
                        report
                            .for_
                            .as_ref()
                            .map(|owner| format!(" for {owner}"))
                            .unwrap_or_default()
                    );
                    render(json, &report, human)
                }
                crate::graph::read::frontier::FrontierReport::Execution(report) => {
                    let human = format!(
                        "execution frontier: {} item(s){}",
                        report.items.len(),
                        report
                            .for_
                            .as_ref()
                            .map(|owner| format!(" for {owner}"))
                            .unwrap_or_default()
                    );
                    render(json, &report, human)
                }
            }
        }
        WorkCommand::ReadinessPolicy { command } => match command {
            ReadinessPolicyCommand::Show { json } => {
                let out = policy::load_authority_policy(store.repo_root())?;
                render(json, &out, readiness_policy_human(&out))
            }
            ReadinessPolicyCommand::Validate { json } => {
                let out = policy::validate_authority_policy_file(store.repo_root())?;
                if out.valid {
                    render(json, &out, "readiness policy valid".to_string())
                } else {
                    Err(PulseError::validation(
                        "readiness_policy_invalid",
                        serde_json::to_string(&out.reason_codes)?,
                    ))
                }
            }
        },
        WorkCommand::Contract { command } => match command {
            ContractCommand::Set {
                ticket_id,
                file,
                expected_revision,
                actor,
                json,
            } => {
                let bytes =
                    std::fs::read(&file).map_err(|error| PulseError::io(file.clone(), error))?;
                let request: ContractSetRequest = serde_json::from_slice(&bytes)
                    .map_err(|error| PulseError::json(file.clone(), error))?;
                let out = store.set_contract(&ticket_id, expected_revision, request, actor)?;
                render(json, &out, format!("updated {}", out.value.id))
            }
            ContractCommand::Show { ticket_id, json } => {
                let out = store.show_contract(&ticket_id)?;
                render(json, &out, format!("contract {}", ticket_id))
            }
        },
        WorkCommand::QaImpact { command } => match command {
            QaImpactCommand::Set {
                ticket_id,
                posture,
                rationale,
                behavioral_owner,
                cases,
                expected_revision,
                actor,
                json,
            } => {
                let update = QaImpactUpdate {
                    posture: posture.into(),
                    rationale,
                    behavioral_owner,
                    affected_case_ids: cases,
                };
                let out = store.set_qa_impact(&ticket_id, expected_revision, update, actor)?;
                render(json, &out, format!("updated {}", out.value.id))
            }
            QaImpactCommand::Show { ticket_id, json } => {
                let out = store.show_qa_impact(&ticket_id)?;
                render(json, &out, format!("qa-impact {}", ticket_id))
            }
        },
        WorkCommand::Shaping { command } => match command {
            ShapingCommand::Apply {
                owner_id,
                receipt,
                expected_revision,
                expected_current_receipt,
                actor,
                json,
            } => {
                let out = store.apply_shaping(
                    &owner_id,
                    expected_revision,
                    &receipt,
                    expected_current_receipt.as_deref(),
                    actor,
                )?;
                render(json, &out, format!("{} {}", out.code, owner_id))
            }
            ShapingCommand::Show { owner_id, json } => {
                let out = store.show_shaping(&owner_id)?;
                render(json, &out, format!("shaping {}", owner_id))
            }
            ShapingCommand::Invalidate {
                owner_id,
                expected_revision,
                reason,
                actor,
                json,
            } => {
                let out = store.invalidate_shaping(&owner_id, expected_revision, reason, actor)?;
                render(json, &out, format!("{} {}", out.code, owner_id))
            }
        },
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

fn readiness_policy_human(report: &policy::AuthorityPolicyReport) -> String {
    if !report.available {
        return "readiness policy unavailable (default deny)".to_string();
    }
    if report.valid {
        format!(
            "readiness policy valid revision {}",
            report.policy_revision.unwrap_or_default()
        )
    } else {
        format!(
            "readiness policy invalid: {}",
            report.reason_codes.join(",")
        )
    }
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
