//! Generated `_index.md` navigation projection: deterministic bytes + read-only
//! freshness check.
//!
//! This module is **read-only on disk**. It computes the expected bytes for the
//! root and selected-area `_index.md` projections and reports whether the
//! on-disk files are current/stale/missing/conflicting. It never writes files;
//! writing is the index orchestrator's job (later phase).
//!
//! Projection content derives from the registry only (metadata + summary). No
//! Markdown parsing, no LLM, no timestamps, no machine paths. Same registry +
//! documents always produce byte-identical projection bytes, so
//! `pulse docs index --check` can compare expected vs on-disk exactly.
//!
//! Authority/lifecycle policy is delegated to [`crate::docs::policy`]:
//! `index=false`, retired/superseded/stale/draft, protected/runtime/work/cache
//! paths and generated navigation `_index.md` are excluded from the projection
//! by default.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::docs::model::{
    DocsRegistry, DocumentAuthority, DocumentKind, DocumentRecord, RetrievalConfig, RetrievalScope,
};
use crate::docs::policy::{eligible_documents, ResolvedRetrieval, RetrievalEligibilityOptions};
use crate::docs::registry_fingerprint;
use crate::storage::safe_repo_relative;
use crate::{PulseError, PulseResult};

/// HTML comment marker that must be present for a file to be recognized as a
/// Pulse-generated projection. Paired with a schema-version marker line.
pub const PROJECTION_MARKER: &str = "<!-- pulse-docs-projection -->";

/// Supported projection schema version. Bumping this requires a migration
/// policy; existing generated markers with an unsupported version are preserved
/// (never silently rewritten) and reported as a conflict by [`check_projections`].
pub const PROJECTION_SCHEMA_VERSION: u32 = 1;

const PROJECTION_SCHEMA_MARKER_PREFIX: &str = "<!-- pulse-docs-projection:schema-version=";
const PROJECTION_COMMENT_END: &str = " -->";
const REPOSITORY_AREA_TITLE: &str = "Repository";
const REPOSITORY_AREA_SUMMARY: &str = "Repository map and policy.";
const VIRTUAL_REPOSITORY_AREA: &str = "@repository";

/// Schema versions this binary recognizes as Pulse-generated projections.
const SUPPORTED_SCHEMA_VERSIONS: &[u32] = &[PROJECTION_SCHEMA_VERSION];

/// What the projection layer materializes, derived from the registry's
/// [`RetrievalConfig`] plus document-driven thresholds/overrides.
///
/// All fields are deterministic; no machine paths or floats participate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionConfig {
    /// Normalized managed documentation root (e.g. `docs`).
    pub root: String,
    /// Whether the root `_index.md` is materialized.
    pub materialize_root_index: bool,
    /// Whether registered `AGENTS.md` (`kind=repository_map`) is surfaced under
    /// the virtual Repository area.
    pub include_repository_map: bool,
    /// Whether registered `PULSE.md` (`kind=policy`) is surfaced under the
    /// virtual Repository area.
    pub include_repository_policy: bool,
    /// Document count at/above which a selected area `_index.md` is materialized.
    pub area_index_threshold: u32,
    /// Sorted set of area paths that get their own `<area>/_index.md`.
    pub materialized_areas: BTreeSet<String>,
}

impl ProjectionConfig {
    /// Derive the projection policy from a registry (config + documents).
    pub fn from_registry(registry: &DocsRegistry) -> Self {
        let config = registry.retrieval_config();
        Self {
            root: normalized_root(&config.root),
            materialize_root_index: config.materialize_root_index,
            include_repository_map: config.include_repository_map,
            include_repository_policy: config.include_repository_policy,
            area_index_threshold: config.area_index_threshold,
            materialized_areas: compute_materialized_areas(registry),
        }
    }
}

/// Convenience wrapper around [`ProjectionConfig::from_registry`].
pub fn projection_config(registry: &DocsRegistry) -> ProjectionConfig {
    ProjectionConfig::from_registry(registry)
}

/// One `_index.md` file to (potentially) materialize.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionTarget {
    /// `None` for the root index; `Some("docs/domain")` for an area index.
    pub area: Option<String>,
    /// Repository-relative projection file path, e.g. `docs/_index.md`.
    pub path: String,
}

/// Compute the set of `_index.md` targets for a registry.
///
/// - Root `docs/_index.md` when `retrieval.materialize_root_index` (default true).
/// - Area `<root>/<area>/_index.md` when a scope forces it, the area meets the
///   document threshold, a document override forces it, or a deeper scope forces
///   it (with eligible documents under it).
///
/// Never generates `_index.md` for every directory blindly.
pub fn projection_targets(registry: &DocsRegistry) -> Vec<ProjectionTarget> {
    let config = registry.retrieval_config();
    let cfg = ProjectionConfig::from_registry(registry);
    let root = normalized_root(&config.root);
    let mut targets = Vec::new();
    if cfg.materialize_root_index {
        targets.push(ProjectionTarget {
            area: None,
            path: format!("{root}/_index.md"),
        });
    }
    for area in &cfg.materialized_areas {
        targets.push(ProjectionTarget {
            area: Some(area.clone()),
            path: format!("{area}/_index.md"),
        });
    }
    targets.sort_by(|left, right| left.path.cmp(&right.path));
    targets
}

/// Per-file freshness classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionStatus {
    /// On-disk bytes equal the expected projection bytes.
    Current,
    /// On-disk file is Pulse-generated but bytes differ from expected.
    Stale,
    /// Expected target is absent on disk.
    Missing,
    /// An existing `_index.md` lacks the Pulse generated marker (user-authored
    /// or unknown contract). Must be preserved, never overwritten.
    Conflict,
}

impl ProjectionStatus {
    const fn rank(self) -> u8 {
        match self {
            Self::Current => 1,
            Self::Stale => 2,
            Self::Missing => 3,
            Self::Conflict => 4,
        }
    }

    /// Return the more severe of two statuses (conflict > missing > stale > current).
    const fn worst(self, other: Self) -> Self {
        if other.rank() > self.rank() {
            other
        } else {
            self
        }
    }

    /// Lowercase stable label used in check/state output.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Stale => "stale",
            Self::Missing => "missing",
            Self::Conflict => "conflict",
        }
    }
}

/// Freshness of a single projection target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionFileState {
    pub area: Option<String>,
    pub path: String,
    pub state: ProjectionStatus,
}

/// Overall projection freshness for a repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionState {
    /// Worst per-target status.
    pub state: ProjectionStatus,
    pub targets: Vec<ProjectionFileState>,
}

/// Read-only `index --check` report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionCheckReport {
    /// True iff every target is current.
    pub ok: bool,
    pub state: ProjectionStatus,
    pub missing: Vec<String>,
    pub stale: Vec<String>,
    pub conflict: Vec<String>,
}

/// Render the deterministic root `docs/_index.md` bytes.
///
/// Deterministic ordering: areas by title (then path), documents by kind
/// precedence, title, path, id. Links are repository-relative portable paths.
/// Only current eligible documents appear. The registry fingerprint (stable,
/// no machine path) is embedded in the marker header.
pub fn render_root_index(registry: &DocsRegistry) -> PulseResult<String> {
    let config = registry.retrieval_config();
    let fingerprint = registry_fingerprint(registry)?;
    let root = normalized_root(&config.root);
    let eligible = eligible_documents(registry, RetrievalEligibilityOptions::default());

    let mut area_groups: BTreeMap<String, Vec<&DocumentRecord>> = BTreeMap::new();
    let mut repository_docs: Vec<&DocumentRecord> = Vec::new();
    for (doc, _) in &eligible {
        if is_repository_member(doc, &config) {
            repository_docs.push(doc);
        } else if let Some(area) = immediate_area_of(&doc.path, &root) {
            area_groups.entry(area).or_default().push(doc);
        } else {
            // Doc directly under the root (no sub-area): still project it,
            // grouped deterministically under the root area.
            area_groups.entry(root.clone()).or_default().push(doc);
        }
    }

    let mut sections: Vec<Section> = Vec::new();
    for (area_path, docs) in &area_groups {
        sections.push(Section {
            sort_title: area_section_title(area_path),
            area_path: area_path.clone(),
            summary: scope_summary_for(area_path, &config.scopes),
            docs: docs.clone(),
        });
    }
    if !repository_docs.is_empty() {
        sections.push(Section {
            sort_title: REPOSITORY_AREA_TITLE.to_string(),
            area_path: VIRTUAL_REPOSITORY_AREA.to_string(),
            summary: Some(REPOSITORY_AREA_SUMMARY.to_string()),
            docs: repository_docs.clone(),
        });
    }
    sections.sort_by(|left, right| {
        left.sort_title
            .cmp(&right.sort_title)
            .then(left.area_path.cmp(&right.area_path))
    });

    let mut out = header("Documentation Index", &fingerprint);
    let mut first = true;
    for section in &sections {
        if !first {
            out.push('\n');
        }
        out.push_str(&render_section(
            &section.sort_title,
            section.summary.as_deref(),
            &section.docs,
            &root,
        ));
        first = false;
    }
    normalize_trailing_newline(&mut out);
    Ok(out)
}

/// Render the deterministic area `<root>/<area>/_index.md` bytes for one area.
///
/// Same ordering/eligibility rules as [`render_root_index`], scoped to the
/// documents under `area_path`.
pub fn render_area_index(registry: &DocsRegistry, area_path: &str) -> PulseResult<String> {
    let config = registry.retrieval_config();
    let fingerprint = registry_fingerprint(registry)?;
    let eligible = eligible_documents(registry, RetrievalEligibilityOptions::default());
    let area = area_path.trim_matches('/');
    let prefix = format!("{area}/");
    let docs: Vec<&DocumentRecord> = eligible
        .iter()
        .map(|(doc, _)| *doc)
        .filter(|doc| doc.path == area || doc.path.starts_with(&prefix))
        .collect();
    let title = area_section_title(area);

    let mut out = header(&format!("{title} Index"), &fingerprint);
    if let Some(summary) = scope_summary_for(area, &config.scopes) {
        out.push_str(summary.trim());
        out.push_str("\n\n");
    }
    out.push_str(&render_doc_list(&docs, area));
    normalize_trailing_newline(&mut out);
    Ok(out)
}

/// True iff the bytes contain the Pulse generated marker AND a supported
/// projection schema marker line. Used to distinguish generated projections
/// from user-authored `_index.md` files (which must be preserved).
pub fn is_pulse_generated(file_bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(file_bytes) else {
        return false;
    };
    if !text.contains(PROJECTION_MARKER) {
        return false;
    }
    SUPPORTED_SCHEMA_VERSIONS.iter().any(|version| {
        text.contains(&format!(
            "{PROJECTION_SCHEMA_MARKER_PREFIX}{version}{PROJECTION_COMMENT_END}"
        ))
    })
}

/// Read-only freshness snapshot: for each target compare expected bytes
/// (computed) vs on-disk.
///
/// - `conflict`: existing file without the Pulse generated marker.
/// - `stale`: existing generated marker but bytes differ.
/// - `missing`: expected target absent.
///
/// Does not write anything.
pub fn projection_state(repo_root: &Path, registry: &DocsRegistry) -> PulseResult<ProjectionState> {
    let targets = projection_targets(registry);
    let mut files = Vec::with_capacity(targets.len());
    for target in &targets {
        let expected = render_target(registry, target)?;
        let abs = repo_root.join(safe_repo_relative(&target.path)?);
        let state = match std::fs::read(&abs) {
            Ok(bytes) => {
                if !is_pulse_generated(&bytes) {
                    ProjectionStatus::Conflict
                } else if bytes.as_slice() == expected.as_bytes() {
                    ProjectionStatus::Current
                } else {
                    ProjectionStatus::Stale
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => ProjectionStatus::Missing,
            Err(error) => return Err(PulseError::io(&abs, error)),
        };
        files.push(ProjectionFileState {
            area: target.area.clone(),
            path: target.path.clone(),
            state,
        });
    }
    let state = files.iter().fold(ProjectionStatus::Current, |worst, file| {
        worst.worst(file.state)
    });
    Ok(ProjectionState {
        state,
        targets: files,
    })
}

/// Read-only report for `pulse docs index --check`: lists missing/stale/conflict
/// targets; `ok` is true only when everything is current.
pub fn check_projections(
    repo_root: &Path,
    registry: &DocsRegistry,
) -> PulseResult<ProjectionCheckReport> {
    let snapshot = projection_state(repo_root, registry)?;
    let mut missing = Vec::new();
    let mut stale = Vec::new();
    let mut conflict = Vec::new();
    for file in &snapshot.targets {
        match file.state {
            ProjectionStatus::Missing => missing.push(file.path.clone()),
            ProjectionStatus::Stale => stale.push(file.path.clone()),
            ProjectionStatus::Conflict => conflict.push(file.path.clone()),
            ProjectionStatus::Current => {}
        }
    }
    Ok(ProjectionCheckReport {
        ok: snapshot.state == ProjectionStatus::Current,
        state: snapshot.state,
        missing,
        stale,
        conflict,
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

struct Section<'a> {
    sort_title: String,
    area_path: String,
    summary: Option<String>,
    docs: Vec<&'a DocumentRecord>,
}

fn render_target(registry: &DocsRegistry, target: &ProjectionTarget) -> PulseResult<String> {
    match &target.area {
        None => render_root_index(registry),
        Some(area) => render_area_index(registry, area),
    }
}

fn compute_materialized_areas(registry: &DocsRegistry) -> BTreeSet<String> {
    let config = registry.retrieval_config();
    let root = normalized_root(&config.root);
    let eligible = eligible_documents(registry, RetrievalEligibilityOptions::default());

    // Group eligible (non-repository) documents by immediate area.
    let mut immediate: BTreeMap<String, Vec<(&DocumentRecord, ResolvedRetrieval)>> =
        BTreeMap::new();
    for (doc, resolved) in &eligible {
        if is_repository_member(doc, &config) {
            continue;
        }
        if let Some(area) = immediate_area_of(&doc.path, &root) {
            immediate.entry(area).or_default().push((*doc, *resolved));
        }
    }

    let mut result: BTreeSet<String> = BTreeSet::new();
    for (area, docs) in &immediate {
        let scope_forces = scope_forces(area, &config.scopes);
        let count_ok = docs.len() as u32 >= config.area_index_threshold;
        let override_ok = docs.iter().any(|(_, resolved)| resolved.materialize_index);
        if scope_forces || count_ok || override_ok {
            result.insert(area.clone());
        }
    }

    // Deeper scope-forced areas (not immediate areas) with eligible docs.
    for scope in &config.scopes {
        if scope.materialize_index != Some(true) {
            continue;
        }
        let scope_area = scope.path.trim_matches('/').to_string();
        if scope_area.is_empty() || result.contains(&scope_area) {
            continue;
        }
        let prefix = format!("{scope_area}/");
        let has_docs = eligible
            .iter()
            .any(|(doc, _)| doc.path == scope_area || doc.path.starts_with(&prefix));
        if has_docs {
            result.insert(scope_area);
        }
    }

    result
}

fn header(title: &str, fingerprint: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {title}\n\n"));
    out.push_str("> Generated by `pulse docs index`. Do not edit manually.\n");
    out.push_str(&format!("> Registry fingerprint: `{fingerprint}`\n\n"));
    out.push_str(PROJECTION_MARKER);
    out.push('\n');
    out.push_str(&format!(
        "{PROJECTION_SCHEMA_MARKER_PREFIX}{PROJECTION_SCHEMA_VERSION}{PROJECTION_COMMENT_END}\n\n"
    ));
    out
}

fn render_section(
    title: &str,
    summary: Option<&str>,
    docs: &[&DocumentRecord],
    index_dir: &str,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("## {title}\n\n"));
    if let Some(summary) = summary {
        let trimmed = summary.trim();
        if !trimmed.is_empty() {
            out.push_str(trimmed);
            out.push_str("\n\n");
        }
    }
    out.push_str(&render_doc_list(docs, index_dir));
    out
}

fn render_doc_list(docs: &[&DocumentRecord], index_dir: &str) -> String {
    let mut sorted: Vec<&DocumentRecord> = docs.to_vec();
    sorted.sort_by_key(|left| doc_sort_key(left));
    let mut out = String::new();
    for doc in sorted {
        let title = document_title(&doc.path);
        let link = relative_link(index_dir, &doc.path);
        out.push_str(&format!("- [{title}]({link})\n"));
        let summary = doc.summary.trim();
        if !summary.is_empty() {
            out.push_str(&format!("  {summary}\n"));
        }
        out.push_str(&format!(
            "  Owner: `{}` · Authority: {}\n",
            doc.owner,
            authority_label(doc.authority)
        ));
    }
    out
}

fn doc_sort_key(doc: &DocumentRecord) -> (u32, String, String, String) {
    (
        kind_display_order(doc.kind),
        document_title(&doc.path),
        doc.path.clone(),
        doc.id.clone(),
    )
}

fn kind_display_order(kind: DocumentKind) -> u32 {
    match kind {
        DocumentKind::Product => 0,
        DocumentKind::Architecture => 1,
        DocumentKind::Domain => 2,
        DocumentKind::Operations => 3,
        DocumentKind::Reference => 4,
        DocumentKind::DecisionProjection => 5,
        DocumentKind::Informational => 6,
        DocumentKind::Policy => 7,
        DocumentKind::RepositoryMap => 8,
        DocumentKind::Generated => 9,
    }
}

fn authority_label(authority: DocumentAuthority) -> &'static str {
    match authority {
        DocumentAuthority::Approved => "approved",
        DocumentAuthority::Informational => "informational",
        DocumentAuthority::Generated => "generated",
        DocumentAuthority::Draft => "draft",
    }
}

fn is_repository_member(doc: &DocumentRecord, config: &RetrievalConfig) -> bool {
    (config.include_repository_map && doc.kind == DocumentKind::RepositoryMap)
        || (config.include_repository_policy && doc.kind == DocumentKind::Policy)
}

fn normalized_root(root: &str) -> String {
    let trimmed = root.trim_matches('/');
    if trimmed.is_empty() {
        "docs".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Strip the root prefix from a repository-relative path. Returns the remainder
/// (e.g. `domain/token.md`) when the path lives under root, else `None`.
fn strip_root<'a>(path: &'a str, root: &str) -> Option<&'a str> {
    let root = root.trim_matches('/');
    let path = path.trim_start_matches('/');
    if path == root {
        return None;
    }
    path.strip_prefix(&format!("{root}/"))
}

/// First path segment under root, when the document lives in a sub-directory.
fn immediate_area_of(path: &str, root: &str) -> Option<String> {
    let rel = strip_root(path, root)?;
    let (first, rest) = rel.split_once('/')?;
    if rest.is_empty() {
        return None;
    }
    Some(format!("{root}/{first}"))
}

/// Whether any scope with `materialize_index == Some(true)` covers `area`
/// (equal or ancestor).
fn scope_forces(area: &str, scopes: &[RetrievalScope]) -> bool {
    let area = area.trim_matches('/');
    scopes.iter().any(|scope| {
        scope.materialize_index == Some(true) && {
            let path = scope.path.trim_matches('/');
            path == area || area.starts_with(&format!("{path}/"))
        }
    })
}

/// Longest-prefix scope summary for an area, if any.
fn scope_summary_for(area: &str, scopes: &[RetrievalScope]) -> Option<String> {
    let area = area.trim_matches('/');
    let mut best: Option<(usize, &str)> = None;
    for scope in scopes {
        let path = scope.path.trim_matches('/');
        if area == path || area.starts_with(&format!("{path}/")) {
            let len = path.len();
            if best.map(|(best_len, _)| len > best_len).unwrap_or(true) {
                best = Some((len, scope.summary.as_str()));
            }
        }
    }
    best.map(|(_, summary)| summary.to_string())
}

fn area_section_title(area_path: &str) -> String {
    let last = area_path
        .trim_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(area_path.trim_matches('/'));
    title_case_segment(last)
}

fn document_title(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    let stem = file.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(file);
    title_case_segment(stem)
}

fn title_case_segment(segment: &str) -> String {
    let words: Vec<String> = segment
        .split(['-', '_'])
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .filter(|word| !word.is_empty())
        .collect();
    if words.is_empty() {
        segment.to_string()
    } else {
        words.join(" ")
    }
}

/// Repository-relative portable link from an index directory to a target path.
fn relative_link(index_dir: &str, target: &str) -> String {
    let index_parts: Vec<&str> = index_dir
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let target_parts: Vec<&str> = target.split('/').filter(|part| !part.is_empty()).collect();
    let mut common = 0;
    while common < index_parts.len()
        && common < target_parts.len()
        && index_parts[common] == target_parts[common]
    {
        common += 1;
    }
    let up = index_parts.len() - common;
    let mut result: Vec<String> = Vec::with_capacity(up + (target_parts.len() - common));
    for _ in 0..up {
        result.push("..".to_string());
    }
    for part in &target_parts[common..] {
        result.push((*part).to_string());
    }
    if result.is_empty() {
        ".".to_string()
    } else {
        result.join("/")
    }
}

fn normalize_trailing_newline(out: &mut String) {
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_link_helper_is_portable() {
        assert_eq!(
            relative_link("docs", "docs/domain/token.md"),
            "domain/token.md"
        );
        assert_eq!(relative_link("docs", "AGENTS.md"), "../AGENTS.md");
        assert_eq!(
            relative_link("docs/domain", "docs/domain/token.md"),
            "token.md"
        );
    }

    #[test]
    fn title_helper_is_deterministic() {
        assert_eq!(
            document_title("docs/domain/token-lifecycle.md"),
            "Token Lifecycle"
        );
        assert_eq!(document_title("AGENTS.md"), "AGENTS");
        assert_eq!(area_section_title("docs/domain"), "Domain");
    }
}
