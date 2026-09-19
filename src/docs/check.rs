//! `pulse docs check` (plan 0022 §12.2): broken internal links under
//! `docs/**`, `AGENTS.md`, `PULSE.md`; every path `docs/README.md` lists;
//! `generated_by.check_argv` staleness. The report reuses
//! `kernel::lane::LaneOutput` exactly (plan: "output JSON theo shape lane
//! §8.4"), so `pulse docs check --write <path>` can be wired directly as
//! the `check-docs` lane's output with no agent in between — the
//! seal-time rule that a `fail` with no checkable finding downgrades to
//! `inconclusive` (plan §8.4) already applies uniformly once
//! `kernel::lane::validate_and_seal` reads it back; this module does not
//! need to replicate that correction itself.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use regex::Regex;

use crate::error::Result;
use crate::kernel::lane::{CheckSpec, CommandRun, Environment, Finding, LaneOutput};
use crate::source;

use super::list_docs;
use super::stale::MaybeStale;

fn link_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\[[^\]]*\]\(([^)]+)\)").expect("valid link regex"))
}

fn markdown_links(text: &str) -> Vec<String> {
    link_pattern()
        .captures_iter(text)
        .map(|capture| capture[1].trim().to_string())
        .collect()
}

fn is_external_or_anchor_only(link: &str) -> bool {
    link.contains("://") || link.starts_with("mailto:") || link.starts_with('#')
}

fn finding(summary: String, owner: String, check: Option<CheckSpec>) -> Finding {
    Finding {
        id: String::new(),
        reference: "-".to_string(),
        summary,
        owner,
        check,
        severity: "medium".to_string(),
        status: "open".to_string(),
    }
}

fn repo_relative(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Broken internal links in every `docs/**/*.md` plus `AGENTS.md`/
/// `PULSE.md` at the repo root — a relative link whose target doesn't
/// resolve to a file on disk.
fn check_broken_links(repo_root: &Path, findings: &mut Vec<Finding>) -> Result<()> {
    let mut files: Vec<std::path::PathBuf> = list_docs(repo_root)?
        .into_iter()
        .map(|doc| repo_root.join(doc.path))
        .collect();
    for extra in ["AGENTS.md", "PULSE.md"] {
        let path = repo_root.join(extra);
        if path.is_file() {
            files.push(path);
        }
    }

    for file in files {
        let text = fs::read_to_string(&file).unwrap_or_default();
        let relative = repo_relative(repo_root, &file);
        for link in markdown_links(&text) {
            if is_external_or_anchor_only(&link) {
                continue;
            }
            let target = link.split('#').next().unwrap_or(&link);
            if target.is_empty() {
                continue;
            }
            let resolved = file.parent().unwrap_or(repo_root).join(target);
            if !resolved.exists() {
                findings.push(finding(
                    format!("{relative} links to {link}, which does not exist"),
                    relative.clone(),
                    None,
                ));
            }
        }
    }
    Ok(())
}

/// Every path a `docs/README.md` bullet names (`- path/to/doc.md ...`)
/// exists. A missing `docs/README.md` is not itself a finding here — plan
/// §12.2 only requires *listed* paths to exist, and `pulse init` already
/// seeds the file.
fn check_readme_paths(repo_root: &Path, findings: &mut Vec<Finding>) {
    let Ok(text) = fs::read_to_string(repo_root.join("docs/README.md")) else {
        return;
    };
    for line in text.lines() {
        let Some(rest) = line.trim_start().strip_prefix("- ") else {
            continue;
        };
        let Some(candidate) = rest.split_whitespace().next() else {
            continue;
        };
        let candidate = candidate.trim_matches('`');
        // Only treat tokens that look like a path (has a `/` and a `.`) as a
        // reference to check — a prose bullet with no path shape is not one.
        if !candidate.contains('/') || !candidate.contains('.') {
            continue;
        }
        if !repo_root.join(candidate).exists() {
            findings.push(finding(
                format!("docs/README.md lists {candidate}, which does not exist"),
                "docs/README.md".to_string(),
                None,
            ));
        }
    }
}

/// Every doc with a `generated_by.check_argv`: run it; a non-zero exit (or
/// a failure to spawn) means the generated content may be stale.
fn check_generated_by(
    repo_root: &Path,
    findings: &mut Vec<Finding>,
    commands_run: &mut Vec<CommandRun>,
) -> Result<()> {
    for doc in list_docs(repo_root)? {
        let Some(generated) = doc.frontmatter.generated_by else {
            continue;
        };
        let Some((program, args)) = generated.check_argv.split_first() else {
            continue;
        };
        let output = Command::new(program)
            .args(args)
            .current_dir(repo_root)
            .output();
        let exit_code = match &output {
            Ok(output) => output.status.code().unwrap_or(-1),
            Err(_) => -1,
        };
        commands_run.push(CommandRun {
            argv: generated.check_argv.clone(),
            exit: Some(i64::from(exit_code)),
            detached: None,
        });
        if exit_code != 0 {
            findings.push(finding(
                format!(
                    "{} may be stale: generated_by.check_argv exited {exit_code}",
                    doc.path
                ),
                doc.path,
                Some(CheckSpec {
                    argv: generated.check_argv,
                    exit: 0,
                }),
            ));
        }
    }
    Ok(())
}

/// # Errors
/// Propagates a docs-tree read error.
pub fn check(repo_root: &Path) -> Result<LaneOutput> {
    check_with(repo_root, None, &[])
}

/// [`check`] plus plan 0025 F3's stale-doc advisory: one low-severity open
/// finding per [`MaybeStale`], naming the ticket whose edits may have left
/// the doc behind. Deliberately advisory — the verdict keys off blocking
/// findings only (mirroring the lane-seal rule that only an open *high*
/// finding turns a pass into a fail), so a low note never does. Advising
/// stays until dogfood proves the true-positive rate high enough to raise
/// the stakes (plan 0025 F3).
///
/// # Errors
/// Propagates a docs-tree read error.
pub fn check_with(
    repo_root: &Path,
    ticket_id: Option<&str>,
    advisory: &[MaybeStale],
) -> Result<LaneOutput> {
    let mut findings = Vec::new();
    let mut commands_run = Vec::new();

    check_broken_links(repo_root, &mut findings)?;
    check_readme_paths(repo_root, &mut findings);
    check_generated_by(repo_root, &mut findings, &mut commands_run)?;

    let subject = ticket_id.unwrap_or("?");
    for stale in advisory {
        findings.push(Finding {
            id: String::new(),
            reference: stale.doc.clone(),
            summary: format!(
                "describes {}; {} matching file(s) changed in {}, doc unchanged: {}",
                stale.pattern,
                stale.because.len(),
                subject,
                stale.because.join(", ")
            ),
            owner: stale.doc.clone(),
            check: None,
            severity: "low".to_string(),
            status: "open".to_string(),
        });
    }

    for (index, item) in findings.iter_mut().enumerate() {
        item.id = format!("F-{}", index + 1);
    }

    let verdict = if findings.iter().any(|f| f.severity != "low") {
        "fail"
    } else {
        "pass"
    }
    .to_string();
    Ok(LaneOutput {
        verdict,
        acceptance: Vec::new(),
        cases: Vec::new(),
        findings,
        commands_run,
        environment: Environment {
            commit: source::head_commit(repo_root).unwrap_or_default(),
            server: None,
            tool: None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
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
        fs::write(dir.path().join("README.md"), "x\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "init"]);
        dir
    }

    #[test]
    fn a_clean_docs_tree_passes() {
        let repo = git_repo();
        fs::create_dir_all(repo.path().join("docs")).unwrap();
        fs::write(repo.path().join("docs/a.md"), "# A\nno links here\n").unwrap();
        let report = check(repo.path()).unwrap();
        assert_eq!(report.verdict, "pass");
        assert!(report.findings.is_empty());
        assert!(!report.environment.commit.is_empty());
    }

    #[test]
    fn a_broken_relative_link_is_a_finding() {
        let repo = git_repo();
        fs::create_dir_all(repo.path().join("docs")).unwrap();
        fs::write(repo.path().join("docs/a.md"), "See [b](b.md) for more.\n").unwrap();
        let report = check(repo.path()).unwrap();
        assert_eq!(report.verdict, "fail");
        assert!(report.findings[0].summary.contains("b.md"));
        assert_eq!(report.findings[0].owner, "docs/a.md");
    }

    #[test]
    fn a_link_to_an_existing_file_is_not_a_finding() {
        let repo = git_repo();
        fs::create_dir_all(repo.path().join("docs")).unwrap();
        fs::write(repo.path().join("docs/a.md"), "See [b](b.md).\n").unwrap();
        fs::write(repo.path().join("docs/b.md"), "# B\n").unwrap();
        let report = check(repo.path()).unwrap();
        assert!(report.findings.is_empty());
    }

    #[test]
    fn an_external_link_and_a_bare_anchor_are_never_findings() {
        let repo = git_repo();
        fs::create_dir_all(repo.path().join("docs")).unwrap();
        fs::write(
            repo.path().join("docs/a.md"),
            "[ext](https://example.com/x) and [anchor](#section)\n",
        )
        .unwrap();
        let report = check(repo.path()).unwrap();
        assert!(report.findings.is_empty());
    }

    #[test]
    fn a_missing_readme_listed_path_is_a_finding() {
        let repo = git_repo();
        fs::create_dir_all(repo.path().join("docs")).unwrap();
        fs::write(
            repo.path().join("docs/README.md"),
            "# Docs map\n- docs/missing.md some description\n",
        )
        .unwrap();
        let report = check(repo.path()).unwrap();
        assert!(report
            .findings
            .iter()
            .any(|f| f.summary.contains("docs/missing.md")));
    }

    #[test]
    fn a_readme_listed_path_that_exists_is_not_a_finding() {
        let repo = git_repo();
        fs::create_dir_all(repo.path().join("docs")).unwrap();
        fs::write(repo.path().join("docs/present.md"), "# Present\n").unwrap();
        fs::write(
            repo.path().join("docs/README.md"),
            "# Docs map\n- docs/present.md is here\n",
        )
        .unwrap();
        let report = check(repo.path()).unwrap();
        assert!(report.findings.is_empty());
    }

    #[test]
    fn a_failing_check_argv_is_a_finding_with_a_check_and_a_commands_run_entry() {
        let repo = git_repo();
        fs::create_dir_all(repo.path().join("docs")).unwrap();
        fs::write(
            repo.path().join("docs/gen.md"),
            "---\ngenerated_by: {argv: [\"true\"], check_argv: [\"false\"]}\n---\n# Gen\n",
        )
        .unwrap();
        let report = check(repo.path()).unwrap();
        assert_eq!(report.verdict, "fail");
        let found = report
            .findings
            .iter()
            .find(|f| f.owner == "docs/gen.md")
            .unwrap();
        assert!(found.check.is_some());
        assert_eq!(report.commands_run.len(), 1);
        assert_eq!(report.commands_run[0].exit, Some(1));
    }

    #[test]
    fn a_passing_check_argv_is_not_a_finding() {
        let repo = git_repo();
        fs::create_dir_all(repo.path().join("docs")).unwrap();
        fs::write(
            repo.path().join("docs/gen.md"),
            "---\ngenerated_by: {argv: [\"true\"], check_argv: [\"true\"]}\n---\n# Gen\n",
        )
        .unwrap();
        let report = check(repo.path()).unwrap();
        assert!(report.findings.is_empty());
        assert_eq!(report.commands_run[0].exit, Some(0));
    }

    // --- Plan 0025 F3: the stale-doc advisory rides along, never flips ---

    #[test]
    fn an_advisory_finding_is_low_and_open_and_never_flips_the_verdict() {
        let repo = git_repo();
        fs::create_dir_all(repo.path().join("docs")).unwrap();
        fs::write(repo.path().join("docs/api.md"), "# API\n").unwrap();
        let advisory = [MaybeStale {
            doc: "docs/api.md".to_string(),
            because: vec!["api/x.py".to_string(), "api/y.py".to_string()],
            pattern: "api/**".to_string(),
        }];
        let report = check_with(repo.path(), Some("TK-a3f9"), &advisory).unwrap();
        assert_eq!(report.verdict, "pass", "a low advisory must not fail");
        assert_eq!(report.findings.len(), 1);
        let finding = &report.findings[0];
        assert_eq!(finding.severity, "low");
        assert_eq!(finding.status, "open");
        assert_eq!(finding.owner, "docs/api.md");
        assert_eq!(finding.reference, "docs/api.md");
        assert_eq!(finding.id, "F-1");
        assert!(finding.check.is_none());
        assert!(finding.summary.contains("describes api/**"));
        assert!(finding
            .summary
            .contains("2 matching file(s) changed in TK-a3f9"));
        assert!(finding.summary.contains("api/x.py, api/y.py"));
    }

    #[test]
    fn an_advisory_finding_alongside_a_structural_one_keeps_fail_and_numbering() {
        let repo = git_repo();
        fs::create_dir_all(repo.path().join("docs")).unwrap();
        fs::write(repo.path().join("docs/a.md"), "See [b](b.md).\n").unwrap();
        let advisory = [MaybeStale {
            doc: "docs/api.md".to_string(),
            because: vec!["api/x.py".to_string()],
            pattern: "api/**".to_string(),
        }];
        let report = check_with(repo.path(), Some("TK-bbbb"), &advisory).unwrap();
        assert_eq!(report.verdict, "fail");
        // Structural findings first, advisory appended, one id sequence.
        let ids: Vec<&str> = report.findings.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, vec!["F-1", "F-2"]);
        assert_eq!(report.findings[1].owner, "docs/api.md");
    }

    #[test]
    fn an_empty_advisory_behaves_exactly_like_check() {
        let repo = git_repo();
        fs::create_dir_all(repo.path().join("docs")).unwrap();
        fs::write(repo.path().join("docs/a.md"), "# A\n").unwrap();
        let plain = check(repo.path()).unwrap();
        let with = check_with(repo.path(), Some("TK-a3f9"), &[]).unwrap();
        assert_eq!(plain.verdict, with.verdict);
        assert_eq!(plain.findings.len(), with.findings.len());
    }
}
