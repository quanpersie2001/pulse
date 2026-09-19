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

/// Plan 0025 E3 — the one law of `misleading`: a learning whose
/// `usage.misleading` count exceeds its `usage.helpful` count is *suspect* —
/// it has been reported as doing more harm than good — and is excluded from
/// both recall surfaces: the packet (`applicable`) AND the enforcement
/// match (`matching`) — a check being reported as misleading must not be
/// allowed to block a handoff. A suspect learning is not retired (that is a
/// human's call, surfaced by `pulse doctor` as `learning_suspect`) and
/// `pulse learn applicable <id> --all` still lists it, marked `suspect`.
pub fn is_suspect(learning: &Learning) -> bool {
    learning.frontmatter.usage.misleading > learning.frontmatter.usage.helpful
}

/// The path portion of a `context.anchors` entry (`path[:symbol]`).
fn anchor_path(anchor: &str) -> &str {
    anchor.split(':').next().unwrap_or(anchor)
}

/// A record's recall triggers: its `context.anchors` paths and its `tags`.
fn issue_triggers(issue: &Value) -> (Vec<&str>, Vec<&str>) {
    let anchors = issue
        .pointer("/context/anchors")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let tags = issue
        .get("tags")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    (anchors, tags)
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

/// Every learning that matches `issue_id`'s anchors/tags and has an allowed
/// status (`active`, plus `candidate` when `include_candidates`) — with no
/// cap. [`applicable`] is this plus the packet's sort-and-truncate; callers
/// that must not lose matches (the `pulse verify` enforcement, plan 0025
/// E2) call this directly: RECALL_LIMIT bounds a packet, not evidence.
///
/// # Errors
/// `issue_not_found` if `issue_id` does not exist; propagates a learnings
/// directory read/parse error.
pub fn matching(
    repo_root: &Path,
    issue_id: &str,
    include_candidates: bool,
) -> Result<Vec<Learning>> {
    let records = read_all(repo_root)?;
    let issue = require(&records, issue_id)?;
    let (anchors, tags) = issue_triggers(issue);
    matching_core(repo_root, &anchors, &tags, include_candidates, false)
}

/// [`matching`] against triggers taken from a record the caller already
/// holds — no store read, so a ticket that is not (yet) persisted still
/// resolves its learnings (`verify::required_names`, plan 0025 E2).
///
/// # Errors
/// Propagates a learnings directory read/parse error.
pub fn matching_for(
    repo_root: &Path,
    anchors: &[&str],
    tags: &[&str],
    include_candidates: bool,
) -> Result<Vec<Learning>> {
    matching_core(repo_root, anchors, tags, include_candidates, false)
}

/// The [`matching`] rule with suspect learnings kept — what `pulse learn
/// applicable --all` needs, so a suspect shows up (marked as such) instead
/// of silently vanishing. Never used for enforcement or the packet.
///
/// # Errors
/// `issue_not_found` if `issue_id` does not exist; propagates a learnings
/// directory read/parse error.
pub fn matching_including_suspects(
    repo_root: &Path,
    issue_id: &str,
    include_candidates: bool,
) -> Result<Vec<Learning>> {
    let records = read_all(repo_root)?;
    let issue = require(&records, issue_id)?;
    let (anchors, tags) = issue_triggers(issue);
    matching_core(repo_root, &anchors, &tags, include_candidates, true)
}

fn matching_core(
    repo_root: &Path,
    anchors: &[&str],
    tags: &[&str],
    include_candidates: bool,
    include_suspects: bool,
) -> Result<Vec<Learning>> {
    Ok(list(repo_root)?
        .into_iter()
        .filter(|learning| {
            let status_ok = learning.frontmatter.status == "active"
                || (include_candidates && learning.frontmatter.status == "candidate");
            let suspect_ok = include_suspects || !is_suspect(learning);
            status_ok && suspect_ok && matches(learning, anchors, tags)
        })
        .collect())
}

/// # Errors
/// `issue_not_found` if `issue_id` does not exist; propagates a learnings
/// directory read/parse error.
pub fn applicable(
    repo_root: &Path,
    issue_id: &str,
    include_candidates: bool,
) -> Result<Vec<Learning>> {
    let mut matched = matching(repo_root, issue_id, include_candidates)?;
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
        learning_with_usage(id, status, applies_to, tags, helpful, 0)
    }

    fn learning_with_usage(
        id: &str,
        status: &str,
        applies_to: Vec<&str>,
        tags: Vec<&str>,
        helpful: u32,
        misleading: u32,
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
                    misleading,
                    ..Default::default()
                },
                check_argv: vec![],
                check_cwd: None,
                cites: vec![],
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

    // --- Plan 0025 E3: misleading has consequences ---

    #[test]
    fn zero_usage_is_still_recalled() {
        let repo = git_repo();
        seed_ticket(repo.path(), vec!["src/auth/x.rs"], vec![]);
        write(
            repo.path(),
            &learning("LRN-5555", "active", vec!["src/auth/**"], vec![], 0),
        )
        .unwrap();
        assert_eq!(applicable(repo.path(), "TK-a3f9", false).unwrap().len(), 1);
    }

    #[test]
    fn a_suspect_learning_is_dropped_from_recall_and_enforcement() {
        // 1 helpful / 2 misleading: reported as doing more harm than good.
        // Both surfaces drop it — the packet AND the verify enforcement (a
        // check reported as misleading must not be allowed to block a
        // handoff, plan 0025 E3).
        let repo = git_repo();
        seed_ticket(repo.path(), vec!["src/auth/x.rs"], vec![]);
        write(
            repo.path(),
            &learning_with_usage("LRN-6666", "active", vec!["src/auth/**"], vec![], 1, 2),
        )
        .unwrap();
        assert!(applicable(repo.path(), "TK-a3f9", false)
            .unwrap()
            .is_empty());
        assert!(matching(repo.path(), "TK-a3f9", false).unwrap().is_empty());
        // `--all` still shows it, so a human can decide to retire it.
        let all = matching_including_suspects(repo.path(), "TK-a3f9", false).unwrap();
        assert_eq!(all.len(), 1);
        assert!(is_suspect(&all[0]));
    }

    #[test]
    fn equal_helpful_and_misleading_counts_are_kept() {
        // The law is strictly `misleading > helpful`: a learning with a
        // spotted record but an even score is still recalled — suspicion
        // alone does not silence institutional memory.
        let repo = git_repo();
        seed_ticket(repo.path(), vec!["src/auth/x.rs"], vec![]);
        write(
            repo.path(),
            &learning_with_usage("LRN-7777", "active", vec!["src/auth/**"], vec![], 2, 2),
        )
        .unwrap();
        assert_eq!(applicable(repo.path(), "TK-a3f9", false).unwrap().len(), 1);
        assert!(!is_suspect(
            &applicable(repo.path(), "TK-a3f9", false).unwrap()[0]
        ));
    }
}
