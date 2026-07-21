use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use pulse::graph::edge::EdgeType;
use pulse::id::WorkKind;
use pulse::{JsonGraphStore, PulseError};
use serde::Serialize;
use serde_json::json;

#[derive(Parser)]
#[command(name = "pulse")]
struct Cli {
    #[arg(long, global = true)]
    repo_root: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Work {
        #[command(subcommand)]
        command: WorkCommand,
    },
    Graph {
        #[command(subcommand)]
        command: GraphCommand,
    },
}

#[derive(Subcommand)]
enum WorkCommand {
    Create {
        #[arg(long)]
        kind: KindArg,
        #[arg(long)]
        title: String,
        #[arg(long)]
        json: bool,
    },
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    List {
        #[arg(long)]
        kind: Option<KindArg>,
        #[arg(long)]
        json: bool,
    },
    Edit {
        id: String,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        title: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum GraphCommand {
    Edge {
        #[command(subcommand)]
        command: EdgeCommand,
    },
    Validate {
        #[arg(long)]
        json: bool,
    },
    Export {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum EdgeCommand {
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
enum KindArg {
    Epic,
    Story,
    Ticket,
    Decision,
}

impl From<KindArg> for WorkKind {
    fn from(value: KindArg) -> Self {
        match value {
            KindArg::Epic => WorkKind::Epic,
            KindArg::Story => WorkKind::Story,
            KindArg::Ticket => WorkKind::Ticket,
            KindArg::Decision => WorkKind::Decision,
        }
    }
}

#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
enum EdgeTypeArg {
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

fn main() {
    let cli = Cli::parse();
    let repo_root = cli
        .repo_root
        .unwrap_or_else(|| std::env::current_dir().expect("current dir"));
    let store = JsonGraphStore::new(repo_root);
    let result = run(store, cli.command);
    if let Err(err) = result {
        print_error(&err);
        std::process::exit(1);
    }
}

fn run(store: JsonGraphStore, command: Command) -> Result<(), PulseError> {
    match command {
        Command::Work { command } => match command {
            WorkCommand::Create { kind, title, json } => {
                let out = store.create_node(kind.into(), title)?;
                render(json, &out, format!("created {}", out.value.id))
            }
            WorkCommand::Show { id, json } => {
                let node = store.show_node(&id)?;
                let human = node.title.clone();
                render(json, &json!({"schema_version": 1, "code": "ok", "node": node}), human)
            }
            WorkCommand::List { kind, json } => {
                let out = store.list_nodes(kind.map(Into::into))?;
                render(json, &out, format!("{} work items", out.items.len()))
            }
            WorkCommand::Edit {
                id,
                expected_revision,
                title,
                json,
            } => {
                let out = store.edit_title(&id, expected_revision, title)?;
                render(json, &out, format!("updated {}", out.value.id))
            }
        },
        Command::Graph { command } => match command {
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
            GraphCommand::Validate { json } => {
                let report = store.validate()?;
                let ok = report.valid;
                render(json, &report, if ok { "valid" } else { "invalid" }.to_string())?;
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
        },
    }
}

fn render<T: Serialize>(json_output: bool, value: &T, human: String) -> Result<(), PulseError> {
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(value)
                .map_err(|e| PulseError::validation("json_serialize_error", e.to_string()))?
        );
    } else {
        println!("{human}");
    }
    Ok(())
}

fn print_error(err: &PulseError) {
    let value = match err {
        PulseError::CasConflict {
            subject,
            expected_revision,
            current_revision,
        } => json!({
            "schema_version": 1,
            "code": err.code(),
            "subject": subject,
            "expected_revision": expected_revision,
            "current_revision": current_revision,
            "message": err.to_string(),
        }),
        _ => json!({
            "schema_version": 1,
            "code": err.code(),
            "message": err.to_string(),
        }),
    };
    eprintln!(
        "{}",
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| err.to_string())
    );
}
