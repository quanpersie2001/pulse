//! Read-only repository documentation checks invoked by `pulse docs validate`.
//!
//! This module aggregates mechanical checks that need repository content or a
//! declared generator. Registry mutation continues to use the structural
//! validator only: registering a document must never execute repository code.

mod generated;
mod links;

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::docs::model::DocsRegistry;
use crate::docs::projection::check_projections;
use crate::docs::validate::{validate_registry, DocsFinding};
use crate::PulseResult;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocsCheckKind {
    Registry,
    InternalLinks,
    GeneratedFreshness,
    NavigationProjections,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocsCheckResult {
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocsCheckReport {
    pub kind: DocsCheckKind,
    pub result: DocsCheckResult,
    pub checked: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocsRepositoryValidationReport {
    pub schema_version: u32,
    pub code: String,
    pub valid: bool,
    pub registry_revision: u64,
    pub checks: Vec<DocsCheckReport>,
    pub errors: Vec<DocsFinding>,
    pub warnings: Vec<DocsFinding>,
}

/// Validate registry structure, current internal links, declared generated
/// freshness commands, and deterministic navigation projections.
///
/// Generated freshness commands execute with `repo_root` as their working
/// directory. Pulse parses the declared command into argv and never invokes a
/// shell, so operators such as `&&`, redirection, and command substitution have
/// no special meaning.
///
/// # Errors
///
/// Returns an error when the repository cannot be read safely or a projection
/// cannot be rendered. Check failures are represented in the returned report.
pub fn validate_repository(
    repo_root: &Path,
    registry: &DocsRegistry,
) -> PulseResult<DocsRepositoryValidationReport> {
    let structural = validate_registry(repo_root, &registry.repository_id, registry)?;
    let mut errors = structural.errors;
    let mut warnings = structural.warnings;
    let registry_failed = !errors.is_empty();
    let registry_count = registry.documents.len();

    if registry_failed {
        sort_findings(&mut errors);
        sort_findings(&mut warnings);
        return Ok(DocsRepositoryValidationReport {
            schema_version: 1,
            code: "invalid_docs_registry".to_string(),
            valid: false,
            registry_revision: registry.revision,
            checks: vec![
                check_report(DocsCheckKind::Registry, true, registry_count),
                skipped_check(DocsCheckKind::InternalLinks),
                skipped_check(DocsCheckKind::GeneratedFreshness),
                skipped_check(DocsCheckKind::NavigationProjections),
            ],
            errors,
            warnings,
        });
    }

    let errors_before_links = errors.len();
    let links_checked = links::validate_internal_links(repo_root, registry, &mut errors)?;
    let links_failed = errors.len() > errors_before_links;

    let errors_before_generated = errors.len();
    let generated_checked =
        generated::validate_generated_freshness(repo_root, registry, &mut errors);
    let generated_failed = errors.len() > errors_before_generated;

    let errors_before_projections = errors.len();
    let projections_checked = validate_projections(repo_root, registry, &mut errors)?;
    let projections_failed = errors.len() > errors_before_projections;

    sort_findings(&mut warnings);
    sort_findings(&mut errors);
    let valid = errors.is_empty();
    Ok(DocsRepositoryValidationReport {
        schema_version: 1,
        code: if valid {
            "ok"
        } else {
            "docs_validation_failed"
        }
        .to_string(),
        valid,
        registry_revision: registry.revision,
        checks: vec![
            check_report(DocsCheckKind::Registry, false, registry_count),
            check_report(DocsCheckKind::InternalLinks, links_failed, links_checked),
            check_report(
                DocsCheckKind::GeneratedFreshness,
                generated_failed,
                generated_checked,
            ),
            check_report(
                DocsCheckKind::NavigationProjections,
                projections_failed,
                projections_checked,
            ),
        ],
        errors,
        warnings,
    })
}

fn check_report(kind: DocsCheckKind, failed: bool, checked: usize) -> DocsCheckReport {
    DocsCheckReport {
        kind,
        result: if failed {
            DocsCheckResult::Failed
        } else {
            DocsCheckResult::Passed
        },
        checked,
    }
}

fn skipped_check(kind: DocsCheckKind) -> DocsCheckReport {
    DocsCheckReport {
        kind,
        result: DocsCheckResult::Skipped,
        checked: 0,
    }
}

fn validate_projections(
    repo_root: &Path,
    registry: &DocsRegistry,
    errors: &mut Vec<DocsFinding>,
) -> PulseResult<usize> {
    let checked = crate::docs::projection_targets(registry).len();
    let report = check_projections(repo_root, registry)?;
    for (code, message, paths) in [
        (
            "docs_index_projection_missing",
            "generated navigation projection is missing",
            report.missing,
        ),
        (
            "docs_index_projection_stale",
            "generated navigation projection is stale",
            report.stale,
        ),
        (
            "docs_index_projection_conflict",
            "generated navigation projection conflicts with user-authored content",
            report.conflict,
        ),
    ] {
        for path in paths {
            errors.push(DocsFinding {
                code: code.to_string(),
                message: message.to_string(),
                document_id: None,
                path: Some(path),
            });
        }
    }
    Ok(checked)
}

fn sort_findings(findings: &mut [DocsFinding]) {
    findings.sort_by(|left, right| {
        left.code
            .cmp(&right.code)
            .then(left.document_id.cmp(&right.document_id))
            .then(left.path.cmp(&right.path))
            .then(left.message.cmp(&right.message))
    });
}
