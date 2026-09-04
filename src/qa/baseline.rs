//! Story QA baseline parsing, validation and execution-scope case resolution.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::canonical_json::hash_bytes;
use crate::graph::model::contract::QaImpactPosture;
use crate::graph::model::node::Node;
use crate::id::WorkKind;
use crate::{PulseError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaBaseline {
    pub schema_version: u32,
    pub story_id: String,
    pub revision: u64,
    pub scope: String,
    #[serde(default)]
    pub risks: Vec<String>,
    pub cases: Vec<QaCase>,
    pub exit_criteria: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaCase {
    pub id: String,
    pub revision: u64,
    pub intent: String,
    pub priority: QaCasePriority,
    #[serde(default)]
    pub risk_refs: Vec<String>,
    pub steps: Vec<String>,
    pub expected: Vec<String>,
    pub surface: String,
    pub applicability: QaCaseApplicability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub non_applicable_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QaCasePriority {
    Critical,
    High,
    Normal,
    Low,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QaCaseApplicability {
    Required,
    NotApplicable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QaBaselineResolution {
    pub owner_id: String,
    pub path: String,
    pub revision: u64,
    pub content_hash: String,
    pub cases: Vec<QaCase>,
}

/// Load and semantically validate the canonical baseline owned by a Story.
///
/// # Errors
///
/// Returns a typed validation error when `qa.md` is missing, malformed, has
/// incomplete coverage, or declares a different Story identity.
pub fn load_story_baseline(repo_root: &Path, story_id: &str) -> Result<QaBaselineResolution> {
    let relative = format!("works/{story_id}/qa.md");
    let path = repo_root.join(&relative);
    let bytes = fs::read(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            PulseError::validation(
                "qa_baseline_missing",
                format!("behavioral owner {story_id} has no {relative}"),
            )
        } else {
            PulseError::io(&path, error)
        }
    })?;
    let markdown = std::str::from_utf8(&bytes).map_err(|_| {
        PulseError::validation("qa_baseline_invalid", "Story QA baseline must be UTF-8")
    })?;
    let json = extract_contract_block(markdown)?;
    let mut baseline: QaBaseline = serde_json::from_str(json).map_err(|error| {
        PulseError::validation(
            "qa_baseline_invalid",
            format!("invalid pulse-qa contract in {relative}: {error}"),
        )
    })?;
    normalize_baseline(&mut baseline);
    validate_baseline(&baseline, story_id)?;
    Ok(QaBaselineResolution {
        owner_id: story_id.to_string(),
        path: relative,
        revision: baseline.revision,
        content_hash: hash_bytes(&bytes),
        cases: baseline.cases,
    })
}

/// Resolve every currently applicable case in a Story qualification baseline.
///
/// # Errors
///
/// Returns a typed validation error when the Story or baseline is invalid, or
/// when the baseline has no applicable cases to qualify.
pub fn resolve_story_cases(repo_root: &Path, story_id: &str) -> Result<QaBaselineResolution> {
    validate_behavioral_owner(repo_root, story_id)?;
    let mut baseline = load_story_baseline(repo_root, story_id)?;
    baseline
        .cases
        .retain(|case| case.applicability == QaCaseApplicability::Required);
    if baseline.cases.is_empty() {
        return Err(PulseError::validation(
            "qa_story_cases_empty",
            "Story qualification requires at least one applicable baseline case",
        ));
    }
    Ok(baseline)
}

/// Resolve a required Ticket QA impact to exact current Story case revisions.
///
/// # Errors
///
/// Returns a typed validation error when the owner Story/baseline is missing,
/// an affected case is absent or non-applicable, or the posture is not
/// `required`.
pub fn resolve_ticket_cases(repo_root: &Path, ticket: &Node) -> Result<QaBaselineResolution> {
    let qa = ticket.qa.as_ref().ok_or_else(|| {
        PulseError::validation(
            "qa_impact_unknown",
            "Ticket QA impact has not been assessed",
        )
    })?;
    if qa.impact.posture != QaImpactPosture::Required {
        return Err(PulseError::validation(
            "qa_checkpoint_not_required",
            "case resolution is only defined for required QA impact",
        ));
    }
    let owner = qa.impact.behavioral_owner.as_deref().ok_or_else(|| {
        PulseError::validation(
            "qa_behavioral_owner_missing",
            "required QA impact needs a behavioral owner",
        )
    })?;
    validate_behavioral_owner(repo_root, owner)?;
    let mut baseline = load_story_baseline(repo_root, owner)?;
    let by_id = baseline
        .cases
        .iter()
        .map(|case| (case.id.as_str(), case))
        .collect::<BTreeMap<_, _>>();
    let mut selected = Vec::new();
    for id in &qa.impact.affected_case_ids {
        let case = by_id.get(id.as_str()).ok_or_else(|| {
            PulseError::validation(
                "qa_case_missing",
                format!("affected QA case {id} does not exist in {owner} baseline"),
            )
        })?;
        if case.applicability != QaCaseApplicability::Required {
            return Err(PulseError::validation(
                "qa_case_not_applicable",
                format!("affected QA case {id} is explicitly not applicable"),
            ));
        }
        selected.push((*case).clone());
    }
    if selected.is_empty() {
        return Err(PulseError::validation(
            "qa_case_selection_empty",
            "required QA impact must select at least one current baseline case",
        ));
    }
    baseline.cases = selected;
    Ok(baseline)
}

fn validate_behavioral_owner(repo_root: &Path, owner: &str) -> Result<()> {
    let path = repo_root
        .join(".pulse/workgraph/nodes")
        .join(format!("{owner}.json"));
    if !path.exists() {
        return Err(PulseError::validation(
            "qa_behavioral_owner_missing",
            format!("behavioral owner {owner} is not present in the work graph"),
        ));
    }
    let node: Node = crate::storage::read_json(&path)?;
    if node.kind != WorkKind::Story {
        return Err(PulseError::validation(
            "qa_behavioral_owner_invalid",
            format!("behavioral owner {owner} is not a Story"),
        ));
    }
    Ok(())
}

fn extract_contract_block(markdown: &str) -> Result<&str> {
    let mut blocks = markdown.match_indices("```pulse-qa");
    let Some((start, marker)) = blocks.next() else {
        return Err(PulseError::validation(
            "qa_baseline_invalid",
            "qa.md must contain one ```pulse-qa fenced JSON contract",
        ));
    };
    if blocks.next().is_some() {
        return Err(PulseError::validation(
            "qa_baseline_invalid",
            "qa.md must contain exactly one pulse-qa contract",
        ));
    }
    let content_start = start + marker.len();
    let content = markdown[content_start..]
        .trim_start_matches([' ', '\t'])
        .strip_prefix('\n')
        .ok_or_else(|| {
            PulseError::validation(
                "qa_baseline_invalid",
                "pulse-qa fence must start JSON on the next line",
            )
        })?;
    let end = content.find("```").ok_or_else(|| {
        PulseError::validation("qa_baseline_invalid", "pulse-qa fence is not closed")
    })?;
    Ok(content[..end].trim())
}

fn normalize_baseline(baseline: &mut QaBaseline) {
    normalize(&mut baseline.risks);
    normalize(&mut baseline.exit_criteria);
    baseline.cases.sort_by(|left, right| left.id.cmp(&right.id));
    for case in &mut baseline.cases {
        normalize(&mut case.risk_refs);
        normalize(&mut case.steps);
        normalize(&mut case.expected);
    }
}

fn validate_baseline(baseline: &QaBaseline, story_id: &str) -> Result<()> {
    if baseline.schema_version != 1 || baseline.revision == 0 || baseline.story_id != story_id {
        return Err(PulseError::validation(
            "qa_baseline_invalid",
            "QA baseline version, revision, or Story identity is invalid",
        ));
    }
    if baseline.scope.trim().is_empty()
        || baseline.exit_criteria.is_empty()
        || baseline.cases.is_empty()
    {
        return Err(PulseError::validation(
            "qa_baseline_invalid",
            "QA baseline needs scope, cases, and exit criteria",
        ));
    }
    let risks = baseline.risks.iter().cloned().collect::<BTreeSet<_>>();
    let mut ids = BTreeSet::new();
    let mut covered_risks = BTreeSet::new();
    for case in &baseline.cases {
        if !ids.insert(&case.id)
            || case.id.trim().is_empty()
            || case.revision == 0
            || case.intent.trim().is_empty()
            || case.steps.is_empty()
            || case.expected.is_empty()
            || case.surface.trim().is_empty()
        {
            return Err(PulseError::validation(
                "qa_case_invalid",
                format!("QA case {} is incomplete or duplicated", case.id),
            ));
        }
        if case.applicability == QaCaseApplicability::NotApplicable
            && case
                .non_applicable_reason
                .as_deref()
                .map_or(true, str::is_empty)
        {
            return Err(PulseError::validation(
                "qa_case_invalid",
                format!("non-applicable QA case {} needs a rationale", case.id),
            ));
        }
        for reference in &case.risk_refs {
            if !risks.contains(reference) {
                return Err(PulseError::validation(
                    "qa_coverage_reference_invalid",
                    format!("QA case {} references unknown risk {reference}", case.id),
                ));
            }
            covered_risks.insert(reference.clone());
        }
    }
    if covered_risks != risks {
        return Err(PulseError::validation(
            "qa_coverage_incomplete",
            "every declared protected risk must map to a QA case",
        ));
    }
    Ok(())
}

fn normalize(values: &mut Vec<String>) {
    values.retain(|value| !value.trim().is_empty());
    for value in values.iter_mut() {
        *value = value.trim().to_string();
    }
    values.sort();
    values.dedup();
}
