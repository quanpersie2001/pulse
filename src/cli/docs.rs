use std::path::PathBuf;

use crate::graph::model::node::DocumentationImpactPosture;
use clap::{Subcommand, ValueEnum};

#[derive(Subcommand)]
pub(crate) enum DocsCommand {
    Register {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        expected_registry_revision: u64,
        #[arg(long)]
        actor: String,
        #[arg(long = "tag")]
        tag: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    Tags {
        #[command(subcommand)]
        command: DocsTagsCommand,
    },
    Edit {
        document_id: String,
        #[arg(long)]
        patch: PathBuf,
        #[arg(long)]
        expected_registry_revision: u64,
        #[arg(long)]
        expected_document_revision: u64,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        json: bool,
    },
    Retire {
        document_id: String,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        expected_registry_revision: u64,
        #[arg(long)]
        expected_document_revision: u64,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        json: bool,
    },
    Supersede {
        old_id: String,
        #[arg(long = "by")]
        replacement_id: String,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        expected_registry_revision: u64,
        #[arg(long)]
        expected_document_revision: u64,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        json: bool,
    },
    List {
        #[arg(long)]
        kind: Option<DocKindArg>,
        #[arg(long)]
        status: Option<DocumentStatusArg>,
        #[arg(long)]
        json: bool,
    },
    Show {
        document_id: String,
        #[arg(long)]
        json: bool,
    },
    Validate {
        #[arg(long)]
        record: bool,
        #[arg(long, requires = "record")]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Index {
        #[arg(long)]
        changed: bool,
        #[arg(long)]
        rebuild: bool,
        #[arg(long)]
        check: bool,
        #[arg(long)]
        json: bool,
    },
    Status {
        #[arg(long)]
        json: bool,
    },
    Search {
        query: String,
        #[arg(long)]
        kind: Option<DocKindArg>,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long)]
        status: Option<DocumentStatusArg>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        no_refresh: bool,
        #[arg(long)]
        explain: bool,
        #[arg(long)]
        include_draft: bool,
        #[arg(long)]
        include_stale: bool,
        #[arg(long = "work")]
        work_id: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Get {
        reference: String,
        #[arg(long)]
        max_lines: Option<u32>,
        #[arg(long)]
        max_bytes: Option<usize>,
        #[arg(long)]
        full: bool,
        #[arg(long)]
        full_section: bool,
        #[arg(long)]
        no_refresh: bool,
        #[arg(long)]
        json: bool,
    },
    Tree {
        path: Option<String>,
        #[arg(long)]
        depth: Option<usize>,
        #[arg(long)]
        include_draft: bool,
        #[arg(long)]
        include_stale: bool,
        #[arg(long)]
        json: bool,
    },
    Applicable {
        #[arg(long = "work")]
        work_id: String,
        #[arg(long)]
        include_draft: bool,
        #[arg(long)]
        include_stale: bool,
        #[arg(long)]
        json: bool,
    },
    Impact {
        ticket_id: String,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        posture: DocumentationPostureArg,
        #[arg(long)]
        rationale: Option<String>,
        #[arg(long = "required-doc")]
        required_doc: Vec<String>,
        #[arg(long = "deferred-to")]
        deferred_to: Vec<String>,
        #[arg(long = "path")]
        path: Vec<String>,
        #[arg(long = "domain")]
        domain: Vec<String>,
        #[arg(long = "label")]
        label: Vec<String>,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum DocumentationPostureArg {
    Required,
    None,
    Deferred,
}

impl From<DocumentationPostureArg> for DocumentationImpactPosture {
    fn from(value: DocumentationPostureArg) -> Self {
        match value {
            DocumentationPostureArg::Required => DocumentationImpactPosture::Required,
            DocumentationPostureArg::None => DocumentationImpactPosture::None,
            DocumentationPostureArg::Deferred => DocumentationImpactPosture::Deferred,
        }
    }
}

#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum DocKindArg {
    Policy,
    Product,
    Architecture,
    Domain,
    Operations,
    Reference,
    Generated,
}

impl From<DocKindArg> for DocumentKind {
    fn from(value: DocKindArg) -> Self {
        match value {
            DocKindArg::Policy => DocumentKind::Policy,
            DocKindArg::Product => DocumentKind::Product,
            DocKindArg::Architecture => DocumentKind::Architecture,
            DocKindArg::Domain => DocumentKind::Domain,
            DocKindArg::Operations => DocumentKind::Operations,
            DocKindArg::Reference => DocumentKind::Reference,
            DocKindArg::Generated => DocumentKind::Generated,
        }
    }
}

#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum DocumentStatusArg {
    Approved,
    Draft,
    Stale,
    Retired,
}

impl From<DocumentStatusArg> for crate::docs::DocumentStatus {
    fn from(value: DocumentStatusArg) -> Self {
        match value {
            DocumentStatusArg::Approved => Self::Approved,
            DocumentStatusArg::Draft => Self::Draft,
            DocumentStatusArg::Stale => Self::Stale,
            DocumentStatusArg::Retired => Self::Retired,
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum DocsTagsCommand {
    Add {
        tag: String,
        #[arg(long)]
        json: bool,
    },
    List {
        #[arg(long)]
        json: bool,
    },
}
use serde_json::json;

use crate::cli::output::render;
use crate::docs::model::{DocumentKind, DocumentPatch, DocumentRecord};
use crate::graph::store::DocumentationImpactUpdate;
use crate::{JsonGraphStore, PulseError};

pub(crate) fn handle(store: &JsonGraphStore, command: DocsCommand) -> Result<(), PulseError> {
    match command {
        DocsCommand::Register {
            file,
            expected_registry_revision,
            actor,
            tag,
            json,
        } => {
            let bytes =
                std::fs::read(&file).map_err(|error| PulseError::io(file.clone(), error))?;
            let mut document: DocumentRecord = serde_json::from_slice(&bytes)
                .map_err(|error| PulseError::json(file.clone(), error))?;
            for value in tag {
                let value = crate::docs::normalize_tag(&value)?;
                if !document.tags.contains(&value) {
                    document.tags.push(value);
                }
            }
            document.tags.sort();
            let out = crate::docs::register(
                store.repo_root(),
                expected_registry_revision,
                document,
                actor,
            )?;
            render(json, &out, format!("registered {}", out.value.id))
        }
        DocsCommand::Edit {
            document_id,
            patch,
            expected_registry_revision,
            expected_document_revision,
            actor,
            json,
        } => {
            let bytes =
                std::fs::read(&patch).map_err(|error| PulseError::io(patch.clone(), error))?;
            let patch_value: DocumentPatch = serde_json::from_slice(&bytes)
                .map_err(|error| PulseError::json(patch.clone(), error))?;
            let out = crate::docs::edit(
                store.repo_root(),
                &document_id,
                expected_registry_revision,
                expected_document_revision,
                patch_value,
                actor,
            )?;
            render(json, &out, format!("updated {}", out.value.id))
        }
        DocsCommand::Retire {
            document_id,
            reason,
            expected_registry_revision,
            expected_document_revision,
            actor,
            json,
        } => {
            let out = crate::docs::retire(
                store.repo_root(),
                &document_id,
                expected_registry_revision,
                expected_document_revision,
                reason,
                actor,
            )?;
            render(json, &out, format!("retired {}", out.value.id))
        }
        DocsCommand::Supersede {
            old_id,
            replacement_id,
            reason,
            expected_registry_revision,
            expected_document_revision,
            actor,
            json,
        } => {
            let out = crate::docs::supersede(
                store.repo_root(),
                &old_id,
                &replacement_id,
                expected_registry_revision,
                expected_document_revision,
                reason,
                actor,
            )?;
            render(json, &out, format!("superseded {}", out.value.id))
        }
        DocsCommand::Tags { command } => match command {
            DocsTagsCommand::Add { tag, json } => {
                let tags = crate::docs::add_tag(store.repo_root(), &tag)?;
                render(
                    json,
                    &tags,
                    format!("added tag {}", crate::docs::normalize_tag(&tag)?),
                )
            }
            DocsTagsCommand::List { json } => {
                let tags = crate::docs::load_tags(store.repo_root())?;
                render(json, &tags, format!("{} tags", tags.tags.len()))
            }
        },
        DocsCommand::List { kind, status, json } => {
            let mut documents = crate::docs::list(store.repo_root())?;
            if let Some(kind) = kind {
                let kind: DocumentKind = kind.into();
                documents.retain(|document| document.kind == kind);
            }
            if let Some(status) = status {
                let status: crate::docs::DocumentStatus = status.into();
                documents.retain(|document| document.status == status);
            }
            documents.sort_by(|left, right| left.id.cmp(&right.id));
            let out = json!({"schema_version": 1, "code": "ok", "documents": documents});
            let count = out["documents"]
                .as_array()
                .map(|items| items.len())
                .unwrap_or(0);
            render(json, &out, format!("{count} documents"))
        }
        DocsCommand::Show { document_id, json } => {
            let document = crate::docs::show(store.repo_root(), &document_id)?;
            render(
                json,
                &json!({"schema_version": 1, "code": "ok", "document": document}),
                document_id,
            )
        }
        DocsCommand::Validate {
            record,
            actor,
            json,
        } => {
            if record
                && actor
                    .as_deref()
                    .map_or(true, |value| value.trim().is_empty())
            {
                return Err(PulseError::validation(
                    "docs_validation_actor_required",
                    "--record requires --actor",
                ));
            }
            let run = crate::kernel::documentation::run_documentation_validation(
                store.repo_root(),
                store.failpoint(),
                actor.as_deref(),
            )?;
            let ok = run.validation.valid;
            if record {
                render(
                    json,
                    &run,
                    run.receipt.as_ref().map_or_else(
                        || "invalid".to_string(),
                        |receipt| receipt.receipt.id.clone(),
                    ),
                )?;
            } else {
                render(
                    json,
                    &run.validation,
                    if ok { "valid" } else { "invalid" }.to_string(),
                )?;
            }
            if ok {
                Ok(())
            } else {
                let code = if run.validation.code == "invalid_docs_registry" {
                    "invalid_docs_registry"
                } else {
                    "docs_validation_failed"
                };
                Err(PulseError::validation(
                    code,
                    "documentation validation failed",
                ))
            }
        }
        DocsCommand::Index {
            changed,
            rebuild,
            check,
            json,
        } => {
            let opts = crate::docs::IndexOptions {
                changed,
                rebuild,
                check,
                ..crate::docs::IndexOptions::default()
            };
            let out = if check {
                crate::docs::check_index(store.repo_root())?
            } else {
                crate::docs::build_index(store.repo_root(), opts)?
            };
            render(json, &out, index_summary(&out))
        }
        DocsCommand::Status { json } => {
            let out = crate::docs::index_status(store.repo_root())?;
            render(json, &out, index_summary(&out))
        }
        DocsCommand::Search {
            query,
            kind,
            tag,
            status,
            limit,
            no_refresh,
            explain,
            include_draft,
            include_stale,
            work_id,
            json,
        } => {
            let work = match work_id {
                Some(work_id) => {
                    let node = store.show_node(&work_id)?;
                    Some(
                        node.documentation
                            .as_ref()
                            .map(|documentation| {
                                crate::docs::WorkDocumentationContext::from((
                                    node.id.as_str(),
                                    node.revision,
                                    documentation,
                                ))
                            })
                            .unwrap_or_else(|| {
                                crate::docs::WorkDocumentationContext::unknown(
                                    node.id.clone(),
                                    node.revision,
                                )
                            }),
                    )
                }
                None => None,
            };
            let out = crate::docs::search_docs(
                store.repo_root(),
                &query,
                crate::docs::SearchOptions {
                    kind: kind.map(Into::into),
                    tag,
                    status: status.map(Into::into),
                    limit,
                    no_refresh,
                    explain,
                    include_draft,
                    include_stale,
                    work,
                    under_repository_fence: false,
                },
            )?;
            render(json, &out, format!("{} docs hits", out.results.len()))
        }
        DocsCommand::Get {
            reference,
            max_lines,
            max_bytes,
            full,
            full_section,
            no_refresh,
            json,
        } => {
            if no_refresh {
                return Err(PulseError::validation(
                "unsupported_option",
                "docs get reads canonical files directly; --no-refresh is only meaningful for docs search",
            ));
            }
            let out = crate::docs::get_docs(
                store.repo_root(),
                &reference,
                crate::docs::GetOptions {
                    max_lines,
                    max_bytes,
                    full,
                    full_section,
                },
            )?;
            render(json, &out, reference)
        }
        DocsCommand::Tree {
            path,
            depth,
            include_draft,
            include_stale,
            json,
        } => {
            let out = crate::docs::docs_tree(
                store.repo_root(),
                path.as_deref(),
                crate::docs::TreeOptions {
                    depth,
                    include_draft,
                    include_stale,
                },
            )?;
            render(json, &out, format!("{} tree nodes", out.nodes.len()))
        }
        DocsCommand::Applicable {
            work_id,
            include_draft,
            include_stale,
            json,
        } => {
            let node = store.show_node(&work_id)?;
            let work = node
                .documentation
                .as_ref()
                .map(|documentation| {
                    crate::docs::WorkDocumentationContext::from((
                        node.id.as_str(),
                        node.revision,
                        documentation,
                    ))
                })
                .unwrap_or_else(|| {
                    crate::docs::WorkDocumentationContext::unknown(node.id.clone(), node.revision)
                });
            let registry = crate::docs::load_registry(store.repo_root())?;
            let resolver = crate::docs::FsContentResolver::new(store.repo_root());
            let out = crate::docs::applicable_docs(
                &work,
                &registry,
                &resolver,
                crate::docs::ApplicabilityOptions {
                    include_draft,
                    include_stale,
                },
            )?;
            render(
                json,
                &out,
                format!(
                    "{} required, {} optional, gate {}",
                    out.required.len(),
                    out.optional.len(),
                    out.gate.status
                ),
            )
        }
        DocsCommand::Impact {
            ticket_id,
            expected_revision,
            posture,
            rationale,
            required_doc,
            deferred_to,
            path,
            domain,
            label,
            actor,
            json,
        } => {
            let out = store.update_documentation_impact(
                &ticket_id,
                expected_revision,
                DocumentationImpactUpdate {
                    posture: posture.into(),
                    rationale,
                    required_documents: required_doc,
                    deferred_to,
                    paths: path,
                    domains: domain,
                    labels: label,
                },
                actor,
            )?;
            render(
                json,
                &out,
                format!("updated documentation impact {}", out.value.id),
            )
        }
    }
}

/// Human summary for `docs index` and `docs status`.
///
/// Both report two independent states: the disposable docs-search cache and
/// the tracked `_index.md` projections. Naming only the cache made a build that
/// had just written a projection print `docs index current`, which describes
/// neither what changed nor what a following `docs validate` would say.
fn index_summary(report: &crate::docs::IndexBuildReport) -> String {
    format!(
        "docs index {}, projections {}",
        report.index.state, report.projections.state
    )
}
