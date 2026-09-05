//! Structural validation for stored graph nodes after the compatibility
//! removal. Ticket semantics live in `works/<id>/ticket.md` and are parsed by
//! `graph::model::brief`; this module only validates the graph-owned fields:
//! classification (role, risk, materialization) and QA impact metadata.

use std::collections::BTreeSet;

use crate::graph::model::contract::{
    stable_code, ContractValidationMode, ContractValidationReport, PublicCreateClassification,
    QaImpactPosture, MAX_COLLECTION, MAX_ID,
};
use crate::graph::model::node::Node;
use crate::id::{kind_for_id, WorkKind};
use crate::{PulseError, PulseResult};

pub fn validate_node_contract(
    node: &Node,
    mode: ContractValidationMode,
) -> ContractValidationReport {
    let mut report = ContractValidationReport::ok();

    if node.contract_revision < 1 {
        report.push(
            "contract_revision_invalid",
            "contract_revision must be >= 1 and distinct from normal revision",
        );
    }

    match node.kind {
        WorkKind::Ticket => validate_ticket_contract(node, mode, &mut report),
        WorkKind::Epic | WorkKind::Story | WorkKind::Decision => {
            validate_non_ticket_contract_fields(node, &mut report);
        }
    }

    report
}

pub fn validate_node_contract_result(node: &Node, mode: ContractValidationMode) -> PulseResult<()> {
    validate_node_contract(node, mode).into_result()
}

pub fn stable_contract_code(code: &str) -> &'static str {
    stable_code(code)
}

pub fn validate_public_create_classification(
    kind: WorkKind,
    classification: &PublicCreateClassification,
) -> PulseResult<()> {
    if kind != WorkKind::Ticket {
        if classification.any_present() {
            return Err(PulseError::validation(
                "work_classification_not_allowed",
                "role, risk, and materialization classification flags are only valid for Ticket creation",
            ));
        }
        return Ok(());
    }

    match (
        classification.role,
        classification.risk,
        classification.materialization,
    ) {
        (role, Some(risk), materialization) if risk.is_assessed() => {
            if materialization.is_some_and(|value| !value.is_assessed()) {
                return Err(PulseError::validation(
                    "risk_materialization_unassessed",
                    "public Ticket creation requires assessed materialization",
                ));
            }
            let _ = role;
            Ok(())
        }
        (_, Some(_), _) => Err(PulseError::validation(
            "risk_materialization_unassessed",
            "public Ticket creation requires assessed risk",
        )),
        _ => Err(PulseError::validation(
            "work_classification_missing",
            "public Ticket creation requires --risk; role defaults to implementation",
        )),
    }
}

fn validate_non_ticket_contract_fields(node: &Node, report: &mut ContractValidationReport) {
    if node.role.is_some()
        || node.risk.is_some()
        || node.materialization.is_some()
        || node.qa.is_some()
    {
        report.push(
            "work_role_invalid",
            "role, risk, materialization, and QA are Ticket-only fields",
        );
    }
}

fn validate_ticket_contract(
    node: &Node,
    mode: ContractValidationMode,
    report: &mut ContractValidationReport,
) {
    let Some(_role) = node.role else {
        report.push("work_role_invalid", "Ticket nodes must declare a role");
        return;
    };
    let Some(risk) = node.risk else {
        report.push(
            "work_classification_missing",
            "Ticket nodes must declare risk",
        );
        return;
    };
    let Some(materialization) = node.materialization else {
        report.push(
            "work_classification_missing",
            "Ticket nodes must declare materialization",
        );
        return;
    };
    let Some(qa) = &node.qa else {
        report.push(
            "qa_impact_unknown",
            "Ticket nodes must carry QA impact metadata",
        );
        return;
    };

    if mode == ContractValidationMode::PublicCreate
        && (!risk.is_assessed() || !materialization.is_assessed())
    {
        report.push(
            "risk_materialization_unassessed",
            "public Ticket creation requires assessed risk and materialization",
        );
    }

    validate_qa_impact(qa, report);
}

fn validate_qa_impact(
    qa: &crate::graph::model::contract::QaMetadata,
    report: &mut ContractValidationReport,
) {
    let rationale_present = qa
        .impact
        .rationale
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    validate_case_ids(&qa.impact.affected_case_ids, report);
    match qa.impact.posture {
        QaImpactPosture::Unknown => {
            if rationale_present
                || qa.impact.behavioral_owner.is_some()
                || !qa.impact.affected_case_ids.is_empty()
            {
                report.push(
                    "qa_impact_invalid",
                    "unknown QA impact must not carry rationale, owner, or case ids",
                );
            }
        }
        QaImpactPosture::None => {
            if !rationale_present {
                report.push("qa_impact_invalid", "qa=none requires a rationale");
            }
            if qa.impact.behavioral_owner.is_some() || !qa.impact.affected_case_ids.is_empty() {
                report.push(
                    "qa_impact_invalid",
                    "qa=none must not carry behavioral owner or case ids",
                );
            }
        }
        QaImpactPosture::CoveredByStoryClose => {
            if !rationale_present {
                report.push(
                    "qa_impact_invalid",
                    "covered_by_story_close requires a rationale",
                );
            }
            match qa.impact.behavioral_owner.as_deref() {
                Some(owner) => match kind_for_id(owner) {
                    Ok(WorkKind::Story) => {}
                    _ => report.push(
                        "qa_impact_invalid",
                        "covered_by_story_close behavioral_owner must be a Story id",
                    ),
                },
                None => report.push(
                    "qa_impact_invalid",
                    "covered_by_story_close requires a behavioral_owner Story id",
                ),
            }
        }
        QaImpactPosture::Required => {
            match qa.impact.behavioral_owner.as_deref() {
                Some(owner) => match kind_for_id(owner) {
                    Ok(WorkKind::Story) => {}
                    _ => report.push(
                        "qa_impact_invalid",
                        "required QA impact behavioral_owner must be a Story id",
                    ),
                },
                None => report.push(
                    "qa_impact_invalid",
                    "required QA impact requires a behavioral_owner Story id",
                ),
            }
            if qa.impact.affected_case_ids.is_empty() {
                report.push(
                    "qa_impact_invalid",
                    "required QA impact requires at least one affected case id",
                );
            }
        }
    }
}

fn validate_case_ids(values: &[String], report: &mut ContractValidationReport) {
    if values.len() > MAX_COLLECTION {
        report.push(
            "qa_impact_invalid",
            "affected_case_ids exceeds the bounded collection limit",
        );
    }
    let mut seen = BTreeSet::new();
    for value in values {
        if !is_portable_case_id(value) {
            report.push(
                "qa_impact_invalid",
                format!(
                    "affected_case_id must be a 1-64 character portable uppercase case identifier: {value}"
                ),
            );
        }
        if !seen.insert(value) {
            report.push(
                "qa_impact_invalid",
                format!("affected_case_ids contains duplicate case id {value}"),
            );
        }
    }
}

fn is_portable_case_id(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_ID {
        return false;
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !is_case_id_boundary_char(first) {
        return false;
    }
    let mut last = first;
    for character in chars {
        if !is_case_id_char(character) {
            return false;
        }
        last = character;
    }
    is_case_id_boundary_char(last)
}

fn is_case_id_char(character: char) -> bool {
    character.is_ascii_uppercase() || character.is_ascii_digit() || character == '-'
}

fn is_case_id_boundary_char(character: char) -> bool {
    character.is_ascii_uppercase() || character.is_ascii_digit()
}
