use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Current docs registry envelope schema version.
///
/// v1 = Slice 4 (no retrieval metadata). v2 = Slice 5 (adds optional retrieval
/// config + per-document retrieval overrides). v1 registries that exactly match
/// the known Slice 4 predecessor are migrated deliberately; they are never
/// silently reinterpreted as current.
pub const DOCS_REGISTRY_SCHEMA_VERSION_V2: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocsRegistry {
    pub schema_version: u32,
    pub revision: u64,
    pub repository_id: String,
    #[serde(default)]
    pub documents: Vec<DocumentRecord>,
    /// Retrieval configuration projection. Present on every v2 registry (set to
    /// deterministic defaults by bootstrap/migration). Optional only so that an
    /// exact v1 predecessor can be loaded for known-predecessor migration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieval: Option<RetrievalConfig>,
}

pub type DocsRegistryEnvelope = DocsRegistry;

impl DocsRegistry {
    pub fn empty(repository_id: String) -> Self {
        Self {
            schema_version: DOCS_REGISTRY_SCHEMA_VERSION_V2,
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
    /// Per-document retrieval overrides. When `None`, registry defaults apply.
    /// Edits that change only this field are retrieval-only: they bump the
    /// registry revision but NOT the receipt-bound document revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieval: Option<DocumentRetrieval>,
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

/// Envelope-level retrieval/indexing configuration (Slice 5+).
///
/// Stored under `retrieval` on the v2 docs registry. Determines the managed
/// documentation root, repository-map/policy inclusion, default indexing/body
/// behavior, bounded retrieval budgets, auto-refresh cost guards and generated
/// navigation projection policy. All values are deterministic defaults that
/// participate in the retrieval fingerprint; none are machine-specific.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RetrievalConfig {
    pub schema_version: u32,
    /// Safe repository-relative managed documentation tree root (default `docs`).
    pub root: String,
    /// Whether a registered `AGENTS.md` (`kind=repository_map`) is a first-class
    /// retrieval input surfaced under the virtual `Repository` area.
    pub include_repository_map: bool,
    /// Whether a registered `PULSE.md` (`kind=policy`) is a first-class retrieval
    /// input surfaced under the virtual `Repository` area.
    pub include_repository_policy: bool,
    /// Default `retrieval.index` for authored approved/informational documents.
    /// Generated output documents are always opt-in regardless of this default.
    pub default_index: bool,
    /// Default `retrieval.include_body`. When false, only title/heading/summary/
    /// aliases/path/domains are indexed; `get` still reads canonical content.
    pub default_include_body: bool,
    /// Default `search --limit` bound (`1..=50`).
    pub default_search_limit: u32,
    /// Default `get --max-lines` bound (`1..=2000`).
    pub default_get_max_lines: u32,
    /// Default `get --max-bytes` bound (`1024..=1_048_576`).
    pub default_get_max_bytes: u32,
    /// Auto-refresh cost guard: max eligible documents a read-oriented query may
    /// build on demand (`1..=10_000`).
    pub auto_refresh_max_documents: u32,
    /// Auto-refresh cost guard: max total eligible source bytes (`1 MiB..=1 GiB`).
    pub auto_refresh_max_source_bytes: u64,
    /// Whether the root `_index.md` navigation projection is materialized.
    pub materialize_root_index: bool,
    /// Document count at/above which a selected area `_index.md` is materialized
    /// even without an explicit scope/override (`1..=1000`).
    pub area_index_threshold: u32,
    /// Area scopes providing navigation summaries and materialization policy.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<RetrievalScope>,
}

impl RetrievalConfig {
    /// Deterministic default retrieval configuration. Same inputs must produce
    /// same bytes/fingerprint; no machine path or timestamp participates.
    pub fn defaults() -> Self {
        Self {
            schema_version: 1,
            root: "docs".to_string(),
            include_repository_map: true,
            include_repository_policy: true,
            default_index: true,
            default_include_body: true,
            default_search_limit: 8,
            default_get_max_lines: 120,
            default_get_max_bytes: 32_768,
            auto_refresh_max_documents: 200,
            auto_refresh_max_source_bytes: 20_971_520, // 20 MiB
            materialize_root_index: true,
            area_index_threshold: 5,
            scopes: Vec::new(),
        }
    }

    pub fn normalize(&mut self) {
        self.scopes
            .sort_by(|left, right| left.path.cmp(&right.path));
        for scope in &mut self.scopes {
            scope.normalize();
        }
    }
}

/// A retrieval area scope: a managed sub-tree with a navigation summary and
/// optional explicit materialization policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RetrievalScope {
    /// Unique, normalized repository-relative path under the retrieval root.
    pub path: String,
    pub summary: String,
    /// Force materialization of this area's `_index.md` regardless of threshold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materialize_index: Option<bool>,
}

impl RetrievalScope {
    pub fn normalize(&mut self) {
        // Stable on read; fields are scalar.
    }
}

/// Per-document retrieval overrides. A change to only these fields is a
/// retrieval-only edit: it invalidates the retrieval fingerprint but does NOT
/// bump the receipt-bound document revision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocumentRetrieval {
    pub index: bool,
    pub include_body: bool,
    pub materialize_index: bool,
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
    /// Per-document retrieval override. `Some(None)` clears the override;
    /// `Some(value)` sets it. Edits touching only this field are retrieval-only.
    pub retrieval: Option<Option<DocumentRetrieval>>,
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
