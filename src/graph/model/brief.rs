//! Pure parser for the Markdown contract owned by an implementation Ticket.
//!
//! The parser deliberately knows only the heading contract. It performs no
//! filesystem access and leaves repository/reference checks to graph validation.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::graph::model::contract::{Materialization, QaImpactPosture};
use crate::{PulseError, PulseResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TicketBrief {
    pub title: Option<String>,
    pub objective: Option<String>,
    pub current_behavior: Option<String>,
    pub target_behavior: Option<String>,
    pub code_anchors: Vec<String>,
    pub required_changes: Vec<String>,
    pub invariants: Vec<String>,
    pub scope: Vec<String>,
    pub non_scope: Vec<String>,
    pub acceptance: Vec<AcceptanceItem>,
    pub verify: Vec<String>,
    pub open_questions: Vec<OpenQuestion>,
    pub implementation_freedom: ImplementationFreedom,
    pub documentation: Option<DocumentationImpactBrief>,
    pub qa: Option<QaImpactBrief>,
    pub expected_handoff: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcceptanceItem {
    pub id: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenQuestion {
    pub disposition: OpenQuestionDisposition,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpenQuestionDisposition {
    Resolved,
    Rejected,
    Delegated,
    Deferred,
    Blocking,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImplementationFreedom {
    pub mode: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentationImpactBrief {
    pub posture: String,
    pub documents: Vec<String>,
    pub rationale: Option<String>,
    pub required_update: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QaImpactBrief {
    pub owner: Option<String>,
    pub posture: String,
    pub cases: Vec<String>,
    pub reason: Option<String>,
}

impl TicketBrief {
    /// Validate requirements that depend on the requested materialization level.
    pub fn validate_for(&self, materialization: Materialization) -> PulseResult<()> {
        required_text(self.objective.as_deref(), "objective")?;
        if self.acceptance.is_empty() {
            return Err(invalid(
                "ticket_brief_acceptance_missing",
                "Acceptance requires at least one AC-* item",
            ));
        }
        if self.code_anchors.is_empty() {
            return Err(invalid(
                "ticket_brief_code_anchors_missing",
                "Code anchors requires at least one path",
            ));
        }
        if self.verify.is_empty() {
            return Err(invalid(
                "ticket_brief_verify_missing",
                "Verify requires at least one command",
            ));
        }
        if materialization != Materialization::R0 {
            required_text(self.current_behavior.as_deref(), "current behavior")?;
            required_text(self.target_behavior.as_deref(), "target behavior")?;
            if self.invariants.is_empty() {
                return Err(invalid(
                    "ticket_brief_invariants_missing",
                    "Invariants requires at least one item",
                ));
            }
        }
        if self
            .open_questions
            .iter()
            .any(|q| q.disposition == OpenQuestionDisposition::Blocking)
        {
            return Err(invalid(
                "ticket_brief_open_question_blocking",
                "open question is blocking",
            ));
        }
        Ok(())
    }
}

/// Parse the conventional `ticket.md` headings.
///
/// Headings are matched case-insensitively. Unknown headings are retained as
/// prose but do not become contract fields.
pub fn parse_ticket_brief(markdown: &str) -> PulseResult<TicketBrief> {
    let sections = sections(markdown);
    let title = markdown.lines().find_map(|line| {
        let (level, text) = heading(line)?;
        (level == 1).then(|| text)
    });
    let objective = text_section(&sections, "objective");
    let current_behavior = text_section(&sections, "current behavior");
    let target_behavior = text_section(&sections, "target behavior");
    let code_anchors = list_section(&sections, "code anchors");
    let required_changes = list_section(&sections, "required changes");
    let invariants = list_section(&sections, "invariants");
    let scope = list_section(&sections, "scope");
    let non_scope = list_section(&sections, "non-scope");
    let verify = list_section(&sections, "verify");
    let expected_handoff = list_section(&sections, "expected handoff");
    let acceptance = parse_acceptance(&sections);
    if sections.contains_key("acceptance") && acceptance.is_empty() {
        return Err(invalid(
            "ticket_brief_acceptance_id_missing",
            "Acceptance items must use `AC-ID: summary`",
        ));
    }
    let open_questions = parse_questions(&sections);
    let implementation_freedom = parse_freedom(&sections);
    let documentation = parse_documentation(&sections);
    let qa = parse_qa(&sections);
    let brief = TicketBrief {
        title,
        objective,
        current_behavior,
        target_behavior,
        code_anchors,
        required_changes,
        invariants,
        scope,
        non_scope,
        acceptance,
        verify,
        open_questions,
        implementation_freedom,
        documentation,
        qa,
        expected_handoff,
    };
    // These are the stable minimum for every implementation Ticket. R1/R2/R3
    // callers additionally invoke validate_for(materialization).
    if brief.objective.is_none() {
        return Err(invalid(
            "ticket_brief_objective_missing",
            "ticket.md must contain ## Objective",
        ));
    }
    if !sections.contains_key("acceptance") {
        return Err(invalid(
            "ticket_brief_acceptance_missing",
            "ticket.md must contain ## Acceptance",
        ));
    }
    if !sections.contains_key("code anchors") {
        return Err(invalid(
            "ticket_brief_code_anchors_missing",
            "ticket.md must contain ## Code anchors",
        ));
    }
    if !sections.contains_key("verify") {
        return Err(invalid(
            "ticket_brief_verify_missing",
            "ticket.md must contain ## Verify",
        ));
    }
    Ok(brief)
}

fn sections(markdown: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut current: Option<String> = None;
    let mut body = String::new();
    for line in markdown.lines() {
        if let Some((level, name)) = heading(line) {
            if level == 2 {
                if let Some(key) = current.take() {
                    out.insert(key, body.trim().to_string());
                }
                current = Some(name.to_ascii_lowercase());
                body.clear();
                continue;
            }
        }
        if current.is_some() {
            body.push_str(line);
            body.push('\n');
        }
    }
    if let Some(key) = current {
        out.insert(key, body.trim().to_string());
    }
    out
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

fn text_section(sections: &BTreeMap<String, String>, name: &str) -> Option<String> {
    sections
        .get(name)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn list_section(sections: &BTreeMap<String, String>, name: &str) -> Vec<String> {
    sections
        .get(name)
        .map(|body| body.lines().filter_map(list_item).collect())
        .unwrap_or_default()
}

fn list_item(line: &str) -> Option<String> {
    let value = line.trim().strip_prefix(['-', '*'])?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn parse_acceptance(sections: &BTreeMap<String, String>) -> Vec<AcceptanceItem> {
    sections
        .get("acceptance")
        .map(|body| {
            body.lines()
                .filter_map(|line| {
                    let value = line.trim().strip_prefix(['-', '*'])?.trim();
                    let (id, summary) = value.split_once(':')?;
                    let id = id.trim();
                    if !id.starts_with("AC-") || id.len() <= 3 || summary.trim().is_empty() {
                        return None;
                    }
                    Some(AcceptanceItem {
                        id: id.to_string(),
                        summary: summary.trim().to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_questions(sections: &BTreeMap<String, String>) -> Vec<OpenQuestion> {
    sections
        .get("open questions")
        .map(|body| {
            body.lines()
                .filter_map(|line| {
                    let value = line.trim().strip_prefix(['-', '*'])?.trim();
                    let (disposition, text) = value.strip_prefix('(')?.split_once(')')?;
                    let disposition = match disposition.trim().to_ascii_lowercase().as_str() {
                        "resolved" => OpenQuestionDisposition::Resolved,
                        "rejected" => OpenQuestionDisposition::Rejected,
                        "delegated" => OpenQuestionDisposition::Delegated,
                        "deferred" => OpenQuestionDisposition::Deferred,
                        "blocking" => OpenQuestionDisposition::Blocking,
                        _ => return None,
                    };
                    Some(OpenQuestion {
                        disposition,
                        text: text.trim().to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_freedom(sections: &BTreeMap<String, String>) -> ImplementationFreedom {
    let detail = text_section(sections, "implementation freedom").unwrap_or_default();
    let mode = detail
        .split(':')
        .next()
        .unwrap_or("guided")
        .trim()
        .to_ascii_lowercase();
    let mode = if matches!(mode.as_str(), "locked" | "guided" | "open") {
        mode
    } else {
        "guided".to_string()
    };
    ImplementationFreedom { mode, detail }
}

fn parse_key_values(sections: &BTreeMap<String, String>, name: &str) -> BTreeMap<String, String> {
    sections
        .get(name)
        .map(|body| {
            body.lines()
                .filter_map(|line| {
                    let line = line.trim().trim_start_matches(['-', '*']).trim();
                    let (key, value) = line.split_once(':')?;
                    Some((key.trim().to_ascii_lowercase(), value.trim().to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_documentation(sections: &BTreeMap<String, String>) -> Option<DocumentationImpactBrief> {
    if !sections.contains_key("documentation impact") {
        return None;
    }
    let values = parse_key_values(sections, "documentation impact");
    let documents = values
        .get("documents")
        .map(|s| {
            s.split(',')
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .collect()
        })
        .unwrap_or_default();
    Some(DocumentationImpactBrief {
        posture: values
            .get("posture")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string()),
        documents,
        rationale: values.get("rationale").cloned(),
        required_update: values.get("required update").cloned(),
    })
}

fn parse_qa(sections: &BTreeMap<String, String>) -> Option<QaImpactBrief> {
    if !sections.contains_key("qa impact") {
        return None;
    }
    let values = parse_key_values(sections, "qa impact");
    let cases = values
        .get("cases")
        .map(|s| {
            s.split(',')
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .collect()
        })
        .unwrap_or_default();
    Some(QaImpactBrief {
        owner: values.get("owner").cloned(),
        posture: values
            .get("posture")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string()),
        cases,
        reason: values.get("reason").cloned(),
    })
}

fn required_text(value: Option<&str>, field: &str) -> PulseResult<()> {
    if value.is_some_and(|v| !v.trim().is_empty()) {
        Ok(())
    } else {
        Err(invalid(
            "ticket_brief_field_missing",
            format!("ticket brief requires {field}"),
        ))
    }
}

fn invalid(code: &'static str, message: impl Into<String>) -> PulseError {
    PulseError::validation(code, message)
}

/// Render the initial implementation Ticket contract.
pub fn implementation_template(id: &str, title: &str, materialization: Materialization) -> String {
    let full = materialization != Materialization::R0;
    format!("# {id} {title}\n\n## Objective\nDescribe the outcome this Ticket must achieve.\n\n{}## Code anchors\n- src/\n\n## Acceptance\n- AC-1: Describe a testable acceptance condition.\n\n## Verify\n- cargo test\n\n{}## Required changes\n- Describe the required change.\n\n## Invariants\n- Describe the invariant that must remain true.\n\n## Implementation freedom\nguided: agent chooses internal structure within this contract.\n\n## Scope\n- Included work.\n\n## Non-scope\n- Work excluded from this Ticket.\n\n## Open questions\n- (delegated) Record implementation choices that are safe to delegate.\n\n## Documentation impact\n- Posture: none\n- Rationale: Explain why durable docs are unaffected.\n- Documents:\n\n## QA impact\n- Owner: ST-000\n- Posture: none\n- Cases:\n- Reason: Explain QA posture.\n\n## Expected handoff\n- Diff and verification results.\n", if full { "## Current behavior\nDescribe the current behavior.\n\n## Target behavior\nDescribe the target behavior.\n\n" } else { "" }, if full { "" } else { "" })
}

/// Render the question-oriented contract for a decision-work Ticket.
pub fn decision_work_template(id: &str, title: &str) -> String {
    format!("# {id} {title}\n\n## Question\nState the precise question this Ticket must answer.\n\n## Expected output\nDescribe the decision-ready output.\n\n## Verify\n- Record research evidence.\n")
}

pub fn qa_posture(value: &str) -> Option<QaImpactPosture> {
    match value.trim().to_ascii_lowercase().as_str() {
        "required" => Some(QaImpactPosture::Required),
        "covered_by_story_close" => Some(QaImpactPosture::CoveredByStoryClose),
        "none" => Some(QaImpactPosture::None),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R0: &str = "# TK-001 Small\n\n## Objective\nDo the thing.\n\n## Code Anchors\n- src/lib.rs\n\n## Acceptance\n- AC-1: It works.\n\n## Verify\n- cargo test\n";

    #[test]
    fn parses_minimal_r0_contract_case_insensitively() {
        let brief = parse_ticket_brief(R0).unwrap();
        brief.validate_for(Materialization::R0).unwrap();
        assert_eq!(brief.acceptance[0].id, "AC-1");
        assert_eq!(brief.code_anchors, vec!["src/lib.rs"]);
    }

    #[test]
    fn parses_full_r1_contract() {
        let text = format!(
            "{R0}\n## Current behavior\nOld.\n\n## Target behavior\nNew.\n\n## Invariants\n- INV\n"
        );
        let brief = parse_ticket_brief(&text).unwrap();
        brief.validate_for(Materialization::R1).unwrap();
    }

    #[test]
    fn rejects_missing_acceptance_or_acceptance_id() {
        let missing = R0.replace("## Acceptance\n- AC-1: It works.\n", "");
        assert_eq!(
            parse_ticket_brief(&missing).unwrap_err().code(),
            "ticket_brief_acceptance_missing"
        );
        let no_id = R0.replace("AC-1", "works");
        assert_eq!(
            parse_ticket_brief(&no_id).unwrap_err().code(),
            "ticket_brief_acceptance_id_missing"
        );
    }

    #[test]
    fn blocking_open_question_fails_materialization_validation() {
        let text = format!("{R0}\n## Open questions\n- (blocking) Which API?\n");
        let brief = parse_ticket_brief(&text).unwrap();
        assert_eq!(
            brief.validate_for(Materialization::R0).unwrap_err().code(),
            "ticket_brief_open_question_blocking"
        );
    }
}
