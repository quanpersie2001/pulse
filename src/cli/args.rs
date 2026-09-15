use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "pulse")]
pub struct Cli {
    #[arg(long, global = true)]
    pub(crate) repo_root: Option<PathBuf>,
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Create `.pulse/`, `PULSE.md` and the store files it needs.
    Init {
        #[arg(long)]
        json: bool,
    },
    Work {
        #[command(subcommand)]
        command: super::work::WorkCommand,
    },
    /// Record a note against `<id>` (append-only event log).
    Note {
        /// Epic, Story, Ticket or Decision id the note targets.
        id: String,
        /// Note text.
        text: String,
        /// Mark this note as friction (the close gate turns it into a
        /// learning candidate).
        #[arg(long)]
        friction: bool,
        /// Actor recording the note (kind:id). Defaults to `PULSE_ACTOR`,
        /// then `git config user.name`.
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Read the append-only event log.
    Events {
        #[command(subcommand)]
        command: EventsCommand,
    },
}

#[derive(Subcommand)]
pub(crate) enum EventsCommand {
    /// Stream recent events (oldest first), optionally following.
    Tail {
        /// Only events with an id greater than this cursor (evt_...).
        #[arg(long, default_value = "0")]
        since: String,
        /// Only events targeting this id.
        #[arg(long)]
        id: Option<String>,
        /// Keep polling for new events.
        #[arg(long, default_value_t = false)]
        follow: bool,
        #[arg(long)]
        json: bool,
    },
    /// Convert the legacy one-file-per-event layout to `<date>.jsonl` once.
    Compact {
        #[arg(long)]
        json: bool,
    },
}
