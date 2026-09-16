mod args;
mod completion;
mod docs;
mod events;
mod init;
mod learn;
pub mod output;
mod packet;
mod run;
mod serve;
mod work;

use clap::Parser;

pub use args::Cli;
pub use output::print_error;

use crate::PulseError;

pub fn parse() -> Cli {
    Cli::parse()
}

pub fn run(cli: Cli) -> Result<(), PulseError> {
    // `serve` deliberately bypasses repo-root resolution: it reads a
    // *workspace* of repos, so it must work from anywhere (Decision 0023).
    match cli.command {
        args::Command::Serve {
            workspace,
            port,
            open,
        } => serve::handle(workspace.as_deref(), port, open),
        other => run_in_repo(other, cli.repo_root),
    }
}

fn run_in_repo(
    command: args::Command,
    repo_root: Option<std::path::PathBuf>,
) -> Result<(), PulseError> {
    let workspace_root = repo_root.unwrap_or_else(|| std::env::current_dir().expect("current dir"));
    let repo_root = crate::source::state_repo_root(&workspace_root)?;

    match command {
        // Unreachable via `run` (it routes Serve before repo-root
        // resolution), but the match must stay exhaustive.
        args::Command::Serve {
            workspace,
            port,
            open,
        } => serve::handle(workspace.as_deref(), port, open),
        args::Command::Init {
            refresh,
            no_register,
            host,
            with_qa_templates,
            json,
        } => init::handle(
            &repo_root,
            refresh,
            no_register,
            host.as_deref(),
            with_qa_templates,
            json,
        ),
        args::Command::Work { command } => work::handle(&repo_root, command),
        args::Command::Run {
            role,
            id,
            ttl,
            continue_limit,
            force,
            actor,
            json,
        } => run::handle_run(
            &repo_root,
            &role,
            &id,
            ttl,
            continue_limit,
            force,
            actor.as_deref(),
            json,
        ),
        args::Command::Release { id, actor, json } => {
            run::handle_release(&repo_root, &id, actor.as_deref(), json)
        }
        args::Command::Packet { id, json } => packet::handle_packet(&repo_root, &id, json),
        args::Command::Checkpoint {
            id,
            from,
            actor,
            json,
        } => packet::handle_checkpoint(&repo_root, &id, &from, actor.as_deref(), json),
        args::Command::Handoff {
            id,
            from,
            actor,
            json,
        } => completion::handle_handoff(&repo_root, &id, &from, actor.as_deref(), json),
        args::Command::Close { id, actor, json } => {
            completion::handle_close(&repo_root, &id, actor.as_deref(), json)
        }
        args::Command::CloseStory { id, actor, json } => {
            completion::handle_close_story(&repo_root, &id, actor.as_deref(), json)
        }
        args::Command::Note {
            id,
            text,
            friction,
            from,
            json,
        } => work::handle_note(&repo_root, &id, &text, friction, from.as_deref(), json),
        args::Command::Events {
            command:
                args::EventsCommand::Tail {
                    since,
                    id,
                    follow,
                    json,
                },
        } => events::handle_tail(&repo_root, &since, id.as_deref(), follow, json),
        args::Command::Learn { command } => learn::handle(&repo_root, command),
        args::Command::Docs { command } => docs::handle(&repo_root, command),
    }
}
