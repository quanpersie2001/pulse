//! `.pulse/learnings/LRN-<hash>.md` file store (plan 0022 §11.1).
//!
//! One markdown file per learning: YAML frontmatter (parsed with
//! `serde_yaml`) followed by a markdown body. The body is kept as an opaque
//! string — Pulse never validates that the `Summary`/`Do`/`Avoid`/`Check`
//! headings are present, only reads them back out (see [`sections`]) when a
//! caller (`kernel::packet`) needs the compact view.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{PulseError, Result};
use crate::storage;

pub const KINDS: [&str; 4] = ["failure", "constraint", "technique", "routing"];
pub const STATUSES: [&str; 3] = ["candidate", "active", "retired"];

const FRONTMATTER_OPEN: &str = "---\n";
const FRONTMATTER_CLOSE: &str = "\n---\n";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageCounts {
    #[serde(default)]
    pub helpful: u32,
    #[serde(default)]
    pub not_needed: u32,
    #[serde(default)]
    pub misleading: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frontmatter {
    pub id: String,
    pub status: String,
    pub kind: String,
    #[serde(default)]
    pub applies_to: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub from: Vec<String>,
    #[serde(default)]
    pub expected_signal: String,
    #[serde(default)]
    pub usage: UsageCounts,
    /// Plan 0025 E2: the argv a human chose to make this learning's check
    /// *enforced* — once the learning is `active`, `pulse verify` runs it for
    /// every matching ticket under the name `learning.<id>`. Empty unless
    /// `--check-argv` was given; skipped when empty so a learning file
    /// written before this field existed renders byte-for-byte as before.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub check_argv: Vec<String>,
    /// Repository-relative working directory for [`Self::check_argv`], in the
    /// same grammar as a `verify[]` entry's `cwd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_cwd: Option<String>,
    /// Plan 0025 E4: code this learning cites, hashed at `learn add` time so
    /// drift is detectable. Skipped when empty for the same render-stability
    /// reason as `check_argv`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cites: Vec<Cite>,
}

/// One hash-pinned code citation: `path` is repository-relative, `lines` is
/// the inclusive `"<from>-<to>"` range (1-based), and `sha256` is the digest
/// of exactly those lines (joined with `\n`, no trailing newline) taken from
/// the file on disk when the learning was added (plan 0025 E4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Cite {
    pub path: String,
    pub lines: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Learning {
    pub frontmatter: Frontmatter,
    pub body: String,
}

fn dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".pulse/learnings")
}

fn path_for(repo_root: &Path, id: &str) -> PathBuf {
    dir(repo_root).join(format!("{id}.md"))
}

pub(crate) fn exists(repo_root: &Path, id: &str) -> bool {
    path_for(repo_root, id).is_file()
}

fn invalid_shape() -> PulseError {
    PulseError::kernel(
        "learning_invalid",
        "expected `---` YAML frontmatter delimiters",
        "a learning file starts with `---`, YAML frontmatter, `---`, then a markdown body (plan 0022 §11.1)",
    )
}

/// # Errors
/// `learning_invalid` if the frontmatter delimiters or YAML are malformed.
pub fn parse(text: &str) -> Result<Learning> {
    let rest = text
        .strip_prefix(FRONTMATTER_OPEN)
        .ok_or_else(invalid_shape)?;
    let end = rest.find(FRONTMATTER_CLOSE).ok_or_else(invalid_shape)?;
    let yaml = &rest[..end];
    let body = rest[end + FRONTMATTER_CLOSE.len()..].to_string();
    let frontmatter: Frontmatter = serde_yaml::from_str(yaml).map_err(|error| {
        PulseError::kernel(
            "learning_invalid",
            format!("frontmatter is not valid YAML: {error}"),
            "see plan 0022 §11.1 for the required frontmatter fields",
        )
    })?;
    Ok(Learning { frontmatter, body })
}

/// # Errors
/// `learning_invalid` if the frontmatter cannot serialize to YAML (it
/// always can for a well-formed [`Frontmatter`]; this only guards against a
/// future field type that doesn't).
pub fn render(learning: &Learning) -> Result<String> {
    let yaml = serde_yaml::to_string(&learning.frontmatter).map_err(|error| {
        PulseError::kernel(
            "learning_invalid",
            format!("could not render frontmatter: {error}"),
            "learning frontmatter must serialize to YAML",
        )
    })?;
    Ok(format!("{FRONTMATTER_OPEN}{yaml}---\n{}", learning.body))
}

/// # Errors
/// `learning_not_found` if no file exists for `id`; `learning_invalid` if it
/// exists but fails to parse.
pub fn read(repo_root: &Path, id: &str) -> Result<Learning> {
    let path = path_for(repo_root, id);
    let bytes = fs::read(&path).map_err(|_| {
        PulseError::kernel(
            "learning_not_found",
            format!("no learning {id}"),
            "check the id with `pulse learn show`",
        )
    })?;
    let text = String::from_utf8(bytes).map_err(|error| {
        PulseError::kernel(
            "learning_invalid",
            format!("{} is not valid UTF-8: {error}", path.display()),
            "learning files must be UTF-8 text",
        )
    })?;
    parse(&text)
}

/// # Errors
/// Propagates an I/O error creating `.pulse/learnings/` or writing the file.
pub fn write(repo_root: &Path, learning: &Learning) -> Result<()> {
    let dir = dir(repo_root);
    fs::create_dir_all(&dir).map_err(|error| PulseError::io(&dir, error))?;
    let path = path_for(repo_root, &learning.frontmatter.id);
    storage::atomic_write(&path, render(learning)?.as_bytes())
}

/// Every learning file, sorted by id. A missing `.pulse/learnings/`
/// directory reads as empty (mirrors [`crate::store::issues::read_all`] on
/// a missing store file).
///
/// # Errors
/// Propagates a directory-read I/O error, or [`parse`]'s error for any file
/// that fails to parse.
pub fn list(repo_root: &Path) -> Result<Vec<Learning>> {
    let dir = dir(repo_root);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut learnings = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|error| PulseError::io(&dir, error))? {
        let entry = entry.map_err(|error| PulseError::io(&dir, error))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|error| PulseError::io(&path, error))?;
        learnings.push(parse(&text)?);
    }
    learnings.sort_by(|a, b| a.frontmatter.id.cmp(&b.frontmatter.id));
    Ok(learnings)
}

/// Split a body into its `## <Heading>` sections, trimmed. A caller-facing
/// helper for `kernel::packet`'s compact `learnings[]` view (plan §9) — the
/// body itself stays opaque everywhere else (plan §11.2: "thân giữ nguyên
/// chuỗi"). A heading with no body text yet, or a body with no headings at
/// all, simply has no entry.
pub fn sections(body: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let mut current: Option<String> = None;
    let mut buffer = String::new();
    for line in body.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            if let Some(name) = current.take() {
                map.insert(name, buffer.trim().to_string());
            }
            current = Some(heading.trim().to_string());
            buffer.clear();
        } else if current.is_some() {
            buffer.push_str(line);
            buffer.push('\n');
        }
    }
    if let Some(name) = current {
        map.insert(name, buffer.trim().to_string());
    }
    map
}

/// `- ` bullet lines of a section, trimmed, in order — how `Do`/`Avoid`
/// become the packet's `do[]`/`avoid[]` arrays (plan §9).
pub fn bullet_items(section: &str) -> Vec<String> {
    section
        .lines()
        .filter_map(|line| line.trim().strip_prefix("- "))
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
---
id: LRN-3f2a
status: candidate
kind: failure
applies_to: [\"src/auth/**\"]
tags: [security]
from: [TK-a3f9]
expected_signal: \"a handoff touching src/auth/** says rotation is atomic\"
usage: {helpful: 0, not_needed: 0, misleading: 0}
---
## Summary
Refresh in parallel invalidates a token when rotation is check-then-act.
## Do
- Use a transaction or optimistic conflict check.
## Avoid
- Splitting the read and the write when rotating.
## Check
- Run 10 refreshes in parallel; exactly one succeeds.
";

    #[test]
    fn parses_the_plan_shape_and_round_trips() {
        let learning = parse(SAMPLE).unwrap();
        assert_eq!(learning.frontmatter.id, "LRN-3f2a");
        assert_eq!(learning.frontmatter.status, "candidate");
        assert_eq!(learning.frontmatter.kind, "failure");
        assert_eq!(learning.frontmatter.applies_to, vec!["src/auth/**"]);
        assert_eq!(learning.frontmatter.tags, vec!["security"]);
        assert_eq!(learning.frontmatter.from, vec!["TK-a3f9"]);
        assert_eq!(learning.frontmatter.usage, UsageCounts::default());

        let rendered = render(&learning).unwrap();
        let reparsed = parse(&rendered).unwrap();
        assert_eq!(reparsed.frontmatter.id, learning.frontmatter.id);
        assert_eq!(reparsed.body, learning.body);
    }

    #[test]
    fn missing_delimiters_are_rejected() {
        let err = parse("no frontmatter here").unwrap_err();
        assert_eq!(err.code(), "learning_invalid");
        assert!(err.hint().is_some());
    }

    #[test]
    fn write_then_read_round_trips_through_disk() {
        let repo = tempfile::tempdir().unwrap();
        let learning = parse(SAMPLE).unwrap();
        write(repo.path(), &learning).unwrap();
        assert!(exists(repo.path(), "LRN-3f2a"));
        let read_back = read(repo.path(), "LRN-3f2a").unwrap();
        assert_eq!(read_back.frontmatter.kind, "failure");
    }

    #[test]
    fn read_of_an_unknown_id_is_learning_not_found() {
        let repo = tempfile::tempdir().unwrap();
        let err = read(repo.path(), "LRN-ffff").unwrap_err();
        assert_eq!(err.code(), "learning_not_found");
    }

    #[test]
    fn list_reads_every_file_sorted_by_id_and_ignores_non_markdown() {
        let repo = tempfile::tempdir().unwrap();
        let mut a = parse(SAMPLE).unwrap();
        a.frontmatter.id = "LRN-bbbb".to_string();
        let mut b = parse(SAMPLE).unwrap();
        b.frontmatter.id = "LRN-aaaa".to_string();
        write(repo.path(), &a).unwrap();
        write(repo.path(), &b).unwrap();
        std::fs::write(dir(repo.path()).join("README.txt"), "not a learning\n").unwrap();

        let all = list(repo.path()).unwrap();
        let ids: Vec<&str> = all.iter().map(|l| l.frontmatter.id.as_str()).collect();
        assert_eq!(ids, ["LRN-aaaa", "LRN-bbbb"]);
    }

    #[test]
    fn missing_learnings_directory_reads_as_empty() {
        let repo = tempfile::tempdir().unwrap();
        assert!(list(repo.path()).unwrap().is_empty());
    }

    #[test]
    fn sections_and_bullet_items_extract_the_four_headings() {
        let learning = parse(SAMPLE).unwrap();
        let sections = sections(&learning.body);
        assert!(sections["Summary"].starts_with("Refresh in parallel"));
        assert_eq!(
            bullet_items(&sections["Do"]),
            vec!["Use a transaction or optimistic conflict check."]
        );
        assert_eq!(
            bullet_items(&sections["Avoid"]),
            vec!["Splitting the read and the write when rotating."]
        );
        assert_eq!(
            bullet_items(&sections["Check"]),
            vec!["Run 10 refreshes in parallel; exactly one succeeds."]
        );
    }

    #[test]
    fn a_missing_heading_reads_as_no_entry() {
        let sections = sections("## Summary\njust a summary\n");
        assert!(!sections.contains_key("Do"));
    }

    // --- Plan 0025 E2/E4: new frontmatter fields must not disturb old files ---

    #[test]
    fn a_pre_e2_learning_file_renders_back_byte_for_byte() {
        // The dogfood repo's existing learnings have no `check_argv`,
        // `check_cwd` or `cites`; parse -> render must be the identity on
        // them (the fields are `default` + skipped when empty).
        let old = "---\nid: LRN-3f2a\nstatus: candidate\nkind: failure\napplies_to:\n- src/auth/**\ntags:\n- security\nfrom:\n- TK-a3f9\nexpected_signal: a handoff touching src/auth/** says rotation is atomic\nusage:\n  helpful: 0\n  not_needed: 0\n  misleading: 0\n---\n## Summary\ns\n";
        let learning = parse(old).unwrap();
        assert_eq!(render(&learning).unwrap(), old);
    }

    #[test]
    fn check_fields_round_trip_through_a_file() {
        let mut learning = parse(SAMPLE).unwrap();
        learning.frontmatter.check_argv = vec!["cargo".to_string(), "test".to_string()];
        learning.frontmatter.check_cwd = Some("web".to_string());
        learning.frontmatter.cites.push(Cite {
            path: "src/auth/refresh.rs".to_string(),
            lines: "10-12".to_string(),
            sha256: "deadbeef".to_string(),
        });
        let rendered = render(&learning).unwrap();
        let reparsed = parse(&rendered).unwrap();
        assert_eq!(reparsed.frontmatter.check_argv, vec!["cargo", "test"]);
        assert_eq!(reparsed.frontmatter.check_cwd.as_deref(), Some("web"));
        assert_eq!(reparsed.frontmatter.cites.len(), 1);
        assert_eq!(reparsed.frontmatter.cites[0].lines, "10-12");
    }
}
