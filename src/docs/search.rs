use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::docs::cache::{classify_against, CacheState};
use crate::docs::index::{build_index, current_generation, index_status, IndexOptions};
use crate::docs::lexical::{
    load_section_records, query as query_lexical, tokenize_query_text, SNIPPET_MAX_BYTES,
};
use crate::docs::model::{DocumentAuthority, DocumentKind};
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SearchReport {
    pub schema_version: u32,
    pub query: String,
    pub normalized_terms: Vec<String>,
    pub index: SearchIndexInfo,
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

    let status = index_status(repo_root)?;
    let generation = match current_generation(repo_root)? {
        Some(generation) if status.index.state == "current" => generation,
        _ if options.no_refresh => {
            return Err(PulseError::validation(
                "docs_index_stale",
                "docs-search index is not current and --no-refresh was requested",
            ));
        }
        _ => {
            build_index(repo_root, IndexOptions::default())?;
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
    if state != CacheState::Current && options.no_refresh {
        return Err(PulseError::validation(
            "docs_index_stale",
            "docs-search index is stale",
        ));
    }
    let limit = options.limit.unwrap_or(8).clamp(1, 50);
    let hits = query_lexical(&generation.tantivy_path, &terms, limit * 4)?;
    let sections = load_section_records(&generation.sections_path)?;
    let sections_by_ref = sections
        .into_iter()
        .map(|section| (section.section_ref.clone(), section))
        .collect::<BTreeMap<_, _>>();
    let mut results = Vec::new();
    let mut returned_snippet_bytes = 0usize;
    for hit in hits {
        let Some(section) = sections_by_ref.get(&hit.section_ref) else {
            continue;
        };
        if !matches_filters(section, &options) {
            continue;
        }
        let snippet = snippet_for_section(repo_root, section, &terms)?;
        returned_snippet_bytes += snippet.len();
        results.push(SearchResult {
            rank: results.len() as u32 + 1,
            score: hit.score,
            lexical_score: hit.score,
            section_ref: section.section_ref.clone(),
            document_id: section.document_id.clone(),
            document_revision: section.document_revision,
            path: section.path.clone(),
            heading_path: section.heading_path.clone(),
            range: section.range,
            document_content_hash: section.document_content_hash.clone(),
            section_content_hash: section.section_content_hash.clone(),
            summary: section.summary.clone(),
            snippet,
            authority: section.authority.clone(),
            lifecycle: section.lifecycle.clone(),
            owner: section.owner.clone(),
            kind: section.kind.clone(),
            matched_fields: hit.matched_fields,
            applicability_reasons: Vec::new(),
        });
        if results.len() >= limit {
            break;
        }
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
        results,
        budget: SearchBudget {
            result_limit: limit,
            snippet_max_bytes: SNIPPET_MAX_BYTES,
            returned_snippet_bytes,
        },
    })
}

fn matches_filters(section: &SectionRecord, options: &SearchOptions) -> bool {
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
    let text = std::fs::read_to_string(&path).map_err(|e| PulseError::io(&path, e))?;
    let mut lines = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let line_no = (idx as u32) + 1;
        if line_no >= section.range.start_line && line_no <= section.range.end_line {
            lines.push(line.to_string());
        }
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
    let mut snippet = lines[begin..end].join("\n");
    if snippet.len() > SNIPPET_MAX_BYTES {
        snippet.truncate(SNIPPET_MAX_BYTES);
    }
    Ok(snippet)
}

fn serde_variant<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}
