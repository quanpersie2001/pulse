use serde::{Deserialize, Serialize};

use crate::{PulseError, PulseResult};

pub const NODE_SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_ID: usize = 64;
pub(crate) const MAX_COLLECTION: usize = 64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ContractValidationMode {
    /// Canonical structural validation for stored graph nodes. This mode
    /// permits draft/incomplete Tickets so current storage can preserve
    /// bounded uncertainty without fabricating readiness.
    CanonicalStorage,
    /// Public `work create` validation. Ticket classification must be explicit
    /// and assessed; the markdown contract is filled in before readiness.
    PublicCreate,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PublicCreateClassification {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<TicketRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk: Option<Risk>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materialization: Option<Materialization>,
}

impl PublicCreateClassification {
    pub fn any_present(&self) -> bool {
        self.role.is_some() || self.risk.is_some() || self.materialization.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractValidationReport {
    pub schema_version: u32,
    pub code: String,
    pub valid: bool,
    pub errors: Vec<ContractFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractFinding {
    pub code: String,
    pub message: String,
}

impl ContractValidationReport {
    pub fn ok() -> Self {
        Self {
            schema_version: 1,
            code: "valid".to_string(),
            valid: true,
            errors: vec![],
        }
    }

    pub fn push(&mut self, code: &'static str, message: impl Into<String>) {
        self.valid = false;
        self.code = "invalid_contract".to_string();
        self.errors.push(ContractFinding {
            code: code.to_string(),
            message: message.into(),
        });
    }

    pub fn extend(&mut self, other: Self) {
        for error in other.errors {
            self.valid = false;
            self.code = "invalid_contract".to_string();
            self.errors.push(error);
        }
    }

    pub fn into_result(self) -> PulseResult<()> {
        if self.valid {
            Ok(())
        } else {
            let first = &self.errors[0];
            Err(PulseError::validation(
                stable_code(&first.code),
                first.message.clone(),
            ))
        }
    }
}

pub(crate) fn stable_code(code: &str) -> &'static str {
    match code {
        "contract_revision_invalid" => "contract_revision_invalid",
        "work_role_invalid" => "work_role_invalid",
        "work_classification_missing" => "work_classification_missing",
        "work_classification_not_allowed" => "work_classification_not_allowed",
        "risk_materialization_unassessed" => "risk_materialization_unassessed",
        "qa_impact_unknown" => "qa_impact_unknown",
        "qa_impact_invalid" => "qa_impact_invalid",
        _ => "contract_validation_failed",
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TicketRole {
    Implementation,
    DecisionWork,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Unassessed,
    Low,
    Medium,
    High,
    Critical,
}

impl Risk {
    pub fn is_assessed(self) -> bool {
        self != Self::Unassessed
    }

    /// Default materialization for a newly-created Ticket.
    pub fn default_materialization(self) -> Materialization {
        match self {
            Self::Low => Materialization::R0,
            Self::Medium => Materialization::R1,
            Self::High => Materialization::R2,
            Self::Critical => Materialization::R3,
            Self::Unassessed => Materialization::Unassessed,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Materialization {
    Unassessed,
    #[serde(rename = "R0")]
    R0,
    #[serde(rename = "R1")]
    R1,
    #[serde(rename = "R2")]
    R2,
    #[serde(rename = "R3")]
    R3,
}

impl Materialization {
    pub fn is_assessed(self) -> bool {
        self != Self::Unassessed
    }

    pub fn requires_invariant(self) -> bool {
        matches!(self, Self::R1 | Self::R2 | Self::R3)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum QaImpactPosture {
    Unknown,
    Required,
    CoveredByStoryClose,
    None,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaMetadata {
    pub impact: QaImpact,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QaImpact {
    pub posture: QaImpactPosture,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behavioral_owner: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affected_case_ids: Vec<String>,
}

impl Default for QaImpact {
    fn default() -> Self {
        Self {
            posture: QaImpactPosture::Unknown,
            rationale: None,
            behavioral_owner: None,
            affected_case_ids: vec![],
        }
    }
}
