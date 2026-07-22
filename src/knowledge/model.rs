use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Learning {
    pub schema_version: u32,
    pub id: String,
    pub revision: u64,
    pub title: String,
    pub status: LearningStatus,
    pub kind: LearningKind,
    pub severity: Severity,
    pub summary: String,
    pub guidance: Guidance,
    pub applicability: Applicability,
    pub provenance: LearningProvenance,
    pub validation: ValidationPosture,
    pub routing: Routing,
    pub promotion: Promotion,
    pub freshness: Freshness,
    pub trust: Trust,
    pub content: Option<ContentBinding>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LearningDraft {
    pub title: String,
    pub kind: LearningKind,
    pub severity: Severity,
    pub summary: String,
    pub guidance: Guidance,
    pub applicability: Applicability,
    #[serde(default)]
    pub provenance_targets: Vec<ProvenanceTargetDraft>,
    #[serde(default)]
    pub source_commits: Vec<String>,
    pub routing: Option<Routing>,
    pub promotion: Option<Promotion>,
    pub freshness: Option<Freshness>,
    pub trust: Option<Trust>,
    pub content: Option<ContentBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct LearningPatch {
    pub title: Option<String>,
    pub severity: Option<Severity>,
    pub summary: Option<String>,
    pub guidance: Option<Guidance>,
    pub applicability: Option<Applicability>,
    pub routing: Option<Routing>,
    pub promotion: Option<Promotion>,
    pub freshness: Option<Freshness>,
    pub trust: Option<Trust>,
    pub content: Option<Option<ContentBinding>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceTargetDraft {
    #[serde(default = "default_derived_from")]
    pub relation: crate::knowledge::relation::RelationType,
    pub kind: crate::knowledge::relation::EndpointKind,
    pub id: String,
    pub revision: Option<u64>,
    pub content_hash: Option<String>,
}

fn default_derived_from() -> crate::knowledge::relation::RelationType {
    crate::knowledge::relation::RelationType::DerivedFrom
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct Guidance {
    #[serde(default)]
    pub r#do: Vec<String>,
    #[serde(default)]
    pub avoid: Vec<String>,
    #[serde(default)]
    pub required_checks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct Applicability {
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub surfaces: Vec<String>,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub symbols: Vec<String>,
    #[serde(default)]
    pub work_kinds: Vec<String>,
    #[serde(default)]
    pub work_labels: Vec<String>,
    #[serde(default)]
    pub technologies: Vec<String>,
    #[serde(default)]
    pub operations: Vec<String>,
    #[serde(default)]
    pub risks: Vec<String>,
    #[serde(default)]
    pub signals: Vec<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub configurations: Vec<String>,
    #[serde(default)]
    pub versions: Vec<String>,
    #[serde(default)]
    pub exclusions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct LearningProvenance {
    #[serde(default)]
    pub relation_ids: Vec<String>,
    #[serde(default)]
    pub source_commits: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ValidationPosture {
    pub confidence: Confidence,
    #[serde(default)]
    pub validated_by: Vec<String>,
    pub validated_at: Option<DateTime<Utc>>,
    pub reproduction_count: u64,
    pub contradiction_status: ContradictionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Routing {
    #[serde(default)]
    pub audiences: Vec<Audience>,
    #[serde(default)]
    pub moments: Vec<Moment>,
    pub prompt_priority: PromptPriority,
    pub max_summary_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Promotion {
    pub state: PromotionState,
    pub rationale: Option<String>,
    #[serde(default)]
    pub relation_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct Freshness {
    pub review_after: Option<NaiveDate>,
    #[serde(default)]
    pub invalidated_by_paths: Vec<String>,
    #[serde(default)]
    pub version_constraints: Vec<String>,
    #[serde(default)]
    pub platform_constraints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Trust {
    pub source: TrustSource,
    pub contains_untrusted_text: bool,
    pub redaction_status: RedactionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContentBinding {
    pub path: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum LearningKind {
    SuccessPattern,
    FailurePattern,
    Correction,
    Ratchet,
    DecisionHeuristic,
    DebuggingTechnique,
    VerificationTechnique,
    ToolingConstraint,
    EnvironmentConstraint,
    IntegrationConstraint,
    PerformanceInsight,
    SecurityInsight,
    ProcessInsight,
    ContextRoutingInsight,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum LearningStatus {
    Candidate,
    Reviewed,
    Validated,
    Promoted,
    Disputed,
    Superseded,
    Retired,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Medium,
    High,
    Enforced,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContradictionStatus {
    None,
    Suspected,
    Confirmed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    Shaper,
    Planner,
    Implementer,
    Debugger,
    Validator,
    Reviewer,
    Qa,
    Orchestrator,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Moment {
    Shape,
    Plan,
    Execute,
    Debug,
    Verify,
    Review,
    Qa,
    Reconcile,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromptPriority {
    Suggested,
    Recommended,
    RequiredWhenApplicable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromotionState {
    Unresolved,
    None,
    Proposed,
    Promoted,
    Deferred,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustSource {
    TrustedRepository,
    ReviewRequired,
    UntrustedExternal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RedactionStatus {
    NotRequired,
    CallerAsserted,
    ReviewRequired,
    Verified,
}

impl Default for ValidationPosture {
    fn default() -> Self {
        Self {
            confidence: Confidence::Low,
            validated_by: Vec::new(),
            validated_at: None,
            reproduction_count: 1,
            contradiction_status: ContradictionStatus::None,
        }
    }
}

impl Default for Routing {
    fn default() -> Self {
        Self {
            audiences: vec![
                Audience::Planner,
                Audience::Implementer,
                Audience::Validator,
                Audience::Reviewer,
            ],
            moments: vec![
                Moment::Shape,
                Moment::Plan,
                Moment::Execute,
                Moment::Verify,
                Moment::Review,
            ],
            prompt_priority: PromptPriority::Suggested,
            max_summary_tokens: 90,
        }
    }
}

impl Default for Promotion {
    fn default() -> Self {
        Self {
            state: PromotionState::Unresolved,
            rationale: None,
            relation_ids: Vec::new(),
        }
    }
}

impl Default for Trust {
    fn default() -> Self {
        Self {
            source: TrustSource::ReviewRequired,
            contains_untrusted_text: false,
            redaction_status: RedactionStatus::CallerAsserted,
        }
    }
}

impl Learning {
    pub fn normalize(&mut self) {
        self.guidance.normalize();
        self.applicability.normalize();
        normalize_strings(&mut self.provenance.relation_ids);
        normalize_strings(&mut self.provenance.source_commits);
        self.validation.validated_by = sorted_unique_taken(&mut self.validation.validated_by);
        self.routing.audiences.sort();
        self.routing.audiences.dedup();
        self.routing.moments.sort();
        self.routing.moments.dedup();
        normalize_strings(&mut self.promotion.relation_ids);
        normalize_strings(&mut self.freshness.invalidated_by_paths);
        normalize_strings(&mut self.freshness.version_constraints);
        normalize_strings(&mut self.freshness.platform_constraints);
    }
}

impl LearningDraft {
    pub fn into_learning(
        self,
        id: String,
        relation_ids: Vec<String>,
        now: DateTime<Utc>,
    ) -> Learning {
        let mut learning = Learning {
            schema_version: 1,
            id,
            revision: 1,
            title: self.title.trim().to_string(),
            status: LearningStatus::Candidate,
            kind: self.kind,
            severity: self.severity,
            summary: self.summary.trim().to_string(),
            guidance: self.guidance,
            applicability: self.applicability,
            provenance: LearningProvenance {
                relation_ids,
                source_commits: self.source_commits,
            },
            validation: ValidationPosture::default(),
            routing: self.routing.unwrap_or_default(),
            promotion: self.promotion.unwrap_or_default(),
            freshness: self.freshness.unwrap_or_default(),
            trust: self.trust.unwrap_or_default(),
            content: self.content,
            created_at: now,
            updated_at: now,
        };
        learning.normalize();
        learning
    }
}

impl Guidance {
    pub fn normalize(&mut self) {
        normalize_strings(&mut self.r#do);
        normalize_strings(&mut self.avoid);
        normalize_strings(&mut self.required_checks);
    }

    pub fn total_items(&self) -> usize {
        self.r#do.len() + self.avoid.len() + self.required_checks.len()
    }
}

impl Applicability {
    pub fn normalize(&mut self) {
        normalize_strings(&mut self.domains);
        normalize_strings(&mut self.surfaces);
        normalize_strings(&mut self.paths);
        normalize_strings(&mut self.symbols);
        normalize_strings(&mut self.work_kinds);
        normalize_strings(&mut self.work_labels);
        normalize_strings(&mut self.technologies);
        normalize_strings(&mut self.operations);
        normalize_strings(&mut self.risks);
        normalize_strings(&mut self.signals);
        normalize_strings(&mut self.platforms);
        normalize_strings(&mut self.configurations);
        normalize_strings(&mut self.versions);
        normalize_strings(&mut self.exclusions);
    }

    pub fn has_positive_dimension(&self) -> bool {
        !self.domains.is_empty()
            || !self.surfaces.is_empty()
            || !self.paths.is_empty()
            || !self.symbols.is_empty()
            || !self.work_kinds.is_empty()
            || !self.work_labels.is_empty()
            || !self.technologies.is_empty()
            || !self.operations.is_empty()
            || !self.risks.is_empty()
            || !self.signals.is_empty()
            || !self.platforms.is_empty()
            || !self.configurations.is_empty()
            || !self.versions.is_empty()
    }

    pub fn has_concrete_dimension(&self) -> bool {
        !self.paths.is_empty()
            || !self.symbols.is_empty()
            || !self.technologies.is_empty()
            || !self.operations.is_empty()
            || !self.risks.is_empty()
            || !self.signals.is_empty()
            || !self.versions.is_empty()
            || !self.work_labels.is_empty()
    }
}

fn normalize_strings(values: &mut Vec<String>) {
    *values = sorted_unique_taken(values);
}

fn sorted_unique_taken(values: &mut Vec<String>) -> Vec<String> {
    let set: BTreeSet<String> = values
        .drain(..)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect();
    set.into_iter().collect()
}
