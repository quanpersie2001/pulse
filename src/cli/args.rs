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
        /// Re-render the Pulse block in `AGENTS.md` and the lane prompts,
        /// leaving the rest of `AGENTS.md` and any existing `runners.json`
        /// untouched.
        #[arg(long)]
        refresh: bool,
        /// Skip registering this repo in the user-level project registry
        /// (~/.pulse/projects.json) that `pulse serve` reads (Decision
        /// 0023).
        #[arg(long, default_value_t = false)]
        no_register: bool,
        /// Copy host-specific detector files (only `claude-code` today).
        #[arg(long)]
        host: Option<String>,
        /// Copy the qa-ui/qa-api lane scripts into `scripts/qa/` (never
        /// overwrites a file already there).
        #[arg(long)]
        with_qa_templates: bool,
        #[arg(long)]
        json: bool,
    },
    Work {
        #[command(subcommand)]
        command: super::work::WorkCommand,
    },
    /// The one bounded JSON a worker reads before doing anything.
    Packet {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Append a checkpoint; does not change status.
    Checkpoint {
        id: String,
        /// JSON file with the checkpoint shape (plan 0022 §4.4/§10.3).
        #[arg(long)]
        from: PathBuf,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Seal a handoff receipt and transition `active -> verifying`.
    Handoff {
        id: String,
        /// JSON file with the handoff shape (plan 0022 §7.2).
        #[arg(long)]
        from: PathBuf,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Run the close gate; on a clean report, `verifying -> done`.
    Close {
        id: String,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Run the close-story gate; on a clean report, the Story becomes `done`.
    CloseStory {
        id: String,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Run a configured role (`worker`, a lane name, or `review`) against a
    /// Ticket.
    Run {
        role: String,
        id: String,
        #[arg(long, default_value_t = 3600)]
        ttl: i64,
        #[arg(long, default_value_t = crate::kernel::run::DEFAULT_CONTINUE_LIMIT)]
        continue_limit: u32,
        /// Run a lane even if it is not in the Ticket's profile.
        #[arg(long)]
        force: bool,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Drop a stuck or expired lease and return the Ticket to `ready`.
    Release {
        id: String,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
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
    /// Manage learnings (plan 0022 §11): friction -> learning -> check.
    Learn {
        #[command(subcommand)]
        command: super::learn::LearnCommand,
    },
    /// Read-only health report: store, receipts, leases, evidence,
    /// the §10.4 detector (plan 0022 §11.3 minimum). Exits non-zero on
    /// any finding, so it can gate a script.
    Doctor {
        #[arg(long)]
        json: bool,
    },
    /// Doc routing and structural checks (plan 0022 §12.2).
    Docs {
        #[command(subcommand)]
        command: super::docs::DocsCommand,
    },
    /// Read-only board server over your registered Pulse projects
    /// (Decision 0023, as amended: `pulse init` registers; serve lists).
    /// `--workspace` additionally scans a directory tree for repos.
    Serve {
        /// Directory to scan for Pulse projects in addition to the
        /// registry (depth <= 4).
        #[arg(long)]
        workspace: Option<PathBuf>,
        /// TCP port to bind on 127.0.0.1.
        #[arg(long, default_value_t = 7777)]
        port: u16,
        /// Open the board in the system browser after binding.
        #[arg(long, default_value_t = false)]
        open: bool,
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
}
