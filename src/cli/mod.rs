mod args;
mod daemon;
mod docs;
mod evidence;
mod graph;
mod knowledge;
pub mod output;
mod work;

use clap::Parser;

pub use args::Cli;
pub use output::print_error;

use crate::{JsonGraphStore, PulseError};

pub fn parse() -> Cli {
    Cli::parse()
}

pub fn run(cli: Cli) -> Result<(), PulseError> {
    let repo_root = cli
        .repo_root
        .unwrap_or_else(|| std::env::current_dir().expect("current dir"));
    #[cfg(any(test, debug_assertions))]
    let store = if cli.test_work_packet_after_first_fence {
        JsonGraphStore::with_work_packet_after_first_fence_failpoint(repo_root)
    } else {
        match cli.test_failpoint {
            Some(failpoint) => JsonGraphStore::with_failpoint(repo_root, failpoint.into()),
            None => JsonGraphStore::new(repo_root),
        }
    };
    #[cfg(not(any(test, debug_assertions)))]
    let store = JsonGraphStore::new(repo_root);

    let explicit_key = cli.idempotency_key.as_deref();
    match cli.command {
        args::Command::Work { command } => work::handle(&store, command),
        args::Command::Docs { command } => docs::handle(&store, command),
        args::Command::Graph { command } => graph::handle(&store, command),
        args::Command::Evidence { command } => evidence::handle(&store, command),
        args::Command::Knowledge { command } => knowledge::handle(&store, command),
        args::Command::Daemon { command } => daemon::handle_daemon(command, explicit_key),
        args::Command::Project { command } => daemon::handle_project(command, explicit_key),
        args::Command::Workspace { command } => daemon::handle_workspace(command, explicit_key),
        args::Command::Session { command } => daemon::handle_session(command, explicit_key),
    }
}
