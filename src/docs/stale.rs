//! Stale-doc arithmetic (plan 0025 F3): given the docs under `docs/**` and
//! the files a ticket changed, which docs DESCRIBE changed code but were
//! not themselves changed?
//!
//! Pure over its inputs — a docs list with parsed frontmatter plus a list
//! of changed paths. No git, no lock, no gate: the result is advice, never
//! a violation. The doc-facing meaning of `applies_to` is "which code this
//! doc describes" (plan 0025 F1), which is exactly what makes the reverse
//! check possible; a doc with `generated_by` is skipped — its own
//! `check_argv` already watches it mechanically.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::error::Result;
use crate::source::glob_match;

use super::{list_docs, Doc};

/// At most this many changed files are named per doc — enough to point a
/// human at the code that moved, without restating the whole diff.
const BECAUSE_LIMIT: usize = 5;

/// A doc that may have been left behind by a ticket's edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MaybeStale {
    /// Repository-relative doc path (e.g. `docs/api.md`).
    pub doc: String,
    /// The changed files that match `pattern`, capped at
    /// [`BECAUSE_LIMIT`], sorted.
    pub because: Vec<String>,
    /// The `applies_to` pattern the changed files matched.
    pub pattern: String,
}

/// Docs whose `applies_to` names changed code the doc did not follow,
/// sorted by doc path. The rules, in order:
///
/// * a doc without `applies_to` describes nothing — never reported;
/// * a doc with `generated_by` is skipped — `check_argv` watches it;
/// * a changed file under `docs/`, or the doc itself, is not evidence —
///   a doc's own edits are the fix, not the rot;
/// * the doc must not be among the changed files itself.
pub fn maybe_stale(docs: &[Doc], changed: &[String]) -> Vec<MaybeStale> {
    let mut staled: Vec<MaybeStale> = Vec::new();
    for doc in docs {
        if doc.frontmatter.generated_by.is_some() || doc.frontmatter.applies_to.is_empty() {
            continue;
        }
        if changed.iter().any(|file| file == &doc.path) {
            continue;
        }
        for pattern in &doc.frontmatter.applies_to {
            let because: Vec<String> = changed
                .iter()
                .filter(|file| {
                    !file.starts_with("docs/")
                        && file.as_str() != doc.path
                        && glob_match(pattern, file)
                })
                .take(BECAUSE_LIMIT)
                .cloned()
                .collect();
            if !because.is_empty() {
                staled.push(MaybeStale {
                    doc: doc.path.clone(),
                    because,
                    pattern: pattern.clone(),
                });
                break;
            }
        }
    }
    staled.sort_by(|a, b| a.doc.cmp(&b.doc));
    staled
}

/// [`maybe_stale`] for one record: the changed files come from the ticket's
/// own fence arithmetic (`kernel::lane::changed_files_for` — worktree dirty
/// paths ∪ the diff since its handoff, fence-ignored and scope-filtered),
/// the docs from `docs/**`. Advisory only; callers warn, never block.
///
/// # Errors
/// Propagates a docs-tree read error or the changed-files git failure.
pub fn for_ticket(repo_root: &Path, ticket: &Value) -> Result<Vec<MaybeStale>> {
    let changed = crate::kernel::lane::changed_files_for(repo_root, ticket)?;
    Ok(maybe_stale(&list_docs(repo_root)?, &changed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(path: &str, applies_to: &[&str]) -> Doc {
        Doc {
            path: path.to_string(),
            frontmatter: crate::docs::DocFrontmatter {
                applies_to: applies_to.iter().map(|s| s.to_string()).collect(),
                tags: vec![],
                generated_by: None,
            },
            lines: 1,
        }
    }

    fn changed(files: &[&str]) -> Vec<String> {
        files.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_matching_change_to_an_unchanged_doc_is_reported() {
        let docs = [doc("docs/api.md", &["api/**"])];
        let staled = maybe_stale(&docs, &changed(&["api/x.py"]));
        assert_eq!(staled.len(), 1);
        assert_eq!(staled[0].doc, "docs/api.md");
        assert_eq!(staled[0].pattern, "api/**");
        assert_eq!(staled[0].because, vec!["api/x.py".to_string()]);
    }

    #[test]
    fn a_doc_that_changed_itself_is_not_reported() {
        let docs = [doc("docs/api.md", &["api/**"])];
        let staled = maybe_stale(&docs, &changed(&["api/x.py", "docs/api.md"]));
        assert!(staled.is_empty());
    }

    #[test]
    fn a_doc_without_frontmatter_describes_nothing_and_is_never_reported() {
        let docs = [doc("docs/api.md", &[])];
        assert!(maybe_stale(&docs, &changed(&["api/x.py"])).is_empty());
    }

    #[test]
    fn a_generated_doc_is_left_to_its_own_check_argv() {
        let mut generated = doc("docs/gen.md", &["api/**"]);
        generated.frontmatter.generated_by = Some(crate::docs::GeneratedBy::default());
        assert!(maybe_stale(&[generated], &changed(&["api/x.py"])).is_empty());
    }

    #[test]
    fn changes_under_docs_alone_are_never_evidence() {
        let docs = [doc("docs/api.md", &["docs/**"])];
        assert!(maybe_stale(&docs, &changed(&["docs/other.md"])).is_empty());
    }

    #[test]
    fn the_deep_glob_grammar_applies() {
        let docs = [doc("docs/arch.md", &["src/**"])];
        let staled = maybe_stale(&docs, &changed(&["src/a/b.rs"]));
        assert_eq!(staled[0].because, vec!["src/a/b.rs".to_string()]);
        // One-level `*` does not cross segments: this change is invisible
        // to the pattern, so no report.
        let docs = [doc("docs/arch.md", &["src/*.rs"])];
        assert!(maybe_stale(&docs, &changed(&["src/a/b.rs"])).is_empty());
    }

    #[test]
    fn because_is_capped_and_results_are_sorted_by_doc_path() {
        let docs = [doc("docs/z.md", &["src/**"]), doc("docs/a.md", &["src/**"])];
        let files: Vec<String> = (0..8).map(|i| format!("src/f{i}.rs")).collect();
        let staled = maybe_stale(&docs, &files);
        assert_eq!(staled.len(), 2);
        assert_eq!(staled[0].doc, "docs/a.md");
        assert_eq!(staled[1].doc, "docs/z.md");
        assert_eq!(staled[0].because.len(), BECAUSE_LIMIT);
    }

    #[test]
    fn a_reported_doc_names_only_one_entry_per_pattern_match() {
        // Two patterns both matching must not duplicate the doc: the first
        // matching pattern wins.
        let docs = [doc("docs/api.md", &["api/**", "api/x.py"])];
        let staled = maybe_stale(&docs, &changed(&["api/x.py"]));
        assert_eq!(staled.len(), 1);
        assert_eq!(staled[0].pattern, "api/**");
    }
}
