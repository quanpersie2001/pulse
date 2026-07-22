use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::docs::model::{DocsRegistry, DocumentRecord, RetrievalConfig};
use crate::docs::policy::{eligible_documents, is_under_managed_root, RetrievalEligibilityOptions};
use crate::docs::registry::load_registry;
use crate::{PulseError, PulseResult};

#[derive(Debug, Clone, Default)]
pub struct TreeOptions {
    pub depth: Option<usize>,
    pub include_draft: bool,
    pub include_stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocsTreeReport {
    pub schema_version: u32,
    pub root: String,
    pub nodes: Vec<TreeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TreeNode {
    pub path: String,
    pub kind: String,
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TreeNode>,
}

pub fn docs_tree(
    repo_root: &Path,
    path: Option<&str>,
    options: TreeOptions,
) -> PulseResult<DocsTreeReport> {
    let registry = load_registry(repo_root)?;
    tree_from_registry(&registry, path, options)
}

pub fn tree_from_registry(
    registry: &DocsRegistry,
    path: Option<&str>,
    options: TreeOptions,
) -> PulseResult<DocsTreeReport> {
    let config = registry.retrieval_config();
    let root = normalize_tree_path(&config.root)?;
    let base = match path {
        Some(path) => normalize_tree_path(path)?,
        None => root.clone(),
    };
    if !same_or_descendant(&base, &root) {
        return Err(PulseError::validation(
            "docs_tree_path_invalid",
            "tree path must be under retrieval root",
        ));
    }

    let mut by_area: BTreeMap<String, Vec<&DocumentRecord>> = BTreeMap::new();
    let eligibility = RetrievalEligibilityOptions {
        include_draft: options.include_draft,
        include_stale: options.include_stale,
    };
    for (doc, _) in eligible_documents(registry, eligibility) {
        if !is_under_managed_root(&doc.path, &config) || !same_or_descendant(&doc.path, &base) {
            continue;
        }
        let area = doc
            .path
            .rsplit_once('/')
            .map(|(area, _)| area)
            .unwrap_or("Repository")
            .to_string();
        by_area.entry(area).or_default().push(doc);
    }
    let max_depth = options.depth.unwrap_or(3);
    let mut nodes = Vec::new();
    for (area, docs) in by_area {
        let mut children = Vec::new();
        if max_depth > 1 {
            let mut docs = docs;
            docs.sort_by(|a, b| a.path.cmp(&b.path).then(a.id.cmp(&b.id)));
            for doc in docs {
                children.push(TreeNode {
                    path: doc.path.clone(),
                    kind: "document".to_string(),
                    summary: Some(doc.summary.clone()),
                    document_id: Some(doc.id.clone()),
                    authority: Some(serde_variant(&doc.authority)),
                    lifecycle: Some(serde_variant(&doc.lifecycle)),
                    owner: Some(doc.owner.clone()),
                    children: Vec::new(),
                });
            }
        }
        nodes.push(TreeNode {
            path: area.clone(),
            kind: "area".to_string(),
            summary: scope_summary(&config, &area),
            document_id: None,
            authority: None,
            lifecycle: None,
            owner: None,
            children,
        });
    }
    nodes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(DocsTreeReport {
        schema_version: 1,
        root,
        nodes,
    })
}

fn normalize_tree_path(path: &str) -> PulseResult<String> {
    if path.trim().is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.split('/').any(|part| part == "..")
    {
        return Err(PulseError::validation(
            "docs_tree_path_invalid",
            "tree path must be a safe repository-relative path",
        ));
    }
    let normalized = path
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/");
    if normalized.is_empty() {
        return Err(PulseError::validation(
            "docs_tree_path_invalid",
            "tree path must not be empty",
        ));
    }
    Ok(normalized)
}

fn same_or_descendant(path: &str, base: &str) -> bool {
    path == base
        || path
            .strip_prefix(base)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn scope_summary(config: &RetrievalConfig, area: &str) -> Option<String> {
    let mut best: Option<(usize, &str)> = None;
    for scope in &config.scopes {
        let path = scope.path.trim_matches('/');
        if same_or_descendant(area, path) || same_or_descendant(path, area) {
            let len = path.len();
            if best.map(|(best_len, _)| len > best_len).unwrap_or(true) {
                best = Some((len, scope.summary.as_str()));
            }
        }
    }
    best.map(|(_, summary)| summary.to_string())
}

fn serde_variant<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}
