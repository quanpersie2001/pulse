use std::path::PathBuf;

use crate::evidence::model::{ReceiptKind, ReceiptResult};
use clap::{Subcommand, ValueEnum};

#[derive(Subcommand)]
pub(crate) enum EvidenceCommand {
    Bootstrap {
        #[arg(long)]
        json: bool,
    },
    Artifact {
        #[command(subcommand)]
        command: ArtifactCommand,
    },
    Receipt {
        #[command(subcommand)]
        command: ReceiptCommand,
    },
}

#[derive(Subcommand)]
pub(crate) enum ArtifactCommand {
    Put {
        path: PathBuf,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        media_type: Option<String>,
        #[arg(long)]
        original_name: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Show {
        digest: String,
        #[arg(long)]
        json: bool,
    },
    Verify {
        digest: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum ReceiptCommand {
    Record {
        #[arg(long)]
        file: PathBuf,
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
        kind: Option<ReceiptKindArg>,
        #[arg(long)]
        subject: Option<String>,
        #[arg(long)]
        result: Option<ReceiptResultArg>,
        #[arg(long)]
        json: bool,
    },
    Verify {
        id: String,
        #[arg(long)]
        current: bool,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum ReceiptKindArg {
    SupersessionReconciliation,
    ShapingValidation,
    DecisionAcceptance,
    DocumentationValidation,
}

impl From<ReceiptKindArg> for ReceiptKind {
    fn from(value: ReceiptKindArg) -> Self {
        match value {
            ReceiptKindArg::SupersessionReconciliation => ReceiptKind::SupersessionReconciliation,
            ReceiptKindArg::ShapingValidation => ReceiptKind::ShapingValidation,
            ReceiptKindArg::DecisionAcceptance => ReceiptKind::DecisionAcceptance,
            ReceiptKindArg::DocumentationValidation => ReceiptKind::DocumentationValidation,
        }
    }
}

#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum ReceiptResultArg {
    Passed,
    Failed,
    Inconclusive,
}

impl From<ReceiptResultArg> for ReceiptResult {
    fn from(value: ReceiptResultArg) -> Self {
        match value {
            ReceiptResultArg::Passed => ReceiptResult::Passed,
            ReceiptResultArg::Failed => ReceiptResult::Failed,
            ReceiptResultArg::Inconclusive => ReceiptResult::Inconclusive,
        }
    }
}

use crate::cli::output::render;
use crate::{JsonGraphStore, PulseError};

pub(crate) fn handle(store: &JsonGraphStore, command: EvidenceCommand) -> Result<(), PulseError> {
    match command {
        EvidenceCommand::Bootstrap { json } => {
            let out = crate::evidence::bootstrap(store.repo_root())?;
            render(json, &out, "evidence bootstrapped".to_string())
        }
        EvidenceCommand::Artifact { command } => match command {
            ArtifactCommand::Put {
                path,
                kind,
                media_type,
                original_name,
                json,
            } => {
                let manifest = crate::evidence::manifest::load(store.repo_root())?;
                let out = crate::evidence::put_artifact(
                    store.repo_root(),
                    store.failpoint(),
                    &path,
                    kind,
                    media_type,
                    original_name,
                    manifest.max_artifact_bytes,
                )?;
                render(json, &out, format!("{} {}", out.code, out.artifact.digest))
            }
            ArtifactCommand::Show { digest, json } => {
                let out = crate::evidence::show_artifact(store.repo_root(), &digest)?;
                render(json, &out, out.digest.clone())
            }
            ArtifactCommand::Verify { digest, json } => {
                let out = crate::evidence::verify_artifact(store.repo_root(), &digest)?;
                render(json, &out, out.code.clone())
            }
        },
        EvidenceCommand::Receipt { command } => match command {
            ReceiptCommand::Record { file, json } => {
                let out =
                    crate::evidence::record_receipt(store.repo_root(), store.failpoint(), &file)?;
                render(json, &out, format!("{} {}", out.code, out.receipt.id))
            }
            ReceiptCommand::Show { id, json } => {
                let out = crate::evidence::show_receipt(store.repo_root(), &id)?;
                render(json, &out, out.receipt.id.clone())
            }
            ReceiptCommand::List {
                kind,
                subject,
                result,
                json,
            } => {
                let out = crate::evidence::list_receipts(
                    store.repo_root(),
                    kind.map(Into::into),
                    subject,
                    result.map(Into::into),
                )?;
                render(json, &out, format!("{} receipts", out.receipts.len()))
            }
            ReceiptCommand::Verify {
                id,
                current,
                source,
                json,
            } => {
                let out = crate::evidence::verify_receipt(
                    store.repo_root(),
                    &id,
                    current,
                    source.as_deref(),
                )?;
                let ok = out.integrity.status == "valid"
                    && (!current || out.bindings.status == "current");
                render(json, &out, out.integrity.status.clone())?;
                if ok {
                    Ok(())
                } else {
                    Err(PulseError::validation(
                        "receipt_hash_mismatch",
                        "receipt verification failed",
                    ))
                }
            }
        },
    }
}
