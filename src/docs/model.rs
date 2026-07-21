use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocsRegistry {
    pub schema_version: u32,
    pub revision: u64,
    pub repository_id: String,
    #[serde(default)]
    pub documents: Vec<DocumentRecord>,
}

pub type DocsRegistryEnvelope = DocsRegistry;

impl DocsRegistry {
    pub fn empty(repository_id: String) -> Self {
        Self {
            schema_version: 1,
            revision: 1,
            repository_id,
            documents: Vec::new(),
        }
    }

    pub fn normalize(&mut self) {
        self.documents.sort_by(|left, right| left.id.cmp(&right.id));
        for document in &mut self.documents {
            document.normalize();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocumentRecord {
    pub id: String,
    pub revision: u64,
    pub path: String,
    pub kind: DocumentKind,
    pub authority: DocumentAuthority,
    pub lifecycle: DocumentLifecycle,
    pub owner: String,
    pub summary: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub scope: DocumentScope,
    pub review_policy: ReviewPolicy,
    pub verification_profile: String,
    pub generated: Option<GeneratedContract>,
    pub superseded_by: Option<String>,
}

impl DocumentRecord {
    pub fn normalize(&mut self) {
        self.aliases.sort();
        self.scope.paths.sort();
        self.scope.domains.sort();
        self.scope.work_labels.sort();
        if let Some(generated) = &mut self.generated {
            generated.sources.sort();
            generated.outputs.sort();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct DocumentScope {
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub work_labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GeneratedContract {
    pub sources: Vec<String>,
    pub command: String,
    pub outputs: Vec<String>,
    pub editable: bool,
    pub freshness_check: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    RepositoryMap,
    Policy,
    Product,
    Architecture,
    Domain,
    Operations,
    Reference,
    DecisionProjection,
    Generated,
    Informational,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DocumentAuthority {
    Draft,
    Approved,
    Informational,
    Generated,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DocumentLifecycle {
    Current,
    SuspectedStale,
    Stale,
    Retired,
    Superseded,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ReviewPolicy {
    #[default]
    None,
    Light,
    Standard,
    Independent,
    Human,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct DocumentPatch {
    pub path: Option<String>,
    pub owner: Option<String>,
    pub summary: Option<String>,
    pub aliases: Option<Vec<String>>,
    pub scope: Option<DocumentScope>,
    pub authority: Option<DocumentAuthority>,
    pub lifecycle: Option<DocumentLifecycle>,
    pub review_policy: Option<ReviewPolicy>,
    pub verification_profile: Option<String>,
    pub generated: Option<Option<GeneratedContract>>,
    pub superseded_by: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkDocumentationContext {
    pub work_id: String,
    pub revision: u64,
    pub posture: DocumentationPosture,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_documents: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub domains: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
}

impl WorkDocumentationContext {
    pub fn unknown(work_id: String, revision: u64) -> Self {
        Self {
            work_id,
            revision,
            posture: DocumentationPosture::Unknown,
            required_documents: Vec::new(),
            paths: Vec::new(),
            domains: Vec::new(),
            labels: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentationPosture {
    Unknown,
    Required,
    None,
    Deferred,
    Investigate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NodeDocumentation {
    pub impact: DocumentationImpact,
    #[serde(default)]
    pub routing: DocumentationRouting,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocumentationImpact {
    pub posture: DocumentationPosture,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(default)]
    pub required_documents: Vec<String>,
    #[serde(default)]
    pub deferred_to: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct DocumentationRouting {
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub labels: Vec<String>,
}

impl From<(&str, u64, &NodeDocumentation)> for WorkDocumentationContext {
    fn from(value: (&str, u64, &NodeDocumentation)) -> Self {
        let (work_id, revision, documentation) = value;
        Self {
            work_id: work_id.to_string(),
            revision,
            posture: documentation.impact.posture,
            required_documents: sorted_unique(documentation.impact.required_documents.clone()),
            paths: sorted_unique(documentation.routing.paths.clone()),
            domains: sorted_unique(documentation.routing.domains.clone()),
            labels: sorted_unique(documentation.routing.labels.clone()),
        }
    }
}

impl From<(&str, u64, &crate::graph::node::DocumentationMetadata)> for WorkDocumentationContext {
    fn from(value: (&str, u64, &crate::graph::node::DocumentationMetadata)) -> Self {
        let (work_id, revision, documentation) = value;
        Self {
            work_id: work_id.to_string(),
            revision,
            posture: match documentation.impact.posture {
                crate::graph::node::DocumentationImpactPosture::Unknown => {
                    DocumentationPosture::Unknown
                }
                crate::graph::node::DocumentationImpactPosture::Required => {
                    DocumentationPosture::Required
                }
                crate::graph::node::DocumentationImpactPosture::None => DocumentationPosture::None,
                crate::graph::node::DocumentationImpactPosture::Deferred => {
                    DocumentationPosture::Deferred
                }
            },
            required_documents: sorted_unique(documentation.impact.required_documents.clone()),
            paths: sorted_unique(documentation.routing.paths.clone()),
            domains: sorted_unique(documentation.routing.domains.clone()),
            labels: sorted_unique(documentation.routing.labels.clone()),
        }
    }
}

pub fn sorted_unique(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
