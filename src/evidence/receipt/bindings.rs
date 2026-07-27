//! Generic work/content/source/artifact binding currentness classification.
//!
//! These helpers classify whether a receipt's bindings are still current against
//! the live repository state (workgraph node revisions, content file hashes,
//! source/git commit status, artifact existence). They are reusable across all
//! receipt kinds and intentionally exclude kind-specific policy interpretation,
//! which keeps the generic envelope proof reusable for both revision-bound
//! receipts and contract-revision-bound shaping receipts.

use crate::canonical_json::hash_bytes;
use crate::evidence::manifest;
use crate::evidence::model::{ReceiptBindings, ReceiptEnvelope, WorkRevisionRef};
use crate::graph::node::Node;
use crate::{PulseError, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub(super) fn binding_staleness(
    repo_root: &Path,
    receipt: &ReceiptEnvelope,
    source: Option<&str>,
) -> Result<Vec<String>> {
    binding_codes_for(repo_root, &receipt.bindings, source)
}

pub(super) fn binding_codes_for(
    repo_root: &Path,
    bindings: &ReceiptBindings,
    source: Option<&str>,
) -> Result<Vec<String>> {
    let mut codes = work_binding_codes(repo_root, bindings)?;
    codes.extend(content_source_binding_codes(repo_root, bindings, source)?);
    Ok(codes)
}

fn work_binding_codes(repo_root: &Path, bindings: &ReceiptBindings) -> Result<Vec<String>> {
    let nodes = load_nodes(repo_root)?;
    let mut codes = Vec::new();
    for work in &bindings.work {
        match nodes.get(&work.id) {
            Some(n) if n.revision == work.revision => {}
            Some(_) => codes.push("work_binding_stale".to_string()),
            None => codes.push("work_binding_missing".to_string()),
        }
    }
    Ok(codes)
}

/// Check content and source binding currentness for a receipt.
///
/// Work normal-revision bindings are intentionally excluded: shaping receipts
/// are current by `contract_revision`, which the graph layer verifies
/// separately. This keeps the generic envelope proof reusable for both
/// revision-bound receipts and contract-revision-bound shaping receipts.
pub fn content_source_binding_codes(
    repo_root: &Path,
    bindings: &ReceiptBindings,
    source: Option<&str>,
) -> Result<Vec<String>> {
    let mut codes = Vec::new();
    for content in &bindings.content {
        let rel = crate::storage::safe_repo_relative(&content.path)?;
        let path = repo_root.join(rel);
        match fs::read(&path) {
            Ok(bytes) if hash_bytes(&bytes) == content.sha256 => {}
            Ok(_) => codes.push("content_binding_stale".to_string()),
            Err(_) => codes.push("content_binding_missing".to_string()),
        }
    }
    if let Some(source_binding) = &bindings.source {
        let manifest = manifest::load(repo_root)?;
        if source_binding.repository_id != manifest.repository_id {
            codes.push("repository_identity_mismatch".to_string());
        }
        if source_binding.kind != "git_commit" {
            codes.push("source_binding_stale".to_string());
        } else if let Some(expected) = source {
            if expected != source_binding.commit {
                codes.push("source_binding_stale".to_string());
            }
        } else {
            let scoped_paths = bindings
                .content
                .iter()
                .map(|content| content.path.clone())
                .collect::<Vec<_>>();
            match crate::source::current_status(repo_root, &source_binding.commit, &scoped_paths) {
                crate::source::SourceBindingStatus::Current => {}
                crate::source::SourceBindingStatus::DirtyUnsupported => {
                    codes.push("dirty_source_unsupported".to_string())
                }
                crate::source::SourceBindingStatus::Unsupported => {
                    codes.push("source_binding_stale".to_string())
                }
                crate::source::SourceBindingStatus::Stale => {
                    codes.push("source_binding_stale".to_string())
                }
            }
        }
    }
    Ok(codes)
}

pub fn code_to_static(code: &str) -> &'static str {
    match code {
        "work_binding_missing" => "work_binding_missing",
        "work_binding_stale" => "work_binding_stale",
        "content_binding_missing" => "content_binding_missing",
        "content_binding_stale" => "content_binding_stale",
        "source_binding_missing" => "source_binding_missing",
        "source_binding_stale" => "source_binding_stale",
        "dirty_source_unsupported" => "dirty_source_unsupported",
        "repository_identity_mismatch" => "repository_identity_mismatch",
        "artifact_not_found" => "artifact_not_found",
        _ => "receipt_schema_invalid",
    }
}

pub(super) fn require_work_binding(
    bindings: &ReceiptBindings,
    needle: &WorkRevisionRef,
) -> Result<()> {
    if bindings
        .work
        .iter()
        .any(|w| w.id == needle.id && w.revision == needle.revision)
    {
        Ok(())
    } else {
        Err(PulseError::validation(
            "work_binding_missing",
            needle.id.clone(),
        ))
    }
}

fn load_nodes(repo_root: &Path) -> Result<BTreeMap<String, Node>> {
    let dir = repo_root.join(".pulse/workgraph/nodes");
    let mut nodes = BTreeMap::new();
    if dir.exists() {
        for entry in fs::read_dir(&dir).map_err(|error| PulseError::io(&dir, error))? {
            let path = entry.map_err(|error| PulseError::io(&dir, error))?.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let n: Node = crate::storage::read_json(&path)?;
                nodes.insert(n.id.clone(), n);
            }
        }
    }
    Ok(nodes)
}
