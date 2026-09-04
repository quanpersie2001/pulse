use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Current docs registry envelope schema version.
pub const DOCS_REGISTRY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocsRegistry {
    pub schema_version: u32,
    pub revision: u64,
    pub repository_id: String,
    #[serde(default)]
    pub documents: Vec<DocumentRecord>,
    /// Retrieval configuration projection. Present on every registry written by
    /// bootstrap as deterministic defaults; optional for read-model helpers that
    /// tolerate absent config by resolving defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieval: Option<RetrievalConfig>,
}

pub type DocsRegistryEnvelope = DocsRegistry;

impl DocsRegistry {
    pub fn empty(repository_id: String) -> Self {
        Self {
            schema_version: DOCS_REGISTRY_SCHEMA_VERSION,
            revision: 1,
            repository_id,
            documents: Vec::new(),
            retrieval: Some(RetrievalConfig::defaults()),
        }
    }

    pub fn normalize(&mut self) {
        self.documents.sort_by(|left, right| left.id.cmp(&right.id));
        if let Some(retrieval) = &mut self.retrieval {
            retrieval.normalize();
        }
        for document in &mut self.documents {
            document.normalize();
        }
    }

    /// Resolved retrieval config: the envelope config when present, otherwise
    /// deterministic defaults. Used by consumers that only need to read config.
    pub fn retrieval_config(&self) -> RetrievalConfig {
        self.retrieval
            .clone()
            .unwrap_or_else(RetrievalConfig::defaults)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocumentRecord {
    pub id: String,
    pub revision: u64,
    pub path: String,
    pub summary: String,
    pub owner: String,
    pub kind: DocumentKind,
    pub status: DocumentStatus,
    #[serde(default)]
    pub scope: DocumentScope,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated: Option<GeneratedContract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
}

impl DocumentRecord {
    pub fn normalize(&mut self) {
        self.scope.paths.sort();
        self.tags.sort();
        self.tags.dedup();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct DocumentScope {
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GeneratedContract {
    pub command: String,
    pub freshness_check: String,
}

/// Envelope-level retrieval/indexing configuration.
///
/// Stored under `retrieval` on the docs registry. Determines the managed
/// documentation root, repository-map/policy inclusion, default indexing/body
/// behavior, bounded retrieval budgets, auto-refresh cost guards and generated
/// navigation projection policy. All values are deterministic defaults that
/// participate in the retrieval fingerprint; none are machine-specific.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RetrievalConfig {
    pub schema_version: u32,
    pub default_search_limit: u32,
    pub default_get_max_lines: u32,
    pub default_get_max_bytes: u32,
}

impl RetrievalConfig {
    /// Deterministic default retrieval configuration. Same inputs must produce
    /// same bytes/fingerprint; no machine path or timestamp participates.
    pub fn defaults() -> Self {
        Self {
            schema_version: 1,
            default_search_limit: 8,
            default_get_max_lines: 120,
            default_get_max_bytes: 32_768,
        }
    }

    pub fn normalize(&mut self) {}
}

/// A retrieval area scope: a managed sub-tree with a navigation summary and
/// optional explicit materialization policy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    Policy,
    Architecture,
    Domain,
    Product,
    Operations,
    Reference,
    Generated,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DocumentStatus {
    Approved,
    Draft,
    Stale,
    Retired,
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
    pub scope: Option<DocumentScope>,
    pub status: Option<DocumentStatus>,
    pub tags: Option<Vec<String>>,
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
    pub tags: Vec<String>,
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
            tags: Vec::new(),
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
            tags: sorted_unique(documentation.routing.labels.clone()),
            domains: sorted_unique(documentation.routing.domains.clone()),
            labels: sorted_unique(documentation.routing.labels.clone()),
        }
    }
}

impl From<(&str, u64, &crate::graph::model::node::DocumentationMetadata)>
    for WorkDocumentationContext
{
    fn from(value: (&str, u64, &crate::graph::model::node::DocumentationMetadata)) -> Self {
        let (work_id, revision, documentation) = value;
        Self {
            work_id: work_id.to_string(),
            revision,
            posture: match documentation.impact.posture {
                crate::graph::model::node::DocumentationImpactPosture::Unknown => {
                    DocumentationPosture::Unknown
                }
                crate::graph::model::node::DocumentationImpactPosture::Required => {
                    DocumentationPosture::Required
                }
                crate::graph::model::node::DocumentationImpactPosture::None => {
                    DocumentationPosture::None
                }
                crate::graph::model::node::DocumentationImpactPosture::Deferred => {
                    DocumentationPosture::Deferred
                }
            },
            required_documents: sorted_unique(documentation.impact.required_documents.clone()),
            paths: sorted_unique(documentation.routing.paths.clone()),
            tags: sorted_unique(documentation.routing.labels.clone()),
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
