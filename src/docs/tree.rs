use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::docs::model::{DocsRegistry, DocumentRecord};
use crate::docs::policy::{eligible_documents, RetrievalEligibilityOptions};
use crate::docs::registry::load_registry;
use crate::PulseResult;

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
    Ok(tree_from_registry(&registry, path, options))
}

pub fn tree_from_registry(
    registry: &DocsRegistry,
    path: Option<&str>,
    options: TreeOptions,
) -> DocsTreeReport {
    let root = registry.retrieval_config().root;
    let base = path.unwrap_or(&root).trim_matches('/').to_string();
    let mut by_area: BTreeMap<String, Vec<&DocumentRecord>> = BTreeMap::new();
    let eligibility = RetrievalEligibilityOptions {
        include_draft: options.include_draft,
        include_stale: options.include_stale,
    };
    for (doc, _) in eligible_documents(registry, eligibility) {
        if !doc.path.starts_with(&base) && doc.path != "AGENTS.md" && doc.path != "PULSE.md" {
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
            path: area,
            kind: "area".to_string(),
            summary: None,
            document_id: None,
            authority: None,
            lifecycle: None,
            owner: None,
            children,
        });
    }
    nodes.sort_by(|a, b| a.path.cmp(&b.path));
    DocsTreeReport {
        schema_version: 1,
        root,
        nodes,
    }
}

fn serde_variant<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}
