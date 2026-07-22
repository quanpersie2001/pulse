use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::canonical_json::{hash_bytes, SHA256_PREFIX};
use crate::id::WorkKind;
use crate::knowledge::manifest::{self, KnowledgeManifest};
use crate::knowledge::model::*;
use crate::knowledge::projection::knowledge_fingerprint;
use crate::knowledge::relation::*;
use crate::{PulseError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeValidationReport {
    pub schema_version: u32,
    pub code: String,
    pub valid: bool,
    pub knowledge_fingerprint: Option<String>,
    pub errors: Vec<KnowledgeFinding>,
    pub warnings: Vec<KnowledgeFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnowledgeFinding {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
}

impl KnowledgeValidationReport {
    pub fn ok(fingerprint: Option<String>) -> Self {
        Self {
            schema_version: 1,
            code: "ok".to_string(),
            valid: true,
            knowledge_fingerprint: fingerprint,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn push_error(
        &mut self,
        code: &str,
        subject: impl Into<Option<String>>,
        message: impl Into<String>,
    ) {
        self.valid = false;
        self.errors.push(KnowledgeFinding {
            code: code.to_string(),
            message: message.into(),
            subject: subject.into(),
        });
    }

    pub fn push_warning(
        &mut self,
        code: &str,
        subject: impl Into<Option<String>>,
        message: impl Into<String>,
    ) {
        self.warnings.push(KnowledgeFinding {
            code: code.to_string(),
            message: message.into(),
            subject: subject.into(),
        });
    }

    pub fn into_result(self) -> Result<Self> {
        if self.valid {
            Ok(self)
        } else {
            let first = self.errors.first().expect("invalid report has error");
            Err(PulseError::validation(
                Box::leak(first.code.clone().into_boxed_str()),
                first.message.clone(),
            ))
        }
    }
}

pub fn validate_knowledge(repo_root: &Path) -> Result<KnowledgeValidationReport> {
    let manifest = match manifest::load(repo_root) {
        Ok(m) => m,
        Err(err) => {
            let mut report = KnowledgeValidationReport::ok(None);
            report.push_error(err.code(), None, err.to_string());
            return Ok(report);
        }
    };
    let (entries, relations) = load_records(repo_root)?;
    validate_loaded(repo_root, &manifest, &entries, &relations)
}

pub(crate) fn validate_loaded(
    repo_root: &Path,
    manifest: &KnowledgeManifest,
    entries: &BTreeMap<String, Learning>,
    relations: &BTreeMap<String, KnowledgeRelation>,
) -> Result<KnowledgeValidationReport> {
    let fingerprint = knowledge_fingerprint(repo_root, manifest).ok();
    let mut report = KnowledgeValidationReport::ok(fingerprint);
    for (id, entry) in entries {
        if !valid_learning_id(id) || entry.id != *id {
            report.push_error(
                "learning_id_invalid",
                Some(id.clone()),
                "learning id or filename is invalid",
            );
        }
        validate_learning(repo_root, entry, relations, &mut report);
    }
    for (id, relation) in relations {
        if relation.id != *id {
            report.push_error(
                "knowledge_relation_id_invalid",
                Some(id.clone()),
                "relation filename does not match relation id",
            );
        }
        validate_relation(repo_root, relation, entries, &mut report);
    }
    validate_cross_relations(entries, relations, &mut report);
    Ok(report)
}

pub(crate) fn load_records(
    repo_root: &Path,
) -> Result<(
    BTreeMap<String, Learning>,
    BTreeMap<String, KnowledgeRelation>,
)> {
    let mut entries = BTreeMap::new();
    let entry_dir = repo_root.join(".pulse/knowledge/entries");
    if entry_dir.exists() {
        let mut files = fs::read_dir(&entry_dir)
            .map_err(|e| PulseError::io(&entry_dir, e))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| PulseError::io(&entry_dir, e))?;
        files.sort_by_key(|e| e.path());
        for file in files {
            let path = file.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let learning: Learning = crate::storage::read_json(&path)?;
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            entries.insert(stem, learning);
        }
    }
    let mut relations = BTreeMap::new();
    let relation_dir = repo_root.join(".pulse/knowledge/relations");
    if relation_dir.exists() {
        let mut files = fs::read_dir(&relation_dir)
            .map_err(|e| PulseError::io(&relation_dir, e))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| PulseError::io(&relation_dir, e))?;
        files.sort_by_key(|e| e.path());
        for file in files {
            let path = file.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let relation: KnowledgeRelation = crate::storage::read_json(&path)?;
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            relations.insert(stem, relation);
        }
    }
    Ok((entries, relations))
}

pub fn validate_learning_for_mutation(
    repo_root: &Path,
    learning: &Learning,
    relations: &BTreeMap<String, KnowledgeRelation>,
) -> Result<()> {
    let mut report = KnowledgeValidationReport::ok(None);
    validate_learning(repo_root, learning, relations, &mut report);
    validate_public_mutation_restrictions(learning, &mut report);
    report.into_result().map(|_| ())
}

pub(crate) fn validate_public_learning_claims(learning: &Learning) -> Result<()> {
    let mut report = KnowledgeValidationReport::ok(None);
    validate_public_mutation_restrictions(learning, &mut report);
    report.into_result().map(|_| ())
}

fn validate_public_mutation_restrictions(entry: &Learning, report: &mut KnowledgeValidationReport) {
    if entry.status != LearningStatus::Candidate {
        report.push_error(
            "learning_status_claim_unsupported",
            Some(entry.id.clone()),
            "public Slice 6 mutations may only create/edit candidate records",
        );
    }
    if entry.validation.confidence != Confidence::Low {
        report.push_error(
            "learning_confidence_claim_unsupported",
            Some(entry.id.clone()),
            "candidate confidence must remain low",
        );
    }
    if entry.routing.prompt_priority != PromptPriority::Suggested {
        report.push_error(
            "learning_routing_invalid",
            Some(entry.id.clone()),
            "candidate routing priority must be suggested",
        );
    }
    if entry.promotion.state == PromotionState::Promoted
        || entry.promotion.state == PromotionState::Deferred
    {
        report.push_error(
            "learning_promotion_invalid",
            Some(entry.id.clone()),
            "promoted/deferred lifecycle claims are deferred",
        );
    }
}

fn validate_learning(
    repo_root: &Path,
    entry: &Learning,
    relations: &BTreeMap<String, KnowledgeRelation>,
    report: &mut KnowledgeValidationReport,
) {
    if entry.schema_version != 1 || !valid_learning_id(&entry.id) {
        report.push_error(
            "learning_id_invalid",
            Some(entry.id.clone()),
            "invalid learning identity",
        );
    }
    if entry.title.trim().is_empty() || entry.title.chars().count() > 200 {
        report.push_error(
            "learning_schema_invalid",
            Some(entry.id.clone()),
            "title is required and bounded",
        );
    }
    if entry.summary.trim().is_empty() || entry.summary.chars().count() > 1000 {
        report.push_error(
            "learning_schema_invalid",
            Some(entry.id.clone()),
            "summary is required and bounded",
        );
    }
    validate_text(&entry.title, &entry.id, report);
    validate_text(&entry.summary, &entry.id, report);
    validate_guidance(entry, report);
    validate_applicability(&entry.id, &entry.applicability, report);
    validate_freshness(&entry.id, &entry.freshness, report);
    if entry.provenance.relation_ids.is_empty() && entry.provenance.source_commits.is_empty() {
        report.push_error(
            "learning_provenance_missing",
            Some(entry.id.clone()),
            "learning requires provenance relation or source commit",
        );
    }
    for commit in &entry.provenance.source_commits {
        if crate::source::resolve_full_commit(repo_root, commit).is_err() {
            report.push_error(
                "knowledge_relation_endpoint_missing",
                Some(entry.id.clone()),
                format!("source commit does not resolve: {commit}"),
            );
        }
    }
    if entry.kind == LearningKind::Ratchet && entry.guidance.required_checks.is_empty() {
        report.push_error(
            "learning_guidance_missing",
            Some(entry.id.clone()),
            "ratchet learning requires required_checks",
        );
    }
    if let Some(content) = &entry.content {
        validate_content(repo_root, &entry.id, content, report);
    }
    for relation_id in &entry.provenance.relation_ids {
        match relations.get(relation_id) {
            Some(relation)
                if relation.relation_type == RelationType::DerivedFrom
                    && relation.from.id == entry.id => {}
            _ => report.push_error(
                "learning_provenance_mismatch",
                Some(entry.id.clone()),
                format!("provenance relation missing or mismatched: {relation_id}"),
            ),
        }
    }
    if let Some(date) = entry.freshness.review_after {
        if date < Utc::now().date_naive() {
            report.push_warning(
                "learning_freshness_stale",
                Some(entry.id.clone()),
                "learning review_after date is in the past",
            );
        }
    }
    if entry.trust.contains_untrusted_text
        || entry.trust.redaction_status == RedactionStatus::ReviewRequired
    {
        report.push_warning(
            "learning_trust_unresolved",
            Some(entry.id.clone()),
            "learning trust posture requires review",
        );
    }
}

fn validate_guidance(entry: &Learning, report: &mut KnowledgeValidationReport) {
    if entry.guidance.total_items() == 0 {
        report.push_error(
            "learning_guidance_missing",
            Some(entry.id.clone()),
            "at least one guidance item is required",
        );
    }
    for value in entry
        .guidance
        .r#do
        .iter()
        .chain(&entry.guidance.avoid)
        .chain(&entry.guidance.required_checks)
    {
        if value.chars().count() > 500 || value.trim().is_empty() {
            report.push_error(
                "learning_schema_invalid",
                Some(entry.id.clone()),
                "guidance item is empty or too long",
            );
        }
        validate_text(value, &entry.id, report);
    }
    if entry.guidance.r#do.len() > 16
        || entry.guidance.avoid.len() > 16
        || entry.guidance.required_checks.len() > 16
    {
        report.push_error(
            "learning_schema_invalid",
            Some(entry.id.clone()),
            "too many guidance items",
        );
    }
}

fn validate_applicability(
    id: &str,
    applicability: &Applicability,
    report: &mut KnowledgeValidationReport,
) {
    if !applicability.has_positive_dimension() {
        report.push_error(
            "learning_applicability_missing",
            Some(id.to_string()),
            "at least one positive applicability dimension is required",
        );
    }
    if !applicability.has_concrete_dimension() {
        report.push_error(
            "learning_applicability_too_broad",
            Some(id.to_string()),
            "candidate applicability needs a concrete trigger",
        );
    }
    for path in &applicability.paths {
        if unsafe_repo_glob(path) {
            report.push_error(
                "learning_content_path_unsafe",
                Some(id.to_string()),
                format!("unsafe applicability path: {path}"),
            );
        }
    }
}

fn validate_freshness(id: &str, freshness: &Freshness, report: &mut KnowledgeValidationReport) {
    for path in &freshness.invalidated_by_paths {
        if unsafe_repo_glob(path) {
            report.push_error(
                "learning_content_path_unsafe",
                Some(id.to_string()),
                format!("unsafe freshness path: {path}"),
            );
        }
    }
}

fn validate_content(
    repo_root: &Path,
    id: &str,
    content: &ContentBinding,
    report: &mut KnowledgeValidationReport,
) {
    if !validate_sha256(&content.content_hash) {
        report.push_error(
            "learning_content_hash_invalid",
            Some(id.to_string()),
            "content hash must be sha256:<64 hex>",
        );
        return;
    }
    let Some(path) = resolve_learning_content_path(repo_root, &content.path) else {
        report.push_error(
            "learning_content_path_unsafe",
            Some(id.to_string()),
            "content path must be repo-relative, inside knowledge/learnings, and must not escape by symlink",
        );
        return;
    };
    match fs::read(&path) {
        Ok(bytes) if hash_bytes(&bytes) == content.content_hash => {}
        Ok(_) => report.push_error(
            "learning_content_hash_stale",
            Some(id.to_string()),
            "content hash does not match bound bytes",
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => report.push_error(
            "learning_content_missing",
            Some(id.to_string()),
            "content binding file is missing",
        ),
        Err(error) => report.push_error(
            "learning_content_missing",
            Some(id.to_string()),
            error.to_string(),
        ),
    }
}

fn validate_relation(
    repo_root: &Path,
    relation: &KnowledgeRelation,
    entries: &BTreeMap<String, Learning>,
    report: &mut KnowledgeValidationReport,
) {
    if relation.schema_version != 1 || relation.revision != 1 {
        report.push_error(
            "knowledge_relation_id_invalid",
            Some(relation.id.clone()),
            "invalid relation schema/revision",
        );
    }
    match deterministic_relation_id(
        relation.relation_type,
        &relation.from.id,
        relation.to.kind,
        &relation.to.id,
    ) {
        Ok(expected) if expected == relation.id => {}
        _ => report.push_error(
            "knowledge_relation_id_invalid",
            Some(relation.id.clone()),
            "relation id is not deterministic",
        ),
    }
    if relation.from.kind != EndpointKind::Learning || !entries.contains_key(&relation.from.id) {
        report.push_error(
            "knowledge_relation_endpoint_missing",
            Some(relation.id.clone()),
            "relation source learning is missing",
        );
    }
    validate_endpoint_hashes(&relation.from, &relation.id, report);
    validate_endpoint_hashes(&relation.to, &relation.id, report);
    if validate_relation_direction(relation.relation_type, relation.to.kind).is_err() {
        report.push_error(
            "knowledge_relation_direction_invalid",
            Some(relation.id.clone()),
            "relation direction is invalid",
        );
    }
    if matches!(
        relation.relation_type,
        RelationType::SupersededBy | RelationType::Corroborates | RelationType::CausedBy
    ) && relation.to.kind == EndpointKind::Learning
        && relation.from.id == relation.to.id
    {
        report.push_error(
            "knowledge_relation_cycle",
            Some(relation.id.clone()),
            "self-edge is not allowed",
        );
    }
    validate_endpoint(repo_root, &relation.to, entries, &relation.id, report);
}

fn validate_endpoint(
    repo_root: &Path,
    endpoint: &Endpoint,
    entries: &BTreeMap<String, Learning>,
    relation_id: &str,
    report: &mut KnowledgeValidationReport,
) {
    match endpoint.kind {
        EndpointKind::Learning => match entries.get(&endpoint.id) {
            Some(learning) => validate_revision_match(
                endpoint.revision,
                learning.revision,
                relation_id,
                "target learning revision mismatch",
                report,
            ),
            None => report.push_error(
                "knowledge_relation_endpoint_missing",
                Some(relation_id.to_string()),
                "target learning does not exist",
            ),
        },
        EndpointKind::Work => match load_work_node(repo_root, &endpoint.id) {
            Ok(node) => validate_revision_match(
                endpoint.revision,
                node.revision,
                relation_id,
                "target work revision mismatch",
                report,
            ),
            Err(_) => report.push_error(
                "knowledge_relation_endpoint_missing",
                Some(relation_id.to_string()),
                "target work does not exist",
            ),
        },
        EndpointKind::Decision => match load_work_node(repo_root, &endpoint.id) {
            Ok(node) if node.kind == WorkKind::Decision => validate_revision_match(
                endpoint.revision,
                node.revision,
                relation_id,
                "target decision revision mismatch",
                report,
            ),
            _ => report.push_error(
                "knowledge_relation_endpoint_missing",
                Some(relation_id.to_string()),
                "target decision does not exist or is not a decision",
            ),
        },
        EndpointKind::Document => {
            let path = repo_root.join(".pulse/docs/registry.json");
            match crate::storage::read_json::<crate::docs::model::DocsRegistryEnvelope>(&path) {
                Ok(registry) => match registry.documents.iter().find(|doc| doc.id == endpoint.id) {
                    Some(doc) => validate_revision_match(
                        endpoint.revision,
                        doc.revision,
                        relation_id,
                        "target document revision mismatch",
                        report,
                    ),
                    None => report.push_error(
                        "knowledge_relation_endpoint_missing",
                        Some(relation_id.to_string()),
                        "target document does not exist",
                    ),
                },
                Err(_) => report.push_error(
                    "knowledge_relation_endpoint_missing",
                    Some(relation_id.to_string()),
                    "target document does not exist",
                ),
            }
        }
        EndpointKind::Receipt => {
            match crate::evidence::receipt::load_receipt(repo_root, &endpoint.id) {
                Ok((_receipt, hash)) => {
                    if let Some(target_hash) = &endpoint.content_hash {
                        if *target_hash != hash {
                            report.push_error(
                                "knowledge_relation_endpoint_hash_mismatch",
                                Some(relation_id.to_string()),
                                "target receipt hash does not match canonical receipt bytes",
                            );
                        }
                    }
                }
                Err(_) => report.push_error(
                    "knowledge_relation_endpoint_missing",
                    Some(relation_id.to_string()),
                    "target receipt does not exist",
                ),
            }
        }
        EndpointKind::Commit => {
            if crate::source::resolve_full_commit(repo_root, &endpoint.id).is_err() {
                report.push_error(
                    "knowledge_relation_endpoint_missing",
                    Some(relation_id.to_string()),
                    "target commit does not resolve",
                );
            }
        }
    }
}

fn validate_endpoint_hashes(
    endpoint: &Endpoint,
    relation_id: &str,
    report: &mut KnowledgeValidationReport,
) {
    if let Some(hash) = &endpoint.content_hash {
        if !validate_sha256(hash) {
            report.push_error(
                "knowledge_relation_endpoint_hash_invalid",
                Some(relation_id.to_string()),
                "endpoint content_hash must be sha256:<64 hex>",
            );
        }
    }
}

fn validate_revision_match(
    bound_revision: Option<u64>,
    current_revision: u64,
    relation_id: &str,
    message: &str,
    report: &mut KnowledgeValidationReport,
) {
    if let Some(bound_revision) = bound_revision {
        if bound_revision != current_revision {
            report.push_error(
                "knowledge_relation_endpoint_revision_mismatch",
                Some(relation_id.to_string()),
                message,
            );
        }
    }
}

fn load_work_node(repo_root: &Path, id: &str) -> Result<crate::graph::node::Node> {
    let path = repo_root
        .join(".pulse/workgraph/nodes")
        .join(format!("{id}.json"));
    crate::storage::read_json::<crate::graph::node::Node>(&path)
}

fn validate_cross_relations(
    entries: &BTreeMap<String, Learning>,
    relations: &BTreeMap<String, KnowledgeRelation>,
    report: &mut KnowledgeValidationReport,
) {
    let mut superseded_by: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut outgoing_contradicts: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for relation in relations.values() {
        match relation.relation_type {
            RelationType::DerivedFrom => {
                if let Some(entry) = entries.get(&relation.from.id) {
                    if !entry.provenance.relation_ids.contains(&relation.id) {
                        report.push_error(
                            "learning_provenance_mismatch",
                            Some(entry.id.clone()),
                            "outgoing derived_from relation is not listed in entry provenance",
                        );
                    }
                }
            }
            RelationType::PromotedTo => {
                if let Some(entry) = entries.get(&relation.from.id) {
                    if !entry.promotion.relation_ids.contains(&relation.id) {
                        report.push_error(
                            "learning_promotion_mismatch",
                            Some(entry.id.clone()),
                            "outgoing promoted_to relation is not listed in entry promotion relation_ids",
                        );
                    }
                }
            }
            RelationType::SupersededBy => {
                superseded_by
                    .entry(relation.from.id.clone())
                    .or_default()
                    .push(relation.to.id.clone());
            }
            RelationType::Contradicts => {
                outgoing_contradicts
                    .entry(relation.from.id.clone())
                    .or_default()
                    .push(relation.to.id.clone());
            }
            _ => {}
        }
    }

    for entry in entries.values() {
        let supersession_targets = superseded_by
            .get(&entry.id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if entry.status == LearningStatus::Superseded {
            if supersession_targets.len() != 1 {
                report.push_error(
                    "learning_supersession_mismatch",
                    Some(entry.id.clone()),
                    "superseded learning must have exactly one outgoing superseded_by relation",
                );
            }
        } else if !supersession_targets.is_empty() {
            report.push_error(
                "learning_supersession_mismatch",
                Some(entry.id.clone()),
                "only superseded learnings may have outgoing superseded_by relations",
            );
        }

        if entry.status == LearningStatus::Disputed
            && entry.validation.contradiction_status == ContradictionStatus::None
            && !outgoing_contradicts.contains_key(&entry.id)
        {
            report.push_error(
                "learning_dispute_mismatch",
                Some(entry.id.clone()),
                "disputed learning requires suspected/confirmed contradiction status or outgoing contradicts relation",
            );
        }

        let promoted = entry.status == LearningStatus::Promoted
            || entry.promotion.state == PromotionState::Promoted;
        if promoted && entry.promotion.relation_ids.is_empty() {
            report.push_error(
                "learning_promotion_mismatch",
                Some(entry.id.clone()),
                "promoted learning requires promoted_to relation_ids",
            );
        }
        for relation_id in &entry.promotion.relation_ids {
            match relations.get(relation_id) {
                Some(relation)
                    if relation.relation_type == RelationType::PromotedTo
                        && relation.from.id == entry.id => {}
                _ => report.push_error(
                    "learning_promotion_mismatch",
                    Some(entry.id.clone()),
                    format!("promotion relation missing or mismatched: {relation_id}"),
                ),
            }
        }
    }

    for (id, targets) in &superseded_by {
        if targets.len() > 1 {
            report.push_error(
                "knowledge_relation_conflict",
                Some(id.clone()),
                "superseded_by has multiple outgoing targets",
            );
        }
    }
    validate_supersession_cycles(&superseded_by, report);
}

fn validate_supersession_cycles(
    superseded_by: &BTreeMap<String, Vec<String>>,
    report: &mut KnowledgeValidationReport,
) {
    for start in superseded_by.keys() {
        let mut seen = std::collections::BTreeSet::new();
        let mut current = start.as_str();
        while let Some(targets) = superseded_by.get(current) {
            let Some(next) = targets.first() else {
                break;
            };
            if next == start || !seen.insert(next.as_str()) {
                report.push_error(
                    "knowledge_relation_cycle",
                    Some(start.clone()),
                    "superseded_by relations must not form a cycle",
                );
                break;
            }
            current = next;
        }
    }
}

fn validate_text(text: &str, id: &str, report: &mut KnowledgeValidationReport) {
    let lower = text.to_ascii_lowercase();
    if text
        .chars()
        .any(|c| c == '\0' || (c.is_control() && c != '\n' && c != '\t'))
        || lower.contains("-----begin private key-----")
        || lower.contains("-----begin rsa private key-----")
        || lower.contains("raw_prompt")
        || lower.contains("transcript")
    {
        report.push_error(
            "learning_schema_invalid",
            Some(id.to_string()),
            "text contains forbidden raw/control/secret-like payload",
        );
    }
}

pub fn valid_learning_id(id: &str) -> bool {
    id.strip_prefix("LRN-")
        .map(|s| s.len() >= 3 && s.chars().all(|c| c.is_ascii_digit()))
        .unwrap_or(false)
}

pub fn unsafe_repo_glob(path: &str) -> bool {
    let p = Path::new(path);
    let protected = [
        ".pulse/runtime",
        ".pulse/cache",
        ".pulse/evidence",
        ".pulse/transactions",
    ];
    p.is_absolute()
        || path.trim().is_empty()
        || path.contains('\0')
        || path.contains('\\')
        || path.starts_with('-')
        || p.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
        || protected
            .iter()
            .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")))
}

pub fn resolve_learning_content_path(repo_root: &Path, path: &str) -> Option<PathBuf> {
    if unsafe_repo_glob(path) || !path.starts_with("knowledge/learnings/") {
        return None;
    }
    let repo = repo_root.canonicalize().ok()?;
    let content_root = repo.join("knowledge/learnings");
    let parent = Path::new(path).parent()?;
    let resolved_parent = repo.join(parent).canonicalize().ok()?;
    let canonical_content_root = if content_root.exists() {
        content_root.canonicalize().ok()?
    } else {
        let root_parent = content_root.parent()?.canonicalize().ok()?;
        root_parent.join(content_root.file_name()?)
    };
    if !resolved_parent.starts_with(&canonical_content_root) {
        return None;
    }
    let candidate = resolved_parent.join(Path::new(path).file_name()?);
    if candidate.exists() {
        let canonical_candidate = candidate.canonicalize().ok()?;
        if !canonical_candidate.starts_with(&canonical_content_root) {
            return None;
        }
        Some(canonical_candidate)
    } else {
        Some(candidate)
    }
}

pub fn validate_sha256(value: &str) -> bool {
    value.len() == SHA256_PREFIX.len() + 64
        && value.starts_with(SHA256_PREFIX)
        && value[SHA256_PREFIX.len()..]
            .chars()
            .all(|c| c.is_ascii_hexdigit())
}
