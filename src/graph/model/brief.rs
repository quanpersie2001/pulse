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
        (level == 1).then_some(text)
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
    let open_questions = parse_questions(&sections)?;
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
        .map(|body| list_items(body))
        .unwrap_or_default()
}

/// Fold a section body into logical list items.
///
/// A `-`/`*` line opens an item; every following non-bullet line joins it with
/// a single space. Wrapping a long bullet is the most ordinary thing a human
/// writes in Markdown, and reading the second line as its own item silently
/// truncated the contract. Text before the first bullet is not a list item and
/// is dropped, as it always was.
fn list_items(body: &str) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(value) = trimmed.strip_prefix(['-', '*']) {
            let value = value.trim();
            if !value.is_empty() {
                items.push(value.to_string());
            }
            continue;
        }
        if let Some(item) = items.last_mut() {
            item.push(' ');
            item.push_str(trimmed);
        }
    }
    items
}

fn parse_acceptance(sections: &BTreeMap<String, String>) -> Vec<AcceptanceItem> {
    sections
        .get("acceptance")
        .map(|body| {
            list_items(body)
                .into_iter()
                .filter_map(|value| {
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

fn parse_questions(sections: &BTreeMap<String, String>) -> PulseResult<Vec<OpenQuestion>> {
    let Some(body) = sections.get("open questions") else {
        return Ok(Vec::new());
    };
    let mut questions: Vec<OpenQuestion> = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(value) = trimmed.strip_prefix(['-', '*']).map(str::trim) else {
            // A wrapped bullet continues the question above it. Only text with
            // no bullet to continue is a genuinely undispositioned question,
            // and the error names the line so the author does not have to
            // guess which one it meant.
            let Some(question) = questions.last_mut() else {
                return Err(invalid(
                    "ticket_brief_open_question_disposition_missing",
                    format!("open question must be a `- (disposition) text` list item: {trimmed}"),
                ));
            };
            question.text.push(' ');
            question.text.push_str(trimmed);
            continue;
        };
        let Some((disposition, text)) = value.strip_prefix('(').and_then(|v| v.split_once(')'))
        else {
            return Err(invalid(
                "ticket_brief_open_question_disposition_missing",
                format!("open question must declare a disposition in parentheses: {trimmed}"),
            ));
        };
        let disposition = match disposition.trim().to_ascii_lowercase().as_str() {
            "resolved" => OpenQuestionDisposition::Resolved,
            "rejected" => OpenQuestionDisposition::Rejected,
            "delegated" => OpenQuestionDisposition::Delegated,
            "deferred" => OpenQuestionDisposition::Deferred,
            "blocking" => OpenQuestionDisposition::Blocking,
            other => {
                return Err(invalid(
                    "ticket_brief_open_question_disposition_missing",
                    format!("open question disposition {other:?} must be resolved, rejected, delegated, deferred, or blocking"),
                ))
            }
        };
        let text = text.trim();
        if text.is_empty() {
            return Err(invalid(
                "ticket_brief_open_question_empty",
                "open question text must not be empty",
            ));
        }
        questions.push(OpenQuestion {
            disposition,
            text: text.to_string(),
        });
    }
    Ok(questions)
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

/// Parse a `Key: value` impact section, bulleted or not.
///
/// A line carrying a colon opens an entry; a line without one continues the
/// value above it, so a wrapped rationale keeps its second half instead of
/// being dropped on the floor.
fn parse_key_values(sections: &BTreeMap<String, String>, name: &str) -> BTreeMap<String, String> {
    let mut values: BTreeMap<String, String> = BTreeMap::new();
    let mut open: Option<String> = None;
    for line in sections.get(name).into_iter().flat_map(|body| body.lines()) {
        let line = line.trim().trim_start_matches(['-', '*']).trim();
        if line.is_empty() {
            continue;
        }
        match line.split_once(':') {
            Some((key, value)) => {
                let key = key.trim().to_ascii_lowercase();
                values.insert(key.clone(), value.trim().to_string());
                open = Some(key);
            }
            None => {
                if let Some(value) = open.as_ref().and_then(|key| values.get_mut(key)) {
                    if !value.is_empty() {
                        value.push(' ');
                    }
                    value.push_str(line);
                }
            }
        }
    }
    values
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
        rationale: rationale_value(&values),
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
        owner: values
            .get("owner")
            .cloned()
            .filter(|value| !value.trim().is_empty()),
        posture: values
            .get("posture")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string()),
        cases,
        reason: rationale_value(&values),
    })
}

fn rationale_value(values: &BTreeMap<String, String>) -> Option<String> {
    values
        .get("rationale")
        .or_else(|| values.get("reason"))
        .cloned()
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
    format!("# {id} {title}\n\n## Objective\nDescribe the outcome this Ticket must achieve.\n\n{}## Code anchors\n- src/\n\n## Acceptance\n- AC-1: Describe a testable acceptance condition.\n\n## Verify\n- cargo test\n\n## Required changes\n- Describe the required change.\n\n## Invariants\n- Describe the invariant that must remain true.\n\n## Implementation freedom\nguided: agent chooses internal structure within this contract.\n\n## Scope\n- Included work.\n\n## Non-scope\n- Work excluded from this Ticket.\n\n## Open questions\n- (delegated) Record implementation choices that are safe to delegate.\n\n## Documentation impact\n- Posture: none\n- Rationale: Explain why durable docs are unaffected.\n- Documents:\n\n## QA impact\n- Owner:\n- Posture: none\n- Cases:\n- Rationale: Explain the QA posture.\n\n## Expected handoff\n- Diff and verification results.\n", if full { "## Current behavior\nDescribe the current behavior.\n\n## Target behavior\nDescribe the target behavior.\n\n" } else { "" })
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

    #[test]
    fn undispositioned_open_question_is_not_silently_ignored() {
        let text = format!("{R0}\n## Open questions\n- Which API?\n");
        assert_eq!(
            parse_ticket_brief(&text).unwrap_err().code(),
            "ticket_brief_open_question_disposition_missing"
        );
    }

    #[test]
    fn a_wrapped_open_question_continues_the_bullet_above_it() {
        // Six of six Track B Tickets failed their first `work sync` on this:
        // the continuation line of a wrapped bullet was read as a new,
        // undispositioned question.
        let text = format!(
            "{R0}\n## Open questions\n- (delegated) Should the retry budget be\n  per request or per session?\n- (deferred) Metrics naming.\n"
        );
        let brief = parse_ticket_brief(&text).unwrap();
        assert_eq!(brief.open_questions.len(), 2);
        assert_eq!(
            brief.open_questions[0].text,
            "Should the retry budget be per request or per session?"
        );
        assert_eq!(
            brief.open_questions[0].disposition,
            OpenQuestionDisposition::Delegated
        );
    }

    #[test]
    fn open_question_errors_name_the_offending_line() {
        // Prose with no bullet above it to continue is still a real error.
        let stray = format!("{R0}\n## Open questions\nWhich API should we call?\n");
        let error = parse_ticket_brief(&stray).unwrap_err();
        assert_eq!(
            error.code(),
            "ticket_brief_open_question_disposition_missing"
        );
        assert!(
            error.to_string().contains("Which API should we call?"),
            "{error}"
        );

        let unknown = format!("{R0}\n## Open questions\n- (maybe) Which API?\n");
        let error = parse_ticket_brief(&unknown).unwrap_err();
        assert!(error.to_string().contains("maybe"), "{error}");
    }

    #[test]
    fn wrapped_bullets_keep_their_second_half_in_every_list_section() {
        let text = "# TK-002 Wrapped\n\n## Objective\nDo it.\n\n\
             ## Code anchors\n- src/auth/session.rs, the branch that refreshes\n  an expired token\n\n\
             ## Acceptance\n- AC-1: An expired token refreshes once and the\n  retry succeeds.\n\n\
             ## Verify\n- cargo test --test graph -- lifecycle\n\n\
             ## QA impact\n- Owner: ST-014\n- Posture: none\n- Cases:\n\
             - Rationale: The change is internal and no observable\n  behavior moves.\n";
        let brief = parse_ticket_brief(text).unwrap();
        assert_eq!(
            brief.code_anchors,
            vec!["src/auth/session.rs, the branch that refreshes an expired token"]
        );
        assert_eq!(
            brief.acceptance[0].summary,
            "An expired token refreshes once and the retry succeeds."
        );
        assert_eq!(
            brief.qa.as_ref().unwrap().reason.as_deref(),
            Some("The change is internal and no observable behavior moves.")
        );
    }

    #[test]
    fn rationale_and_reason_are_interchangeable_in_both_impact_sections() {
        let canonical = format!(
            "{R0}\n## Documentation impact\n- Posture: none\n- Rationale: No docs impact.\n- Documents:\n\n\
             ## QA impact\n- Owner:\n- Posture: none\n- Cases:\n- Rationale: No QA surface.\n"
        );
        let aliased = format!(
            "{R0}\n## Documentation impact\n- Posture: none\n- Reason: No docs impact.\n- Documents:\n\n\
             ## QA impact\n- Owner:\n- Posture: none\n- Cases:\n- Reason: No QA surface.\n"
        );
        let crossed = format!(
            "{R0}\n## Documentation impact\n- Posture: none\n- Reason: docs via alias.\n- Documents:\n\n\
             ## QA impact\n- Owner:\n- Posture: none\n- Cases:\n- Rationale: QA via canonical.\n"
        );
        for (text, docs, qa) in [
            (&canonical, "No docs impact.", "No QA surface."),
            (&aliased, "No docs impact.", "No QA surface."),
            (&crossed, "docs via alias.", "QA via canonical."),
        ] {
            let brief = parse_ticket_brief(text).unwrap();
            assert_eq!(
                brief.documentation.as_ref().unwrap().rationale.as_deref(),
                Some(docs)
            );
            assert_eq!(brief.qa.as_ref().unwrap().reason.as_deref(), Some(qa));
        }
    }
}
