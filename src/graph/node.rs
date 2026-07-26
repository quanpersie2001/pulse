use crate::graph::contract::{
    DecisionWorkContract, ImplementationContract, Materialization, QaMetadata, Risk,
    ShapingPointer, TicketRole, NODE_SCHEMA_VERSION,
};
use crate::id::{validate_work_id, WorkKind};
use crate::{PulseError, PulseResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub schema_version: u32,
    pub id: String,
    pub kind: WorkKind,
    pub revision: u64,
    pub contract_revision: u64,
    pub title: String,
    pub status: NodeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<StatusReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<DocumentationMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<TicketRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk: Option<Risk>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materialization: Option<Materialization>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qa: Option<QaMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implementation: Option<ImplementationContract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_work: Option<DecisionWorkContract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shaping: Option<ShapingPointer>,
    pub content_dir: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StatusReason {
    pub code: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

impl StatusReason {
    pub fn new(
        code: impl Into<String>,
        summary: impl Into<String>,
        reference: Option<String>,
    ) -> PulseResult<Self> {
        let code = code.into();
        let summary = summary.into();
        if code.trim().is_empty() {
            return Err(PulseError::validation(
                "invalid_status_reason",
                "status reason code must not be empty",
            ));
        }
        if summary.trim().is_empty() {
            return Err(PulseError::validation(
                "reason_required",
                "status reason summary must not be empty",
            ));
        }
        Ok(Self {
            code,
            summary,
            reference,
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Draft,
    Shaped,
    Ready,
    Active,
    Verifying,
    Done,
    Rework,
    Blocked,
    Cancelled,
    Superseded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocumentationMetadata {
    pub impact: DocumentationImpact,
    #[serde(default, skip_serializing_if = "DocumentationRouting::is_empty")]
    pub routing: DocumentationRouting,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocumentationImpact {
    pub posture: DocumentationImpactPosture,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_documents: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deferred_to: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DocumentationImpactPosture {
    Unknown,
    Required,
    None,
    Deferred,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocumentationRouting {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub domains: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
}

impl DocumentationRouting {
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty() && self.domains.is_empty() && self.labels.is_empty()
    }

    pub fn validate(&self) -> PulseResult<()> {
        validate_unique_non_empty("documentation_routing_path", &self.paths)?;
        validate_unique_non_empty("documentation_routing_domain", &self.domains)?;
        validate_unique_non_empty("documentation_routing_label", &self.labels)?;
        for path in &self.paths {
            crate::storage::safe_repo_relative(path)?;
        }
        for domain in &self.domains {
            validate_slug("documentation_domain_invalid", domain)?;
        }
        for label in &self.labels {
            validate_slug("documentation_label_invalid", label)?;
        }
        Ok(())
    }
}

impl DocumentationImpact {
    pub fn validate(&self, public_mutation: bool) -> PulseResult<()> {
        if public_mutation && self.posture == DocumentationImpactPosture::Unknown {
            return Err(PulseError::validation(
                "documentation_impact_unknown_not_settable",
                "unknown documentation impact is only the missing/default state",
            ));
        }
        let rationale_present = self
            .rationale
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        match self.posture {
            DocumentationImpactPosture::Unknown => {
                if rationale_present
                    || !self.required_documents.is_empty()
                    || !self.deferred_to.is_empty()
                {
                    return Err(PulseError::validation(
                        "documentation_impact_invalid",
                        "unknown documentation impact must not carry rationale or references",
                    ));
                }
            }
            DocumentationImpactPosture::Required => {
                if self.required_documents.is_empty() {
                    return Err(PulseError::validation(
                        "documentation_required_missing",
                        "required documentation impact needs at least one required document",
                    ));
                }
                if !self.deferred_to.is_empty() {
                    return Err(PulseError::validation(
                        "documentation_impact_invalid",
                        "required documentation impact must not carry deferred work references",
                    ));
                }
            }
            DocumentationImpactPosture::None => {
                if !rationale_present {
                    return Err(PulseError::validation(
                        "documentation_rationale_required",
                        "none documentation impact requires non-empty rationale",
                    ));
                }
                if !self.required_documents.is_empty() || !self.deferred_to.is_empty() {
                    return Err(PulseError::validation(
                        "documentation_impact_invalid",
                        "none documentation impact must not carry required documents or deferred work references",
                    ));
                }
            }
            DocumentationImpactPosture::Deferred => {
                if !rationale_present {
                    return Err(PulseError::validation(
                        "documentation_rationale_required",
                        "deferred documentation impact requires non-empty rationale",
                    ));
                }
                if self.deferred_to.is_empty() {
                    return Err(PulseError::validation(
                        "documentation_defer_target_missing",
                        "deferred documentation impact requires at least one follow-up work item",
                    ));
                }
                if !self.required_documents.is_empty() {
                    return Err(PulseError::validation(
                        "documentation_impact_invalid",
                        "deferred documentation impact must not carry required documents",
                    ));
                }
            }
        }
        validate_unique_non_empty("documentation_required_document", &self.required_documents)?;
        validate_unique_non_empty("documentation_deferred_to", &self.deferred_to)?;
        for document_id in &self.required_documents {
            validate_document_id(document_id)?;
        }
        for work_id in &self.deferred_to {
            validate_work_id(work_id)?;
        }
        Ok(())
    }
}

impl DocumentationMetadata {
    pub fn posture(&self) -> DocumentationImpactPosture {
        self.impact.posture
    }

    pub fn validate(&self, public_mutation: bool) -> PulseResult<()> {
        self.impact.validate(public_mutation)?;
        self.routing.validate()?;
        Ok(())
    }
}

impl Node {
    pub fn documentation_posture(&self) -> DocumentationImpactPosture {
        self.documentation
            .as_ref()
            .map(DocumentationMetadata::posture)
            .unwrap_or(DocumentationImpactPosture::Unknown)
    }

    pub fn new(id: String, kind: WorkKind, title: String, now: DateTime<Utc>) -> PulseResult<Self> {
        if title.trim().is_empty() {
            return Err(PulseError::validation(
                "invalid_title",
                "title must not be empty",
            ));
        }
        let ticket_defaults = kind == WorkKind::Ticket;
        Ok(Self {
            schema_version: NODE_SCHEMA_VERSION,
            content_dir: format!("works/{id}"),
            id,
            kind,
            revision: 1,
            contract_revision: 1,
            title,
            status: NodeStatus::Draft,
            status_reason: None,
            documentation: None,
            role: ticket_defaults.then_some(TicketRole::Implementation),
            risk: ticket_defaults.then_some(Risk::Unassessed),
            materialization: ticket_defaults.then_some(Materialization::Unassessed),
            qa: ticket_defaults.then_some(QaMetadata::default()),
            implementation: None,
            decision_work: None,
            shaping: None,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn normalize_contract_fields(&mut self) {
        if let Some(contract) = &mut self.implementation {
            contract.normalize();
        }
        if let Some(contract) = &mut self.decision_work {
            contract.normalize();
        }
        if let Some(qa) = &mut self.qa {
            qa.impact.affected_case_ids.sort();
            qa.impact.affected_case_ids.dedup();
        }
    }
}

fn validate_unique_non_empty(field: &'static str, values: &[String]) -> PulseResult<()> {
    let mut seen = std::collections::BTreeSet::new();
    for value in values {
        if value.trim().is_empty() {
            return Err(PulseError::validation(
                "documentation_reference_invalid",
                format!("{field} entries must not be empty"),
            ));
        }
        if !seen.insert(value) {
            return Err(PulseError::validation(
                "documentation_reference_duplicate",
                format!("{field} contains duplicate entry {value}"),
            ));
        }
    }
    Ok(())
}

fn validate_document_id(value: &str) -> PulseResult<()> {
    let suffix = value.strip_prefix("DOC-").ok_or_else(|| {
        PulseError::validation(
            "document_id_invalid",
            format!("document id must start with DOC-: {value}"),
        )
    })?;
    if !(3..=64).contains(&suffix.len())
        || !suffix
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
        || suffix.starts_with('-')
        || suffix.ends_with('-')
    {
        return Err(PulseError::validation(
            "document_id_invalid",
            format!("document id must match DOC-[A-Z0-9][A-Z0-9-]{{2,63}}: {value}"),
        ));
    }
    Ok(())
}

fn validate_slug(code: &'static str, value: &str) -> PulseResult<()> {
    if value.trim().is_empty()
        || !value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || value.starts_with('-')
        || value.ends_with('-')
    {
        return Err(PulseError::validation(
            code,
            format!("slug must contain lowercase letters, digits, or hyphens: {value}"),
        ));
    }
    Ok(())
}
