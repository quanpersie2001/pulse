use std::path::PathBuf;

#[cfg(debug_assertions)]
use crate::storage::transaction::TransactionFailpoint;
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "pulse")]
pub struct Cli {
    #[arg(long, global = true)]
    pub(crate) repo_root: Option<PathBuf>,
    #[cfg(debug_assertions)]
    #[arg(long, global = true, hide = true, value_enum)]
    pub(crate) test_failpoint: Option<FailpointArg>,
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
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

#[cfg(debug_assertions)]
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
