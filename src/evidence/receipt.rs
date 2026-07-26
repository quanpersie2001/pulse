use crate::canonical_json::{hash_bytes, to_canonical_bytes};
use crate::event::{event_path, EventEnvelope};
use crate::evidence::artifact::artifact_exists;
use crate::evidence::manifest;
use crate::evidence::model::*;
use crate::graph::node::Node;
use crate::id::new_event_id;
use crate::storage::transaction::{
    commit_prepared_transaction, prepare_transaction, FileState, TransactionFailpoint,
    TransactionIntent,
};
use crate::storage::WriteGuard;
use crate::{PulseError, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Created,
    Unchanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceiptOutcome {
    pub schema_version: u32,
    pub code: String,
    pub status: ReceiptStatus,
    pub receipt: ReceiptEnvelope,
    pub receipt_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceiptList {
    pub schema_version: u32,
    pub receipts: Vec<ReceiptSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceiptSummary {
    pub id: String,
    pub kind: ReceiptKind,
    pub subject: SubjectRef,
    pub result: ReceiptResult,
    pub recorded_at: chrono::DateTime<Utc>,
    pub receipt_hash: String,
}

pub fn record_receipt(
    repo_root: &Path,
    failpoint: Option<TransactionFailpoint>,
    file: &Path,
) -> Result<ReceiptOutcome> {
    let input_bytes = fs::read(file).map_err(|error| PulseError::io(file, error))?;
    if input_bytes.len() > 262_144 {
        return Err(PulseError::validation(
            "receipt_schema_invalid",
            "receipt too large",
        ));
    }
    let mut receipt: ReceiptEnvelope =
        serde_json::from_slice(&input_bytes).map_err(|error| PulseError::json(file, error))?;
    validate_receipt_id(&receipt.id)?;
    normalize_bindings(&mut receipt);
    let manifest = manifest::load(repo_root)?;
    if input_bytes.len() as u64 > manifest.max_inline_receipt_bytes {
        return Err(PulseError::validation(
            "receipt_schema_invalid",
            "receipt exceeds manifest max_inline_receipt_bytes",
        ));
    }
    validate_manifest_kind(&manifest, &receipt)?;
    validate_envelope(repo_root, &receipt, true)?;
    let canonical = to_canonical_bytes(&receipt)?;
    let receipt_hash = hash_bytes(&canonical);
    let receipt_path = receipt_path(repo_root, &receipt.id);

    let _guard = WriteGuard::acquire(repo_root)?;
    crate::storage::bootstrap(repo_root)?;
    crate::storage::transaction::recover_prepared_transactions(repo_root)?;
    manifest::bootstrap(repo_root)?;

    if receipt_path.exists() {
        let existing =
            fs::read(&receipt_path).map_err(|error| PulseError::io(&receipt_path, error))?;
        if existing == canonical {
            return Ok(ReceiptOutcome {
                schema_version: 1,
                code: "unchanged".to_string(),
                status: ReceiptStatus::Unchanged,
                receipt,
                receipt_hash,
            });
        }
        return Err(PulseError::validation(
            "receipt_id_conflict",
            "same receipt id has different bytes",
        ));
    }

    let event = EventEnvelope::new(
        new_event_id(),
        "evidence.receipt.recorded",
        receipt.actor.id.clone(),
        receipt.id.clone(),
        json!({
            "receipt_id": receipt.id,
            "receipt_kind": receipt.kind,
            "receipt_hash": receipt_hash,
            "subject": receipt.subject.id,
            "result": receipt.result,
        }),
        Utc::now(),
    );
    let intent = TransactionIntent::prepared(
        event.id.clone(),
        event.event_type.clone(),
        event.actor.clone(),
        receipt_path.clone(),
        event_path(repo_root, &event),
        FileState::Absent,
        FileState::Present {
            hash: receipt_hash.clone(),
            revision: 1,
        },
        serde_json::to_value(&event)?,
    )?;
    let prepared = prepare_transaction(repo_root, intent)?;
    commit_prepared_transaction(&prepared, &canonical, failpoint)?;
    Ok(ReceiptOutcome {
        schema_version: 1,
        code: "receipt_recorded".to_string(),
        status: ReceiptStatus::Created,
        receipt,
        receipt_hash,
    })
}

pub fn show_receipt(repo_root: &Path, id: &str) -> Result<ReceiptOutcome> {
    let (receipt, hash) = load_receipt(repo_root, id)?;
    Ok(ReceiptOutcome {
        schema_version: 1,
        code: "ok".to_string(),
        status: ReceiptStatus::Unchanged,
        receipt,
        receipt_hash: hash,
    })
}

pub fn list_receipts(
    repo_root: &Path,
    kind: Option<ReceiptKind>,
    subject: Option<String>,
    result: Option<ReceiptResult>,
) -> Result<ReceiptList> {
    let dir = repo_root.join(".pulse/evidence/receipts");
    let mut receipts = Vec::new();
    if dir.exists() {
        for entry in fs::read_dir(&dir).map_err(|error| PulseError::io(&dir, error))? {
            let path = entry.map_err(|error| PulseError::io(&dir, error))?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
            let receipt: ReceiptEnvelope =
                serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))?;
            if kind.as_ref().is_some_and(|k| k != &receipt.kind) {
                continue;
            }
            if subject.as_ref().is_some_and(|s| s != &receipt.subject.id) {
                continue;
            }
            if result.as_ref().is_some_and(|r| r != &receipt.result) {
                continue;
            }
            receipts.push(ReceiptSummary {
                id: receipt.id,
                kind: receipt.kind,
                subject: receipt.subject,
                result: receipt.result,
                recorded_at: receipt.recorded_at,
                receipt_hash: hash_bytes(&bytes),
            });
        }
    }
    receipts.sort_by(|a, b| a.recorded_at.cmp(&b.recorded_at).then(a.id.cmp(&b.id)));
    Ok(ReceiptList {
        schema_version: 1,
        receipts,
    })
}

pub fn verify_receipt(
    repo_root: &Path,
    id: &str,
    current: bool,
    source: Option<&str>,
) -> Result<ValidationReport> {
    let (receipt, hash) = load_receipt(repo_root, id)?;
    let mut integrity = Vec::new();
    if let Err(err) = validate_envelope(repo_root, &receipt, false) {
        integrity.push(err.code().to_string());
    }
    if !has_recording_event(repo_root, id, &hash)? {
        integrity.push("receipt_recording_event_missing".to_string());
    }
    let binding_codes = if current {
        binding_staleness(repo_root, &receipt, source)?
    } else {
        Vec::new()
    };
    let bindings = ValidationDimension {
        status: if !current {
            "not_checked"
        } else if binding_codes.is_empty() {
            "current"
        } else {
            "stale"
        }
        .to_string(),
        reason_codes: binding_codes,
    };
    let (registry, policy, authorization, gate_eligible) = docs_validation_dimensions(
        repo_root,
        &receipt,
        current,
        integrity.is_empty(),
        bindings.status == "current",
    )?;
    Ok(ValidationReport {
        schema_version: 1,
        receipt_id: id.to_string(),
        receipt_hash: hash,
        integrity: ValidationDimension {
            status: if integrity.is_empty() {
                "valid"
            } else {
                "invalid"
            }
            .to_string(),
            reason_codes: integrity,
        },
        bindings,
        registry,
        policy,
        authorization,
        gate_eligible,
    })
}

pub fn load_receipt(repo_root: &Path, id: &str) -> Result<(ReceiptEnvelope, String)> {
    validate_receipt_id(id)?;
    let path = receipt_path(repo_root, id);
    if !path.exists() {
        return Err(PulseError::validation("receipt_not_found", id));
    }
    let bytes = fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
    let receipt: ReceiptEnvelope =
        serde_json::from_slice(&bytes).map_err(|error| PulseError::json(&path, error))?;
    Ok((receipt, hash_bytes(&bytes)))
}

pub fn validate_for_supersession(
    repo_root: &Path,
    id: &str,
    old_id: &str,
    old_rev: u64,
    target_id: &str,
    target_rev: u64,
) -> Result<ReceiptReference> {
    let (receipt, hash) = load_receipt(repo_root, id)?;
    validate_envelope(repo_root, &receipt, false)?;
    if receipt.kind != ReceiptKind::SupersessionReconciliation
        || receipt.result != ReceiptResult::Passed
    {
        return Err(PulseError::validation(
            "receipt_result_ineligible",
            "supersession requires passed reconciliation receipt",
        ));
    }
    let ReceiptPayload::SupersessionReconciliation(payload) = &receipt.payload else {
        return Err(PulseError::validation(
            "receipt_kind_unsupported",
            "payload mismatch",
        ));
    };
    if payload.old.id != old_id || payload.old.revision != old_rev {
        return Err(PulseError::validation(
            "supersession_receipt_mismatch",
            "old work mismatch",
        ));
    }
    let (rid, rrev) = match &payload.target {
        SupersessionReceiptTarget::Replacement { id, revision }
        | SupersessionReceiptTarget::DecisionExplanation { id, revision } => (id, *revision),
    };
    if rid != target_id || rrev != target_rev {
        return Err(PulseError::validation(
            "supersession_receipt_mismatch",
            "target mismatch",
        ));
    }
    let report = verify_receipt(repo_root, id, true, None)?;
    if report.integrity.status != "valid" || report.bindings.status != "current" {
        return Err(PulseError::validation(
            "supersession_receipt_mismatch",
            "receipt is not current and valid",
        ));
    }
    Ok(ReceiptReference {
        id: id.to_string(),
        hash,
    })
}

fn validate_envelope(repo_root: &Path, receipt: &ReceiptEnvelope, record_time: bool) -> Result<()> {
    if receipt.schema_version != 1 || receipt.receipt_version != 1 {
        return Err(PulseError::validation(
            "receipt_version_unsupported",
            "unsupported receipt version",
        ));
    }
    validate_receipt_id(&receipt.id)?;
    validate_manifest_kind(&manifest::load(repo_root)?, receipt)?;
    if receipt.actor.id.trim().is_empty() || receipt.subject.id.trim().is_empty() {
        return Err(PulseError::validation(
            "receipt_schema_invalid",
            "actor and subject are required",
        ));
    }
    if receipt.kind.as_str() != payload_kind(&receipt.payload) {
        return Err(PulseError::validation(
            "receipt_kind_unsupported",
            "kind/payload mismatch",
        ));
    }
    validate_bindings(repo_root, &receipt.bindings, record_time)?;
    match &receipt.payload {
        ReceiptPayload::SupersessionReconciliation(p) => validate_supersession_payload(receipt, p),
        ReceiptPayload::ShapingValidation(p) => validate_shaping_payload(receipt, p),
        ReceiptPayload::DocumentationValidation(p) => validate_docs_payload(receipt, p),
    }
}

fn validate_bindings(
    repo_root: &Path,
    bindings: &ReceiptBindings,
    record_time: bool,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    for work in &bindings.work {
        if !seen.insert(&work.id) {
            return Err(PulseError::validation(
                "work_binding_missing",
                "duplicate work binding",
            ));
        }
    }
    for artifact in &bindings.artifacts {
        if !artifact_exists(repo_root, &artifact.sha256) {
            return Err(PulseError::validation(
                "artifact_not_found",
                artifact.sha256.clone(),
            ));
        }
    }
    if record_time {
        let codes = binding_codes_for(repo_root, bindings, None)?;
        if let Some(code) = codes.into_iter().next() {
            return Err(PulseError::validation(
                code_to_static(&code),
                "receipt bindings are not current",
            ));
        }
    }
    Ok(())
}

fn validate_supersession_payload(
    receipt: &ReceiptEnvelope,
    p: &SupersessionReconciliationPayload,
) -> Result<()> {
    if p.payload_version != 1 || receipt.result != ReceiptResult::Passed {
        return Err(PulseError::validation(
            "receipt_result_ineligible",
            "supersession receipt must be passed v1",
        ));
    }
    if receipt.bindings.source.is_none() {
        return Err(PulseError::validation(
            "source_binding_missing",
            "source binding required",
        ));
    }
    if receipt.bindings.content.is_empty() {
        return Err(PulseError::validation(
            "content_binding_missing",
            "content binding required",
        ));
    }
    require_work_binding(&receipt.bindings, &p.old)?;
    let target_ref = match &p.target {
        SupersessionReceiptTarget::Replacement { id, revision }
        | SupersessionReceiptTarget::DecisionExplanation { id, revision } => WorkRevisionRef {
            id: id.clone(),
            revision: *revision,
        },
    };
    require_work_binding(&receipt.bindings, &target_ref)?;
    if p.claim == SupersessionReceiptClaim::FollowUpRequired && p.follow_up_work.is_empty() {
        return Err(PulseError::validation(
            "work_binding_missing",
            "follow_up_required needs follow up work",
        ));
    }
    for work in &p.follow_up_work {
        require_work_binding(&receipt.bindings, work)?;
    }
    Ok(())
}

fn validate_shaping_payload(receipt: &ReceiptEnvelope, p: &ShapingValidationPayload) -> Result<()> {
    if p.payload_version != 1 {
        return Err(PulseError::validation(
            "receipt_version_unsupported",
            "payload",
        ));
    }
    require_work_binding(&receipt.bindings, &p.owning_work)
}

fn validate_docs_payload(
    receipt: &ReceiptEnvelope,
    p: &DocumentationValidationPayload,
) -> Result<()> {
    if p.payload_version != 1 {
        return Err(PulseError::validation(
            "receipt_version_unsupported",
            "payload",
        ));
    }
    if receipt.bindings.source.is_none() || receipt.bindings.content.is_empty() {
        return Err(PulseError::validation(
            "source_binding_missing",
            "documentation requires source/content binding",
        ));
    }
    for doc in &p.documents {
        validate_registry_document_identity(doc)?;
        validate_document_entry_common(receipt, &doc.path, &doc.content_hash)?;
    }
    for check in &p.checks {
        if check.kind.trim().is_empty() {
            return Err(PulseError::validation(
                "receipt_schema_invalid",
                "check kind is required",
            ));
        }
        if let Some(d) = &check.artifact {
            if !receipt.bindings.artifacts.iter().any(|a| &a.sha256 == d) {
                return Err(PulseError::validation("artifact_not_found", d.clone()));
            }
        }
    }
    Ok(())
}

fn validate_registry_document_identity(doc: &DocumentationValidationDocument) -> Result<()> {
    let document_id = doc.document_id.as_deref().ok_or_else(|| {
        PulseError::validation(
            "document_receipt_registry_mismatch",
            "document_id is required for documentation payload",
        )
    })?;
    validate_document_id(document_id)?;
    let Some(document_revision) = doc.document_revision else {
        return Err(PulseError::validation(
            "document_receipt_registry_mismatch",
            "document_revision is required for documentation payload",
        ));
    };
    if document_revision == 0 {
        return Err(PulseError::validation(
            "document_receipt_registry_mismatch",
            "document revision must be positive",
        ));
    }
    Ok(())
}

fn validate_document_entry_common(
    receipt: &ReceiptEnvelope,
    path: &str,
    content_hash: &str,
) -> Result<()> {
    crate::storage::safe_repo_relative(path)?;
    if !content_hash.starts_with("sha256:") || content_hash.len() != 71 {
        return Err(PulseError::validation(
            "receipt_schema_invalid",
            "content hash must be sha256:<hex>",
        ));
    }
    if !receipt
        .bindings
        .content
        .iter()
        .any(|c| c.path == path && c.sha256 == content_hash)
    {
        return Err(PulseError::validation(
            "content_binding_missing",
            path.to_string(),
        ));
    }
    Ok(())
}

fn docs_validation_dimensions(
    repo_root: &Path,
    receipt: &ReceiptEnvelope,
    current: bool,
    integrity_valid: bool,
    bindings_current: bool,
) -> Result<(
    ValidationDimension,
    ValidationDimension,
    ValidationDimension,
    bool,
)> {
    let ReceiptPayload::DocumentationValidation(payload) = &receipt.payload else {
        return Ok((
            ValidationDimension {
                status: "not_applicable".to_string(),
                reason_codes: Vec::new(),
            },
            ValidationDimension {
                status: "not_applicable".to_string(),
                reason_codes: Vec::new(),
            },
            ValidationDimension {
                status: "not_evaluated".to_string(),
                reason_codes: vec!["authority_resolver_unavailable".to_string()],
            },
            false,
        ));
    };

    if payload.payload_version != 1 {
        return Ok((
            ValidationDimension {
                status: "invalid".to_string(),
                reason_codes: vec!["receipt_version_unsupported".to_string()],
            },
            ValidationDimension {
                status: "not_evaluated".to_string(),
                reason_codes: Vec::new(),
            },
            ValidationDimension {
                status: "not_evaluated".to_string(),
                reason_codes: vec!["authority_resolver_unavailable".to_string()],
            },
            false,
        ));
    }

    let registry = load_docs_registry(repo_root)?;
    let mut registry_codes = Vec::new();
    let mut policies = BTreeSet::new();
    for doc in &payload.documents {
        validate_doc_against_registry(
            repo_root,
            &registry,
            doc,
            &mut registry_codes,
            &mut policies,
        )?;
    }
    registry_codes.sort();
    registry_codes.dedup();

    let mut policy_codes = Vec::new();
    // Authorization is deliberately mechanical/unresolved in the evidence
    // foundation. Registry/policy checks below can prove only structure; they
    // must never promote an actor or review policy into an authorized gate pass.
    let mut authorization_codes = vec!["authority_resolver_unavailable".to_string()];
    let mut authorization_status = "not_evaluated";
    for policy in &policies {
        match policy.as_str() {
            "none" => {}
            "light" => require_passed_checks(payload, &["content_review"], &mut policy_codes),
            "standard" => require_passed_checks(
                payload,
                &["link_check", "semantic_review"],
                &mut policy_codes,
            ),
            "independent" => {
                require_passed_checks(
                    payload,
                    &["link_check", "semantic_review"],
                    &mut policy_codes,
                );
                authorization_status = "unresolved";
                authorization_codes.push("independent_authorization_unresolved".to_string());
            }
            "human" => {
                policy_codes.push("human_approval_unresolved".to_string());
                authorization_status = "unresolved";
                authorization_codes.push("human_approval_unresolved".to_string());
            }
            _ => policy_codes.push("document_receipt_policy_incomplete".to_string()),
        }
    }
    policy_codes.sort();
    policy_codes.dedup();
    authorization_codes.sort();
    authorization_codes.dedup();

    let registry_dimension = ValidationDimension {
        status: if !current {
            "not_checked"
        } else if registry_codes.is_empty() {
            "current"
        } else if registry_codes.iter().any(|code| {
            matches!(
                code.as_str(),
                "document_retired" | "document_superseded" | "document_stale"
            )
        }) {
            "not_current"
        } else {
            "mismatch"
        }
        .to_string(),
        reason_codes: if current { registry_codes } else { Vec::new() },
    };
    let policy_dimension = ValidationDimension {
        status: if policy_codes.is_empty() {
            "structurally_satisfied"
        } else {
            "incomplete"
        }
        .to_string(),
        reason_codes: policy_codes,
    };
    let authorization_dimension = ValidationDimension {
        status: authorization_status.to_string(),
        reason_codes: authorization_codes,
    };
    let gate_eligible = integrity_valid
        && bindings_current
        && registry_dimension.status == "current"
        && policy_dimension.status == "structurally_satisfied"
        && authorization_dimension.status == "not_evaluated"
        && policies.iter().all(|policy| policy == "none");

    Ok((
        registry_dimension,
        policy_dimension,
        authorization_dimension,
        gate_eligible,
    ))
}

fn validate_doc_against_registry(
    repo_root: &Path,
    registry: &DocsRegistrySnapshot,
    doc: &DocumentationValidationDocument,
    registry_codes: &mut Vec<String>,
    policies: &mut BTreeSet<String>,
) -> Result<()> {
    let Some(document_id) = doc.document_id.as_deref() else {
        registry_codes.push("document_receipt_registry_mismatch".to_string());
        return Ok(());
    };
    let Some(record) = registry
        .documents
        .iter()
        .find(|candidate| candidate.id == document_id)
    else {
        registry_codes.push("document_receipt_registry_mismatch".to_string());
        if registry
            .documents
            .iter()
            .any(|candidate| candidate.path == doc.path)
        {
            registry_codes.push("document_receipt_wrong_id_for_path".to_string());
        }
        return Ok(());
    };
    policies.insert(record.review_policy.clone());
    if record.path != doc.path {
        registry_codes.push("document_receipt_registry_mismatch".to_string());
    }
    if doc.document_revision != Some(record.revision) {
        registry_codes.push("document_receipt_revision_stale".to_string());
    }
    validate_registry_record_state(repo_root, record, doc, registry_codes)
}

fn validate_registry_record_state(
    repo_root: &Path,
    record: &DocsRegistryDocument,
    doc: &DocumentationValidationDocument,
    registry_codes: &mut Vec<String>,
) -> Result<()> {
    match current_content_hash(repo_root, &record.path)? {
        Some(current_hash) if current_hash == doc.content_hash => {}
        Some(_) => registry_codes.push("document_receipt_registry_mismatch".to_string()),
        None => registry_codes.push("document_receipt_registry_mismatch".to_string()),
    }
    match record.lifecycle.as_str() {
        "current" => {}
        "retired" => registry_codes.push("document_retired".to_string()),
        "superseded" => registry_codes.push("document_superseded".to_string()),
        "stale" | "suspected_stale" => registry_codes.push("document_stale".to_string()),
        _ => registry_codes.push("document_receipt_registry_mismatch".to_string()),
    }
    Ok(())
}

fn binding_staleness(
    repo_root: &Path,
    receipt: &ReceiptEnvelope,
    source: Option<&str>,
) -> Result<Vec<String>> {
    binding_codes_for(repo_root, &receipt.bindings, source)
}

fn binding_codes_for(
    repo_root: &Path,
    bindings: &ReceiptBindings,
    source: Option<&str>,
) -> Result<Vec<String>> {
    let mut codes = Vec::new();
    let nodes = load_nodes(repo_root)?;
    for work in &bindings.work {
        match nodes.get(&work.id) {
            Some(n) if n.revision == work.revision => {}
            Some(_) => codes.push("work_binding_stale".to_string()),
            None => codes.push("work_binding_missing".to_string()),
        }
    }
    for content in &bindings.content {
        let rel = crate::storage::safe_repo_relative(&content.path)?;
        let path = repo_root.join(rel);
        match fs::read(&path) {
            Ok(bytes) if hash_bytes(&bytes) == content.sha256 => {}
            Ok(_) => codes.push("content_binding_stale".to_string()),
            Err(_) => codes.push("content_binding_missing".to_string()),
        }
    }
    if let Some(source_binding) = &bindings.source {
        let manifest = manifest::load(repo_root)?;
        if source_binding.repository_id != manifest.repository_id {
            codes.push("repository_identity_mismatch".to_string());
        }
        if source_binding.kind != "git_commit" {
            codes.push("source_binding_stale".to_string());
        } else if let Some(expected) = source {
            if expected != source_binding.commit {
                codes.push("source_binding_stale".to_string());
            }
        } else {
            let scoped_paths = bindings
                .content
                .iter()
                .map(|content| content.path.clone())
                .collect::<Vec<_>>();
            match crate::source::current_status(repo_root, &source_binding.commit, &scoped_paths) {
                crate::source::SourceBindingStatus::Current => {}
                crate::source::SourceBindingStatus::DirtyUnsupported => {
                    codes.push("dirty_source_unsupported".to_string())
                }
                crate::source::SourceBindingStatus::Unsupported => {
                    codes.push("source_binding_stale".to_string())
                }
                crate::source::SourceBindingStatus::Stale => {
                    codes.push("source_binding_stale".to_string())
                }
            }
        }
    }
    Ok(codes)
}

fn code_to_static(code: &str) -> &'static str {
    match code {
        "work_binding_missing" => "work_binding_missing",
        "work_binding_stale" => "work_binding_stale",
        "content_binding_missing" => "content_binding_missing",
        "content_binding_stale" => "content_binding_stale",
        "source_binding_missing" => "source_binding_missing",
        "source_binding_stale" => "source_binding_stale",
        "dirty_source_unsupported" => "dirty_source_unsupported",
        "repository_identity_mismatch" => "repository_identity_mismatch",
        "artifact_not_found" => "artifact_not_found",
        _ => "receipt_schema_invalid",
    }
}

#[derive(Debug, Clone)]
struct DocsRegistrySnapshot {
    documents: Vec<DocsRegistryDocument>,
}

#[derive(Debug, Clone)]
struct DocsRegistryDocument {
    id: String,
    revision: u64,
    path: String,
    lifecycle: String,
    review_policy: String,
}

fn load_docs_registry(repo_root: &Path) -> Result<DocsRegistrySnapshot> {
    let registry = crate::docs::load_registry_or_empty(repo_root)?;
    Ok(DocsRegistrySnapshot {
        documents: registry
            .documents
            .into_iter()
            .map(|document| DocsRegistryDocument {
                id: document.id,
                revision: document.revision,
                path: document.path,
                lifecycle: lifecycle_name(document.lifecycle).to_string(),
                review_policy: review_policy_name(document.review_policy).to_string(),
            })
            .collect(),
    })
}

fn lifecycle_name(lifecycle: crate::docs::DocumentLifecycle) -> &'static str {
    match lifecycle {
        crate::docs::DocumentLifecycle::Current => "current",
        crate::docs::DocumentLifecycle::SuspectedStale => "suspected_stale",
        crate::docs::DocumentLifecycle::Stale => "stale",
        crate::docs::DocumentLifecycle::Retired => "retired",
        crate::docs::DocumentLifecycle::Superseded => "superseded",
    }
}

fn review_policy_name(policy: crate::docs::ReviewPolicy) -> &'static str {
    match policy {
        crate::docs::ReviewPolicy::None => "none",
        crate::docs::ReviewPolicy::Light => "light",
        crate::docs::ReviewPolicy::Standard => "standard",
        crate::docs::ReviewPolicy::Independent => "independent",
        crate::docs::ReviewPolicy::Human => "human",
    }
}

fn current_content_hash(repo_root: &Path, path: &str) -> Result<Option<String>> {
    let rel = crate::storage::safe_repo_relative(path)?;
    let path = repo_root.join(rel);
    match fs::read(&path) {
        Ok(bytes) => Ok(Some(hash_bytes(&bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(PulseError::io(path, error)),
    }
}

fn require_passed_checks(
    payload: &DocumentationValidationPayload,
    required: &[&str],
    policy_codes: &mut Vec<String>,
) {
    for required_kind in required {
        if !payload
            .checks
            .iter()
            .any(|check| check.kind == *required_kind && check.result == ReceiptResult::Passed)
        {
            policy_codes.push("document_receipt_policy_incomplete".to_string());
        }
    }
}

fn validate_document_id(id: &str) -> Result<()> {
    let Some(rest) = id.strip_prefix("DOC-") else {
        return Err(PulseError::validation(
            "document_receipt_registry_mismatch",
            "document id must start with DOC-",
        ));
    };
    if (3..=64).contains(&rest.len())
        && rest
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
    {
        Ok(())
    } else {
        Err(PulseError::validation(
            "document_receipt_registry_mismatch",
            "invalid document id",
        ))
    }
}

fn load_nodes(repo_root: &Path) -> Result<BTreeMap<String, Node>> {
    let dir = repo_root.join(".pulse/workgraph/nodes");
    let mut nodes = BTreeMap::new();
    if dir.exists() {
        for entry in fs::read_dir(&dir).map_err(|error| PulseError::io(&dir, error))? {
            let path = entry.map_err(|error| PulseError::io(&dir, error))?.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let n: Node = crate::storage::read_json(&path)?;
                nodes.insert(n.id.clone(), n);
            }
        }
    }
    Ok(nodes)
}

fn require_work_binding(bindings: &ReceiptBindings, needle: &WorkRevisionRef) -> Result<()> {
    if bindings
        .work
        .iter()
        .any(|w| w.id == needle.id && w.revision == needle.revision)
    {
        Ok(())
    } else {
        Err(PulseError::validation(
            "work_binding_missing",
            needle.id.clone(),
        ))
    }
}
fn payload_kind(payload: &ReceiptPayload) -> &'static str {
    match payload {
        ReceiptPayload::SupersessionReconciliation(_) => "supersession_reconciliation",
        ReceiptPayload::ShapingValidation(_) => "shaping_validation",
        ReceiptPayload::DocumentationValidation(_) => "documentation_validation",
    }
}
fn normalize_bindings(receipt: &mut ReceiptEnvelope) {
    receipt.bindings.work.sort_by(|a, b| a.id.cmp(&b.id));
    receipt.bindings.content.sort_by(|a, b| a.path.cmp(&b.path));
    receipt
        .bindings
        .artifacts
        .sort_by(|a, b| a.sha256.cmp(&b.sha256));
}
fn validate_manifest_kind(
    manifest: &manifest::EvidenceManifest,
    receipt: &ReceiptEnvelope,
) -> Result<()> {
    if !manifest
        .receipt_schemas
        .contains_key(&receipt.receipt_version.to_string())
    {
        return Err(PulseError::validation(
            "receipt_version_unsupported",
            "unknown receipt envelope version",
        ));
    }
    let versions = manifest
        .receipt_kinds
        .get(receipt.kind.as_str())
        .ok_or_else(|| {
            PulseError::validation("receipt_kind_unsupported", "unknown receipt kind")
        })?;
    let payload_version = match &receipt.payload {
        ReceiptPayload::SupersessionReconciliation(payload) => payload.payload_version,
        ReceiptPayload::ShapingValidation(payload) => payload.payload_version,
        ReceiptPayload::DocumentationValidation(payload) => payload.payload_version,
    };
    if !versions.contains_key(&payload_version.to_string()) {
        return Err(PulseError::validation(
            "receipt_version_unsupported",
            "unknown receipt payload version",
        ));
    }
    Ok(())
}

fn validate_receipt_id(id: &str) -> Result<()> {
    let suffix = id.strip_prefix("rcpt_");
    if suffix.is_some_and(|suffix| {
        suffix.len() == 26
            && suffix
                .chars()
                .all(|c| matches!(c, '0'..='9' | 'A'..='H' | 'J'..='K' | 'M'..='N' | 'P'..='T' | 'V'..='Z'))
    }) {
        Ok(())
    } else {
        Err(PulseError::validation(
            "receipt_schema_invalid",
            "invalid receipt id",
        ))
    }
}
fn receipt_path(repo_root: &Path, id: &str) -> PathBuf {
    repo_root
        .join(".pulse/evidence/receipts")
        .join(format!("{id}.json"))
}

fn has_recording_event(repo_root: &Path, id: &str, hash: &str) -> Result<bool> {
    let dir = repo_root.join(".pulse/events");
    if !dir.exists() {
        return Ok(false);
    }
    let mut count = 0;
    for day in fs::read_dir(&dir).map_err(|error| PulseError::io(&dir, error))? {
        let day = day.map_err(|error| PulseError::io(&dir, error))?.path();
        if !day.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&day).map_err(|error| PulseError::io(&day, error))? {
            let path = entry.map_err(|error| PulseError::io(&day, error))?.path();
            let Ok(event): std::result::Result<crate::event::EventEnvelope, _> =
                crate::storage::read_json(&path)
            else {
                continue;
            };
            if event.event_type == "evidence.receipt.recorded"
                && event.payload.get("receipt_id").and_then(|v| v.as_str()) == Some(id)
                && event.payload.get("receipt_hash").and_then(|v| v.as_str()) == Some(hash)
            {
                count += 1;
            }
        }
    }
    if count > 1 {
        return Err(PulseError::validation(
            "receipt_recording_event_ambiguous",
            id,
        ));
    }
    Ok(count == 1)
}
