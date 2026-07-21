use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use pulse::docs::model::{
    DocumentAuthority, DocumentKind, DocumentLifecycle, DocumentPatch, DocumentRecord,
};
use pulse::evidence::model::{ReceiptKind, ReceiptResult};
use pulse::graph::edge::EdgeType;
use pulse::graph::lifecycle::TransitionReason;
use pulse::graph::node::{DocumentationImpactPosture, NodeStatus};
use pulse::graph::store::{DocumentationImpactUpdate, SupersessionAssertion, SupersessionTarget};
use pulse::id::WorkKind;
#[cfg(debug_assertions)]
use pulse::storage::transaction::TransactionFailpoint;
use pulse::{JsonGraphStore, PulseError};
use serde::Serialize;
use serde_json::json;

#[derive(Parser)]
#[command(name = "pulse")]
struct Cli {
    #[arg(long, global = true)]
    repo_root: Option<PathBuf>,
    #[cfg(debug_assertions)]
    #[arg(long, global = true, hide = true, value_enum)]
    test_failpoint: Option<FailpointArg>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Work {
        #[command(subcommand)]
        command: WorkCommand,
    },
    Docs {
        #[command(subcommand)]
        command: DocsCommand,
    },
    Graph {
        #[command(subcommand)]
        command: GraphCommand,
    },
    Evidence {
        #[command(subcommand)]
        command: EvidenceCommand,
    },
}

#[derive(Subcommand)]
enum DocsCommand {
    Register {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        expected_registry_revision: u64,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        json: bool,
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
        authority: Option<DocAuthorityArg>,
        #[arg(long)]
        lifecycle: Option<DocLifecycleArg>,
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
enum DocumentationPostureArg {
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
enum DocKindArg {
    RepositoryMap,
    Policy,
    Product,
    Architecture,
    Domain,
    Operations,
    Reference,
    DecisionProjection,
    Generated,
    Informational,
}

impl From<DocKindArg> for DocumentKind {
    fn from(value: DocKindArg) -> Self {
        match value {
            DocKindArg::RepositoryMap => DocumentKind::RepositoryMap,
            DocKindArg::Policy => DocumentKind::Policy,
            DocKindArg::Product => DocumentKind::Product,
            DocKindArg::Architecture => DocumentKind::Architecture,
            DocKindArg::Domain => DocumentKind::Domain,
            DocKindArg::Operations => DocumentKind::Operations,
            DocKindArg::Reference => DocumentKind::Reference,
            DocKindArg::DecisionProjection => DocumentKind::DecisionProjection,
            DocKindArg::Generated => DocumentKind::Generated,
            DocKindArg::Informational => DocumentKind::Informational,
        }
    }
}

#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
enum DocAuthorityArg {
    Draft,
    Approved,
    Informational,
    Generated,
}

impl From<DocAuthorityArg> for DocumentAuthority {
    fn from(value: DocAuthorityArg) -> Self {
        match value {
            DocAuthorityArg::Draft => DocumentAuthority::Draft,
            DocAuthorityArg::Approved => DocumentAuthority::Approved,
            DocAuthorityArg::Informational => DocumentAuthority::Informational,
            DocAuthorityArg::Generated => DocumentAuthority::Generated,
        }
    }
}

#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
enum DocLifecycleArg {
    Current,
    SuspectedStale,
    Stale,
    Retired,
    Superseded,
}

impl From<DocLifecycleArg> for DocumentLifecycle {
    fn from(value: DocLifecycleArg) -> Self {
        match value {
            DocLifecycleArg::Current => DocumentLifecycle::Current,
            DocLifecycleArg::SuspectedStale => DocumentLifecycle::SuspectedStale,
            DocLifecycleArg::Stale => DocumentLifecycle::Stale,
            DocLifecycleArg::Retired => DocumentLifecycle::Retired,
            DocLifecycleArg::Superseded => DocumentLifecycle::Superseded,
        }
    }
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
    Supersede {
        old_id: String,
        #[arg(long = "by", conflicts_with = "decision")]
        by: Option<String>,
        #[arg(long, conflicts_with = "by")]
        decision: Option<String>,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        assertion: Option<PathBuf>,
        #[arg(long)]
        reconciliation_receipt: Option<String>,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        json: bool,
    },
    Transition {
        id: String,
        #[arg(long = "to")]
        to: StatusArg,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        reason_code: Option<String>,
        #[arg(long = "reason")]
        reason: Option<String>,
        #[arg(long)]
        reference: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Executability {
        id: String,
        #[arg(long)]
        json: bool,
    },
    Rollup {
        id: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum EvidenceCommand {
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
enum ArtifactCommand {
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
enum ReceiptCommand {
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

#[derive(Subcommand)]
enum GraphCommand {
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

#[cfg(debug_assertions)]
#[allow(clippy::enum_variant_names)]
#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
enum FailpointArg {
    AfterIntent,
    AfterCanonical,
    AfterMultiTargetFirst,
    AfterMultiTargetAll,
    AfterEvent,
}

#[cfg(debug_assertions)]
impl From<FailpointArg> for TransactionFailpoint {
    fn from(value: FailpointArg) -> Self {
        match value {
            FailpointArg::AfterIntent => TransactionFailpoint::AfterIntent,
            FailpointArg::AfterCanonical => TransactionFailpoint::AfterCanonical,
            FailpointArg::AfterMultiTargetFirst => TransactionFailpoint::AfterMultiTargetFirst,
            FailpointArg::AfterMultiTargetAll => TransactionFailpoint::AfterMultiTargetAll,
            FailpointArg::AfterEvent => TransactionFailpoint::AfterEvent,
        }
    }
}

#[derive(Clone, ValueEnum)]
enum KindArg {
    Epic,
    Story,
    Ticket,
    Decision,
}

#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
enum StatusArg {
    Draft,
    Shaped,
    Ready,
    Active,
    Verifying,
    Done,
    Rework,
    Blocked,
    Cancelled,
    Superseded,
}

impl From<StatusArg> for NodeStatus {
    fn from(value: StatusArg) -> Self {
        match value {
            StatusArg::Draft => NodeStatus::Draft,
            StatusArg::Shaped => NodeStatus::Shaped,
            StatusArg::Ready => NodeStatus::Ready,
            StatusArg::Active => NodeStatus::Active,
            StatusArg::Verifying => NodeStatus::Verifying,
            StatusArg::Done => NodeStatus::Done,
            StatusArg::Rework => NodeStatus::Rework,
            StatusArg::Blocked => NodeStatus::Blocked,
            StatusArg::Cancelled => NodeStatus::Cancelled,
            StatusArg::Superseded => NodeStatus::Superseded,
        }
    }
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

#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
enum ReceiptKindArg {
    SupersessionReconciliation,
    ShapingValidation,
    DocumentationValidation,
}

impl From<ReceiptKindArg> for ReceiptKind {
    fn from(value: ReceiptKindArg) -> Self {
        match value {
            ReceiptKindArg::SupersessionReconciliation => ReceiptKind::SupersessionReconciliation,
            ReceiptKindArg::ShapingValidation => ReceiptKind::ShapingValidation,
            ReceiptKindArg::DocumentationValidation => ReceiptKind::DocumentationValidation,
        }
    }
}

#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
enum ReceiptResultArg {
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

fn main() {
    let cli = Cli::parse();
    let repo_root = cli
        .repo_root
        .unwrap_or_else(|| std::env::current_dir().expect("current dir"));
    #[cfg(debug_assertions)]
    let store = match cli.test_failpoint {
        Some(failpoint) => JsonGraphStore::with_failpoint(repo_root, failpoint.into()),
        None => JsonGraphStore::new(repo_root),
    };
    #[cfg(not(debug_assertions))]
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
                render(
                    json,
                    &json!({"schema_version": 1, "code": "ok", "node": node}),
                    human,
                )
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
            WorkCommand::Supersede {
                old_id,
                by,
                decision,
                expected_revision,
                reason,
                assertion,
                reconciliation_receipt,
                actor,
                json,
            } => {
                let target = match (by, decision) {
                    (Some(id), None) => SupersessionTarget::Replacement { id },
                    (None, Some(id)) => SupersessionTarget::Decision { id },
                    _ => {
                        return Err(PulseError::validation(
                            "invalid_supersession_target_form",
                            "choose exactly one of --by or --decision",
                        ));
                    }
                };
                let out = match (reconciliation_receipt, assertion) {
                    (Some(receipt_id), None) => store.supersede_work_with_receipt(
                        &old_id,
                        target,
                        expected_revision,
                        reason,
                        receipt_id,
                        actor,
                    )?,
                    (None, Some(assertion)) => {
                        let assertion_bytes = std::fs::read(&assertion)
                            .map_err(|error| PulseError::io(assertion.clone(), error))?;
                        let assertion: SupersessionAssertion =
                            serde_json::from_slice(&assertion_bytes)
                                .map_err(|error| PulseError::json(assertion.clone(), error))?;
                        store.supersede_work(
                            &old_id,
                            target,
                            expected_revision,
                            reason,
                            assertion,
                            actor,
                        )?
                    }
                    _ => {
                        return Err(PulseError::validation(
                            "supersession_receipt_mismatch",
                            "choose exactly one of --reconciliation-receipt or --assertion",
                        ))
                    }
                };
                render(json, &out, format!("{} {}", out.code, out.value.node.id))
            }
            WorkCommand::Transition {
                id,
                to,
                expected_revision,
                actor,
                reason_code,
                reason,
                reference,
                json,
            } => {
                let transition_reason = match (reason_code, reason, reference) {
                    (None, None, None) => None,
                    (Some(code), Some(summary), reference) => Some(TransitionReason {
                        code,
                        summary,
                        reference,
                    }),
                    _ => {
                        return Err(PulseError::validation(
                            "missing_status_reason",
                            "transition reason requires --reason-code and --reason together",
                        ));
                    }
                };
                let out = store.transition_node(
                    &id,
                    to.into(),
                    expected_revision,
                    transition_reason,
                    actor,
                )?;
                render(json, &out, format!("transitioned {}", out.value.id))
            }
            WorkCommand::Executability { id, json } => {
                let out = store.executability(&id)?;
                render(
                    json,
                    &out,
                    format!("{:?} {}", out.structural_state, out.subject),
                )
            }
            WorkCommand::Rollup { id, json } => {
                let out = store.rollup(&id)?;
                render(json, &out, format!("rollup {}", out.subject))
            }
        },
        Command::Docs { command } => match command {
            DocsCommand::Register {
                file,
                expected_registry_revision,
                actor,
                json,
            } => {
                let bytes =
                    std::fs::read(&file).map_err(|error| PulseError::io(file.clone(), error))?;
                let document: DocumentRecord = serde_json::from_slice(&bytes)
                    .map_err(|error| PulseError::json(file.clone(), error))?;
                let out = pulse::docs::register(
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
                let out = pulse::docs::edit(
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
                let out = pulse::docs::retire(
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
                let out = pulse::docs::supersede(
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
            DocsCommand::List {
                kind,
                authority,
                lifecycle,
                json,
            } => {
                let mut documents = pulse::docs::list(store.repo_root())?;
                if let Some(kind) = kind {
                    let kind: DocumentKind = kind.into();
                    documents.retain(|document| document.kind == kind);
                }
                if let Some(authority) = authority {
                    let authority: DocumentAuthority = authority.into();
                    documents.retain(|document| document.authority == authority);
                }
                if let Some(lifecycle) = lifecycle {
                    let lifecycle: DocumentLifecycle = lifecycle.into();
                    documents.retain(|document| document.lifecycle == lifecycle);
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
                let document = pulse::docs::show(store.repo_root(), &document_id)?;
                render(
                    json,
                    &json!({"schema_version": 1, "code": "ok", "document": document}),
                    document_id,
                )
            }
            DocsCommand::Validate { json } => {
                let registry = pulse::docs::registry::load_registry_unvalidated(store.repo_root())?;
                let report = pulse::docs::validate_registry(
                    store.repo_root(),
                    &registry.repository_id,
                    &registry,
                )?;
                let ok = report.valid;
                render(
                    json,
                    &report,
                    if ok { "valid" } else { "invalid" }.to_string(),
                )?;
                if ok {
                    Ok(())
                } else {
                    Err(PulseError::validation(
                        "invalid_docs_registry",
                        "docs registry is invalid",
                    ))
                }
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
                        pulse::docs::WorkDocumentationContext::from((
                            node.id.as_str(),
                            node.revision,
                            documentation,
                        ))
                    })
                    .unwrap_or_else(|| {
                        pulse::docs::WorkDocumentationContext::unknown(
                            node.id.clone(),
                            node.revision,
                        )
                    });
                let registry = pulse::docs::load_registry(store.repo_root())?;
                let resolver = pulse::docs::FsContentResolver::new(store.repo_root());
                let out = pulse::docs::applicable_docs(
                    &work,
                    &registry,
                    &resolver,
                    pulse::docs::ApplicabilityOptions {
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
        },
        Command::Evidence { command } => match command {
            EvidenceCommand::Bootstrap { json } => {
                let out = pulse::evidence::bootstrap(store.repo_root())?;
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
                    let manifest = pulse::evidence::manifest::load(store.repo_root())?;
                    let out = pulse::evidence::put_artifact(
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
                    let out = pulse::evidence::show_artifact(store.repo_root(), &digest)?;
                    render(json, &out, out.digest.clone())
                }
                ArtifactCommand::Verify { digest, json } => {
                    let out = pulse::evidence::verify_artifact(store.repo_root(), &digest)?;
                    render(json, &out, out.code.clone())
                }
            },
            EvidenceCommand::Receipt { command } => match command {
                ReceiptCommand::Record { file, json } => {
                    let out = pulse::evidence::record_receipt(
                        store.repo_root(),
                        store.failpoint(),
                        &file,
                    )?;
                    render(json, &out, format!("{} {}", out.code, out.receipt.id))
                }
                ReceiptCommand::Show { id, json } => {
                    let out = pulse::evidence::show_receipt(store.repo_root(), &id)?;
                    render(json, &out, out.receipt.id.clone())
                }
                ReceiptCommand::List {
                    kind,
                    subject,
                    result,
                    json,
                } => {
                    let out = pulse::evidence::list_receipts(
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
                    let out = pulse::evidence::verify_receipt(
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
