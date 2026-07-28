use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::canonical_json::hash_bytes;
use crate::docs::applicability::{applicable_docs, ApplicabilityOptions, FsContentResolver};
use crate::docs::cache::{classify_against, CacheState};
use crate::docs::index::{
    build_search_cache, cache_state_error_code, current_generation, index_status, IndexOptions,
};
use crate::docs::lexical::{query as query_lexical, tokenize_query_text, SNIPPET_MAX_BYTES};
use crate::docs::model::{DocumentAuthority, DocumentKind, WorkDocumentationContext};
use crate::docs::registry::load_registry;
use crate::docs::section::{SectionRange, SectionRecord};
use crate::{PulseError, PulseResult};

#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    pub kind: Option<DocumentKind>,
    pub domain: Option<String>,
    pub authority: Option<DocumentAuthority>,
    pub limit: Option<usize>,
    pub no_refresh: bool,
    pub explain: bool,
    pub include_draft: bool,
    pub include_stale: bool,
    pub work: Option<WorkDocumentationContext>,
    /// Internal callers that already hold the repository write fence cannot use
    /// the default search path because registry/applicability helpers acquire
    /// that same non-reentrant fence. When true, search uses read-only registry
    /// loads, refuses auto-refresh, and assumes the caller has already made the
    /// cache current and will revalidate the graph/docs snapshot before using
    /// the results.
    pub under_repository_fence: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SearchReport {
    pub schema_version: u32,
    pub query: String,
    pub normalized_terms: Vec<String>,
    pub index: SearchIndexInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work: Option<SearchWorkInfo>,
    pub results: Vec<SearchResult>,
    pub budget: SearchBudget,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SearchIndexInfo {
    pub fingerprint: Option<String>,
    pub generation_id: Option<String>,
    pub state: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SearchWorkInfo {
    pub id: String,
    pub revision: u64,
    pub documentation_posture: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SearchResult {
    pub rank: u32,
    pub score: f64,
    pub lexical_score: f64,
    pub section_ref: String,
    pub document_id: String,
    pub document_revision: u64,
    pub path: String,
    pub heading_path: Vec<String>,
    pub range: SectionRange,
    pub document_content_hash: String,
    pub section_content_hash: String,
    pub summary: String,
    pub snippet: String,
    pub authority: String,
    pub lifecycle: String,
    pub owner: String,
    pub kind: String,
    pub matched_fields: Vec<String>,
    pub applicability_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SearchBudget {
    pub result_limit: usize,
    pub snippet_max_bytes: usize,
    pub returned_snippet_bytes: usize,
}

pub fn search_docs(
    repo_root: &Path,
    query: &str,
    options: SearchOptions,
) -> PulseResult<SearchReport> {
    let trimmed = query.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 256 {
        return Err(PulseError::validation(
            "docs_search_query_invalid",
            "query must be non-empty and <=256 characters",
        ));
    }
    let terms = tokenize_query_text(trimmed);
    if terms.is_empty() || terms.len() > 32 {
        return Err(PulseError::validation(
            "docs_search_query_invalid",
            "query produced no valid terms or too many terms",
        ));
    }

    let build_options = crate::docs::policy::RetrievalEligibilityOptions {
        include_draft: options.include_draft,
        include_stale: options.include_stale,
    };
    let status = if options.under_repository_fence || options.include_draft || options.include_stale
    {
        crate::docs::index::index_status_with_options(repo_root, build_options)?
    } else {
        index_status(repo_root)?
    };
    let refresh_options = IndexOptions {
        changed: false,
        rebuild: options.include_draft || options.include_stale,
        check: false,
        include_draft: options.include_draft,
        include_stale: options.include_stale,
    };
    let generation = match current_generation(repo_root) {
        Ok(Some(generation)) if status.index.state == "current" => generation,
        _ if options.no_refresh || options.under_repository_fence => {
            return Err(PulseError::validation(
                cache_state_error_code(&status.index.state),
                format!(
                    "docs-search index is {} and refresh is disabled",
                    status.index.state
                ),
            ));
        }
        _ => {
            crate::docs::index::ensure_auto_refresh_allowed(repo_root)?;
            build_search_cache(repo_root, refresh_options)?;
            current_generation(repo_root)?.ok_or_else(|| {
                PulseError::validation(
                    "docs_index_missing",
                    "docs-search generation missing after build",
                )
            })?
        }
    };

    // Reclassify against current status after any refresh.
    let state = classify_against(repo_root, &generation.state.fingerprint)?.0;
    if state != CacheState::Current {
        if options.no_refresh || options.under_repository_fence {
            return Err(PulseError::validation(
                cache_state_error_code(state.as_str()),
                format!("docs-search index is {}", state.as_str()),
            ));
        }
        crate::docs::index::ensure_auto_refresh_allowed(repo_root)?;
        build_search_cache(repo_root, refresh_options)?;
    }
    let generation = current_generation(repo_root)?.ok_or_else(|| {
        PulseError::validation(
            "docs_index_missing",
            "docs-search generation missing after refresh",
        )
    })?;
    let registry = if options.under_repository_fence {
        crate::storage::read_json(&crate::docs::registry::registry_path(repo_root))?
    } else {
        load_registry(repo_root)?
    };
    let default_limit = registry.retrieval_config().default_search_limit as usize;
    let limit = options.limit.unwrap_or(default_limit).clamp(1, 50);
    let applicability = applicability_by_document(repo_root, &options)?;
    let hits = query_lexical(
        &generation.tantivy_path,
        &sanitized_terms(&terms),
        limit * 4,
    )?;
    let mut candidates = Vec::new();
    for hit in hits {
        if !matches_filters(&hit.section, &options) {
            continue;
        }
        let applicability_reasons = applicability
            .get(&hit.section.document_id)
            .cloned()
            .unwrap_or_default();
        let score = adjusted_score(hit.score, &applicability_reasons, options.work.is_some());
        candidates.push(SearchCandidate {
            score,
            lexical_score: hit.score,
            section: hit.section,
            matched_fields: hit.matched_fields,
            applicability_reasons,
        });
    }
    candidates.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| b.lexical_score.total_cmp(&a.lexical_score))
            .then_with(|| a.section.section_ref.cmp(&b.section.section_ref))
    });
    candidates.truncate(limit);

    let mut results = Vec::new();
    let mut returned_snippet_bytes = 0usize;
    for (idx, candidate) in candidates.into_iter().enumerate() {
        let snippet = snippet_for_section(repo_root, &candidate.section, &terms)?;
        returned_snippet_bytes += snippet.len();
        results.push(SearchResult {
            rank: idx as u32 + 1,
            score: candidate.score,
            lexical_score: candidate.lexical_score,
            section_ref: candidate.section.section_ref,
            document_id: candidate.section.document_id,
            document_revision: candidate.section.document_revision,
            path: candidate.section.path,
            heading_path: candidate.section.heading_path,
            range: candidate.section.range,
            document_content_hash: candidate.section.document_content_hash,
            section_content_hash: candidate.section.section_content_hash,
            summary: candidate.section.summary,
            snippet,
            authority: candidate.section.authority,
            lifecycle: candidate.section.lifecycle,
            owner: candidate.section.owner,
            kind: candidate.section.kind,
            matched_fields: if options.explain {
                candidate.matched_fields
            } else {
                Vec::new()
            },
            applicability_reasons: if options.explain || options.work.is_some() {
                candidate.applicability_reasons
            } else {
                Vec::new()
            },
        });
    }
    Ok(SearchReport {
        schema_version: 1,
        query: trimmed.to_string(),
        normalized_terms: terms,
        index: SearchIndexInfo {
            fingerprint: Some(generation.state.fingerprint.clone()),
            generation_id: Some(generation.state.generation_id.clone()),
            state: CacheState::Current.as_str().to_string(),
            mode: "lexical".to_string(),
        },
        work: options.work.as_ref().map(|work| SearchWorkInfo {
            id: work.work_id.clone(),
            revision: work.revision,
            documentation_posture: serde_variant(&work.posture),
        }),
        results,
        budget: SearchBudget {
            result_limit: limit,
            snippet_max_bytes: SNIPPET_MAX_BYTES,
            returned_snippet_bytes,
        },
    })
}

fn applicability_by_document(
    repo_root: &Path,
    options: &SearchOptions,
) -> PulseResult<BTreeMap<String, Vec<String>>> {
    let Some(work) = &options.work else {
        return Ok(BTreeMap::new());
    };
    let registry = if options.under_repository_fence {
        crate::storage::read_json(&crate::docs::registry::registry_path(repo_root))?
    } else {
        load_registry(repo_root)?
    };
    let resolver = FsContentResolver::new(repo_root);
    let report = applicable_docs(
        work,
        &registry,
        &resolver,
        ApplicabilityOptions {
            include_draft: options.include_draft,
            include_stale: options.include_stale,
        },
    )?;
    let mut out = BTreeMap::new();
    for doc in report.required {
        out.insert(doc.id, doc.reasons);
    }
    for doc in report.optional {
        out.entry(doc.id).or_insert(doc.reasons);
    }
    Ok(out)
}

fn sanitized_terms(terms: &[String]) -> Vec<String> {
    terms
        .iter()
        .map(|term| term.replace(':', " "))
        .flat_map(|term| tokenize_query_text(&term))
        .collect()
}

#[derive(Debug)]
struct SearchCandidate {
    score: f64,
    lexical_score: f64,
    section: SectionRecord,
    matched_fields: Vec<String>,
    applicability_reasons: Vec<String>,
}

fn adjusted_score(lexical_score: f64, reasons: &[String], apply_work_boost: bool) -> f64 {
    if !apply_work_boost || lexical_score <= 0.0 {
        return lexical_score;
    }
    let boost_fraction = reasons
        .iter()
        .map(|reason| match reason.as_str() {
            "explicit_required_document" => 0.20,
            "impact_required" => 0.20,
            "explicit_required_document_replacement" => 0.16,
            "supersession_replacement" => 0.12,
            "path_scope_match" => 0.12,
            "domain_scope_match" => 0.10,
            "label_scope_match" => 0.08,
            _ => 0.0,
        })
        .sum::<f64>()
        .min(0.20);
    lexical_score + (lexical_score * boost_fraction)
}

fn matches_filters(section: &SectionRecord, options: &SearchOptions) -> bool {
    if !options.include_draft && section.authority == "draft" {
        return false;
    }
    if !options.include_stale
        && (section.lifecycle == "stale" || section.lifecycle == "suspected_stale")
    {
        return false;
    }
    if let Some(kind) = options.kind {
        if section.kind != serde_variant(&kind) {
            return false;
        }
    }
    if let Some(authority) = options.authority {
        if section.authority != serde_variant(&authority) {
            return false;
        }
    }
    if let Some(domain) = &options.domain {
        if !section.domains.iter().any(|d| d == domain) {
            return false;
        }
    }
    true
}

fn snippet_for_section(
    repo_root: &Path,
    section: &SectionRecord,
    terms: &[String],
) -> PulseResult<String> {
    let path = repo_root.join(crate::storage::safe_repo_relative(&section.path)?);
    let bytes = std::fs::read(&path).map_err(|e| PulseError::io(&path, e))?;
    if hash_bytes(&bytes) != section.document_content_hash {
        return indexed_snippet(section, terms);
    }
    let text = String::from_utf8(bytes).map_err(|e| {
        PulseError::validation(
            "utf8_error",
            format!("document is not valid UTF-8 for snippet: {e}"),
        )
    })?;
    snippet_from_text(&text, section, terms)
}

fn indexed_snippet(section: &SectionRecord, terms: &[String]) -> PulseResult<String> {
    let text = [
        section.heading_path.join(" "),
        section.summary.clone(),
        section.aliases.join(" "),
        section.domains.join(" "),
    ]
    .into_iter()
    .filter(|part| !part.trim().is_empty())
    .collect::<Vec<_>>()
    .join("\n");
    snippet_from_text(&text, section, terms)
}

fn snippet_from_text(text: &str, section: &SectionRecord, terms: &[String]) -> PulseResult<String> {
    let mut lines = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let line_no = (idx as u32) + 1;
        if line_no >= section.range.start_line && line_no <= section.range.end_line {
            lines.push(line.to_string());
        }
    }
    if lines.is_empty() {
        lines = text.lines().map(str::to_string).collect();
    }
    let start_idx = lines
        .iter()
        .position(|line| {
            let lower = line.to_lowercase();
            terms.iter().any(|term| lower.contains(term))
        })
        .unwrap_or(0);
    let begin = start_idx.saturating_sub(1);
    let end = (start_idx + 4).min(lines.len());
    let snippet = lines[begin..end].join("\n");
    Ok(truncate_utf8(&snippet, SNIPPET_MAX_BYTES))
}

fn truncate_utf8(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

fn serde_variant<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}
