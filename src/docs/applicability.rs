use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::canonical_json::hash_bytes;
use crate::docs::model::{
    DocsRegistry, DocumentAuthority, DocumentKind, DocumentLifecycle, DocumentRecord,
    DocumentationPosture, WorkDocumentationContext,
};
use crate::docs::registry::registry_fingerprint;
use crate::storage;
use crate::PulseResult;

#[derive(Debug, Clone, Copy, Default)]
pub struct ApplicabilityOptions {
    pub include_draft: bool,
    pub include_stale: bool,
}

pub trait ContentResolver {
    fn resolve(&self, document: &DocumentRecord) -> ContentState;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentState {
    Present { hash: String },
    Missing,
    Protected,
    Unsafe,
}

pub struct FsContentResolver<'a> {
    repo_root: &'a Path,
}

impl<'a> FsContentResolver<'a> {
    pub fn new(repo_root: &'a Path) -> Self {
        Self { repo_root }
    }
}

impl ContentResolver for FsContentResolver<'_> {
    fn resolve(&self, document: &DocumentRecord) -> ContentState {
        if is_protected_path(&document.path) || is_generated_navigation_path(&document.path) {
            return ContentState::Protected;
        }
        let relative = match storage::safe_repo_relative(&document.path) {
            Ok(path) => path,
            Err(_) => return ContentState::Unsafe,
        };
        let full = self.repo_root.join(relative);
        let meta = match fs::metadata(&full) {
            Ok(meta) => meta,
            Err(_) => return ContentState::Missing,
        };
        if !meta.is_file() {
            return ContentState::Missing;
        }
        match fs::read(&full) {
            Ok(bytes) => ContentState::Present {
                hash: hash_bytes(&bytes),
            },
            Err(_) => ContentState::Missing,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ApplicableDocsReport {
    pub schema_version: u32,
    pub work: ApplicableWork,
    pub registry: ApplicableRegistry,
    pub required: Vec<ApplicableDocument>,
    pub optional: Vec<ApplicableDocument>,
    pub write_candidates: Vec<WriteCandidate>,
    pub excluded: Vec<ExcludedDocument>,
    pub gate: ApplicabilityGate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ApplicableWork {
    pub id: String,
    pub revision: u64,
    pub documentation_posture: DocumentationPosture,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ApplicableRegistry {
    pub revision: u64,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ApplicableDocument {
    pub id: String,
    pub path: String,
    pub kind: DocumentKind,
    pub authority: DocumentAuthority,
    pub owner: String,
    pub summary: String,
    pub content_hash: String,
    pub document_revision: u64,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WriteCandidate {
    pub id: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExcludedDocument {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub reason_codes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ApplicabilityGate {
    pub status: String,
    pub reason_codes: Vec<String>,
    pub policy_status: String,
}

pub fn applicable_docs(
    work: &WorkDocumentationContext,
    registry: &DocsRegistry,
    resolver: &dyn ContentResolver,
    options: ApplicabilityOptions,
) -> PulseResult<ApplicableDocsReport> {
    let mut documents = registry.documents.clone();
    documents.sort_by(|a, b| a.id.cmp(&b.id));
    let by_id: BTreeMap<String, DocumentRecord> = documents
        .iter()
        .map(|document| (document.id.clone(), document.clone()))
        .collect();
    let explicit_required: BTreeSet<_> = work.required_documents.iter().cloned().collect();

    let mut required = Vec::new();
    let mut optional = Vec::new();
    let mut write_candidates = Vec::new();
    let mut excluded_by_id: BTreeMap<String, ExcludedDocument> = BTreeMap::new();
    let mut gate_reasons = Vec::new();

    if work.posture == DocumentationPosture::Unknown {
        gate_reasons.push("documentation_impact_unknown".to_string());
    }

    for required_id in &explicit_required {
        if !by_id.contains_key(required_id) {
            push_excluded(
                &mut excluded_by_id,
                ExcludedDocument {
                    id: required_id.clone(),
                    path: None,
                    reason_codes: vec!["required_document_missing".to_string()],
                    replacement: None,
                },
            );
            gate_reasons.push("required_document_missing".to_string());
        }
    }

    for document in &documents {
        let matched_scope_reasons = scope_reasons(document, work);
        let is_required = explicit_required.contains(&document.id);
        let should_evaluate = is_required || !matched_scope_reasons.is_empty();
        let mut reasons = Vec::new();
        if is_required {
            reasons.push("explicit_required_document".to_string());
        }
        reasons.extend(matched_scope_reasons);
        reasons = sort_reasons(reasons);

        let ineligible = ineligible_reasons(document, resolver, options);
        if !ineligible.reason_codes.is_empty() {
            if should_evaluate || should_always_exclude(document, &ineligible.reason_codes) {
                push_excluded(
                    &mut excluded_by_id,
                    ExcludedDocument {
                        id: document.id.clone(),
                        path: Some(document.path.clone()),
                        reason_codes: sort_reasons(ineligible.reason_codes.clone()),
                        replacement: document.superseded_by.clone(),
                    },
                );
            }
            if is_required {
                gate_reasons.extend(required_gate_reasons(&ineligible.reason_codes));
                if document.lifecycle == DocumentLifecycle::Superseded {
                    if let Some(replacement_id) = &document.superseded_by {
                        match by_id.get(replacement_id) {
                            Some(replacement) => {
                                let mut replacement_reasons = vec![
                                    "supersession_replacement".to_string(),
                                    "explicit_required_document_replacement".to_string(),
                                ];
                                replacement_reasons.extend(scope_reasons(replacement, work));
                                let replacement_ineligible =
                                    ineligible_reasons(replacement, resolver, options);
                                if replacement_ineligible.reason_codes.is_empty() {
                                    if let ContentState::Present { hash } =
                                        resolver.resolve(replacement)
                                    {
                                        optional.push(applicable_document(
                                            replacement,
                                            hash,
                                            sort_reasons(replacement_reasons),
                                        ));
                                    }
                                } else {
                                    push_excluded(
                                        &mut excluded_by_id,
                                        ExcludedDocument {
                                            id: replacement.id.clone(),
                                            path: Some(replacement.path.clone()),
                                            reason_codes: sort_reasons(
                                                replacement_ineligible.reason_codes,
                                            ),
                                            replacement: replacement.superseded_by.clone(),
                                        },
                                    );
                                }
                            }
                            None => gate_reasons.push("required_document_missing".to_string()),
                        }
                    }
                }
            }
            continue;
        }

        if !should_evaluate {
            continue;
        }

        let hash = match resolver.resolve(document) {
            ContentState::Present { hash } => hash,
            _ => continue,
        };
        if is_required {
            required.push(applicable_document(document, hash, reasons.clone()));
            write_candidates.push(WriteCandidate {
                id: document.id.clone(),
                reasons: sort_reasons(vec![
                    "impact_required".to_string(),
                    "explicit_required_document".to_string(),
                ]),
            });
        } else {
            optional.push(applicable_document(document, hash, reasons));
        }
    }

    required.sort_by(|a, b| a.id.cmp(&b.id));
    optional.sort_by(|a, b| a.id.cmp(&b.id));
    optional.dedup_by(|a, b| a.id == b.id);
    write_candidates.sort_by(|a, b| a.id.cmp(&b.id));
    write_candidates.dedup_by(|a, b| a.id == b.id);
    let excluded = excluded_by_id.into_values().collect();
    let gate_reasons = sort_reasons(gate_reasons);
    let gate_status = if gate_reasons.is_empty() {
        "complete"
    } else {
        "incomplete"
    };

    Ok(ApplicableDocsReport {
        schema_version: 1,
        work: ApplicableWork {
            id: work.work_id.clone(),
            revision: work.revision,
            documentation_posture: work.posture,
        },
        registry: ApplicableRegistry {
            revision: registry.revision,
            fingerprint: registry_fingerprint(registry)?,
        },
        required,
        optional,
        write_candidates,
        excluded,
        gate: ApplicabilityGate {
            status: gate_status.to_string(),
            reason_codes: gate_reasons,
            policy_status: "not_evaluated".to_string(),
        },
    })
}

fn applicable_document(
    document: &DocumentRecord,
    content_hash: String,
    reasons: Vec<String>,
) -> ApplicableDocument {
    ApplicableDocument {
        id: document.id.clone(),
        path: document.path.clone(),
        kind: document.kind,
        authority: document.authority,
        owner: document.owner.clone(),
        summary: document.summary.clone(),
        content_hash,
        document_revision: document.revision,
        reasons,
    }
}

#[derive(Debug, Clone)]
struct Ineligible {
    reason_codes: Vec<String>,
}

fn ineligible_reasons(
    document: &DocumentRecord,
    resolver: &dyn ContentResolver,
    options: ApplicabilityOptions,
) -> Ineligible {
    let mut reasons = Vec::new();
    match document.lifecycle {
        DocumentLifecycle::Current => {}
        DocumentLifecycle::SuspectedStale => {
            if !options.include_stale {
                reasons.push("document_suspected_stale".to_string());
            }
        }
        DocumentLifecycle::Stale => {
            if !options.include_stale {
                reasons.push("document_stale".to_string());
            }
        }
        DocumentLifecycle::Retired => reasons.push("document_retired".to_string()),
        DocumentLifecycle::Superseded => reasons.push("document_superseded".to_string()),
    }
    match document.authority {
        DocumentAuthority::Approved | DocumentAuthority::Generated => {}
        DocumentAuthority::Draft => {
            if !options.include_draft {
                reasons.push("document_draft".to_string());
            }
        }
        DocumentAuthority::Informational => reasons.push("document_not_authoritative".to_string()),
    }
    if is_protected_path(&document.path) {
        reasons.push("document_protected".to_string());
    }
    if is_generated_navigation_path(&document.path) {
        reasons.push("document_generated_navigation".to_string());
    }
    match resolver.resolve(document) {
        ContentState::Present { .. } => {}
        ContentState::Missing => reasons.push("document_content_missing".to_string()),
        ContentState::Protected => reasons.push("document_protected".to_string()),
        ContentState::Unsafe => reasons.push("document_path_unsafe".to_string()),
    }
    Ineligible {
        reason_codes: sort_reasons(reasons),
    }
}

fn scope_reasons(document: &DocumentRecord, work: &WorkDocumentationContext) -> Vec<String> {
    let mut reasons = Vec::new();
    if any_path_scope_matches(&document.scope.paths, &work.paths) {
        reasons.push("path_scope_match".to_string());
    }
    if intersects(&document.scope.domains, &work.domains) {
        reasons.push("domain_scope_match".to_string());
    }
    if intersects(&document.scope.work_labels, &work.labels) {
        reasons.push("label_scope_match".to_string());
    }
    sort_reasons(reasons)
}

fn any_path_scope_matches(patterns: &[String], paths: &[String]) -> bool {
    patterns.iter().any(|pattern| {
        paths.iter().any(|path| {
            glob_match(pattern, path)
                || glob_match(path, pattern)
                || pattern == path
                || pattern.trim_end_matches("/**") == path.trim_end_matches("/**")
        })
    })
}

fn intersects(left: &[String], right: &[String]) -> bool {
    let right: BTreeSet<_> = right.iter().collect();
    left.iter().any(|item| right.contains(item))
}

fn glob_match(pattern: &str, value: &str) -> bool {
    if pattern == "**" || pattern == value {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return value == prefix || value.starts_with(&format!("{prefix}/"));
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return value.starts_with(prefix);
    }
    false
}

fn should_always_exclude(document: &DocumentRecord, reasons: &[String]) -> bool {
    document.lifecycle == DocumentLifecycle::Retired
        || document.lifecycle == DocumentLifecycle::Superseded
        || reasons.iter().any(|reason| {
            matches!(
                reason.as_str(),
                "document_generated_navigation" | "document_protected"
            )
        })
}

fn required_gate_reasons(ineligible: &[String]) -> Vec<String> {
    ineligible
        .iter()
        .map(|reason| match reason.as_str() {
            "document_content_missing" => "required_document_missing",
            "document_stale" | "document_suspected_stale" => "required_document_stale",
            "document_draft" | "document_not_authoritative" => {
                "required_document_not_authoritative"
            }
            "document_retired" => "required_document_retired",
            "document_superseded" => "required_document_superseded",
            "document_protected" | "document_generated_navigation" | "document_path_unsafe" => {
                "required_document_ineligible"
            }
            _ => reason.as_str(),
        })
        .map(str::to_string)
        .collect()
}

fn push_excluded(excluded: &mut BTreeMap<String, ExcludedDocument>, item: ExcludedDocument) {
    excluded
        .entry(item.id.clone())
        .and_modify(|existing| {
            existing.reason_codes.extend(item.reason_codes.clone());
            existing.reason_codes = sort_reasons(existing.reason_codes.clone());
            if existing.replacement.is_none() {
                existing.replacement = item.replacement.clone();
            }
            if existing.path.is_none() {
                existing.path = item.path.clone();
            }
        })
        .or_insert(item);
}

pub fn sort_reasons(reasons: Vec<String>) -> Vec<String> {
    let mut reasons = reasons;
    reasons.sort_by(|a, b| {
        let left = reason_precedence(a);
        let right = reason_precedence(b);
        left.cmp(&right).then_with(|| a.cmp(b))
    });
    reasons.dedup();
    reasons
}

fn reason_precedence(reason: &str) -> usize {
    REASON_PRECEDENCE
        .iter()
        .position(|candidate| *candidate == reason)
        .unwrap_or(REASON_PRECEDENCE.len())
}

const REASON_PRECEDENCE: &[&str] = &[
    "documentation_impact_unknown",
    "impact_required",
    "explicit_required_document",
    "explicit_required_document_replacement",
    "supersession_replacement",
    "path_scope_match",
    "domain_scope_match",
    "label_scope_match",
    "required_document_missing",
    "required_document_stale",
    "required_document_not_authoritative",
    "required_document_retired",
    "required_document_superseded",
    "required_document_ineligible",
    "document_content_missing",
    "document_stale",
    "document_suspected_stale",
    "document_draft",
    "document_not_authoritative",
    "document_retired",
    "document_superseded",
    "document_protected",
    "document_generated_navigation",
    "document_path_unsafe",
];

pub fn is_protected_path(path: &str) -> bool {
    path == ".pulse/migrations/docs-backups" || path.starts_with(".pulse/migrations/docs-backups/")
}

pub fn is_generated_navigation_path(path: &str) -> bool {
    path.starts_with("docs/") && (path == "docs/_index.md" || path.ends_with("/_index.md"))
}
