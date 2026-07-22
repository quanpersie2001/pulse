use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::docs::model::{
    DocsRegistry, DocumentAuthority, DocumentKind, DocumentLifecycle, DocumentRecord,
    RetrievalConfig, RetrievalScope,
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
    if registry.schema_version != crate::docs::model::DOCS_REGISTRY_SCHEMA_VERSION_V2
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
    if let Some(retrieval) = &registry.retrieval {
        validate_retrieval_config(retrieval, &mut errors);
    }
    let mut last_id: Option<&str> = None;
    let mut ids = BTreeSet::new();
    let mut paths: BTreeMap<String, String> = BTreeMap::new();
    let documents_by_id: BTreeMap<_, _> = registry
        .documents
        .iter()
        .map(|doc| (doc.id.as_str(), doc))
        .collect();
    let by_id: BTreeSet<_> = registry
        .documents
        .iter()
        .map(|doc| doc.id.clone())
        .collect();

    for document in &registry.documents {
        if let Some(previous) = last_id {
            if previous > document.id.as_str() {
                errors.push(finding(
                    "docs_registry_not_canonical",
                    "documents must be sorted lexically by id",
                    Some(document.id.clone()),
                    Some(document.path.clone()),
                ));
            }
        }
        last_id = Some(&document.id);
        validate_document(repo_root, document, &mut errors)?;
        if !ids.insert(document.id.clone()) {
            errors.push(finding(
                "document_id_duplicate",
                format!("duplicate document id {}", document.id),
                Some(document.id.clone()),
                None,
            ));
        }
        let canonical_path = document.path.to_ascii_lowercase();
        if let Some(existing) = paths.insert(canonical_path, document.id.clone()) {
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
        if document.lifecycle == DocumentLifecycle::Superseded {
            match &document.superseded_by {
                Some(target) if target != &document.id && by_id.contains(target) => {
                    if let Some(target_doc) = documents_by_id.get(target.as_str()) {
                        if matches!(
                            target_doc.lifecycle,
                            DocumentLifecycle::Retired | DocumentLifecycle::Stale
                        ) {
                            errors.push(finding(
                                "document_lifecycle_invalid",
                                "supersession target must not be retired or stale",
                                Some(document.id.clone()),
                                Some(document.path.clone()),
                            ));
                        }
                    }
                }
                _ => errors.push(finding(
                    "document_lifecycle_invalid",
                    "superseded documents must reference an existing replacement",
                    Some(document.id.clone()),
                    Some(document.path.clone()),
                )),
            }
        } else if document.superseded_by.is_some() {
            errors.push(finding(
                "document_lifecycle_invalid",
                "only superseded documents may declare superseded_by",
                Some(document.id.clone()),
                Some(document.path.clone()),
            ));
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
    validate_owner(document, errors);
    validate_summary(document, errors);
    validate_aliases(document, errors);
    validate_path(repo_root, document, errors);
    validate_scope(document, errors);
    validate_generated(document, errors);
    Ok(())
}

fn validate_owner(document: &DocumentRecord, errors: &mut Vec<DocsFinding>) {
    let Some((kind, id)) = document.owner.split_once(':') else {
        errors.push(finding(
            "document_owner_missing",
            "registered documents must declare typed owner",
            Some(document.id.clone()),
            Some(document.path.clone()),
        ));
        return;
    };
    if !matches!(kind, "human" | "team" | "role" | "system") || id.trim().is_empty() {
        errors.push(finding(
            "document_owner_missing",
            "owner must be human/team/role/system with non-empty id",
            Some(document.id.clone()),
            Some(document.path.clone()),
        ));
    }
}

fn validate_summary(document: &DocumentRecord, errors: &mut Vec<DocsFinding>) {
    if document.summary.trim().is_empty() || document.summary.chars().count() > 500 {
        errors.push(finding(
            "document_summary_invalid",
            "registered documents must declare a non-empty <=500 char summary",
            Some(document.id.clone()),
            Some(document.path.clone()),
        ));
    }
}

fn validate_aliases(document: &DocumentRecord, errors: &mut Vec<DocsFinding>) {
    if document.aliases.len() > 32 {
        errors.push(finding(
            "document_aliases_invalid",
            "aliases are limited to 32 entries",
            Some(document.id.clone()),
            Some(document.path.clone()),
        ));
    }
    let mut sorted = document.aliases.clone();
    sorted.sort();
    if sorted != document.aliases {
        errors.push(finding(
            "document_aliases_invalid",
            "aliases must be sorted",
            Some(document.id.clone()),
            Some(document.path.clone()),
        ));
    }
    let mut seen = BTreeSet::new();
    for alias in &document.aliases {
        let normalized = alias.trim().to_ascii_lowercase();
        if alias.trim().is_empty() || alias.chars().count() > 120 || !seen.insert(normalized) {
            errors.push(finding(
                "document_aliases_invalid",
                "aliases must be unique non-empty strings <=120 chars",
                Some(document.id.clone()),
                Some(document.path.clone()),
            ));
        }
    }
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
    if document.path.starts_with("works/") || document.path == "works" {
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
    if document.lifecycle == DocumentLifecycle::Current
        && matches!(
            document.authority,
            DocumentAuthority::Approved | DocumentAuthority::Generated
        )
    {
        match crate::storage::paths::resolve_repo_relative(repo_root, &document.path) {
            Ok(full_path) => {
                if !full_path.exists()
                    || !fs::metadata(&full_path)
                        .map(|m| m.is_file())
                        .unwrap_or(false)
                {
                    errors.push(finding(
                        "document_content_missing",
                        "current approved/generated document content path is missing",
                        Some(document.id.clone()),
                        Some(document.path.clone()),
                    ));
                }
            }
            Err(error) => errors.push(finding(
                error.code(),
                error.to_string(),
                Some(document.id.clone()),
                Some(document.path.clone()),
            )),
        }
    }
}

fn validate_scope(document: &DocumentRecord, errors: &mut Vec<DocsFinding>) {
    for (field, values) in [
        ("paths", &document.scope.paths),
        ("domains", &document.scope.domains),
        ("work_labels", &document.scope.work_labels),
    ] {
        let mut sorted = values.clone();
        sorted.sort();
        if sorted != *values {
            errors.push(finding(
                "document_scope_invalid",
                format!("scope.{field} must be sorted"),
                Some(document.id.clone()),
                Some(document.path.clone()),
            ));
        }
        let mut seen = BTreeSet::new();
        for value in values {
            if !seen.insert(value) {
                errors.push(finding(
                    "document_scope_invalid",
                    format!("scope.{field} must be unique"),
                    Some(document.id.clone()),
                    Some(document.path.clone()),
                ));
            }
        }
    }
    for path in &document.scope.paths {
        if !safe_glob(path) {
            errors.push(finding(
                "document_scope_invalid",
                format!("unsafe scope path {path}"),
                Some(document.id.clone()),
                Some(document.path.clone()),
            ));
        }
    }
    for domain in &document.scope.domains {
        if !valid_slug(domain) {
            errors.push(finding(
                "document_scope_invalid",
                format!("invalid domain slug {domain}"),
                Some(document.id.clone()),
                Some(document.path.clone()),
            ));
        }
    }
    for label in &document.scope.work_labels {
        if !valid_slug(label) {
            errors.push(finding(
                "document_scope_invalid",
                format!("invalid work label {label}"),
                Some(document.id.clone()),
                Some(document.path.clone()),
            ));
        }
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
    if config.root.trim().is_empty()
        || config.root.starts_with('/')
        || config.root.contains("\\")
        || config.root.contains("//")
        || config.root == "."
        || config.root == ".."
        || config.root.split('/').any(|c| c == "..")
    {
        errors.push(finding(
            "docs_registry_retrieval_config_invalid",
            "retrieval root must be a safe repository-relative directory",
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
    if !(1..=10_000).contains(&config.auto_refresh_max_documents) {
        errors.push(finding(
            "docs_registry_retrieval_config_invalid",
            "auto_refresh_max_documents must be in 1..=10000",
            None,
            None,
        ));
    }
    if !(1_048_576..=1_073_741_824).contains(&config.auto_refresh_max_source_bytes) {
        errors.push(finding(
            "docs_registry_retrieval_config_invalid",
            "auto_refresh_max_source_bytes must be in 1MiB..=1GiB",
            None,
            None,
        ));
    }
    if !(1..=1000).contains(&config.area_index_threshold) {
        errors.push(finding(
            "docs_registry_retrieval_config_invalid",
            "area_index_threshold must be in 1..=1000",
            None,
            None,
        ));
    }
    let mut seen = BTreeSet::new();
    let mut sorted = config
        .scopes
        .iter()
        .map(|scope| scope.path.clone())
        .collect::<Vec<_>>();
    sorted.sort();
    if sorted
        != config
            .scopes
            .iter()
            .map(|scope| scope.path.clone())
            .collect::<Vec<_>>()
    {
        errors.push(finding(
            "docs_registry_retrieval_config_invalid",
            "retrieval scopes must be sorted by path",
            None,
            None,
        ));
    }
    for scope in &config.scopes {
        validate_retrieval_scope(scope, &mut seen, errors);
    }
}

fn validate_retrieval_scope(
    scope: &RetrievalScope,
    seen: &mut BTreeSet<String>,
    errors: &mut Vec<DocsFinding>,
) {
    let normalized = normalize_scope_path(&scope.path);
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.contains("\\")
        || normalized.contains("//")
        || normalized.ends_with("/_index.md")
        || normalized == "_index.md"
    {
        errors.push(finding(
            "docs_registry_retrieval_config_invalid",
            format!(
                "retrieval scope path {} must be safe and not generated navigation",
                scope.path
            ),
            None,
            None,
        ));
    }
    if !seen.insert(normalized) {
        errors.push(finding(
            "docs_registry_retrieval_config_invalid",
            "retrieval scope paths must be unique",
            None,
            None,
        ));
    }
    if scope.summary.trim().is_empty() || scope.summary.chars().count() > 500 {
        errors.push(finding(
            "docs_registry_retrieval_config_invalid",
            "retrieval scope summary must be non-empty <=500 chars",
            None,
            None,
        ));
    }
}

fn normalize_scope_path(path: &str) -> String {
    let trimmed = path.trim_matches('/');
    let mut parts: Vec<&str> = Vec::new();
    for component in trimmed.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        parts.push(component);
    }
    parts.join("/")
}

fn validate_generated(document: &DocumentRecord, errors: &mut Vec<DocsFinding>) {
    let generated_expected = document.generated.is_some()
        || document.kind == DocumentKind::Generated
        || document.authority == DocumentAuthority::Generated;
    if generated_expected {
        if document.kind != DocumentKind::Generated
            || document.authority != DocumentAuthority::Generated
        {
            errors.push(finding(
                "document_generated_contract_invalid",
                "generated contract requires kind=generated and authority=generated",
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
        if contract.command.trim().is_empty()
            || contract.freshness_check.trim().is_empty()
            || contract.sources.is_empty()
            || contract.outputs.is_empty()
        {
            errors.push(finding(
                "document_generated_contract_invalid",
                "generated contract fields must be non-empty",
                Some(document.id.clone()),
                Some(document.path.clone()),
            ));
        }
        for path in contract.sources.iter().chain(contract.outputs.iter()) {
            if !safe_glob(path) {
                errors.push(finding(
                    "document_generated_contract_invalid",
                    format!("unsafe generated path {path}"),
                    Some(document.id.clone()),
                    Some(document.path.clone()),
                ));
            }
        }
        if !contract
            .outputs
            .iter()
            .any(|pattern| glob_matches(pattern, &document.path))
        {
            errors.push(finding(
                "document_generated_contract_invalid",
                "registered generated document path must be inside generated outputs",
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

fn valid_document_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("DOC-") else {
        return false;
    };
    if rest.len() < 3 || rest.len() > 64 {
        return false;
    }
    let mut chars = rest.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_uppercase() || ch.is_ascii_digit())
        && chars.all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '-')
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        && !value.starts_with('-')
        && !value.ends_with('-')
}

fn safe_glob(pattern: &str) -> bool {
    if pattern.trim().is_empty()
        || pattern.starts_with('/')
        || pattern.contains('\\')
        || pattern.contains("//")
    {
        return false;
    }
    let path = PathBuf::from(pattern);
    if path.is_absolute() {
        return false;
    }
    for component in pattern.split('/') {
        if component.is_empty() || component == "." || component == ".." || component.contains("..")
        {
            return false;
        }
    }
    !is_protected_path(pattern) && !pattern.starts_with("works/") && pattern != "works"
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

fn glob_matches(pattern: &str, path: &str) -> bool {
    if pattern == "**" || pattern == path {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return path == prefix || path.starts_with(&format!("{prefix}/"));
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return path.starts_with(prefix);
    }
    false
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
