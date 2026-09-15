mod args;
mod completion;
mod events;
mod init;
pub mod output;
mod work;

use clap::Parser;

pub use args::Cli;
pub use output::print_error;

use crate::PulseError;

pub fn parse() -> Cli {
    Cli::parse()
}

pub fn run(cli: Cli) -> Result<(), PulseError> {
    let workspace_root = cli
        .repo_root
        .unwrap_or_else(|| std::env::current_dir().expect("current dir"));
    let repo_root = crate::source::state_repo_root(&workspace_root)?;

    match cli.command {
        args::Command::Init { json } => init::handle(&repo_root, json),
        args::Command::Work { command } => work::handle(&repo_root, command),
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
        args::Command::Events {
            command: args::EventsCommand::Compact { json },
        } => events::handle_compact(&repo_root, json),
    }
}
