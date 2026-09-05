use std::path::PathBuf;

use crate::knowledge::model::{LearningKind, LearningStatus};
use crate::knowledge::relation::{EndpointKind, RelationType};
use clap::{Subcommand, ValueEnum};

#[derive(Subcommand)]
pub(crate) enum KnowledgeCommand {
    Create {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        json: bool,
    },
    /// Capture a learning candidate from a Ticket: the draft file carries
    /// the content (title, kind, severity, summary, guidance,
    /// applicability); provenance is derived from the Ticket's graph state
    /// and execution evidence.
    Capture {
        /// Ticket whose outcome the learning captures.
        #[arg(long)]
        from: String,
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        json: bool,
    },
    /// Record an evidence-backed validation: candidate|reviewed ->
    /// validated.
    ValidateLearning {
        learning_id: String,
        /// Evidence receipt id that proves the learning.
        #[arg(long)]
        evidence: String,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        json: bool,
    },
    /// Promote a validated learning into a registry document: validated ->
    /// promoted, recording the promoted_to relation with the document's
    /// current revision and content hash.
    Promote {
        learning_id: String,
        #[arg(long)]
        document: String,
        #[arg(long)]
        rationale: Option<String>,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        json: bool,
    },
    Show {
        learning_id: String,
        #[arg(long)]
        json: bool,
    },
    List {
        #[arg(long)]
        status: Option<KnowledgeStatusArg>,
        #[arg(long)]
        kind: Option<KnowledgeKindArg>,
        #[arg(long)]
        json: bool,
    },
    Edit {
        learning_id: String,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        patch: PathBuf,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        json: bool,
    },
    Relation {
        #[command(subcommand)]
        command: KnowledgeRelationCommand,
    },
    Validate {
        #[arg(long)]
        json: bool,
    },
    Export {
        #[arg(long)]
        json: bool,
    },
    Status {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum KnowledgeRelationCommand {
    Add {
        learning_id: String,
        #[arg(long = "type")]
        relation_type: KnowledgeRelationTypeArg,
        #[arg(long)]
        to_kind: KnowledgeEndpointKindArg,
        #[arg(long)]
        to: String,
        #[arg(long)]
        target_revision: Option<u64>,
        #[arg(long)]
        target_hash: Option<String>,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum KnowledgeStatusArg {
    Candidate,
    Reviewed,
    Validated,
    Promoted,
    Disputed,
    Superseded,
    Retired,
}

impl From<KnowledgeStatusArg> for LearningStatus {
    fn from(value: KnowledgeStatusArg) -> Self {
        match value {
            KnowledgeStatusArg::Candidate => LearningStatus::Candidate,
            KnowledgeStatusArg::Reviewed => LearningStatus::Reviewed,
            KnowledgeStatusArg::Validated => LearningStatus::Validated,
            KnowledgeStatusArg::Promoted => LearningStatus::Promoted,
            KnowledgeStatusArg::Disputed => LearningStatus::Disputed,
            KnowledgeStatusArg::Superseded => LearningStatus::Superseded,
            KnowledgeStatusArg::Retired => LearningStatus::Retired,
        }
    }
}

#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum KnowledgeKindArg {
    SuccessPattern,
    FailurePattern,
    Correction,
    Ratchet,
    DecisionHeuristic,
    DebuggingTechnique,
    VerificationTechnique,
    ToolingConstraint,
    EnvironmentConstraint,
    IntegrationConstraint,
    PerformanceInsight,
    SecurityInsight,
    ProcessInsight,
    ContextRoutingInsight,
}

impl From<KnowledgeKindArg> for LearningKind {
    fn from(value: KnowledgeKindArg) -> Self {
        match value {
            KnowledgeKindArg::SuccessPattern => LearningKind::SuccessPattern,
            KnowledgeKindArg::FailurePattern => LearningKind::FailurePattern,
            KnowledgeKindArg::Correction => LearningKind::Correction,
            KnowledgeKindArg::Ratchet => LearningKind::Ratchet,
            KnowledgeKindArg::DecisionHeuristic => LearningKind::DecisionHeuristic,
            KnowledgeKindArg::DebuggingTechnique => LearningKind::DebuggingTechnique,
            KnowledgeKindArg::VerificationTechnique => LearningKind::VerificationTechnique,
            KnowledgeKindArg::ToolingConstraint => LearningKind::ToolingConstraint,
            KnowledgeKindArg::EnvironmentConstraint => LearningKind::EnvironmentConstraint,
            KnowledgeKindArg::IntegrationConstraint => LearningKind::IntegrationConstraint,
            KnowledgeKindArg::PerformanceInsight => LearningKind::PerformanceInsight,
            KnowledgeKindArg::SecurityInsight => LearningKind::SecurityInsight,
            KnowledgeKindArg::ProcessInsight => LearningKind::ProcessInsight,
            KnowledgeKindArg::ContextRoutingInsight => LearningKind::ContextRoutingInsight,
        }
    }
}

#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum KnowledgeRelationTypeArg {
    DerivedFrom,
    Corroborates,
    Contradicts,
    SupersededBy,
    PromotedTo,
    ImplementedBy,
    AppliedTo,
    CausedBy,
}

impl From<KnowledgeRelationTypeArg> for RelationType {
    fn from(value: KnowledgeRelationTypeArg) -> Self {
        match value {
            KnowledgeRelationTypeArg::DerivedFrom => RelationType::DerivedFrom,
            KnowledgeRelationTypeArg::Corroborates => RelationType::Corroborates,
            KnowledgeRelationTypeArg::Contradicts => RelationType::Contradicts,
            KnowledgeRelationTypeArg::SupersededBy => RelationType::SupersededBy,
            KnowledgeRelationTypeArg::PromotedTo => RelationType::PromotedTo,
            KnowledgeRelationTypeArg::ImplementedBy => RelationType::ImplementedBy,
            KnowledgeRelationTypeArg::AppliedTo => RelationType::AppliedTo,
            KnowledgeRelationTypeArg::CausedBy => RelationType::CausedBy,
        }
    }
}

#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum KnowledgeEndpointKindArg {
    Learning,
    Work,
    Receipt,
    Commit,
    Document,
    Decision,
}

impl From<KnowledgeEndpointKindArg> for EndpointKind {
    fn from(value: KnowledgeEndpointKindArg) -> Self {
        match value {
            KnowledgeEndpointKindArg::Learning => EndpointKind::Learning,
            KnowledgeEndpointKindArg::Work => EndpointKind::Work,
            KnowledgeEndpointKindArg::Receipt => EndpointKind::Receipt,
            KnowledgeEndpointKindArg::Commit => EndpointKind::Commit,
            KnowledgeEndpointKindArg::Document => EndpointKind::Document,
            KnowledgeEndpointKindArg::Decision => EndpointKind::Decision,
        }
    }
}

use crate::cli::output::render;
use crate::knowledge::model::{LearningDraft, LearningPatch};
use crate::knowledge::store::{
    KnowledgeStore, OperationContext as KnowledgeOperationContext, RelationAdd,
};
use crate::{JsonGraphStore, PulseError};

pub(crate) fn handle(store: &JsonGraphStore, command: KnowledgeCommand) -> Result<(), PulseError> {
    let knowledge = {
        #[cfg(debug_assertions)]
        {
            match store.failpoint() {
                Some(failpoint) => KnowledgeStore::with_failpoint(store.repo_root(), failpoint),
                None => KnowledgeStore::new(store.repo_root()),
            }
        }
        #[cfg(not(debug_assertions))]
        {
            KnowledgeStore::new(store.repo_root())
        }
    };
    match command {
        KnowledgeCommand::Create { file, actor, json } => {
            let bytes =
                std::fs::read(&file).map_err(|error| PulseError::io(file.clone(), error))?;
            let draft: LearningDraft = serde_json::from_slice(&bytes)
                .map_err(|error| PulseError::json(file.clone(), error))?;
            let out = knowledge.create(
                draft,
                KnowledgeOperationContext {
                    actor,
                    now: chrono::Utc::now(),
                },
            )?;
            render(json, &out, format!("created {}", out.value.id))
        }
        KnowledgeCommand::Capture {
            from,
            file,
            actor,
            json,
        } => {
            let bytes =
                std::fs::read(&file).map_err(|error| PulseError::io(file.clone(), error))?;
            let mut draft: LearningDraft = serde_json::from_slice(&bytes)
                .map_err(|error| PulseError::json(file.clone(), error))?;
            let node = store.show_node(&from)?;
            draft
                .provenance_targets
                .push(crate::knowledge::model::ProvenanceTargetDraft {
                    relation: RelationType::DerivedFrom,
                    kind: EndpointKind::Work,
                    id: node.id.clone(),
                    revision: Some(node.revision),
                    content_hash: None,
                });
            for commit in ticket_source_commits(knowledge.repo_root(), &from) {
                if !draft.source_commits.contains(&commit) {
                    draft.source_commits.push(commit);
                }
            }
            let out = knowledge.create(
                draft,
                KnowledgeOperationContext {
                    actor,
                    now: chrono::Utc::now(),
                },
            )?;
            render(json, &out, format!("captured {} from {from}", out.value.id))
        }
        KnowledgeCommand::ValidateLearning {
            learning_id,
            evidence,
            actor,
            json,
        } => {
            let out = knowledge.transition_status(
                &learning_id,
                LearningStatus::Validated,
                Some(&evidence),
                None,
                None,
                KnowledgeOperationContext {
                    actor,
                    now: chrono::Utc::now(),
                },
            )?;
            render(
                json,
                &out,
                format!("validated {} -> validated", out.value.id),
            )
        }
        KnowledgeCommand::Promote {
            learning_id,
            document,
            rationale,
            actor,
            json,
        } => {
            let out = knowledge.transition_status(
                &learning_id,
                LearningStatus::Promoted,
                None,
                Some(&document),
                rationale,
                KnowledgeOperationContext {
                    actor,
                    now: chrono::Utc::now(),
                },
            )?;
            render(
                json,
                &out,
                format!("{} promoted to {document}", out.value.id),
            )
        }
        KnowledgeCommand::Show { learning_id, json } => {
            let out = knowledge.show(&learning_id)?;
            render(json, &out, learning_id)
        }
        KnowledgeCommand::List { status, kind, json } => {
            let out = knowledge.list(status.map(Into::into), kind.map(Into::into))?;
            render(json, &out, format!("{} learnings", out.items.len()))
        }
        KnowledgeCommand::Edit {
            learning_id,
            expected_revision,
            patch,
            actor,
            json,
        } => {
            let bytes =
                std::fs::read(&patch).map_err(|error| PulseError::io(patch.clone(), error))?;
            let patch_value: LearningPatch = serde_json::from_slice(&bytes)
                .map_err(|error| PulseError::json(patch.clone(), error))?;
            let out = knowledge.edit(
                &learning_id,
                expected_revision,
                patch_value,
                KnowledgeOperationContext {
                    actor,
                    now: chrono::Utc::now(),
                },
            )?;
            render(json, &out, format!("updated {}", out.value.id))
        }
        KnowledgeCommand::Relation { command } => match command {
            KnowledgeRelationCommand::Add {
                learning_id,
                relation_type,
                to_kind,
                to,
                target_revision,
                target_hash,
                expected_revision,
                actor,
                json,
            } => {
                let out = knowledge.add_relation(
                    &learning_id,
                    RelationAdd {
                        relation_type: relation_type.into(),
                        to_kind: to_kind.into(),
                        to,
                        target_revision,
                        target_hash,
                        expected_revision,
                    },
                    KnowledgeOperationContext {
                        actor,
                        now: chrono::Utc::now(),
                    },
                )?;
                render(json, &out, format!("{} {}", out.code, out.relation_id))
            }
        },
        KnowledgeCommand::Validate { json } => {
            let out = knowledge.validate()?;
            let ok = out.valid;
            render(json, &out, if ok { "valid" } else { "invalid" }.to_string())?;
            if ok {
                Ok(())
            } else {
                Err(PulseError::validation(
                    "invalid_knowledge_store",
                    "knowledge store is invalid",
                ))
            }
        }
        KnowledgeCommand::Export { json } => {
            let out = knowledge.export()?;
            render(json, &out, format!("{} learnings", out.counts.entries))
        }
        KnowledgeCommand::Status { json } => {
            let out = knowledge.status()?;
            render(json, &out, format!("knowledge cache {:?}", out.cache_state))
        }
    }
}

/// Source commits bound to a Ticket's execution handoffs, newest last.
fn ticket_source_commits(repo_root: &std::path::Path, ticket_id: &str) -> Vec<String> {
    let directory = repo_root.join(".pulse/evidence/execution/handoffs");
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Vec::new();
    };
    let mut commits = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(receipt) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };
        if receipt["ticket_id"].as_str() == Some(ticket_id) {
            if let Some(commit) = receipt["source_commit"].as_str() {
                if !commits.iter().any(|existing| existing == commit) {
                    commits.push(commit.to_string());
                }
            }
        }
    }
    commits
}
