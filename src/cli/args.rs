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
        /// Re-render the Pulse block in `AGENTS.md` for this CLI version.
        /// Content outside the markers is never touched, and a hand-edited
        /// block is reported rather than overwritten.
        #[arg(long)]
        refresh: bool,
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
    /// Record a note targeting a Ticket (append-only event log).
    Note {
        /// Work node the note targets (Epic, Story, Ticket or Decision id).
        #[arg(long, alias = "ticket")]
        work: String,
        /// Note message (bounded).
        #[arg(long)]
        message: String,
        /// Author of the note (kind:id).
        #[arg(long, default_value = "human:unknown")]
        from: String,
        /// Note kind; `friction` marks harness friction the close gate
        /// turns into a learning candidate (Decision 0009 §4).
        #[arg(long, value_enum, default_value_t = NoteKindArg::Note)]
        kind: NoteKindArg,
        #[arg(long)]
        json: bool,
    },
    /// Read the append-only event log.
    Events {
        #[command(subcommand)]
        command: EventsCommand,
    },
    /// Execute a configured runner role against a Ticket.
    Run {
        /// Runner role defined in .pulse/config/runners.json.
        role: String,
        /// Ticket to run the role against. Required for worker and reviewer
        /// runs and for qa ticket_checkpoint runs.
        #[arg(long)]
        ticket: Option<String>,
        /// Worker lease TTL in seconds.
        #[arg(long, default_value_t = crate::kernel::DEFAULT_RUN_TTL_SECONDS)]
        ttl_seconds: u64,
        /// Optional run idempotency key; defaults to one stable lease per
        /// ticket and role.
        #[arg(long, default_value = "")]
        idempotency_key: String,
        /// Workspace isolation: checkout by default; worktree forces a
        /// Pulse-owned worktree (and is the only way past another Ticket's
        /// live lease).
        #[arg(long, value_enum, default_value_t = IsolationArg::Auto)]
        isolation: IsolationArg,
        /// QA execution scope: ticket_checkpoint (default) runs the Ticket's
        /// affected cases; story_close qualifies the whole Story baseline.
        #[arg(long, value_enum, default_value_t = RunScopeArg::TicketCheckpoint)]
        scope: RunScopeArg,
        /// Story id for qa `--scope story_close` runs.
        #[arg(long)]
        story: Option<String>,
        /// Release the interrupted run's stale lease and start fresh when the
        /// packet drifted; without this flag drifted resumes are refused.
        #[arg(long, default_value_t = false)]
        acknowledge_drift: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum EventsCommand {
    /// Stream recent events (oldest first), optionally following.
    Tail {
        /// Only events with an id greater than this cursor (evt_...).
        #[arg(long, default_value = "0")]
        since: String,
        /// Only events targeting this work id.
        #[arg(long)]
        ticket: Option<String>,
        /// Keep polling for new events.
        #[arg(long, default_value_t = false)]
        follow: bool,
        #[arg(long)]
        json: bool,
    },
}

/// Workspace isolation policy for `pulse run`.
#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
#[value(rename_all = "snake_case")]
pub(crate) enum IsolationArg {
    Auto,
    Worktree,
}

/// Kind of a `pulse note` record.
#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
#[value(rename_all = "snake_case")]
pub(crate) enum NoteKindArg {
    Note,
    Friction,
}

/// QA execution scope for `pulse run qa`.
#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
#[value(rename_all = "snake_case")]
pub(crate) enum RunScopeArg {
    TicketCheckpoint,
    StoryClose,
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
