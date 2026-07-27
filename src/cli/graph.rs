use crate::graph::edge::EdgeType;
use clap::{Subcommand, ValueEnum};

#[derive(Subcommand)]
pub(crate) enum GraphCommand {
    Edge {
        #[command(subcommand)]
        command: EdgeCommand,
    },
    Recover {
        #[arg(long)]
        json: bool,
    },
    Bootstrap {
        #[arg(long)]
        json: bool,
    },
    Validate {
        #[arg(long)]
        json: bool,
    },
    Export {
        #[arg(long)]
        json: bool,
    },
    Neighborhood {
        id: String,
        #[arg(long, default_value_t = 1)]
        depth: usize,
        #[arg(long)]
        json: bool,
    },
    AffectedBy {
        id: String,
        #[arg(long = "relation")]
        relation: Option<EdgeTypeArg>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum EdgeCommand {
    Add {
        #[arg(long = "type")]
        edge_type: EdgeTypeArg,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum EdgeTypeArg {
    Parent,
    BlockedBy,
    PreferredAfter,
    SupersededBy,
    Related,
    Duplicates,
}

impl From<EdgeTypeArg> for EdgeType {
    fn from(value: EdgeTypeArg) -> Self {
        match value {
            EdgeTypeArg::Parent => EdgeType::Parent,
            EdgeTypeArg::BlockedBy => EdgeType::BlockedBy,
            EdgeTypeArg::PreferredAfter => EdgeType::PreferredAfter,
            EdgeTypeArg::SupersededBy => EdgeType::SupersededBy,
            EdgeTypeArg::Related => EdgeType::Related,
            EdgeTypeArg::Duplicates => EdgeType::Duplicates,
        }
    }
}

use serde_json::json;

use crate::cli::output::render;
use crate::{JsonGraphStore, PulseError};

pub(crate) fn handle(store: &JsonGraphStore, command: GraphCommand) -> Result<(), PulseError> {
    match command {
        GraphCommand::Edge { command } => match command {
            EdgeCommand::Add {
                edge_type,
                from,
                to,
                actor,
                json,
            } => {
                let out = store.add_edge(edge_type.into(), from, to, actor)?;
                render(json, &out, format!("{} {}", out.code, out.value.id))
            }
        },
        GraphCommand::Recover { json } => {
            store.recover()?;
            render(
                json,
                &json!({"schema_version": 1, "code": "recovered"}),
                "recovered".to_string(),
            )
        }
        GraphCommand::Bootstrap { json } => {
            store.bootstrap()?;
            render(
                json,
                &json!({"schema_version": 1, "code": "bootstrapped"}),
                "bootstrapped".to_string(),
            )
        }
        GraphCommand::Validate { json } => {
            let report = store.validate()?;
            let ok = report.valid;
            render(
                json,
                &report,
                if ok { "valid" } else { "invalid" }.to_string(),
            )?;
            if ok {
                Ok(())
            } else {
                Err(PulseError::validation("invalid_graph", "graph is invalid"))
            }
        }
        GraphCommand::Export { json } => {
            let projection = store.export()?;
            render(json, &projection, projection.graph_fingerprint.clone())
        }
        GraphCommand::Neighborhood { id, depth, json } => {
            let out = store.neighborhood(&id, depth)?;
            render(json, &out, format!("neighborhood {}", out.subject))
        }
        GraphCommand::AffectedBy { id, relation, json } => {
            let out = store.affected_by(&id, relation.map(Into::into))?;
            render(json, &out, format!("affected-by {}", out.subject))
        }
    }
}
