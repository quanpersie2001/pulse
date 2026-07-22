use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::docs::model::{
    DocumentAuthority, DocumentKind, DocumentationPosture, WorkDocumentationContext,
};
use crate::docs::search::{search_docs, SearchOptions};
use crate::{PulseError, PulseResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RetrievalEvalFixture {
    pub id: String,
    pub query: String,
    #[serde(default)]
    pub filters: RetrievalEvalFilters,
    #[serde(default)]
    pub work_context: Option<RetrievalEvalWorkContext>,
    pub expected: RetrievalEvalExpected,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RetrievalEvalFilters {
    pub domain: Option<String>,
    pub kind: Option<DocumentKind>,
    pub authority: Option<DocumentAuthority>,
    pub limit: Option<usize>,
    #[serde(default)]
    pub explain: bool,
    #[serde(default)]
    pub include_draft: bool,
    #[serde(default)]
    pub include_stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RetrievalEvalWorkContext {
    #[serde(default = "default_eval_work_id")]
    pub work_id: String,
    #[serde(default = "default_eval_work_revision")]
    pub revision: u64,
    #[serde(default = "default_eval_posture")]
    pub posture: DocumentationPosture,
    #[serde(default)]
    pub required_documents: Vec<String>,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub labels: Vec<String>,
}

impl From<&RetrievalEvalWorkContext> for WorkDocumentationContext {
    fn from(value: &RetrievalEvalWorkContext) -> Self {
        Self {
            work_id: value.work_id.clone(),
            revision: value.revision,
            posture: value.posture,
            required_documents: value.required_documents.clone(),
            paths: value.paths.clone(),
            domains: value.domains.clone(),
            labels: value.labels.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RetrievalEvalExpected {
    #[serde(default)]
    pub top_k: Vec<String>,
    #[serde(default)]
    pub must_exclude: Vec<String>,
    #[serde(default)]
    pub reason_codes: Vec<String>,
    pub max_first_relevant_rank: Option<u32>,
    pub max_context_bytes_before_first_relevant: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RetrievalEvalReport {
    pub schema_version: u32,
    pub fixture_count: usize,
    pub passed: bool,
    pub recall_at_k: f64,
    pub mean_reciprocal_rank: f64,
    pub must_exclude_violations: usize,
    pub results: Vec<RetrievalEvalResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RetrievalEvalResult {
    pub id: String,
    pub passed: bool,
    pub first_relevant_rank: Option<u32>,
    pub reciprocal_rank: f64,
    pub relevant_found: Vec<String>,
    pub must_exclude_hits: Vec<String>,
    pub context_bytes_before_first_relevant: usize,
    pub reason_codes: Vec<String>,
}

pub fn load_retrieval_eval_fixtures(path: &Path) -> PulseResult<Vec<RetrievalEvalFixture>> {
    let text = std::fs::read_to_string(path).map_err(|e| PulseError::io(path, e))?;
    let mut fixtures = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let fixture: RetrievalEvalFixture = serde_json::from_str(line).map_err(|e| {
            PulseError::validation(
                "docs_retrieval_eval_invalid",
                format!("invalid retrieval eval fixture line {}: {e}", idx + 1),
            )
        })?;
        fixtures.push(fixture);
    }
    Ok(fixtures)
}

pub fn run_retrieval_evals(
    repo_root: &Path,
    fixture_path: &Path,
) -> PulseResult<RetrievalEvalReport> {
    let fixtures = load_retrieval_eval_fixtures(fixture_path)?;
    run_retrieval_eval_fixtures(repo_root, &fixtures)
}

pub fn run_retrieval_eval_fixtures(
    repo_root: &Path,
    fixtures: &[RetrievalEvalFixture],
) -> PulseResult<RetrievalEvalReport> {
    let mut results = Vec::new();
    let mut total_relevant = 0usize;
    let mut total_found = 0usize;
    let mut rr_sum = 0.0f64;
    let mut must_exclude_violations = 0usize;

    for fixture in fixtures {
        let search = search_docs(
            repo_root,
            &fixture.query,
            SearchOptions {
                kind: fixture.filters.kind,
                domain: fixture.filters.domain.clone(),
                authority: fixture.filters.authority,
                limit: fixture.filters.limit,
                explain: fixture.filters.explain,
                include_draft: fixture.filters.include_draft,
                include_stale: fixture.filters.include_stale,
                work: fixture
                    .work_context
                    .as_ref()
                    .map(WorkDocumentationContext::from),
                ..SearchOptions::default()
            },
        )?;
        let hit_refs = search
            .results
            .iter()
            .map(|result| result.section_ref.clone())
            .collect::<Vec<_>>();
        let mut relevant_found = Vec::new();
        let mut first_relevant_rank = None;
        let mut context_bytes_before_first_relevant = 0usize;
        for expected in &fixture.expected.top_k {
            total_relevant += 1;
            if let Some(index) = hit_refs.iter().position(|hit| hit == expected) {
                total_found += 1;
                relevant_found.push(expected.clone());
                let rank = index as u32 + 1;
                if first_relevant_rank.map_or(true, |current| rank < current) {
                    first_relevant_rank = Some(rank);
                    context_bytes_before_first_relevant = search
                        .results
                        .iter()
                        .take(index)
                        .map(|result| result.snippet.len())
                        .sum();
                }
            }
        }
        let must_exclude_hits = fixture
            .expected
            .must_exclude
            .iter()
            .filter(|excluded| hit_refs.iter().any(|hit| hit == *excluded))
            .cloned()
            .collect::<Vec<_>>();
        must_exclude_violations += must_exclude_hits.len();
        let reciprocal_rank = first_relevant_rank
            .map(|rank| 1.0 / rank as f64)
            .unwrap_or(0.0);
        rr_sum += reciprocal_rank;
        let mut reason_codes = Vec::new();
        if let Some(max_rank) = fixture.expected.max_first_relevant_rank {
            if first_relevant_rank.map_or(true, |rank| rank > max_rank) {
                reason_codes.push("docs_search_miss".to_string());
            }
        }
        if let Some(max_bytes) = fixture.expected.max_context_bytes_before_first_relevant {
            if context_bytes_before_first_relevant > max_bytes {
                reason_codes.push("docs_context_bloat".to_string());
            }
        }
        if !must_exclude_hits.is_empty() {
            reason_codes.push("docs_search_noise".to_string());
        }
        let passed = reason_codes_match(&reason_codes, &fixture.expected.reason_codes);
        results.push(RetrievalEvalResult {
            id: fixture.id.clone(),
            passed,
            first_relevant_rank,
            reciprocal_rank,
            relevant_found,
            must_exclude_hits,
            context_bytes_before_first_relevant,
            reason_codes,
        });
    }
    let recall_at_k = if total_relevant == 0 {
        1.0
    } else {
        total_found as f64 / total_relevant as f64
    };
    let mean_reciprocal_rank = if fixtures.is_empty() {
        0.0
    } else {
        rr_sum / fixtures.len() as f64
    };
    let passed = results.iter().all(|result| result.passed) && must_exclude_violations == 0;
    Ok(RetrievalEvalReport {
        schema_version: 1,
        fixture_count: fixtures.len(),
        passed,
        recall_at_k,
        mean_reciprocal_rank,
        must_exclude_violations,
        results,
    })
}

fn reason_codes_match(observed: &[String], expected: &[String]) -> bool {
    let mut observed = observed.to_vec();
    observed.sort();
    observed.dedup();
    let mut expected = expected.to_vec();
    expected.sort();
    expected.dedup();
    observed == expected
}

fn default_eval_work_id() -> String {
    "TK-EVAL".to_string()
}

fn default_eval_work_revision() -> u64 {
    1
}

fn default_eval_posture() -> DocumentationPosture {
    DocumentationPosture::Required
}
