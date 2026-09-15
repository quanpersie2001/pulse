//! `pulse work {new,show,list,tree,ready,update,dep,transition}` (plan 0022
//! §6). Thin renderer: parses args, resolves the actor, calls
//! `kernel::issues`/`kernel::ready`/`store::issues`, renders the result.

use std::path::PathBuf;

use clap::Subcommand;
use serde_json::{Map, Value};

use crate::cli::output::render;
use crate::identity::actor::resolve_actor;
use crate::kernel::issues::{self, DepType, NoteKind as KernelNoteKind};
use crate::store::issues::read_all;
use crate::PulseError;

#[derive(Subcommand)]
pub(crate) enum WorkCommand {
    /// Create a new draft record.
    New {
        /// epic | story | ticket | decision.
        kind: String,
        title: String,
        #[arg(long)]
        story: Option<String>,
        #[arg(long)]
        epic: Option<String>,
        #[arg(long)]
        risk: Option<String>,
        #[arg(long)]
        surface: Option<String>,
        /// JSON file with additional fields to seed (role, objective, ...).
        #[arg(long)]
        from: Option<PathBuf>,
        #[arg(long)]
        actor: Option<String>,
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
        kind: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        story: Option<String>,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long)]
        ready: bool,
        #[arg(long)]
        json: bool,
    },
    /// Epic -> Story -> Ticket, with status. Root at `id` if given.
    Tree {
        id: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Run the ready gate; on a clean report, `draft -> ready`.
    Ready {
        id: String,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Merge fields into a record. `--set k=v` may repeat; `--from` loads a
    /// JSON object; `--stdin` reads one from standard input. Runtime-owned
    /// fields (`lease`, `verdicts`, `checkpoints`) are rejected.
    Update {
        id: String,
        #[arg(long = "set")]
        set: Vec<String>,
        #[arg(long)]
        from: Option<PathBuf>,
        #[arg(long)]
        stdin: bool,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Dep {
        #[command(subcommand)]
        command: DepCommand,
    },
    /// Manual status change: draft|ready|blocked -> cancelled, or
    /// ready|active -> blocked.
    Transition {
        id: String,
        #[arg(long = "to")]
        to: String,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum DepCommand {
    Add {
        id: String,
        /// blocked_by | supersedes.
        dep_type: String,
        other: String,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Rm {
        id: String,
        dep_type: String,
        other: String,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

fn parse_dep_type(raw: &str) -> Result<DepType, PulseError> {
    match raw {
        "blocked_by" => Ok(DepType::BlockedBy),
        "supersedes" => Ok(DepType::Supersedes),
        other => Err(PulseError::kernel(
            "dep_type_invalid",
            format!("unknown dep type {other}"),
            "dep type must be blocked_by or supersedes",
        )),
    }
}

fn load_from_file(path: &PathBuf) -> Result<Map<String, Value>, PulseError> {
    let bytes = std::fs::read(path).map_err(|error| PulseError::io(path, error))?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
        PulseError::kernel(
            "from_file_invalid",
            format!("{} is not valid JSON: {error}", path.display()),
            "--from expects a JSON object of fields to merge",
        )
    })?;
    into_object(value)
}

fn into_object(value: Value) -> Result<Map<String, Value>, PulseError> {
    match value {
        Value::Object(map) => Ok(map),
        _ => Err(PulseError::kernel(
            "from_file_invalid",
            "expected a JSON object at the top level",
            "--from/--stdin expects a JSON object of fields to merge",
        )),
    }
}

/// Parse `--set key=value` pairs; `value` is parsed as JSON when possible
/// (so `--set risk=\"low\"` and `--set risk=low` both work, and
/// `--set tags=[\"a\"]` produces an array), falling back to a plain string.
fn parse_set_pairs(pairs: &[String]) -> Result<Map<String, Value>, PulseError> {
    let mut map = Map::new();
    for pair in pairs {
        let Some((key, value)) = pair.split_once('=') else {
            return Err(PulseError::kernel(
                "set_invalid",
                format!("--set {pair} is missing '='"),
                "use --set key=value, e.g. --set risk=low",
            ));
        };
        let parsed = serde_json::from_str::<Value>(value)
            .unwrap_or_else(|_| Value::String(value.to_string()));
        map.insert(key.to_string(), parsed);
    }
    Ok(map)
}

pub(crate) fn handle(repo_root: &std::path::Path, command: WorkCommand) -> Result<(), PulseError> {
    match command {
        WorkCommand::New {
            kind,
            title,
            story,
            epic,
            risk,
            surface,
            from,
            actor,
            json,
        } => {
            let actor = resolve_actor(repo_root, actor.as_deref())?;
            let mut fields = match from {
                Some(path) => load_from_file(&path)?,
                None => Map::new(),
            };
            if let Some(story) = story {
                fields.insert("story".to_string(), Value::String(story));
            }
            if let Some(epic) = epic {
                fields.insert("epic".to_string(), Value::String(epic));
            }
            if let Some(risk) = risk {
                fields.insert("risk".to_string(), Value::String(risk));
            }
            if let Some(surface) = surface {
                fields.insert("surface".to_string(), Value::String(surface));
            }
            let record = issues::create(repo_root, &actor, &kind, &title, fields)?;
            let id = record.get("id").and_then(Value::as_str).unwrap_or("");
            render(json, &record, format!("created {id}"))
        }
        WorkCommand::Show { id, json } => {
            let records = read_all(repo_root)?;
            let record = records
                .iter()
                .find(|record| record.get("id").and_then(Value::as_str) == Some(id.as_str()))
                .ok_or_else(|| {
                    PulseError::kernel(
                        "issue_not_found",
                        format!("no record with id {id}"),
                        "check the id with `pulse work list`",
                    )
                })?;
            render(json, record, format!("{record}"))
        }
        WorkCommand::List {
            kind,
            status,
            story,
            tag,
            ready,
            json,
        } => {
            let mut records = read_all(repo_root)?;
            records.retain(|record| {
                kind.as_deref().map_or(true, |k| {
                    record.get("kind").and_then(Value::as_str) == Some(k)
                }) && status.as_deref().map_or(true, |s| {
                    record.get("status").and_then(Value::as_str) == Some(s)
                }) && story.as_deref().map_or(true, |s| {
                    record.get("story").and_then(Value::as_str) == Some(s)
                }) && tag.as_deref().map_or(true, |t| {
                    record
                        .get("tags")
                        .and_then(Value::as_array)
                        .is_some_and(|tags| {
                            tags.iter().any(|tag_value| tag_value.as_str() == Some(t))
                        })
                }) && (!ready || record.get("status").and_then(Value::as_str) == Some("ready"))
            });
            let human = records
                .iter()
                .map(|record| {
                    format!(
                        "{} [{}] {}",
                        record.get("id").and_then(Value::as_str).unwrap_or("?"),
                        record.get("status").and_then(Value::as_str).unwrap_or("?"),
                        record.get("title").and_then(Value::as_str).unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            render(json, &records, human)
        }
        WorkCommand::Tree { id, json } => {
            let records = read_all(repo_root)?;
            let tree = build_tree(&records, id.as_deref());
            let human = render_tree_human(&tree);
            render(json, &tree, human)
        }
        WorkCommand::Ready { id, actor, json } => {
            let actor = resolve_actor(repo_root, actor.as_deref())?;
            let record = issues::ready(repo_root, &actor, &id)?;
            render(json, &record, format!("{id} is ready"))
        }
        WorkCommand::Update {
            id,
            set,
            from,
            stdin,
            actor,
            json,
        } => {
            let actor = resolve_actor(repo_root, actor.as_deref())?;
            let mut fields = parse_set_pairs(&set)?;
            if let Some(path) = from {
                fields.extend(load_from_file(&path)?);
            }
            if stdin {
                let mut buffer = String::new();
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut buffer)
                    .map_err(|error| PulseError::io("<stdin>", error))?;
                let value: Value = serde_json::from_str(&buffer).map_err(|error| {
                    PulseError::kernel(
                        "from_file_invalid",
                        format!("stdin is not valid JSON: {error}"),
                        "--stdin expects a JSON object of fields to merge",
                    )
                })?;
                fields.extend(into_object(value)?);
            }
            let record = issues::update(repo_root, &actor, &id, fields)?;
            render(json, &record, format!("updated {id}"))
        }
        WorkCommand::Dep { command } => handle_dep(repo_root, command),
        WorkCommand::Transition {
            id,
            to,
            reason,
            actor,
            json,
        } => {
            let actor = resolve_actor(repo_root, actor.as_deref())?;
            let record = issues::transition(repo_root, &actor, &id, &to, &reason)?;
            render(json, &record, format!("{id} -> {to}"))
        }
    }
}

fn handle_dep(repo_root: &std::path::Path, command: DepCommand) -> Result<(), PulseError> {
    match command {
        DepCommand::Add {
            id,
            dep_type,
            other,
            actor,
            json,
        } => {
            let actor = resolve_actor(repo_root, actor.as_deref())?;
            let dep_type = parse_dep_type(&dep_type)?;
            let record = issues::dep_add(repo_root, &actor, &id, dep_type, &other)?;
            render(json, &record, format!("{id} now depends on {other}"))
        }
        DepCommand::Rm {
            id,
            dep_type,
            other,
            actor,
            json,
        } => {
            let actor = resolve_actor(repo_root, actor.as_deref())?;
            let dep_type = parse_dep_type(&dep_type)?;
            let record = issues::dep_rm(repo_root, &actor, &id, dep_type, &other)?;
            render(json, &record, format!("{id} no longer depends on {other}"))
        }
    }
}

pub(crate) fn handle_note(
    repo_root: &std::path::Path,
    id: &str,
    text: &str,
    friction: bool,
    from: Option<&str>,
    json: bool,
) -> Result<(), PulseError> {
    let actor = resolve_actor(repo_root, from)?;
    let kind = if friction {
        KernelNoteKind::Friction
    } else {
        KernelNoteKind::Note
    };
    let record = issues::append_note(repo_root, &actor, id, text, kind)?;
    render(
        json,
        &record,
        format!("{} recorded for {id}", kind.as_str()),
    )
}

#[derive(Debug, Default, serde::Serialize)]
struct TreeEpic {
    id: String,
    title: String,
    status: String,
    stories: Vec<TreeStory>,
}

#[derive(Debug, Default, serde::Serialize)]
struct TreeStory {
    id: String,
    title: String,
    status: String,
    tickets: Vec<TreeTicket>,
}

#[derive(Debug, Default, serde::Serialize)]
struct TreeTicket {
    id: String,
    title: String,
    status: String,
}

fn field<'a>(record: &'a Value, key: &str) -> &'a str {
    record.get(key).and_then(Value::as_str).unwrap_or("")
}

fn build_tree(records: &[Value], root: Option<&str>) -> Vec<TreeEpic> {
    let epics = records.iter().filter(|record| {
        field(record, "kind") == "epic" && root.map_or(true, |root| field(record, "id") == root)
    });
    epics
        .map(|epic| {
            let epic_id = field(epic, "id").to_string();
            let stories = records
                .iter()
                .filter(|record| {
                    field(record, "kind") == "story" && field(record, "epic") == epic_id
                })
                .map(|story| {
                    let story_id = field(story, "id").to_string();
                    let tickets = records
                        .iter()
                        .filter(|record| {
                            field(record, "kind") == "ticket" && field(record, "story") == story_id
                        })
                        .map(|ticket| TreeTicket {
                            id: field(ticket, "id").to_string(),
                            title: field(ticket, "title").to_string(),
                            status: field(ticket, "status").to_string(),
                        })
                        .collect();
                    TreeStory {
                        id: story_id,
                        title: field(story, "title").to_string(),
                        status: field(story, "status").to_string(),
                        tickets,
                    }
                })
                .collect();
            TreeEpic {
                id: epic_id,
                title: field(epic, "title").to_string(),
                status: field(epic, "status").to_string(),
                stories,
            }
        })
        .collect()
}

fn render_tree_human(tree: &[TreeEpic]) -> String {
    let mut lines = Vec::new();
    for epic in tree {
        lines.push(format!("{} [{}] {}", epic.id, epic.status, epic.title));
        for story in &epic.stories {
            lines.push(format!("  {} [{}] {}", story.id, story.status, story.title));
            for ticket in &story.tickets {
                lines.push(format!(
                    "    {} [{}] {}",
                    ticket.id, ticket.status, ticket.title
                ));
            }
        }
    }
    lines.join("\n")
}
