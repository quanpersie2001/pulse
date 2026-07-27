use std::path::PathBuf;

use crate::graph::contract::{Materialization, QaImpactPosture, Risk, TicketRole};
use crate::graph::node::NodeStatus;
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

impl From<FrontierKindArg> for crate::graph::frontier::FrontierKind {
    fn from(value: FrontierKindArg) -> Self {
        match value {
            FrontierKindArg::Decision => crate::graph::frontier::FrontierKind::Decision,
            FrontierKindArg::Execution => crate::graph::frontier::FrontierKind::Execution,
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
use crate::graph::contract::PublicCreateClassification;
use crate::graph::lifecycle::TransitionReason;
use crate::graph::store::{ContractSetRequest, QaImpactUpdate, SupersessionTarget};
use crate::{policy, JsonGraphStore, PulseError};

pub(crate) fn handle(store: &JsonGraphStore, command: WorkCommand) -> Result<(), PulseError> {
    match command {
        WorkCommand::Create {
            kind,
            title,
            role,
            risk,
            materialization,
            json,
        } => {
            let classification = PublicCreateClassification {
                role: role.map(Into::into),
                risk: risk.map(Into::into),
                materialization: materialization.map(Into::into),
            };
            let out = store.create_node_public_with_context(
                kind.into(),
                title,
                classification,
                crate::graph::store::OperationContext::default(),
            )?;
            render(json, &out, format!("created {}", out.value.id))
        }
        WorkCommand::Show { id, json } => {
            let node = store.show_node(&id)?;
            let human = node.title.clone();
            render(
                json,
                &json!({"schema_version": 1, "code": "ok", "node": node}),
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
                && profile.as_deref() != Some(crate::graph::readiness::READINESS_PROFILE)
            {
                return Err(PulseError::validation(
                    "readiness_profile_unsupported",
                    format!(
                        "unsupported readiness profile; only {} is available in this release",
                        crate::graph::readiness::READINESS_PROFILE
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
                            crate::graph::readiness::GateStatus::Passed
                                | crate::graph::readiness::GateStatus::NotApplicable
                        )
                    })
                    .count()
            );
            render(json, &out, human)?;
            if out.status == crate::graph::readiness::ReadinessStatus::Ready {
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
                crate::graph::frontier::FrontierReport::Decision(report) => {
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
                crate::graph::frontier::FrontierReport::Execution(report) => {
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
    }
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
