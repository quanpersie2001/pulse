//! Thin CLI adapter for Story QA baseline inspection and Ticket case resolution.

use clap::Subcommand;

use crate::cli::output::render;
use crate::{JsonGraphStore, PulseError};

#[derive(Subcommand)]
pub(crate) enum QaCommand {
    /// Parse and validate the current canonical Story baseline.
    Baseline {
        story_id: String,
        #[arg(long)]
        json: bool,
    },
    /// Resolve a required Ticket QA impact to exact current baseline cases.
    Resolve {
        ticket_id: String,
        #[arg(long)]
        json: bool,
    },
}

pub(crate) fn handle(store: &JsonGraphStore, command: QaCommand) -> Result<(), PulseError> {
    match command {
        QaCommand::Baseline { story_id, json } => {
            let baseline = crate::qa::load_story_baseline(store.repo_root(), &story_id)?;
            render(
                json,
                &baseline,
                format!(
                    "{} baseline {} ({:?}): {} cases",
                    baseline.owner_id,
                    baseline.content_hash,
                    baseline.posture,
                    baseline.cases.len()
                ),
            )
        }
        QaCommand::Resolve { ticket_id, json } => {
            let ticket = store.show_node(&ticket_id)?;
            let resolution = crate::qa::resolve_ticket_cases(store.repo_root(), &ticket)?;
            render(
                json,
                &resolution,
                format!(
                    "{} -> {} baseline {}: {} cases",
                    ticket_id,
                    resolution.owner_id,
                    resolution.content_hash,
                    resolution.cases.len()
                ),
            )
        }
    }
}
