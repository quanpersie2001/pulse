//! `pulse learn add|show|friction|dismiss|applicable|activate|retire`
//! (plan 0022 §11.2; friction/dismiss are plan 0025 E1). Thin renderer:
//! resolves the actor, parses `--from`, calls `pulse::learn`, renders the
//! result.

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
        /// Friction this learning classifies, as `<subject>#<evt id>` —
        /// listed by `pulse learn friction <subject>`; repeatable. Appended
        /// to the learning's `from` citations (plan 0025 E1).
        #[arg(long = "friction")]
        frictions: Vec<String>,
        /// Check argv as a JSON array of strings (plan 0025 E2), e.g.
        /// '["cargo","test","--lib"]'. Once a human activates the learning,
        /// `pulse verify` runs this for every matching ticket.
        #[arg(long = "check-argv", value_name = "JSON_ARRAY")]
        check_argv: Option<String>,
        /// Working directory for --check-argv, relative to the repo root.
        #[arg(long = "check-cwd", value_name = "DIR")]
        check_cwd: Option<String>,
        /// Code this learning cites, as `<path>:<from>-<to>` (1-based,
        /// inclusive; repeatable). Pulse hashes the exact lines from disk —
        /// the agent never types a hash (plan 0025 E4).
        #[arg(long = "cite", value_name = "PATH:FROM-TO")]
        cites: Vec<String>,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// One learning (`show <id>`) or all of them (`show` — Decision 0023's
    /// CLI-leaf pairing cut merged the old `learn list` in here).
    Show {
        id: Option<String>,
        #[arg(long)]
        status: Option<String>,
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
    /// Friction notes still waiting to be classified (plan 0025 E1):
    /// unclassified by default; `--all` also shows what a learning already
    /// cites or a dismissal already explains. One per line:
    /// `<evt id>  <subject>  <state>  <text>`.
    Friction {
        /// Only this record's frictions.
        id: Option<String>,
        /// Also show frictions already classified (learned/dismissed).
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    /// Record why frictions stay Ticket-specific (plan 0025 E1): one
    /// `friction.dismissed` event each. Every actor may dismiss — a
    /// dismissal is a classification, not an erasure. Without friction ids
    /// or `--all` nothing is selected. Idempotent: frictions a learning
    /// already cites (or an earlier dismissal already explains) are skipped.
    Dismiss {
        /// The record the frictions sit on.
        subject: String,
        /// Friction event ids from `pulse learn friction <subject>`.
        #[arg(value_name = "EVT_ID")]
        keys: Vec<String>,
        /// Every unclassified friction of the subject.
        #[arg(long)]
        all: bool,
        #[arg(long, visible_alias = "--why")]
        reason: String,
        #[arg(long)]
        actor: Option<String>,
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

/// A typo in the subject of `learn friction`/`learn dismiss` must look like
/// one, not like an empty list.
fn require_issue(repo_root: &std::path::Path, id: &str) -> Result<(), PulseError> {
    let records = crate::store::issues::read_all(repo_root)?;
    if records
        .iter()
        .any(|record| record.get("id").and_then(|v| v.as_str()) == Some(id))
    {
        return Ok(());
    }
    Err(PulseError::kernel(
        "issue_not_found",
        format!("no record with id {id}"),
        "check the id with `pulse list`; ids are hash-based and never recycled",
    ))
}

fn truncate(text: &str, max: usize) -> &str {
    match text.char_indices().nth(max) {
        Some((byte, _)) => &text[..byte],
        None => text,
    }
}

fn cite_invalid(value: &str) -> PulseError {
    PulseError::kernel(
        "learning_cite_invalid",
        format!("--cite {value:?} is not <path>:<from>-<to>"),
        "a cite is a repo-relative path, a colon, then a 1-based inclusive line range, \
         e.g. --cite src/auth/refresh.rs:40-58",
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
            frictions,
            check_argv,
            check_cwd,
            cites,
            actor,
            json,
        } => {
            let actor = resolve_actor(repo_root, actor.as_deref())?;
            let check_argv = match check_argv {
                Some(raw) => match serde_json::from_str::<Vec<String>>(&raw) {
                    Ok(argv) => argv,
                    Err(_) => {
                        return Err(PulseError::kernel(
                            "learning_invalid",
                            "--check-argv is not a JSON array of strings",
                            r#"pass a JSON array like '["cargo","test","--lib"]'"#,
                        ))
                    }
                },
                None => Vec::new(),
            };
            let mut cite_specs = Vec::new();
            for cite in &cites {
                let Some((path, lines)) = cite.rsplit_once(':') else {
                    return Err(cite_invalid(cite));
                };
                if path.is_empty() || !lines.contains('-') {
                    return Err(cite_invalid(cite));
                }
                cite_specs.push(learn::CiteSpec {
                    path: path.to_string(),
                    lines: lines.to_string(),
                });
            }
            let extras = learn::AddExtras {
                frictions,
                check_argv,
                check_cwd,
                cites: cite_specs,
            };
            let input = if let Some(path) = from {
                let text =
                    std::fs::read_to_string(&path).map_err(|error| PulseError::io(&path, error))?;
                learn::AddInput::FromFile(text)
            } else {
                let title = title.ok_or_else(|| {
                    PulseError::kernel(
                        "learning_invalid",
                        "no --from file and no --title given",
                        "pass --from <file>, or --title <t> --kind <failure|constraint|technique|routing>",
                    )
                })?;
                let kind = kind.ok_or_else(|| {
                    PulseError::kernel(
                        "learning_invalid",
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
            let learning = learn::add(repo_root, &actor, input, extras)?;
            let id = learning.frontmatter.id.clone();
            render(json, &learning_value(&learning), format!("created {id}"))
        }
        LearnCommand::Show {
            id: None,
            status,
            json,
        } => {
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
        LearnCommand::Show {
            id: Some(id), json, ..
        } => {
            let learning = store::read(repo_root, &id)?;
            let human = store::render(&learning)?;
            render(json, &learning_value(&learning), human)
        }
        LearnCommand::Applicable { id, all, json } => {
            // Plan 0025 E3: `--all` widens the net — candidates AND suspect
            // learnings (misleading > helpful), each suspect marked, so a
            // learning that slipped into disrepute stays visible until a
            // human retires it.
            let matched = if all {
                learn::recall::matching_including_suspects(repo_root, &id, true)?
            } else {
                learn::recall::matching(repo_root, &id, false)?
            };
            let mut matched = matched;
            matched.sort_by(|a, b| {
                b.frontmatter
                    .usage
                    .helpful
                    .cmp(&a.frontmatter.usage.helpful)
                    .then_with(|| a.frontmatter.id.cmp(&b.frontmatter.id))
            });
            let values: Vec<Value> = matched
                .iter()
                .map(|learning| {
                    let mut value = learning_value(learning);
                    if let Some(object) = value.as_object_mut() {
                        object.insert(
                            "suspect".to_string(),
                            json!(learn::recall::is_suspect(learning)),
                        );
                    }
                    value
                })
                .collect();
            let human = matched
                .iter()
                .map(|learning| {
                    if learn::recall::is_suspect(learning) {
                        format!("{} [suspect: misleading > helpful]", summary_line(learning))
                    } else {
                        summary_line(learning)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            render(json, &values, human)
        }
        LearnCommand::Friction { id, all, json } => {
            if let Some(subject) = &id {
                require_issue(repo_root, subject)?;
            }
            let frictions = learn::friction::list(repo_root, id.as_deref())?;
            let shown: Vec<&learn::friction::Friction> = frictions
                .iter()
                .filter(|friction| {
                    all || friction.state == learn::friction::FrictionState::Unclassified
                })
                .collect();
            let values: Vec<Value> = shown
                .iter()
                .map(|friction| serde_json::to_value(friction).unwrap_or_else(|_| json!({})))
                .collect();
            let human = shown
                .iter()
                .map(|friction| {
                    format!(
                        "{}  {}  {}  {}",
                        friction.key,
                        friction.subject,
                        friction.state.label(),
                        truncate(&friction.text, 80),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            render(json, &values, human)
        }
        LearnCommand::Dismiss {
            subject,
            keys,
            all,
            reason,
            actor,
            json,
        } => {
            if reason.trim().is_empty() {
                return Err(PulseError::kernel(
                    "friction_reason_missing",
                    "--reason is empty; a dismissal without a why is an erasure",
                    "say what makes the friction Ticket-specific: \"<ticket-specific: …>\"",
                ));
            }
            if keys.is_empty() && !all {
                return Err(PulseError::kernel(
                    "friction_selection_missing",
                    format!("no friction ids given for {subject} and --all is not set"),
                    "list the frictions with `pulse learn friction <subject-id>`, then pass their \
                     evt ids here, or pass --all to dismiss every unclassified one",
                ));
            }
            let actor = resolve_actor(repo_root, actor.as_deref())?;
            let outcome = learn::dismiss(repo_root, &actor, &subject, &keys, all, &reason)?;
            let mut human = outcome
                .dismissed
                .iter()
                .map(|key| format!("dismissed {key}"))
                .chain(
                    outcome
                        .skipped
                        .iter()
                        .map(|key| format!("skipped {key} (already classified)")),
                )
                .collect::<Vec<_>>()
                .join("\n");
            if human.is_empty() {
                human = format!("{subject}: nothing to dismiss");
            }
            render(
                json,
                &json!({
                    "subject": subject,
                    "dismissed": outcome.dismissed,
                    "skipped": outcome.skipped,
                }),
                human,
            )
        }
        LearnCommand::Activate { id, actor, json } => {
            let actor = resolve_actor(repo_root, actor.as_deref())?;
            let learning = learn::activate(repo_root, &actor, &id)?;
            // Plan 0025 E2: activation is the human's decision to let Pulse
            // *run* the check — the message must say so, never bury it.
            let human = if learning.frontmatter.check_argv.is_empty() {
                format!("{id} is active")
            } else {
                format!(
                    "{id} is active — this check will now run in `pulse verify` for every matching ticket"
                )
            };
            render(json, &learning_value(&learning), human)
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
