//! Recall (plan 0022 §11.2): match a Ticket/Story's own `context.anchors`
//! and `tags` against every learning's `applies_to`/`tags`, `active` only
//! unless `include_candidates` (the packet caller never sets it — only
//! `pulse learn applicable --all` does).

use std::path::Path;

use serde_json::Value;

use crate::error::Result;
use crate::kernel::issues::require;
use crate::source::glob_match;
use crate::store::issues::read_all;

use super::store::{list, Learning};

const RECALL_LIMIT: usize = 5;

/// The path portion of a `context.anchors` entry (`path[:symbol]`).
fn anchor_path(anchor: &str) -> &str {
    anchor.split(':').next().unwrap_or(anchor)
}

fn matches(learning: &Learning, anchors: &[&str], tags: &[&str]) -> bool {
    let by_anchor = learning.frontmatter.applies_to.iter().any(|pattern| {
        anchors
            .iter()
            .any(|anchor| glob_match(pattern, anchor_path(anchor)))
    });
    let by_tag = learning
        .frontmatter
        .tags
        .iter()
        .any(|tag| tags.contains(&tag.as_str()));
    by_anchor || by_tag
}

/// # Errors
/// `issue_not_found` if `issue_id` does not exist; propagates a learnings
/// directory read/parse error.
pub fn applicable(
    repo_root: &Path,
    issue_id: &str,
    include_candidates: bool,
) -> Result<Vec<Learning>> {
    let records = read_all(repo_root)?;
    let issue = require(&records, issue_id)?;
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

    let mut matched: Vec<Learning> = list(repo_root)?
        .into_iter()
        .filter(|learning| {
            let status_ok = learning.frontmatter.status == "active"
                || (include_candidates && learning.frontmatter.status == "candidate");
            status_ok && matches(learning, &anchors, &tags)
        })
        .collect();

    matched.sort_by(|a, b| {
        b.frontmatter
            .usage
            .helpful
            .cmp(&a.frontmatter.usage.helpful)
            .then_with(|| a.frontmatter.id.cmp(&b.frontmatter.id))
    });
    matched.truncate(RECALL_LIMIT);
    Ok(matched)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learn::store::{write, Frontmatter, UsageCounts};
    use serde_json::json;
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

    fn learning(
        id: &str,
        status: &str,
        applies_to: Vec<&str>,
        tags: Vec<&str>,
        helpful: u32,
    ) -> Learning {
        Learning {
            frontmatter: Frontmatter {
                id: id.to_string(),
                status: status.to_string(),
                kind: "failure".to_string(),
                applies_to: applies_to.into_iter().map(str::to_string).collect(),
                tags: tags.into_iter().map(str::to_string).collect(),
                from: Vec::new(),
                expected_signal: String::new(),
                usage: UsageCounts {
                    helpful,
                    ..Default::default()
                },
            },
            body: "## Summary\ns\n".to_string(),
        }
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

    #[test]
    fn matches_by_anchor_path_ignoring_the_symbol_suffix() {
        let repo = git_repo();
        seed_ticket(repo.path(), vec!["src/auth/refresh.rs:rotate"], vec![]);
        write(
            repo.path(),
            &learning("LRN-1111", "active", vec!["src/auth/**"], vec![], 0),
        )
        .unwrap();
        let matched = applicable(repo.path(), "TK-a3f9", false).unwrap();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].frontmatter.id, "LRN-1111");
    }

    #[test]
    fn matches_by_tag_when_applies_to_misses() {
        let repo = git_repo();
        seed_ticket(repo.path(), vec!["src/other.rs"], vec!["security"]);
        write(
            repo.path(),
            &learning(
                "LRN-2222",
                "active",
                vec!["src/auth/**"],
                vec!["security"],
                0,
            ),
        )
        .unwrap();
        let matched = applicable(repo.path(), "TK-a3f9", false).unwrap();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].frontmatter.id, "LRN-2222");
    }

    #[test]
    fn candidate_only_shows_with_include_candidates() {
        let repo = git_repo();
        seed_ticket(repo.path(), vec!["src/auth/x.rs"], vec![]);
        write(
            repo.path(),
            &learning("LRN-3333", "candidate", vec!["src/auth/**"], vec![], 0),
        )
        .unwrap();
        assert!(applicable(repo.path(), "TK-a3f9", false)
            .unwrap()
            .is_empty());
        let matched = applicable(repo.path(), "TK-a3f9", true).unwrap();
        assert_eq!(matched.len(), 1);
    }

    #[test]
    fn retired_never_shows_even_with_include_candidates() {
        let repo = git_repo();
        seed_ticket(repo.path(), vec!["src/auth/x.rs"], vec![]);
        write(
            repo.path(),
            &learning("LRN-4444", "retired", vec!["src/auth/**"], vec![], 0),
        )
        .unwrap();
        assert!(applicable(repo.path(), "TK-a3f9", true).unwrap().is_empty());
    }

    #[test]
    fn results_are_capped_at_five_sorted_by_helpful_descending() {
        let repo = git_repo();
        seed_ticket(repo.path(), vec!["src/auth/x.rs"], vec![]);
        for i in 0_u32..7 {
            write(
                repo.path(),
                &learning(
                    &format!("LRN-{i:04x}"),
                    "active",
                    vec!["src/auth/**"],
                    vec![],
                    i,
                ),
            )
            .unwrap();
        }
        let matched = applicable(repo.path(), "TK-a3f9", false).unwrap();
        assert_eq!(matched.len(), 5);
        let helpfuls: Vec<u32> = matched
            .iter()
            .map(|l| l.frontmatter.usage.helpful)
            .collect();
        assert_eq!(helpfuls, vec![6, 5, 4, 3, 2]);
    }

    #[test]
    fn unknown_issue_id_is_reported() {
        let repo = git_repo();
        let err = applicable(repo.path(), "TK-ffff", false).unwrap_err();
        assert_eq!(err.code(), "issue_not_found");
    }
}
