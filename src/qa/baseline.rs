//! Story QA baseline parsing, validation and execution-scope case resolution.
//!
//! Decision 0010: `works/<STORY>/qa.md` is markdown with conventional headings
//! and carries no fenced JSON. JSON exists only at the runner boundary
//! (`qa-input.json`), which the kernel writes from this parse; the runner never
//! reads `qa.md`. Case currentness is the SHA-256 of the case section, never a
//! hand-written revision, so editing one case stales only that case.
//!
//! State touched: reads `works/<STORY>/qa.md` and the owning Story node under
//! `.pulse/workgraph/nodes/`. Everything below [`parse_baseline`] is pure.
//!
//! Invariant: the `content_hash` reported here is the hash of the exact bytes
//! on disk, because a `qa_checkpoint` receipt content-binds that path and
//! binding currentness re-hashes the file. `case_hash` is a normalized section
//! hash and is payload only.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::canonical_json::hash_bytes;
use crate::graph::model::contract::QaImpactPosture;
use crate::graph::model::node::Node;
use crate::id::WorkKind;
use crate::{PulseError, Result};

/// The parsed `qa.md` contract owned by a Story.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaBaseline {
    pub story_id: String,
    pub scope: String,
    pub posture: QaBaselinePosture,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub posture_reason: Option<String>,
    #[serde(default)]
    pub risks: Vec<QaRisk>,
    pub exit_criteria: Vec<String>,
    pub cases: Vec<QaCase>,
}

/// A protected risk declared under `## Risks`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaRisk {
    pub id: String,
    pub summary: String,
}

/// How the Story intends its baseline to be exercised (`## Posture`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QaBaselinePosture {
    Automated,
    Hybrid,
    ManualStructured,
    StaticProof,
    NotApplicable,
}

/// One `### QA-NNN` case, verbatim enough that a runner never re-reads `qa.md`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaCase {
    pub id: String,
    pub title: String,
    /// SHA-256 of the normalized case section: currentness without a revision.
    pub case_hash: String,
    pub intent: String,
    pub surface: QaCaseSurface,
    pub priority: QaCasePriority,
    pub applicability: QaCaseApplicability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub non_applicable_reason: Option<String>,
    #[serde(default)]
    pub risk_refs: Vec<String>,
    #[serde(default)]
    pub preconditions: Vec<String>,
    pub steps: Vec<String>,
    pub expected: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
    /// The optional `pulse-check` block: an executable check a script runner
    /// can run without interpreting prose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<QaCheck>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QaCaseSurface {
    Cli,
    Api,
    Ui,
    Job,
    Docs,
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

/// A `pulse-check` block: argv and assertions, never a shell string.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaCheck {
    pub run: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    pub assert: Vec<QaAssertion>,
}

/// The fixed assertion vocabulary of a `pulse-check` block.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QaAssertion {
    ExitCode(i64),
    StdoutLine(String),
    StdoutContains(String),
    StderrContains(String),
    StdoutJsonPath {
        path: String,
        equals: serde_json::Value,
    },
    FileUnchanged(String),
    FileContains {
        path: String,
        text: String,
    },
}

/// The current baseline resolved for one execution scope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QaBaselineResolution {
    pub owner_id: String,
    pub path: String,
    pub posture: QaBaselinePosture,
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
    let baseline = parse_baseline(markdown)?;
    validate_baseline(&baseline, story_id)?;
    Ok(QaBaselineResolution {
        owner_id: story_id.to_string(),
        path: relative,
        posture: baseline.posture,
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

/// Resolve a required Ticket QA impact to the exact current Story case hashes.
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

// ---------------------------------------------------------------------------
// Markdown contract parsing
// ---------------------------------------------------------------------------

/// Parse the conventional `qa.md` headings into the typed baseline.
///
/// # Errors
///
/// Returns a typed validation error for a missing or malformed heading, an
/// unknown `Key:` line, or an invalid `pulse-check` block.
pub fn parse_baseline(markdown: &str) -> Result<QaBaseline> {
    let sections = split_sections(markdown);
    let story_id = sections
        .iter()
        .find(|section| section.level == 1)
        .map(|section| first_token(&section.title))
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            invalid(
                "qa_baseline_invalid",
                "qa.md must start with `# <STORY-ID> …`",
            )
        })?;

    let mut scope = None;
    let mut posture = None;
    let mut posture_reason = None;
    let mut risks = Vec::new();
    let mut exit_criteria = Vec::new();
    let mut cases = Vec::new();
    let mut saw_cases_heading = false;
    let mut in_cases = false;

    for section in &sections {
        if section.level <= 1 {
            continue;
        }
        if section.level == 2 {
            in_cases = false;
            match section.title.to_ascii_lowercase().as_str() {
                "scope" => scope = Some(section.body.trim().to_string()),
                "posture" => {
                    let (value, reason) = parse_posture(&section.body)?;
                    posture = Some(value);
                    posture_reason = reason;
                }
                "risks" => risks = parse_risks(&section.body)?,
                "exit criteria" => exit_criteria = bullets(&section.body),
                "cases" => {
                    saw_cases_heading = true;
                    in_cases = true;
                }
                // Unknown level-2 headings stay prose, exactly like ticket.md.
                _ => {}
            }
            continue;
        }
        if section.level == 3 && in_cases {
            cases.push(parse_case(section)?);
        }
    }

    if !saw_cases_heading {
        return Err(invalid(
            "qa_baseline_invalid",
            "qa.md must contain ## Cases",
        ));
    }
    Ok(QaBaseline {
        story_id,
        scope: scope.unwrap_or_default(),
        posture: posture
            .ok_or_else(|| invalid("qa_baseline_invalid", "qa.md must contain ## Posture"))?,
        posture_reason,
        risks,
        exit_criteria,
        cases,
    })
}

fn validate_baseline(baseline: &QaBaseline, story_id: &str) -> Result<()> {
    if baseline.story_id != story_id {
        return Err(invalid(
            "qa_baseline_invalid",
            format!(
                "qa.md declares {} but is owned by {story_id}",
                baseline.story_id
            ),
        ));
    }
    if baseline.scope.trim().is_empty() || baseline.exit_criteria.is_empty() {
        return Err(invalid(
            "qa_baseline_invalid",
            "QA baseline needs a non-empty ## Scope and at least one ## Exit criteria bullet",
        ));
    }
    if baseline.posture == QaBaselinePosture::NotApplicable
        && baseline
            .posture_reason
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
    {
        return Err(invalid(
            "qa_baseline_invalid",
            "posture not_applicable needs a one-line reason",
        ));
    }
    if baseline.cases.is_empty() && baseline.posture != QaBaselinePosture::NotApplicable {
        return Err(invalid(
            "qa_baseline_invalid",
            "QA baseline needs at least one ### QA- case",
        ));
    }

    let mut risk_ids = BTreeSet::new();
    for risk in &baseline.risks {
        if !is_risk_id(&risk.id) || risk.summary.trim().is_empty() {
            return Err(invalid(
                "qa_baseline_invalid",
                format!(
                    "risk {} must match RISK-<NAME> and carry a summary",
                    risk.id
                ),
            ));
        }
        if !risk_ids.insert(risk.id.as_str()) {
            return Err(invalid(
                "qa_baseline_invalid",
                format!("risk {} is declared twice", risk.id),
            ));
        }
    }

    let mut ids = BTreeSet::new();
    let mut covered_risks = BTreeSet::new();
    for case in &baseline.cases {
        if !is_case_id(&case.id) {
            return Err(invalid(
                "qa_case_invalid",
                format!("QA case id {} must match QA-NNN", case.id),
            ));
        }
        if !ids.insert(case.id.as_str()) {
            return Err(invalid(
                "qa_case_invalid",
                format!("QA case {} is declared twice", case.id),
            ));
        }
        if case.title.trim().is_empty()
            || case.intent.trim().is_empty()
            || case.steps.is_empty()
            || case.expected.is_empty()
        {
            return Err(invalid(
                "qa_case_invalid",
                format!(
                    "QA case {} is missing a title, Intent, Steps or Expected",
                    case.id
                ),
            ));
        }
        if case.applicability == QaCaseApplicability::NotApplicable
            && case
                .non_applicable_reason
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
        {
            return Err(invalid(
                "qa_case_invalid",
                format!("non-applicable QA case {} needs a Reason", case.id),
            ));
        }
        if case.check.is_some() && !matches!(case.surface, QaCaseSurface::Cli | QaCaseSurface::Api)
        {
            return Err(invalid(
                "qa_check_surface_invalid",
                format!(
                    "QA case {} carries a pulse-check block but its surface is not cli or api",
                    case.id
                ),
            ));
        }
        for reference in &case.risk_refs {
            if !risk_ids.contains(reference.as_str()) {
                return Err(invalid(
                    "qa_coverage_reference_invalid",
                    format!("QA case {} references unknown risk {reference}", case.id),
                ));
            }
            covered_risks.insert(reference.as_str());
        }
    }
    if covered_risks != risk_ids {
        return Err(invalid(
            "qa_coverage_incomplete",
            "every declared protected risk must map to a QA case",
        ));
    }
    Ok(())
}

/// One heading and the lines that follow it up to the next heading.
struct Section {
    level: u8,
    title: String,
    body: String,
    /// Heading line plus body, normalized for hashing.
    normalized: String,
}

/// Split markdown into headings and their bodies, ignoring headings that only
/// appear inside fenced blocks (a `pulse-check` block may contain anything).
fn split_sections(markdown: &str) -> Vec<Section> {
    let mut sections = Vec::new();
    let mut fenced = false;
    let mut current: Option<OpenSection> = None;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        } else if !fenced {
            if let Some((level, title)) = heading(line) {
                sections.extend(current.take().map(OpenSection::close));
                current = Some(OpenSection {
                    level,
                    title,
                    heading_line: line.to_string(),
                    body: String::new(),
                });
                continue;
            }
        }
        if let Some(open) = current.as_mut() {
            open.body.push_str(line);
            open.body.push('\n');
        }
    }
    sections.extend(current.map(OpenSection::close));
    sections
}

struct OpenSection {
    level: u8,
    title: String,
    heading_line: String,
    body: String,
}

impl OpenSection {
    fn close(self) -> Section {
        let normalized = normalize_for_hash(&format!("{}\n{}", self.heading_line, self.body));
        Section {
            level: self.level,
            title: self.title,
            body: self.body,
            normalized,
        }
    }
}

fn heading(line: &str) -> Option<(u8, String)> {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 6 || !trimmed[hashes..].starts_with([' ', '\t']) {
        return None;
    }
    Some((
        hashes as u8,
        trimmed[hashes..]
            .trim()
            .trim_end_matches('#')
            .trim()
            .to_string(),
    ))
}

/// Line endings normalized and trailing whitespace stripped, so reflowing
/// invisible characters never stales a case (Decision 0010 §Revision và hash).
fn normalize_for_hash(text: &str) -> String {
    text.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

fn parse_posture(body: &str) -> Result<(QaBaselinePosture, Option<String>)> {
    let mut lines = body.lines().map(str::trim).filter(|line| !line.is_empty());
    let value = lines
        .next()
        .ok_or_else(|| invalid("qa_baseline_invalid", "## Posture must declare a value"))?;
    let posture = match value.to_ascii_lowercase().as_str() {
        "automated" => QaBaselinePosture::Automated,
        "hybrid" => QaBaselinePosture::Hybrid,
        "manual_structured" => QaBaselinePosture::ManualStructured,
        "static_proof" => QaBaselinePosture::StaticProof,
        "not_applicable" => QaBaselinePosture::NotApplicable,
        other => {
            return Err(invalid(
                "qa_baseline_invalid",
                format!("## Posture value {other:?} is not one of automated, hybrid, manual_structured, static_proof, not_applicable"),
            ))
        }
    };
    let reason = lines.collect::<Vec<_>>().join(" ");
    Ok((posture, (!reason.is_empty()).then_some(reason)))
}

fn parse_risks(body: &str) -> Result<Vec<QaRisk>> {
    let mut risks = Vec::new();
    for item in bullets(body) {
        let (id, summary) = item.split_once(':').ok_or_else(|| {
            invalid(
                "qa_baseline_invalid",
                format!("risk {item:?} must be written as `RISK-<NAME>: <summary>`"),
            )
        })?;
        risks.push(QaRisk {
            id: id.trim().to_string(),
            summary: summary.trim().to_string(),
        });
    }
    Ok(risks)
}

fn bullets(body: &str) -> Vec<String> {
    body.lines()
        .filter_map(|line| {
            let value = line.trim().strip_prefix(['-', '*'])?.trim();
            (!value.is_empty()).then(|| value.to_string())
        })
        .collect()
}

/// Fields a case line may open. Anything else is `qa_baseline_unknown_field`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CaseField {
    Intent,
    Surface,
    Priority,
    Applicability,
    Reason,
    Risks,
    Preconditions,
    Steps,
    Expected,
    Evidence,
}

impl CaseField {
    fn parse(key: &str) -> Option<Self> {
        Some(match key {
            "intent" => Self::Intent,
            "surface" => Self::Surface,
            "priority" => Self::Priority,
            "applicability" => Self::Applicability,
            "reason" => Self::Reason,
            "risks" => Self::Risks,
            "preconditions" => Self::Preconditions,
            "steps" => Self::Steps,
            "expected" => Self::Expected,
            "evidence" => Self::Evidence,
            _ => return None,
        })
    }
}

#[derive(Default)]
struct CaseFields {
    intent: Option<String>,
    surface: Option<String>,
    priority: Option<String>,
    applicability: Option<String>,
    reason: Option<String>,
    risks: Vec<String>,
    preconditions: Vec<String>,
    steps: Vec<String>,
    expected: Vec<String>,
    evidence: Vec<String>,
}

fn parse_case(section: &Section) -> Result<QaCase> {
    let id = first_token(&section.title);
    let title = section.title[id.len()..].trim().to_string();
    let mut fields = CaseFields::default();
    let mut check = None;
    let mut open: Option<CaseField> = None;

    let mut lines = section.body.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(language) = trimmed.strip_prefix("```") {
            let mut block = String::new();
            for line in lines.by_ref() {
                if line.trim_start().starts_with("```") {
                    break;
                }
                block.push_str(line);
                block.push('\n');
            }
            if language.trim() == "pulse-check" {
                if check.is_some() {
                    return Err(invalid(
                        "qa_check_invalid",
                        format!("QA case {id} declares more than one pulse-check block"),
                    ));
                }
                check = Some(parse_check(&block, &id)?);
            }
            open = None;
            continue;
        }
        let indented = line.len() > line.trim_start().len();
        if indented {
            let Some(field) = open else { continue };
            let value = strip_item_marker(trimmed);
            if value.is_empty() {
                continue;
            }
            push_field(&mut fields, field, value);
            continue;
        }
        let content = trimmed.strip_prefix(['-', '*']).unwrap_or(trimmed).trim();
        let Some((key, value)) = content.split_once(':') else {
            // Prose at case level closes the open field and is not a contract.
            open = None;
            continue;
        };
        let field =
            CaseField::parse(key.trim().to_ascii_lowercase().as_str()).ok_or_else(|| {
                invalid(
                    "qa_baseline_unknown_field",
                    format!("QA case {id} has an unknown field line: {trimmed}"),
                )
            })?;
        open = Some(field);
        let value = value.trim();
        if !value.is_empty() {
            push_field(&mut fields, field, value.to_string());
        }
    }

    let surface = fields
        .surface
        .as_deref()
        .map(str::trim)
        .ok_or_else(|| missing_field(&id, "Surface"))?
        .to_ascii_lowercase();
    let surface = match surface.as_str() {
        "cli" => QaCaseSurface::Cli,
        "api" => QaCaseSurface::Api,
        "ui" => QaCaseSurface::Ui,
        "job" => QaCaseSurface::Job,
        "docs" => QaCaseSurface::Docs,
        other => {
            return Err(invalid(
                "qa_case_invalid",
                format!("QA case {id} surface {other:?} is not cli, api, ui, job or docs"),
            ))
        }
    };
    let priority = match fields
        .priority
        .as_deref()
        .map(str::trim)
        .ok_or_else(|| missing_field(&id, "Priority"))?
        .to_ascii_lowercase()
        .as_str()
    {
        "critical" => QaCasePriority::Critical,
        "high" => QaCasePriority::High,
        "normal" => QaCasePriority::Normal,
        "low" => QaCasePriority::Low,
        other => {
            return Err(invalid(
                "qa_case_invalid",
                format!("QA case {id} priority {other:?} is not critical, high, normal or low"),
            ))
        }
    };
    let applicability = match fields
        .applicability
        .as_deref()
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_else(|| "required".to_string())
        .as_str()
    {
        "required" => QaCaseApplicability::Required,
        "not_applicable" => QaCaseApplicability::NotApplicable,
        other => {
            return Err(invalid(
                "qa_case_invalid",
                format!("QA case {id} applicability {other:?} is not required or not_applicable"),
            ))
        }
    };
    let mut risk_refs = fields.risks;
    risk_refs.sort();
    risk_refs.dedup();

    Ok(QaCase {
        case_hash: hash_bytes(section.normalized.as_bytes()),
        id,
        title,
        intent: fields.intent.unwrap_or_default(),
        surface,
        priority,
        applicability,
        non_applicable_reason: fields.reason,
        risk_refs,
        preconditions: fields.preconditions,
        steps: fields.steps,
        expected: fields.expected,
        evidence: fields.evidence,
        check,
    })
}

fn push_field(fields: &mut CaseFields, field: CaseField, value: String) {
    match field {
        CaseField::Intent => append_text(&mut fields.intent, value),
        CaseField::Surface => append_text(&mut fields.surface, value),
        CaseField::Priority => append_text(&mut fields.priority, value),
        CaseField::Applicability => append_text(&mut fields.applicability, value),
        CaseField::Reason => append_text(&mut fields.reason, value),
        CaseField::Risks => fields.risks.extend(
            value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string),
        ),
        CaseField::Preconditions => fields.preconditions.push(value),
        CaseField::Steps => fields.steps.push(value),
        CaseField::Expected => fields.expected.push(value),
        CaseField::Evidence => fields.evidence.push(value),
    }
}

fn append_text(slot: &mut Option<String>, value: String) {
    match slot {
        Some(existing) => {
            existing.push(' ');
            existing.push_str(&value);
        }
        None => *slot = Some(value),
    }
}

/// Strip a `-`, `*` or `12.` list marker from an indented item line.
fn strip_item_marker(line: &str) -> String {
    if let Some(rest) = line.strip_prefix(['-', '*']) {
        return rest.trim().to_string();
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        if let Some(rest) = line[digits..].strip_prefix(['.', ')']) {
            return rest.trim().to_string();
        }
    }
    line.trim().to_string()
}

fn missing_field(case_id: &str, field: &str) -> PulseError {
    invalid(
        "qa_case_invalid",
        format!("QA case {case_id} is missing {field}"),
    )
}

// ---------------------------------------------------------------------------
// pulse-check block
// ---------------------------------------------------------------------------

/// Shell metacharacters. `run` is argv, never a shell string, so a case that
/// needs composition must be written as separate cases.
const SHELL_OPERATORS: [&str; 8] = ["&&", "||", "|", ";", ">", "<", "`", "$("];

fn parse_check(block: &str, case_id: &str) -> Result<QaCheck> {
    let mut run = None;
    let mut cwd = None;
    let mut stdin = None;
    let mut timeout_seconds = None;
    let mut env = BTreeMap::new();
    let mut assertions = Vec::new();
    let mut open: Option<String> = None;

    for line in block.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indented = line.len() > line.trim_start().len();
        if indented {
            match open.as_deref() {
                Some("env") => {
                    let item = strip_item_marker(trimmed);
                    let (key, value) = item.split_once(':').ok_or_else(|| {
                        check_invalid(case_id, format!("env entry {item:?} needs `KEY: value`"))
                    })?;
                    env.insert(key.trim().to_string(), scalar(value.trim()));
                }
                Some("assert") => assertions.push(parse_assertion(case_id, trimmed)?),
                _ => {
                    return Err(check_invalid(
                        case_id,
                        format!("indented line {trimmed:?} does not belong to env or assert"),
                    ))
                }
            }
            continue;
        }
        let (key, value) = trimmed.split_once(':').ok_or_else(|| {
            check_invalid(case_id, format!("line {trimmed:?} is not `key: value`"))
        })?;
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        match key.as_str() {
            "run" => run = Some(parse_argv(case_id, value)?),
            "cwd" => cwd = Some(scalar(value)),
            "stdin" => stdin = Some(scalar(value)),
            "timeout_seconds" => {
                timeout_seconds = Some(value.parse::<u64>().map_err(|_| {
                    check_invalid(
                        case_id,
                        format!("timeout_seconds {value:?} is not an integer"),
                    )
                })?)
            }
            "env" | "assert" => {}
            other => {
                return Err(check_invalid(
                    case_id,
                    format!("unknown pulse-check key {other:?}"),
                ))
            }
        }
        open = Some(key);
    }

    let run =
        run.ok_or_else(|| check_invalid(case_id, "pulse-check needs a run command".into()))?;
    if assertions.is_empty() {
        return Err(check_invalid(
            case_id,
            "pulse-check needs at least one assertion".into(),
        ));
    }
    Ok(QaCheck {
        run,
        cwd,
        env,
        stdin,
        timeout_seconds,
        assert: assertions,
    })
}

fn parse_argv(case_id: &str, command: &str) -> Result<Vec<String>> {
    let command = scalar(command);
    if let Some(operator) = SHELL_OPERATORS
        .iter()
        .find(|operator| command.contains(*operator))
    {
        return Err(invalid(
            "qa_check_shell_operator",
            format!(
                "QA case {case_id} run uses shell operator {operator:?}; pulse-check executes argv without a shell, so split it into separate cases"
            ),
        ));
    }
    let mut argv = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    let mut escaped = false;
    for character in command.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '"' | '\'' if quote.is_none() => {
                quote = Some(character);
                started = true;
            }
            character if Some(character) == quote => quote = None,
            character if character.is_whitespace() && quote.is_none() => {
                if started {
                    argv.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            character => {
                current.push(character);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return Err(check_invalid(
            case_id,
            "run has an unterminated quote".into(),
        ));
    }
    if started {
        argv.push(current);
    }
    if argv.is_empty() {
        return Err(check_invalid(case_id, "run is empty".into()));
    }
    Ok(argv)
}

fn parse_assertion(case_id: &str, line: &str) -> Result<QaAssertion> {
    let item = strip_item_marker(line);
    let (key, value) = item
        .split_once(':')
        .ok_or_else(|| check_invalid(case_id, format!("assertion {item:?} needs `key: value`")))?;
    let key = key.trim().to_ascii_lowercase();
    let value = value.trim();
    Ok(match key.as_str() {
        "exit_code" => QaAssertion::ExitCode(value.parse::<i64>().map_err(|_| {
            check_invalid(case_id, format!("exit_code {value:?} is not an integer"))
        })?),
        "stdout_line" => QaAssertion::StdoutLine(scalar(value)),
        "stdout_contains" => QaAssertion::StdoutContains(scalar(value)),
        "stderr_contains" => QaAssertion::StderrContains(scalar(value)),
        "file_unchanged" => QaAssertion::FileUnchanged(scalar(value)),
        "stdout_json_path" => {
            let map = flow_map(case_id, value)?;
            QaAssertion::StdoutJsonPath {
                path: required_flow_string(case_id, &map, "path")?,
                equals: map.get("equals").cloned().ok_or_else(|| {
                    check_invalid(case_id, "stdout_json_path needs `equals`".into())
                })?,
            }
        }
        "file_contains" => {
            let map = flow_map(case_id, value)?;
            QaAssertion::FileContains {
                path: required_flow_string(case_id, &map, "path")?,
                text: required_flow_string(case_id, &map, "text")?,
            }
        }
        other => {
            return Err(check_invalid(
                case_id,
                format!("unknown assertion {other:?}"),
            ))
        }
    })
}

/// Parse a one-line `{key: value, key: value}` mapping into JSON values.
fn flow_map(case_id: &str, value: &str) -> Result<BTreeMap<String, serde_json::Value>> {
    let inner = value
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
        .ok_or_else(|| {
            check_invalid(case_id, format!("{value:?} must be a `{{key: value}}` map"))
        })?;
    let mut map = BTreeMap::new();
    for entry in split_flow_entries(inner) {
        let (key, raw) = entry.split_once(':').ok_or_else(|| {
            check_invalid(case_id, format!("map entry {entry:?} needs `key: value`"))
        })?;
        let raw = raw.trim();
        let parsed = serde_json::from_str::<serde_json::Value>(raw)
            .unwrap_or_else(|_| serde_json::Value::String(scalar(raw)));
        map.insert(key.trim().to_string(), parsed);
    }
    Ok(map)
}

fn split_flow_entries(inner: &str) -> Vec<String> {
    let mut entries = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for character in inner.chars() {
        match character {
            '"' | '\'' if quote.is_none() => {
                quote = Some(character);
                current.push(character);
            }
            character if Some(character) == quote => {
                quote = None;
                current.push(character);
            }
            ',' if quote.is_none() => entries.push(std::mem::take(&mut current)),
            character => current.push(character),
        }
    }
    if !current.trim().is_empty() {
        entries.push(current);
    }
    entries
        .into_iter()
        .map(|entry| entry.trim().to_string())
        .filter(|entry| !entry.is_empty())
        .collect()
}

fn required_flow_string(
    case_id: &str,
    map: &BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Result<String> {
    map.get(key)
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .ok_or_else(|| check_invalid(case_id, format!("map needs a string `{key}`")))
}

/// Minimal YAML scalar: double quotes carry JSON escapes, single quotes are
/// literal, anything else is the trimmed text.
fn scalar(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        if let Ok(parsed) = serde_json::from_str::<String>(value) {
            return parsed;
        }
    }
    if value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'') {
        return value[1..value.len() - 1].to_string();
    }
    value.to_string()
}

fn first_token(value: &str) -> String {
    value
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string()
}

fn is_risk_id(value: &str) -> bool {
    value.strip_prefix("RISK-").is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
    })
}

fn is_case_id(value: &str) -> bool {
    value
        .strip_prefix("QA-")
        .is_some_and(|rest| rest.len() >= 3 && rest.chars().all(|c| c.is_ascii_digit()))
}

fn invalid(code: &'static str, message: impl Into<String>) -> PulseError {
    PulseError::validation(code, message)
}

fn check_invalid(case_id: &str, message: String) -> PulseError {
    invalid(
        "qa_check_invalid",
        format!("QA case {case_id} pulse-check is invalid: {message}"),
    )
}

/// Render the initial `qa.md` contract for a new Story.
pub fn baseline_template(story_id: &str, title: &str) -> String {
    format!(
        "# {story_id} QA baseline — {title}\n\n\
         Why these cases define \"correct\" for this Story.\n\n\
         ## Scope\nOne sentence: the promised behavior this baseline protects.\n\n\
         ## Posture\nautomated\n\n\
         ## Risks\n- RISK-NAME: What goes wrong if this behavior regresses.\n\n\
         ## Exit criteria\n- Every required case passes on the candidate source.\n\n\
         ## Cases\n\n\
         ### QA-001 Short title\n\
         - Intent: What the user observes, without naming implementation.\n\
         - Surface: api\n\
         - Priority: high\n\
         - Risks: RISK-NAME\n\
         - Preconditions:\n  - Initial state.\n\
         - Steps:\n  1. Observable action.\n\
         - Expected:\n  - One checkable condition per line.\n\
         - Evidence:\n  - Artifact the runner must produce.\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASELINE: &str = r#"# ST-002 QA baseline — Due dates

Prose for humans.

## Scope
Due dates are stored and surfaced unchanged.

## Posture
automated

## Risks
- RISK-BAD-DATE: an invalid date corrupts the state file.

## Exit criteria
- Every required case passes on the candidate source.

## Cases

### QA-004 Invalid date is rejected
- Intent: An impossible date prints InvalidDate and leaves the data untouched.
- Surface: cli
- Priority: high
- Risks: RISK-BAD-DATE
- Preconditions:
  - State file is empty.
- Steps:
  1. Run add t1 Buy --due 2026-02-30.
- Expected:
  - stdout carries the line InvalidDate.
  - exit code is 2.
- Evidence:
  - stdout.

```pulse-check
run: node src/cli.mjs add t1 Buy --due 2026-02-30
env:
  TODOLIST_STATE: $STATE_FILE
assert:
  - exit_code: 2
  - stdout_line: InvalidDate
  - file_unchanged: $STATE_FILE
```
"#;

    #[test]
    fn parses_the_heading_contract_including_the_check_block() {
        let baseline = parse_baseline(BASELINE).unwrap();
        validate_baseline(&baseline, "ST-002").unwrap();
        assert_eq!(baseline.posture, QaBaselinePosture::Automated);
        assert_eq!(baseline.risks[0].id, "RISK-BAD-DATE");
        let case = &baseline.cases[0];
        assert_eq!(case.id, "QA-004");
        assert_eq!(case.title, "Invalid date is rejected");
        assert_eq!(case.surface, QaCaseSurface::Cli);
        assert_eq!(case.applicability, QaCaseApplicability::Required);
        assert_eq!(case.steps, vec!["Run add t1 Buy --due 2026-02-30."]);
        assert_eq!(case.preconditions, vec!["State file is empty."]);
        assert_eq!(case.evidence, vec!["stdout."]);
        let check = case.check.as_ref().unwrap();
        assert_eq!(check.run[0], "node");
        assert_eq!(check.run.len(), 7);
        assert_eq!(check.env["TODOLIST_STATE"], "$STATE_FILE");
        assert_eq!(
            check.assert,
            vec![
                QaAssertion::ExitCode(2),
                QaAssertion::StdoutLine("InvalidDate".into()),
                QaAssertion::FileUnchanged("$STATE_FILE".into()),
            ]
        );
        assert!(case.case_hash.starts_with("sha256:"));
    }

    #[test]
    fn case_hash_ignores_trailing_whitespace_but_tracks_content() {
        let baseline = parse_baseline(BASELINE).unwrap();
        let padded = BASELINE.replace("- Surface: cli\n", "- Surface: cli   \n");
        let reworded = BASELINE.replace("exit code is 2.", "exit code is 3.");
        assert_eq!(
            baseline.cases[0].case_hash,
            parse_baseline(&padded).unwrap().cases[0].case_hash
        );
        assert_ne!(
            baseline.cases[0].case_hash,
            parse_baseline(&reworded).unwrap().cases[0].case_hash
        );
    }

    #[test]
    fn rejects_an_unknown_key_line_by_name() {
        let text = BASELINE.replace("- Surface: cli\n", "- Surface: cli\n- Revision: 3\n");
        let error = parse_baseline(&text).unwrap_err();
        assert_eq!(error.code(), "qa_baseline_unknown_field");
        assert!(error.to_string().contains("Revision: 3"), "{error}");
    }

    #[test]
    fn rejects_missing_required_case_fields() {
        let text = BASELINE.replace("- Priority: high\n", "");
        assert_eq!(parse_baseline(&text).unwrap_err().code(), "qa_case_invalid");
        let text = BASELINE.replace(
            "  - stdout carries the line InvalidDate.\n  - exit code is 2.\n",
            "",
        );
        let baseline = parse_baseline(&text).unwrap();
        assert_eq!(
            validate_baseline(&baseline, "ST-002").unwrap_err().code(),
            "qa_case_invalid"
        );
    }

    #[test]
    fn rejects_duplicate_case_ids_and_unknown_risk_references() {
        let duplicated = format!(
            "{BASELINE}\n### QA-004 Same id again\n- Intent: x\n- Surface: cli\n- Priority: low\n- Steps:\n  1. x\n- Expected:\n  - x\n"
        );
        let baseline = parse_baseline(&duplicated).unwrap();
        assert_eq!(
            validate_baseline(&baseline, "ST-002").unwrap_err().code(),
            "qa_case_invalid"
        );
        let stray = BASELINE.replace("- Risks: RISK-BAD-DATE\n", "- Risks: RISK-GHOST\n");
        let baseline = parse_baseline(&stray).unwrap();
        assert_eq!(
            validate_baseline(&baseline, "ST-002").unwrap_err().code(),
            "qa_coverage_reference_invalid"
        );
    }

    #[test]
    fn rejects_a_shell_operator_in_a_check_command() {
        let text = BASELINE.replace(
            "run: node src/cli.mjs add t1 Buy --due 2026-02-30\n",
            "run: node src/cli.mjs add t1 Buy && node src/cli.mjs list\n",
        );
        assert_eq!(
            parse_baseline(&text).unwrap_err().code(),
            "qa_check_shell_operator"
        );
    }

    #[test]
    fn rejects_a_check_block_on_a_non_executable_surface() {
        let text = BASELINE.replace("- Surface: cli\n", "- Surface: ui\n");
        let baseline = parse_baseline(&text).unwrap();
        assert_eq!(
            validate_baseline(&baseline, "ST-002").unwrap_err().code(),
            "qa_check_surface_invalid"
        );
    }

    #[test]
    fn rejects_a_baseline_owned_by_another_story() {
        let baseline = parse_baseline(BASELINE).unwrap();
        assert_eq!(
            validate_baseline(&baseline, "ST-001").unwrap_err().code(),
            "qa_baseline_invalid"
        );
    }

    #[test]
    fn parses_structured_assertions_and_quoted_scalars() {
        let text = BASELINE.replace(
            "  - stdout_line: InvalidDate\n",
            "  - stdout_line: \"t1\\tBuy\"\n  - stdout_json_path: {path: \"$.outcome\", equals: \"Completed\"}\n  - file_contains: {path: state.json, text: t1}\n",
        );
        let check = parse_baseline(&text).unwrap().cases[0]
            .check
            .clone()
            .unwrap();
        assert!(check
            .assert
            .contains(&QaAssertion::StdoutLine("t1\tBuy".into())));
        assert!(check.assert.contains(&QaAssertion::StdoutJsonPath {
            path: "$.outcome".into(),
            equals: serde_json::json!("Completed"),
        }));
        assert!(check.assert.contains(&QaAssertion::FileContains {
            path: "state.json".into(),
            text: "t1".into(),
        }));
    }

    #[test]
    fn not_applicable_posture_allows_an_empty_case_list_with_a_reason() {
        let text = "# ST-009 QA baseline — Internal refactor\n\n## Scope\nNo user-visible behavior.\n\n## Posture\nnot_applicable\nThe Story only moves private modules.\n\n## Exit criteria\n- The refactor lands with the existing suite green.\n\n## Cases\n";
        let baseline = parse_baseline(text).unwrap();
        validate_baseline(&baseline, "ST-009").unwrap();
        assert!(baseline.cases.is_empty());
        let without_reason = text.replace("The Story only moves private modules.\n", "");
        let baseline = parse_baseline(&without_reason).unwrap();
        assert_eq!(
            validate_baseline(&baseline, "ST-009").unwrap_err().code(),
            "qa_baseline_invalid"
        );
    }
}
