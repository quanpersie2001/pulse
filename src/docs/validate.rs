use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::docs::model::{
    DocsRegistry, DocumentKind, DocumentRecord, DocumentStatus, RetrievalConfig,
};
use crate::storage;
use crate::PulseResult;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocsValidationReport {
    pub schema_version: u32,
    pub code: String,
    pub valid: bool,
    pub registry_revision: u64,
    pub errors: Vec<DocsFinding>,
    pub warnings: Vec<DocsFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocsFinding {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl DocsValidationReport {
    pub fn into_result(self) -> PulseResult<Self> {
        if self.valid {
            Ok(self)
        } else {
            Err(crate::PulseError::validation(
                "invalid_docs_registry",
                serde_json::to_string(&self.errors)
                    .unwrap_or_else(|_| "invalid docs registry".to_string()),
            ))
        }
    }
}

pub fn validate_registry(
    repo_root: &Path,
    _repository_id: &str,
    registry: &DocsRegistry,
) -> PulseResult<DocsValidationReport> {
    let mut errors = Vec::new();
    let warnings = Vec::new();
    if registry.schema_version != crate::docs::model::DOCS_REGISTRY_SCHEMA_VERSION
        || registry.revision == 0
        || !registry.repository_id.starts_with("repo_")
    {
        errors.push(finding(
            "docs_registry_schema_invalid",
            "unsupported docs registry envelope",
            None,
            None,
        ));
    }
    if let Some(config) = &registry.retrieval {
        validate_retrieval_config(config, &mut errors);
    }

    let mut last_id = None;
    let mut ids = BTreeSet::new();
    let mut paths = BTreeMap::new();
    for document in &registry.documents {
        if last_id.is_some_and(|previous: &str| previous > document.id.as_str()) {
            errors.push(finding(
                "docs_registry_not_canonical",
                "documents must be sorted lexically by id",
                Some(document.id.clone()),
                Some(document.path.clone()),
            ));
        }
        last_id = Some(document.id.as_str());
        validate_document(repo_root, document, &mut errors)?;
        if !ids.insert(document.id.clone()) {
            errors.push(finding(
                "document_id_duplicate",
                format!("duplicate document id {}", document.id),
                Some(document.id.clone()),
                None,
            ));
        }
        if let Some(existing) =
            paths.insert(document.path.to_ascii_lowercase(), document.id.clone())
        {
            errors.push(finding(
                "document_path_duplicate",
                format!(
                    "documents {existing} and {} share path {}",
                    document.id, document.path
                ),
                Some(document.id.clone()),
                Some(document.path.clone()),
            ));
        }
        if let Some(target) = &document.superseded_by {
            if target == &document.id
                || !ids.contains(target)
                    && !registry
                        .documents
                        .iter()
                        .any(|candidate| &candidate.id == target)
            {
                errors.push(finding(
                    "document_supersession_invalid",
                    "superseded_by must reference another existing document",
                    Some(document.id.clone()),
                    Some(document.path.clone()),
                ));
            }
            if document.status != DocumentStatus::Retired {
                errors.push(finding(
                    "document_supersession_invalid",
                    "a superseded document must be retired",
                    Some(document.id.clone()),
                    Some(document.path.clone()),
                ));
            }
        }
    }
    for document in &registry.documents {
        if has_supersession_cycle(document, registry) {
            errors.push(finding(
                "document_supersession_cycle",
                "document supersession chain contains a cycle",
                Some(document.id.clone()),
                Some(document.path.clone()),
            ));
        }
    }
    Ok(DocsValidationReport {
        schema_version: 1,
        code: if errors.is_empty() { "ok" } else { "invalid" }.to_string(),
        valid: errors.is_empty(),
        registry_revision: registry.revision,
        errors,
        warnings,
    })
}

fn validate_document(
    repo_root: &Path,
    document: &DocumentRecord,
    errors: &mut Vec<DocsFinding>,
) -> PulseResult<()> {
    if !valid_document_id(&document.id) {
        errors.push(finding(
            "document_id_invalid",
            format!("invalid document id {}", document.id),
            Some(document.id.clone()),
            None,
        ));
    }
    if document.revision == 0 {
        errors.push(finding(
            "document_revision_invalid",
            "document revision must be positive",
            Some(document.id.clone()),
            Some(document.path.clone()),
        ));
    }
    if document.owner.trim().is_empty() {
        errors.push(finding(
            "document_owner_missing",
            "registered documents must declare an owner",
            Some(document.id.clone()),
            Some(document.path.clone()),
        ));
    }
    if document.summary.trim().is_empty() || document.summary.chars().count() > 500 {
        errors.push(finding(
            "document_summary_invalid",
            "registered documents must declare a non-empty <=500 char summary",
            Some(document.id.clone()),
            Some(document.path.clone()),
        ));
    }
    validate_path(repo_root, document, errors);
    validate_scope(document, errors);
    validate_tags(repo_root, document, errors)?;
    validate_generated(document, errors);
    Ok(())
}

fn validate_path(repo_root: &Path, document: &DocumentRecord, errors: &mut Vec<DocsFinding>) {
    if storage::safe_repo_relative(&document.path).is_err() {
        errors.push(finding(
            "document_path_unsafe",
            "document path must be repository-relative and traversal-free",
            Some(document.id.clone()),
            Some(document.path.clone()),
        ));
        return;
    }
    if is_protected_path(&document.path) {
        errors.push(finding(
            "document_migration_backup_forbidden",
            "protected Pulse migration backup paths may not be registered",
            Some(document.id.clone()),
            Some(document.path.clone()),
        ));
    }
    if document.path == "works" || document.path.starts_with("works/") {
        errors.push(finding(
            "document_work_content_forbidden",
            "work prose must not be registered as durable documentation",
            Some(document.id.clone()),
            Some(document.path.clone()),
        ));
    }
    if is_generated_navigation_path(&document.path) {
        errors.push(finding(
            "document_generated_navigation",
            "generated navigation _index.md files are not authoritative content",
            Some(document.id.clone()),
            Some(document.path.clone()),
        ));
    }
    if matches!(document.status, DocumentStatus::Approved) {
        if let Ok(full_path) =
            crate::storage::paths::resolve_repo_relative(repo_root, &document.path)
        {
            if !full_path.is_file() {
                errors.push(finding(
                    "document_content_missing",
                    "approved document content path is missing",
                    Some(document.id.clone()),
                    Some(document.path.clone()),
                ));
            }
        }
    }
}

fn validate_scope(document: &DocumentRecord, errors: &mut Vec<DocsFinding>) {
    let mut sorted = document.scope.paths.clone();
    sorted.sort();
    if sorted != document.scope.paths {
        errors.push(finding(
            "document_scope_invalid",
            "scope.paths must be sorted",
            Some(document.id.clone()),
            Some(document.path.clone()),
        ));
    }
    let mut seen = BTreeSet::new();
    for path in &document.scope.paths {
        if !seen.insert(path) || !safe_glob(path) {
            errors.push(finding(
                "document_scope_invalid",
                format!("scope.paths must be unique safe globs: {path}"),
                Some(document.id.clone()),
                Some(document.path.clone()),
            ));
        }
    }
}

fn validate_tags(
    repo_root: &Path,
    document: &DocumentRecord,
    errors: &mut Vec<DocsFinding>,
) -> PulseResult<()> {
    let mut sorted = document.tags.clone();
    sorted.sort();
    sorted.dedup();
    if sorted != document.tags
        || document
            .tags
            .iter()
            .any(|tag| crate::docs::normalize_tag(tag).is_err())
    {
        errors.push(finding(
            "document_tags_invalid",
            "tags must be unique, sorted lowercase slugs",
            Some(document.id.clone()),
            Some(document.path.clone()),
        ));
    }
    let path = crate::docs::tags_path(repo_root);
    if path.exists() {
        let tags = crate::docs::load_tags(repo_root)?;
        for tag in &document.tags {
            if !tags.tags.contains(tag) {
                errors.push(finding(
                    "docs_tag_unknown",
                    format!("tag is not registered: {tag}"),
                    Some(document.id.clone()),
                    Some(document.path.clone()),
                ));
            }
        }
    }
    Ok(())
}

fn validate_generated(document: &DocumentRecord, errors: &mut Vec<DocsFinding>) {
    let generated = document.generated.is_some() || document.kind == DocumentKind::Generated;
    if generated {
        if document.kind != DocumentKind::Generated {
            errors.push(finding(
                "document_generated_contract_invalid",
                "generated contract requires kind=generated",
                Some(document.id.clone()),
                Some(document.path.clone()),
            ));
        }
        let Some(contract) = &document.generated else {
            errors.push(finding(
                "document_generated_contract_invalid",
                "generated documents must declare generated contract",
                Some(document.id.clone()),
                Some(document.path.clone()),
            ));
            return;
        };
        if contract.command.trim().is_empty() || contract.freshness_check.trim().is_empty() {
            errors.push(finding(
                "document_generated_contract_invalid",
                "generated command and freshness_check must be non-empty",
                Some(document.id.clone()),
                Some(document.path.clone()),
            ));
        }
    } else if document.generated.is_some() {
        errors.push(finding(
            "document_generated_contract_invalid",
            "authored documents must use generated=null",
            Some(document.id.clone()),
            Some(document.path.clone()),
        ));
    }
}

pub fn validate_retrieval_config(config: &RetrievalConfig, errors: &mut Vec<DocsFinding>) {
    if config.schema_version != 1 {
        errors.push(finding(
            "docs_registry_retrieval_config_invalid",
            "retrieval schema_version must be 1",
            None,
            None,
        ));
    }
    if !(1..=50).contains(&config.default_search_limit) {
        errors.push(finding(
            "docs_registry_retrieval_config_invalid",
            "default_search_limit must be in 1..=50",
            None,
            None,
        ));
    }
    if !(1..=2000).contains(&config.default_get_max_lines) {
        errors.push(finding(
            "docs_registry_retrieval_config_invalid",
            "default_get_max_lines must be in 1..=2000",
            None,
            None,
        ));
    }
    if !(1024..=1_048_576).contains(&config.default_get_max_bytes) {
        errors.push(finding(
            "docs_registry_retrieval_config_invalid",
            "default_get_max_bytes must be in 1024..=1_048_576",
            None,
            None,
        ));
    }
}

fn valid_document_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("DOC-") else {
        return false;
    };
    if !(3..=64).contains(&rest.len()) {
        return false;
    }
    rest.chars()
        .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '-')
}
fn safe_glob(pattern: &str) -> bool {
    !pattern.trim().is_empty()
        && !pattern.starts_with('/')
        && !pattern.contains('\\')
        && !pattern.contains("//")
        && pattern
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != ".." && !part.contains(".."))
        && !is_protected_path(pattern)
        && !pattern.starts_with("works/")
        && pattern != "works"
}
fn has_supersession_cycle(document: &DocumentRecord, registry: &DocsRegistry) -> bool {
    let by_id: BTreeMap<_, _> = registry
        .documents
        .iter()
        .map(|doc| (doc.id.as_str(), doc))
        .collect();
    let mut seen = BTreeSet::new();
    let mut current = document;
    while let Some(next) = current.superseded_by.as_deref() {
        if !seen.insert(current.id.as_str()) || next == document.id {
            return true;
        }
        let Some(next_doc) = by_id.get(next).copied() else {
            return false;
        };
        current = next_doc;
    }
    false
}
pub fn is_protected_path(path: &str) -> bool {
    path == ".pulse/migrations/docs-backups" || path.starts_with(".pulse/migrations/docs-backups/")
}
pub fn is_generated_navigation_path(path: &str) -> bool {
    path.starts_with("docs/") && (path == "docs/_index.md" || path.ends_with("/_index.md"))
}
fn finding(
    code: impl Into<String>,
    message: impl Into<String>,
    document_id: Option<String>,
    path: Option<String>,
) -> DocsFinding {
    DocsFinding {
        code: code.into(),
        message: message.into(),
        document_id,
        path,
    }
}
