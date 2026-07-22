//! Shared documentation policy: lifecycle/path rules with consumer-specific
//! eligibility.
//!
//! Slice 5 separates two eligibility notions that Slice 4 conflated via a
//! single applicability path:
//!
//! - **Applicability eligibility** (Slice 4): deterministic document-level
//!   routing for a work item — is this doc *required* context? Lives in
//!   [`crate::docs::applicability`].
//! - **Search/retrieval eligibility** (Slice 5): is this document a valid input
//!   to the lexical index and `search`? Informational docs are searchable but
//!   never become required applicability; retired/superseded/draft/protected
//!   docs are excluded by default.
//!
//! This module owns the shared lifecycle/path predicates and the retrieval
//! eligibility resolver so both consumers share one source of truth without one
//! ambiguous `eligible()` boolean.

use crate::docs::model::{
    DocumentAuthority, DocumentKind, DocumentLifecycle, DocumentRecord, RetrievalConfig,
};

/// Search/retrieval eligibility options (mirror applicability flags but scoped
/// to retrieval policy).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetrievalEligibilityOptions {
    pub include_draft: bool,
    pub include_stale: bool,
}

/// Resolved per-document retrieval policy after applying registry defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedRetrieval {
    pub index: bool,
    pub include_body: bool,
    pub materialize_index: bool,
}

impl ResolvedRetrieval {
    /// Resolve a document's effective retrieval policy against registry defaults.
    /// Generated output documents are always opt-in regardless of `default_index`.
    pub fn for_document(document: &DocumentRecord, config: &RetrievalConfig) -> Self {
        let is_generated = document.kind == DocumentKind::Generated
            || document.authority == DocumentAuthority::Generated;
        let default_index = if is_generated {
            false
        } else {
            config.default_index
        };
        let (index, include_body, materialize_index) = match &document.retrieval {
            Some(override_) => (
                // Per-document override wins. Generated docs require explicit
                // opt-in; the override cannot inherit the default because
                // `default_index` already excludes generated docs.
                override_.index,
                override_.include_body,
                override_.materialize_index,
            ),
            None => (default_index, config.default_include_body, false),
        };
        Self {
            index,
            include_body,
            materialize_index,
        }
    }
}

/// Why a document is excluded from retrieval. Empty => eligible.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RetrievalExclusion {
    pub reason_codes: Vec<String>,
}

impl RetrievalExclusion {
    pub fn is_eligible(&self) -> bool {
        self.reason_codes.is_empty()
    }
}

/// Classify a single document's retrieval eligibility. Pure (no IO): path
/// existence/UTF-8 is verified by the indexer, not here. This returns lifecycle,
/// authority, path-policy and retrieval-flag reasons only.
pub fn retrieval_exclusion(
    document: &DocumentRecord,
    config: &RetrievalConfig,
    options: RetrievalEligibilityOptions,
) -> RetrievalExclusion {
    let mut reasons = Vec::new();

    match document.lifecycle {
        DocumentLifecycle::Current => {}
        DocumentLifecycle::SuspectedStale | DocumentLifecycle::Stale => {
            if !options.include_stale {
                reasons.push("document_stale".to_string());
            }
        }
        DocumentLifecycle::Retired => reasons.push("document_retired".to_string()),
        DocumentLifecycle::Superseded => reasons.push("document_superseded".to_string()),
    }

    let _is_generated = document.kind == DocumentKind::Generated
        || document.authority == DocumentAuthority::Generated;
    match document.authority {
        DocumentAuthority::Approved | DocumentAuthority::Informational => {}
        DocumentAuthority::Generated => {
            // Generated docs are opt-in; resolved below via index flag.
        }
        DocumentAuthority::Draft => {
            if !options.include_draft {
                reasons.push("document_draft".to_string());
            }
        }
    }

    if is_protected_path(&document.path) {
        reasons.push("document_protected".to_string());
    }
    if is_work_content_path(&document.path) {
        reasons.push("document_work_content".to_string());
    }
    if is_runtime_or_cache_path(&document.path) {
        reasons.push("document_protected".to_string());
    }
    if is_generated_navigation_path(&document.path) {
        reasons.push("document_generated_navigation".to_string());
    }

    // index=false removes from index/search but not from registry/applicability.
    let resolved = ResolvedRetrieval::for_document(document, config);
    if !resolved.index {
        reasons.push("retrieval_index_disabled".to_string());
    }

    reasons.sort();
    reasons.dedup();
    RetrievalExclusion {
        reason_codes: reasons,
    }
}

/// Whether a document path is under the managed retrieval root (or is a
/// registered repository map / policy file). Repository-relative, no IO.
pub fn is_under_retrieval_root(path: &str, config: &RetrievalConfig) -> bool {
    let root = config.root.trim_start_matches('/');
    if path == "AGENTS.md" || path == "PULSE.md" {
        return true;
    }
    path == root
        || path.starts_with(&format!("{root}/"))
        || path.starts_with('/')
            && path
                .trim_start_matches('/')
                .starts_with(&format!("{root}/"))
}

/// Pulse migration backup paths may never be indexed.
pub fn is_protected_path(path: &str) -> bool {
    if path == ".pulse/migrations/docs-backups"
        || path.starts_with(".pulse/migrations/docs-backups/")
    {
        return true;
    }
    false
}

/// Work prose must not be indexed as durable documentation.
pub fn is_work_content_path(path: &str) -> bool {
    path == "works" || path.starts_with("works/")
}

/// `.pulse/runtime`, `.pulse/cache`, `.pulse/evidence` and `.git` are never
/// retrieval inputs.
pub fn is_runtime_or_cache_path(path: &str) -> bool {
    matches_path_prefix(path, ".pulse/runtime")
        || matches_path_prefix(path, ".pulse/cache")
        || matches_path_prefix(path, ".pulse/evidence")
        || matches_path_prefix(path, ".pulse/workgraph")
        || matches_path_prefix(path, ".git")
}

/// Generated navigation `_index.md` files are never authoritative/indexed.
pub fn is_generated_navigation_path(path: &str) -> bool {
    path == "docs/_index.md" || path.ends_with("/_index.md")
}

fn matches_path_prefix(path: &str, prefix: &str) -> bool {
    path == prefix || path.starts_with(&format!("{prefix}/"))
}
