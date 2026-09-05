mod args;
mod docs;
mod evidence;
mod graph;
mod init;
mod knowledge;
pub mod output;
mod qa;
mod run;
mod work;

use clap::Parser;

use self::args::IsolationArg;

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
        JsonGraphStore::with_work_packet_after_first_fence_failpoint(repo_root.clone())
    } else {
        match cli.test_failpoint {
            Some(failpoint) => JsonGraphStore::with_failpoint(repo_root.clone(), failpoint.into()),
            None => JsonGraphStore::new(repo_root.clone()),
        }
    };
    #[cfg(not(any(test, debug_assertions)))]
    let store = JsonGraphStore::new(repo_root.clone());

    let explicit_key = cli.idempotency_key.as_deref();
    match cli.command {
        args::Command::Init { actor, json } => init::handle(&repo_root, actor.as_deref(), json),
        args::Command::Work { command } => work::handle(&store, command, explicit_key),
        args::Command::Docs { command } => docs::handle(&store, command),
        args::Command::Graph { command } => graph::handle(&store, command),
        args::Command::Evidence { command } => evidence::handle(&store, command),
        args::Command::Knowledge { command } => knowledge::handle(&store, command),
        args::Command::Qa { command } => qa::handle(&store, command),
        args::Command::Run {
            role,
            ticket,
            ttl_seconds,
            idempotency_key,
            isolation,
            acknowledge_drift,
            json,
        } => run::handle(
            &store,
            &run::RunOptions {
                role: &role,
                ticket: &ticket,
                ttl_seconds,
                idempotency_key: &if idempotency_key.is_empty() {
                    explicit_key.unwrap_or_default().to_string()
                } else {
                    idempotency_key
                },
                forced_worktree: matches!(isolation, IsolationArg::Worktree),
                acknowledge_drift,
            },
            json,
        ),
    }
}
