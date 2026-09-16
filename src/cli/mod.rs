mod args;
mod completion;
mod docs;
mod events;
mod init;
mod learn;
pub mod output;
mod packet;
mod run;
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
        args::Command::Init {
            refresh,
            host,
            with_qa_templates,
            json,
        } => init::handle(
            &repo_root,
            refresh,
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
