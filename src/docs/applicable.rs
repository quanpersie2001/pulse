//! `pulse docs applicable <id>` (plan 0022 §12.2): the same match rule as
//! `learn::recall` — glob `applies_to` against `context.anchors` (path
//! before `:`), or `tags` intersecting the issue's own `tags` — applied to
//! every doc under `docs/**` instead of every learning.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::error::Result;
use crate::kernel::issues::require;
use crate::source::glob_match;
use crate::store::issues::read_all;

use super::{list_docs, Doc};

const APPLICABLE_LIMIT: usize = 8;

#[derive(Debug, Clone, Serialize)]
pub struct Match {
    pub path: String,
    pub why: String,
    pub lines: usize,
}

fn anchor_path(anchor: &str) -> &str {
    anchor.split(':').next().unwrap_or(anchor)
}

fn why(doc: &Doc, anchors: &[&str], tags: &[&str]) -> Option<String> {
    for pattern in &doc.frontmatter.applies_to {
        if let Some(anchor) = anchors
            .iter()
            .find(|anchor| glob_match(pattern, anchor_path(anchor)))
        {
            return Some(format!("anchor {anchor} matches applies_to {pattern}"));
        }
    }
    for tag in &doc.frontmatter.tags {
        if tags.contains(&tag.as_str()) {
            return Some(format!("tag {tag}"));
        }
    }
    None
}

/// # Errors
/// `issue_not_found` if `id` does not exist; propagates a docs-tree read
/// error.
pub fn applicable(repo_root: &Path, id: &str) -> Result<Vec<Match>> {
    let records = read_all(repo_root)?;
    let issue = require(&records, id)?;
    let anchors: Vec<&str> = issue
        .pointer("/context/anchors")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let tags: Vec<&str> = issue
        .get("tags")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let mut matched: Vec<Match> = list_docs(repo_root)?
        .into_iter()
        .filter_map(|doc| {
            why(&doc, &anchors, &tags).map(|why| Match {
                path: doc.path,
                why,
                lines: doc.lines,
            })
        })
        .collect();
    matched.sort_by(|a, b| a.path.cmp(&b.path));
    matched.truncate(APPLICABLE_LIMIT);
    Ok(matched)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use std::process::Command as StdCommand;

    fn git_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(StdCommand::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .status()
                .unwrap()
                .success());
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "test"]);
        std::fs::write(dir.path().join("README.md"), "x\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "init"]);
        dir
    }

    fn seed_ticket(repo: &Path, anchors: Vec<&str>, tags: Vec<&str>) {
        crate::store::issues::mutate(repo, |mut records| {
            records.push(json!({
                "schema": 3, "id": "TK-a3f9", "kind": "ticket", "title": "t",
                "status": "draft", "revision": 1,
                "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
                "role": "implementation",
                "context": {"anchors": anchors},
                "tags": tags,
            }));
            Ok(records)
        })
        .unwrap();
    }

    fn write_doc(repo: &Path, path: &str, frontmatter: &str, body: &str) {
        let full = repo.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, format!("---\n{frontmatter}\n---\n{body}")).unwrap();
    }

    #[test]
    fn matches_by_anchor_path() {
        let repo = git_repo();
        seed_ticket(repo.path(), vec!["src/auth/refresh.rs:rotate"], vec![]);
        write_doc(
            repo.path(),
            "docs/auth.md",
            "applies_to: [\"src/auth/**\"]\ntags: []",
            "# Auth\n",
        );
        let matched = applicable(repo.path(), "TK-a3f9").unwrap();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].path, "docs/auth.md");
        assert!(matched[0].why.contains("applies_to"));
    }

    #[test]
    fn matches_by_tag() {
        let repo = git_repo();
        seed_ticket(repo.path(), vec![], vec!["security"]);
        write_doc(
            repo.path(),
            "docs/security.md",
            "applies_to: []\ntags: [security]",
            "# Security\n",
        );
        let matched = applicable(repo.path(), "TK-a3f9").unwrap();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].why, "tag security");
    }

    #[test]
    fn a_doc_with_no_frontmatter_never_matches() {
        let repo = git_repo();
        seed_ticket(repo.path(), vec!["src/auth/x.rs"], vec![]);
        fs::create_dir_all(repo.path().join("docs")).unwrap();
        fs::write(repo.path().join("docs/plain.md"), "# Plain\n").unwrap();
        assert!(applicable(repo.path(), "TK-a3f9").unwrap().is_empty());
    }

    #[test]
    fn results_are_capped_at_eight() {
        let repo = git_repo();
        seed_ticket(repo.path(), vec!["src/auth/x.rs"], vec![]);
        for i in 0..10 {
            write_doc(
                repo.path(),
                &format!("docs/d{i}.md"),
                "applies_to: [\"src/auth/**\"]",
                "# D\n",
            );
        }
        assert_eq!(applicable(repo.path(), "TK-a3f9").unwrap().len(), 8);
    }
}
