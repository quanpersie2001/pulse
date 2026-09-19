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
        /// leaving the rest of `AGENTS.md` untouched.
        #[arg(long)]
        refresh: bool,
        /// Skip registering this repo in the user-level project registry
        /// (~/.pulse/projects.json) that `pulse serve` reads (Decision
        /// 0023).
        #[arg(long, default_value_t = false)]
        no_register: bool,
        /// Copy the qa-ui/qa-api lane scripts into `scripts/qa/` (never
        /// overwrites a file already there).
        #[arg(long)]
        with_qa_templates: bool,
        #[arg(long)]
        json: bool,
    },
    /// Create records, read them, and edit them: the graph side of Pulse
    /// (plan 0022 §4/§7) — everything that is not a gate or a receipt.
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
    /// Take the Ticket's lease and transition it `ready -> active`, so the
    /// session about to work on it holds the lease `checkpoint`/`handoff`
    /// require.
    Claim {
        id: String,
        /// Seconds the lease stays live before `pulse doctor` reports it as
        /// expired and anyone may `pulse release` it.
        #[arg(long, default_value_t = 3600)]
        ttl: i64,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// What can run right now (decision 0025 B5): ready tickets whose
    /// files are free, greedily packed so overlapping tickets serialize.
    Frontier {
        /// Only tickets of this Story.
        story: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Run every command a Ticket declares in `verify[]`, in order, and
    /// record what Pulse observed (decision 0026). Never a shell.
    Verify {
        id: String,
        /// Seconds any single declared command may run before it is killed.
        #[arg(long, default_value_t = 900)]
        timeout: u64,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Prepare and seal one review/qa lane. Pulse never dispatches the lane
    /// itself: the host spawns it, Pulse decides what it may see and whether
    /// its output counts as evidence.
    Lane {
        #[command(subcommand)]
        command: LaneCommand,
    },
    /// Drop a stuck or expired lease and return the Ticket to `ready`.
    Release {
        id: String,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Reserve additional files for a Ticket whose live lease you hold,
    /// appending them to its `touches` mid-run (decision 0025 B4).
    Reserve {
        id: String,
        /// Files this Ticket additionally edits, as repo-relative paths or
        /// globs in the `source::glob_match` grammar.
        #[arg(required = true)]
        paths: Vec<String>,
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
        /// Mark this note as friction (stays unclassified until a learning
        /// cites it or `pulse learn dismiss` records why not; an
        /// unclassified friction blocks `close-story`).
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
    /// Read-only health report: torn store lines, unreadable receipts,
    /// expired leases, orphan evidence, lanes prepared but never sealed
    /// (plan 0022 §11.3 minimum). Exits non-zero on any finding, so it can
    /// gate a script.
    Doctor {
        #[arg(long)]
        json: bool,
    },
    /// The numbers plan 0022 measured by hand, computed from the event log,
    /// receipts, store and learnings (plan 0025 E6). Read-only.
    Metrics {
        /// Only count events/receipts from this point on: RFC3339
        /// ("2026-09-18T00:00:00Z") or a date ("2026-09-18", UTC midnight).
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Doc routing and structural checks (plan 0022 §12.2).
    Docs {
        #[command(subcommand)]
        command: super::docs::DocsCommand,
    },
    /// The pre-edit gate a host hook calls before a file-writing tool
    /// fires (plan 0025 G1): this is the only place a reservation binds on
    /// an edit that never runs a `pulse` command. Pulse never installs
    /// anything into a host — `pulse hook snippet <host>` prints the
    /// configuration for you to paste.
    Hook {
        #[command(subcommand)]
        command: super::hook::HookCommand,
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
pub(crate) enum LaneCommand {
    /// Write the lane's bounded input file and record the pre-run source
    /// snapshot `pulse lane seal` compares against. Prints the input path.
    Input {
        id: String,
        role: String,
        /// Prepare a lane that is not in the record's profile.
        #[arg(long)]
        force: bool,
        /// Which seat of a review panel this run is (1-based; decision
        /// 0027). Required for a role the profile declares a `panels` entry
        /// for, refused otherwise.
        #[arg(long)]
        seat: Option<u32>,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Validate `.pulse/evidence/<id>/<role>.json` and record the lane
    /// receipt. A `fail` verdict returns the Ticket to `active`.
    Seal {
        id: String,
        role: String,
        /// Which seat of a review panel this run is (1-based; decision
        /// 0027).
        #[arg(long)]
        seat: Option<u32>,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Reconcile a review panel: without `--prepare`, arbitrate every seat's
    /// findings and seal the one `lane` receipt the close gate reads
    /// (decision 0027 C3).
    Reconcile {
        id: String,
        role: String,
        /// Write the blind round-2 input and snapshot the tree the second
        /// round runs against, instead of sealing.
        #[arg(long)]
        prepare: bool,
        /// Seconds any single `check.argv` may run before it is killed.
        #[arg(long, default_value_t = 900)]
        timeout: u64,
        #[arg(long)]
        actor: Option<String>,
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
