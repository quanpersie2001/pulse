//! `pulse learn add|list|show|applicable|activate|retire` (plan 0022 §11.2).
//! Thin renderer: resolves the actor, parses `--from`, calls `pulse::learn`,
//! renders the result.

use std::path::PathBuf;

use clap::Subcommand;
use serde_json::{json, Value};

use crate::cli::output::render;
use crate::identity::actor::resolve_actor;
use crate::learn::{self, store};
use crate::PulseError;

#[derive(Subcommand)]
pub(crate) enum LearnCommand {
    /// Create a candidate learning: `--from <file>` (a complete learning
    /// file) or `--title`/`--kind` (Pulse synthesizes the rest).
    Add {
        #[arg(long)]
        from: Option<PathBuf>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long = "applies-to")]
        applies_to: Vec<String>,
        #[arg(long)]
        tags: Vec<String>,
        #[arg(long = "expected-signal")]
        expected_signal: Option<String>,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Learnings matching `<id>`'s anchors/tags (plan §11.2). `active` only
    /// unless `--all` (which also surfaces `candidate`).
    Applicable {
        id: String,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    /// `candidate -> active`; requires `usage.helpful >= 1`.
    Activate {
        id: String,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Retire {
        id: String,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

fn learning_value(learning: &store::Learning) -> Value {
    let mut value = serde_json::to_value(&learning.frontmatter).unwrap_or_else(|_| json!({}));
    if let Some(object) = value.as_object_mut() {
        object.insert("body".to_string(), json!(learning.body));
    }
    value
}

fn summary_line(learning: &store::Learning) -> String {
    format!(
        "{} [{}] {}",
        learning.frontmatter.id, learning.frontmatter.status, learning.frontmatter.kind
    )
}

pub(crate) fn handle(repo_root: &std::path::Path, command: LearnCommand) -> Result<(), PulseError> {
    match command {
        LearnCommand::Add {
            from,
            title,
            kind,
            applies_to,
            tags,
            expected_signal,
            actor,
            json,
        } => {
            let actor = resolve_actor(repo_root, actor.as_deref())?;
            let input = if let Some(path) = from {
                let text =
                    std::fs::read_to_string(&path).map_err(|error| PulseError::io(&path, error))?;
                learn::AddInput::FromFile(text)
            } else {
                let title = title.ok_or_else(|| {
                    PulseError::kernel(
                        "learn_add_invalid",
                        "no --from file and no --title given",
                        "pass --from <file>, or --title <t> --kind <failure|constraint|technique|routing>",
                    )
                })?;
                let kind = kind.ok_or_else(|| {
                    PulseError::kernel(
                        "learn_add_invalid",
                        "no --from file and no --kind given",
                        "pass --from <file>, or --title <t> --kind <failure|constraint|technique|routing>",
                    )
                })?;
                learn::AddInput::Fields {
                    title,
                    kind,
                    applies_to,
                    tags,
                    expected_signal: expected_signal.unwrap_or_default(),
                }
            };
            let learning = learn::add(repo_root, &actor, input)?;
            let id = learning.frontmatter.id.clone();
            render(json, &learning_value(&learning), format!("created {id}"))
        }
        LearnCommand::List { status, json } => {
            let mut learnings = store::list(repo_root)?;
            if let Some(status) = status {
                learnings.retain(|learning| learning.frontmatter.status == status);
            }
            let values: Vec<Value> = learnings.iter().map(learning_value).collect();
            let human = learnings
                .iter()
                .map(summary_line)
                .collect::<Vec<_>>()
                .join("\n");
            render(json, &values, human)
        }
        LearnCommand::Show { id, json } => {
            let learning = store::read(repo_root, &id)?;
            let human = store::render(&learning)?;
            render(json, &learning_value(&learning), human)
        }
        LearnCommand::Applicable { id, all, json } => {
            let matched = learn::recall::applicable(repo_root, &id, all)?;
            let values: Vec<Value> = matched.iter().map(learning_value).collect();
            let human = matched
                .iter()
                .map(summary_line)
                .collect::<Vec<_>>()
                .join("\n");
            render(json, &values, human)
        }
        LearnCommand::Activate { id, actor, json } => {
            let actor = resolve_actor(repo_root, actor.as_deref())?;
            let learning = learn::activate(repo_root, &actor, &id)?;
            render(json, &learning_value(&learning), format!("{id} is active"))
        }
        LearnCommand::Retire {
            id,
            reason,
            actor,
            json,
        } => {
            let actor = resolve_actor(repo_root, actor.as_deref())?;
            let learning = learn::retire(repo_root, &actor, &id, &reason)?;
            render(json, &learning_value(&learning), format!("{id} retired"))
        }
    }
}
