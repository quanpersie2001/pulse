use clap::{Args, Subcommand};
use serde_json::Value;
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::daemon::persistence::StateStore;
use crate::daemon::protocol::{DaemonRequest, RequestEnvelope};
use crate::daemon::transport::local::{self, LocalClient};
use crate::{PulseError, Result};

#[derive(Subcommand)]
pub(crate) enum DaemonCommand {
    Start,
    Status,
    Stop,
    Doctor,
    #[command(hide = true)]
    Serve,
}

#[derive(Subcommand)]
pub(crate) enum ProjectCommand {
    Open(ProjectOpenArgs),
    List {
        #[arg(long)]
        all: bool,
    },
    Archive {
        project_id: String,
    },
}

#[derive(Args)]
pub(crate) struct ProjectOpenArgs {
    pub(crate) root: String,
}

#[derive(Subcommand)]
pub(crate) enum WorkspaceCommand {
    Create(WorkspaceCreateArgs),
    List {
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        all: bool,
    },
    Archive {
        workspace_id: String,
    },
    Restore {
        workspace_id: String,
    },
}

#[derive(Args)]
pub(crate) struct WorkspaceCreateArgs {
    pub(crate) project_id: String,
    #[arg(long)]
    pub(crate) name: String,
    #[arg(long, value_enum, default_value = "local")]
    pub(crate) isolation: IsolationArg,
    #[arg(long)]
    pub(crate) base_commit: Option<String>,
}

#[derive(Clone, clap::ValueEnum)]
pub(crate) enum IsolationArg {
    Local,
    Worktree,
}

#[derive(Subcommand)]
pub(crate) enum SessionCommand {
    Assign(SessionAssignArgs),
    Acknowledge {
        saga_id: String,
        #[arg(long)]
        acknowledgement_id: String,
    },
    AcknowledgeBound {
        saga_id: String,
        #[arg(long)]
        acknowledgement_id: String,
        #[arg(long)]
        lease_id: String,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        packet_fingerprint: String,
        #[arg(long)]
        delivery_id: String,
    },
    Assignment {
        saga_id: String,
    },
    Handoff(SessionHandoffArgs),
    Verify(SessionVerifyArgs),
    Create(SessionCreateArgs),
    List {
        #[arg(long)]
        workspace: Option<String>,
        #[arg(long)]
        all: bool,
    },
    Show {
        session_id: String,
    },
    Inspect {
        session_id: String,
    },
    Logs {
        session_id: String,
    },
    Send {
        session_id: String,
        input: String,
    },
    Interrupt {
        session_id: String,
    },
    Close {
        session_id: String,
    },
    ForceClose {
        session_id: String,
    },
    Attach {
        session_id: String,
    },
    Resume(SessionResumeArgs),
    Archive {
        session_id: String,
    },
    Timeline {
        #[arg(long)]
        session: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long)]
        cursor_epoch: Option<String>,
        #[arg(long)]
        cursor_sequence: Option<u64>,
    },
}

#[derive(Args)]
pub(crate) struct SessionHandoffArgs {
    pub(crate) saga_id: String,
    #[arg(long)]
    pub(crate) source_commit: String,
    #[arg(long)]
    pub(crate) summary: String,
    #[arg(long, value_delimiter = ',')]
    pub(crate) changed_paths: Vec<String>,
    #[arg(long, value_delimiter = ',')]
    pub(crate) evidence_receipts: Vec<String>,
}

#[derive(Args)]
pub(crate) struct SessionVerifyArgs {
    pub(crate) saga_id: String,
    #[arg(long)]
    pub(crate) actor: String,
    #[arg(long)]
    pub(crate) source_commit: String,
    #[arg(long, value_enum)]
    pub(crate) disposition: VerificationDispositionArg,
    #[arg(long)]
    pub(crate) summary: String,
    #[arg(long)]
    pub(crate) checks: std::path::PathBuf,
}

#[derive(Clone, clap::ValueEnum)]
pub(crate) enum VerificationDispositionArg {
    Passed,
    Rework,
    Blocked,
}

#[derive(Args)]
pub(crate) struct SessionAssignArgs {
    pub(crate) project_id: String,
    pub(crate) ticket_id: String,
    #[arg(long)]
    pub(crate) actor: String,
    #[arg(long)]
    pub(crate) assignee: String,
    #[arg(long, value_delimiter = ',')]
    pub(crate) capabilities: Vec<String>,
    #[arg(long, value_enum, default_value = "worktree")]
    pub(crate) isolation: IsolationArg,
    #[arg(long, default_value = "codex")]
    pub(crate) provider: String,
    #[arg(long, default_value = "{}")]
    pub(crate) provider_options: String,
    #[arg(long, default_value_t = crate::reservation::DEFAULT_TTL_SECONDS)]
    pub(crate) ttl_seconds: u64,
}

#[derive(Args)]
pub(crate) struct SessionCreateArgs {
    pub(crate) workspace_id: String,
    #[arg(long, default_value = "codex")]
    pub(crate) provider: String,
    #[arg(long)]
    pub(crate) parent: Option<String>,
    #[arg(long, default_value = "{}")]
    pub(crate) provider_options: String,
}

#[derive(Args)]
pub(crate) struct SessionResumeArgs {
    pub(crate) session_id: String,
    #[arg(long, default_value = "{}")]
    pub(crate) provider_options: String,
}

pub(crate) fn handle_daemon(command: DaemonCommand, explicit_key: Option<&str>) -> Result<()> {
    let store = StateStore::discover()?;
    match command {
        DaemonCommand::Serve => local::serve(store),
        DaemonCommand::Start => {
            if let Ok(client) = LocalClient::discover(&store) {
                if let Ok(response) =
                    request_with_client(&client, DaemonRequest::Status, fresh_key("daemon_status"))
                {
                    return print_response(response);
                }
            }
            let executable =
                std::env::current_exe().map_err(|error| PulseError::io("<current-exe>", error))?;
            Command::new(executable)
                .args(["daemon", "serve"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|error| PulseError::io("<daemon-spawn>", error))?;
            local::wait_until_ready(&store, Duration::from_secs(10))?;
            let client = LocalClient::discover(&store)?;
            print_response(request_with_client(
                &client,
                DaemonRequest::Status,
                fresh_key("daemon_status"),
            )?)
        }
        DaemonCommand::Status => print_response(request(
            &store,
            DaemonRequest::Status,
            fresh_key("daemon_status"),
        )?),
        DaemonCommand::Stop => {
            dispatch_with_store(&store, DaemonRequest::Shutdown, "daemon_stop", explicit_key)
        }
        DaemonCommand::Doctor => {
            let state = store.load()?;
            let endpoint = LocalClient::discover(&store)?;
            let response = request_with_client(
                &endpoint,
                DaemonRequest::Handshake {
                    client_name: "pulse-cli".to_string(),
                    client_version: env!("CARGO_PKG_VERSION").to_string(),
                },
                fresh_key("daemon_doctor"),
            )?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "status": "ok",
                    "state_schema_version": state.schema_version,
                    "handshake": response,
                }))?
            );
            Ok(())
        }
    }
}

pub(crate) fn handle_project(command: ProjectCommand, explicit_key: Option<&str>) -> Result<()> {
    let request = match command {
        ProjectCommand::Open(args) => DaemonRequest::ProjectOpen { root: args.root },
        ProjectCommand::List { all } => DaemonRequest::ProjectList {
            include_archived: all,
        },
        ProjectCommand::Archive { project_id } => DaemonRequest::ProjectArchive { project_id },
    };
    dispatch_with(request, "project", explicit_key)
}

pub(crate) fn handle_workspace(
    command: WorkspaceCommand,
    explicit_key: Option<&str>,
) -> Result<()> {
    let request = match command {
        WorkspaceCommand::Create(args) => DaemonRequest::WorkspaceCreate {
            project_id: args.project_id,
            name: args.name,
            isolation: match args.isolation {
                IsolationArg::Local => crate::daemon::workspace::IsolationMode::Local,
                IsolationArg::Worktree => crate::daemon::workspace::IsolationMode::Worktree,
            },
            base_commit: args.base_commit,
        },
        WorkspaceCommand::List { project, all } => DaemonRequest::WorkspaceList {
            project_id: project,
            include_archived: all,
        },
        WorkspaceCommand::Archive { workspace_id } => {
            DaemonRequest::WorkspaceArchive { workspace_id }
        }
        WorkspaceCommand::Restore { workspace_id } => {
            DaemonRequest::WorkspaceRestore { workspace_id }
        }
    };
    dispatch_with(request, "workspace", explicit_key)
}

pub(crate) fn handle_session(command: SessionCommand, explicit_key: Option<&str>) -> Result<()> {
    let request = match command {
        SessionCommand::Assign(args) => DaemonRequest::AssignmentStart {
            project_id: args.project_id,
            ticket_id: args.ticket_id,
            actor: args.actor,
            assignee: args.assignee,
            capabilities: args.capabilities,
            isolation: match args.isolation {
                IsolationArg::Local => crate::daemon::workspace::IsolationMode::Local,
                IsolationArg::Worktree => crate::daemon::workspace::IsolationMode::Worktree,
            },
            provider_id: args.provider,
            provider_options: serde_json::from_str::<Value>(&args.provider_options)?,
            ttl_seconds: args.ttl_seconds,
        },
        SessionCommand::Acknowledge {
            saga_id,
            acknowledgement_id,
        } => DaemonRequest::AssignmentAcknowledge {
            saga_id,
            acknowledgement_id,
        },
        SessionCommand::AcknowledgeBound {
            saga_id,
            acknowledgement_id,
            lease_id,
            session_id,
            packet_fingerprint,
            delivery_id,
        } => DaemonRequest::AssignmentAcknowledgeBound {
            saga_id,
            acknowledgement_id,
            lease_id,
            session_id,
            packet_fingerprint,
            delivery_id,
        },
        SessionCommand::Assignment { saga_id } => DaemonRequest::AssignmentInspect { saga_id },
        SessionCommand::Handoff(args) => DaemonRequest::HandoffSubmit {
            saga_id: args.saga_id,
            source_commit: args.source_commit,
            summary: args.summary,
            changed_paths: args.changed_paths,
            evidence_receipt_ids: args.evidence_receipts,
        },
        SessionCommand::Verify(args) => {
            let bytes =
                std::fs::read(&args.checks).map_err(|error| PulseError::io(&args.checks, error))?;
            let checks = serde_json::from_slice(&bytes)
                .map_err(|error| PulseError::json(&args.checks, error))?;
            DaemonRequest::VerificationComplete {
                saga_id: args.saga_id,
                actor: args.actor,
                source_commit: args.source_commit,
                disposition: match args.disposition {
                    VerificationDispositionArg::Passed => {
                        crate::execution::VerificationDisposition::Passed
                    }
                    VerificationDispositionArg::Rework => {
                        crate::execution::VerificationDisposition::Rework
                    }
                    VerificationDispositionArg::Blocked => {
                        crate::execution::VerificationDisposition::Blocked
                    }
                },
                summary: args.summary,
                checks,
            }
        }
        SessionCommand::Create(args) => DaemonRequest::SessionCreate {
            workspace_id: args.workspace_id,
            provider_id: args.provider,
            parent_session_id: args.parent,
            provider_options: serde_json::from_str::<Value>(&args.provider_options)?,
        },
        SessionCommand::List { workspace, all } => DaemonRequest::SessionList {
            workspace_id: workspace,
            include_archived: all,
        },
        SessionCommand::Show { session_id } => DaemonRequest::SessionShow { session_id },
        SessionCommand::Inspect { session_id } => DaemonRequest::SessionInspect { session_id },
        SessionCommand::Logs { session_id } => DaemonRequest::SessionLogs { session_id },
        SessionCommand::Send { session_id, input } => {
            DaemonRequest::SessionSend { session_id, input }
        }
        SessionCommand::Interrupt { session_id } => DaemonRequest::SessionInterrupt { session_id },
        SessionCommand::Close { session_id } => DaemonRequest::SessionClose { session_id },
        SessionCommand::ForceClose { session_id } => {
            DaemonRequest::SessionForceClose { session_id }
        }
        SessionCommand::Attach { session_id } => DaemonRequest::SessionAttach { session_id },
        SessionCommand::Resume(args) => DaemonRequest::SessionResume {
            session_id: args.session_id,
            provider_options: serde_json::from_str::<Value>(&args.provider_options)?,
        },
        SessionCommand::Archive { session_id } => DaemonRequest::SessionArchive { session_id },
        SessionCommand::Timeline {
            session,
            limit,
            cursor_epoch,
            cursor_sequence,
        } => {
            let cursor = match (cursor_epoch, cursor_sequence) {
                (Some(epoch), Some(sequence)) => {
                    Some(crate::daemon::timeline::TimelineCursor { epoch, sequence })
                }
                (None, None) => None,
                _ => {
                    return Err(PulseError::validation(
                        "timeline_cursor_invalid",
                        "cursor epoch and sequence must be supplied together",
                    ))
                }
            };
            DaemonRequest::TimelineList {
                cursor,
                limit,
                session_id: session,
            }
        }
    };
    dispatch_with(request, "session", explicit_key)
}

fn dispatch_with(
    request_value: DaemonRequest,
    prefix: &str,
    explicit_key: Option<&str>,
) -> Result<()> {
    let store = StateStore::discover()?;
    dispatch_with_store(&store, request_value, prefix, explicit_key)
}

fn dispatch_with_store(
    store: &StateStore,
    request_value: DaemonRequest,
    prefix: &str,
    explicit_key: Option<&str>,
) -> Result<()> {
    let key = selected_idempotency_key(request_value.is_mutating(), explicit_key, prefix)?;
    print_response(request(store, request_value, key)?)
}

fn selected_idempotency_key(
    is_mutating: bool,
    explicit_key: Option<&str>,
    prefix: &str,
) -> Result<String> {
    match (is_mutating, explicit_key) {
        (true, Some(key)) if key.trim().is_empty() || key.trim() != key => {
            Err(PulseError::validation(
                "idempotency_key_invalid",
                "explicit idempotency key must be non-empty and have no surrounding whitespace",
            ))
        }
        (true, Some(key)) => Ok(key.to_string()),
        (true, None) => Ok(fresh_key(prefix)),
        (false, _) => Ok(String::new()),
    }
}

fn request(
    store: &StateStore,
    request_value: DaemonRequest,
    idempotency_key: String,
) -> Result<crate::daemon::protocol::DaemonResponse> {
    let client = LocalClient::discover(store)?;
    request_with_client(&client, request_value, idempotency_key)
}

fn request_with_client(
    client: &LocalClient,
    request_value: DaemonRequest,
    idempotency_key: String,
) -> Result<crate::daemon::protocol::DaemonResponse> {
    let envelope = RequestEnvelope::new(request_value, idempotency_key);
    let response = client.request(envelope)?;
    response.response.map_err(|error| {
        PulseError::validation(
            "daemon_request_failed",
            format!("daemon error {}: {}", error.code, error.message),
        )
    })
}

fn print_response(response: crate::daemon::protocol::DaemonResponse) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}

fn fresh_key(prefix: &str) -> String {
    format!("{prefix}_{}", ulid::Ulid::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn cli_parses_session_resume_and_global_idempotency_key() {
        let cli = match crate::cli::Cli::try_parse_from([
            "pulse",
            "--idempotency-key",
            "stable-resume",
            "session",
            "resume",
            "ses_test",
            "--provider-options",
            "{}",
        ]) {
            Ok(cli) => cli,
            Err(error) => panic!("session resume CLI should parse: {error}"),
        };
        assert_eq!(cli.idempotency_key.as_deref(), Some("stable-resume"));
        assert!(matches!(
            cli.command,
            crate::cli::args::Command::Session {
                command: SessionCommand::Resume(SessionResumeArgs { session_id, .. })
            } if session_id == "ses_test"
        ));
    }

    #[test]
    fn cli_parses_daemon_backed_session_inspect_and_logs() {
        let inspect = crate::cli::Cli::try_parse_from(["pulse", "session", "inspect", "ses_test"])
            .expect("session inspect CLI should parse");
        assert!(matches!(
            inspect.command,
            crate::cli::args::Command::Session {
                command: SessionCommand::Inspect { session_id }
            } if session_id == "ses_test"
        ));
        let logs = crate::cli::Cli::try_parse_from(["pulse", "session", "logs", "ses_test"])
            .expect("session logs CLI should parse");
        assert!(matches!(
            logs.command,
            crate::cli::args::Command::Session {
                command: SessionCommand::Logs { session_id }
            } if session_id == "ses_test"
        ));
    }

    #[test]
    fn explicit_idempotency_key_validation_matches_daemon_contract() {
        assert_eq!(
            selected_idempotency_key(true, Some("stable"), "session").unwrap(),
            "stable"
        );
        assert_eq!(
            selected_idempotency_key(false, Some("ignored"), "session").unwrap(),
            ""
        );
        assert_eq!(
            selected_idempotency_key(true, Some("  "), "session")
                .unwrap_err()
                .code(),
            "idempotency_key_invalid"
        );
        assert_eq!(
            selected_idempotency_key(true, Some(" padded "), "session")
                .unwrap_err()
                .code(),
            "idempotency_key_invalid"
        );
    }
}
