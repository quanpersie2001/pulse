use std::path::PathBuf;

#[cfg(debug_assertions)]
use crate::storage::transaction::TransactionFailpoint;
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "pulse")]
pub struct Cli {
    #[arg(long, global = true)]
    pub(crate) repo_root: Option<PathBuf>,
    /// Stable idempotency key for mutating offline Core commands (work
    /// handoff/verify/close-style proofs). Runtime reads ignore this option;
    /// offline Core commands keep their own command-specific contracts.
    #[arg(long, global = true, default_value = None)]
    pub(crate) idempotency_key: Option<String>,
    #[cfg(any(test, debug_assertions))]
    #[arg(long, global = true, hide = true, value_enum)]
    pub(crate) test_failpoint: Option<FailpointArg>,
    #[cfg(any(test, debug_assertions))]
    #[arg(long, global = true, hide = true)]
    pub(crate) test_work_packet_after_first_fence: bool,
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    Init {
        /// Principal that owns the initial Core grants (for example,
        /// `human:alice`). Defaults to the repository's Git user.name.
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Work {
        #[command(subcommand)]
        command: super::work::WorkCommand,
    },
    Docs {
        #[command(subcommand)]
        command: super::docs::DocsCommand,
    },
    Graph {
        #[command(subcommand)]
        command: super::graph::GraphCommand,
    },
    Evidence {
        #[command(subcommand)]
        command: super::evidence::EvidenceCommand,
    },
    Knowledge {
        #[command(subcommand)]
        command: super::knowledge::KnowledgeCommand,
    },
    Qa {
        #[command(subcommand)]
        command: super::qa::QaCommand,
    },
    /// Execute a configured runner role against a Ticket.
    Run {
        /// Runner role defined in .pulse/config/runners.json.
        role: String,
        /// Ticket to run the role against.
        #[arg(long)]
        ticket: String,
        /// Worker lease TTL in seconds.
        #[arg(long, default_value_t = crate::kernel::DEFAULT_RUN_TTL_SECONDS)]
        ttl_seconds: u64,
        /// Optional run idempotency key; defaults to one stable lease per
        /// ticket and role.
        #[arg(long, default_value = "")]
        idempotency_key: String,
        #[arg(long)]
        json: bool,
    },
}

#[allow(clippy::enum_variant_names)]
#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum FailpointArg {
    AfterIntent,
    AfterCanonical,
    AfterMultiTargetFirst,
    AfterMultiTargetAll,
    AfterEvent,
}

#[cfg(any(test, debug_assertions))]
impl From<FailpointArg> for TransactionFailpoint {
    fn from(value: FailpointArg) -> Self {
        match value {
            FailpointArg::AfterIntent => TransactionFailpoint::AfterIntent,
            FailpointArg::AfterCanonical => TransactionFailpoint::AfterCanonical,
            FailpointArg::AfterMultiTargetFirst => TransactionFailpoint::AfterMultiTargetFirst,
            FailpointArg::AfterMultiTargetAll => TransactionFailpoint::AfterMultiTargetAll,
            FailpointArg::AfterEvent => TransactionFailpoint::AfterEvent,
        }
    }
}
