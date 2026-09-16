//! Docs (plan 0022 §12.2): optional frontmatter (`applies_to`, `tags`,
//! `generated_by`) on `docs/**/*.md`, routed the same way learnings are
//! (`docs::applicable`, reusing `learn`'s match rule) and checked for
//! structural rot (`docs::check`).

pub mod applicable;
pub mod check;

use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::error::{PulseError, Result};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedBy {
    #[serde(default)]
    pub argv: Vec<String>,
    #[serde(default)]
    pub check_argv: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocFrontmatter {
    #[serde(default)]
    pub applies_to: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub generated_by: Option<GeneratedBy>,
}

#[derive(Debug, Clone)]
pub struct Doc {
    /// Repository-relative, forward-slash path (e.g. `docs/operations/run.md`).
    pub path: String,
    pub frontmatter: DocFrontmatter,
    pub lines: usize,
}

const FRONTMATTER_OPEN: &str = "---\n";
const FRONTMATTER_CLOSE: &str = "\n---\n";

/// Frontmatter is optional (plan §12.2: "Doc không có frontmatter vẫn hợp
/// lệ, chỉ không được route") — a doc that doesn't start with `---`, or
/// whose frontmatter fails to parse, simply has none; `docs check` still
/// covers it for broken links.
fn parse_frontmatter(text: &str) -> DocFrontmatter {
    let Some(rest) = text.strip_prefix(FRONTMATTER_OPEN) else {
        return DocFrontmatter::default();
    };
    let Some(end) = rest.find(FRONTMATTER_CLOSE) else {
        return DocFrontmatter::default();
    };
    serde_yaml::from_str(&rest[..end]).unwrap_or_default()
}

/// Every `docs/**/*.md` file, sorted by path. A missing `docs/` reads as
/// empty (mirrors `learn::store::list` on a missing `.pulse/learnings/`).
///
/// # Errors
/// Propagates a directory/file read I/O error.
pub(crate) fn list_docs(repo_root: &Path) -> Result<Vec<Doc>> {
    let root = repo_root.join("docs");
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut docs = Vec::new();
    let mut pending = vec![root];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).map_err(|error| PulseError::io(&dir, error))? {
            let entry = entry.map_err(|error| PulseError::io(&dir, error))?;
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let text = fs::read_to_string(&path).map_err(|error| PulseError::io(&path, error))?;
            let relative = path
                .strip_prefix(repo_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            docs.push(Doc {
                path: relative,
                lines: text.lines().count(),
                frontmatter: parse_frontmatter(&text),
            });
        }
    }
    docs.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(docs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_doc_without_frontmatter_still_lists_with_defaults() {
        let repo = tempfile::tempdir().unwrap();
        fs::create_dir_all(repo.path().join("docs")).unwrap();
        fs::write(
            repo.path().join("docs/plain.md"),
            "# Plain\nno frontmatter\n",
        )
        .unwrap();
        let docs = list_docs(repo.path()).unwrap();
        assert_eq!(docs.len(), 1);
        assert!(docs[0].frontmatter.applies_to.is_empty());
        assert_eq!(docs[0].lines, 2);
    }

    #[test]
    fn a_doc_with_frontmatter_parses_applies_to_tags_and_generated_by() {
        let repo = tempfile::tempdir().unwrap();
        fs::create_dir_all(repo.path().join("docs")).unwrap();
        fs::write(
            repo.path().join("docs/routed.md"),
            "---\napplies_to: [\"src/auth/**\"]\ntags: [security]\ngenerated_by: {argv: [\"true\"], check_argv: [\"true\"]}\n---\n# Routed\n",
        )
        .unwrap();
        let docs = list_docs(repo.path()).unwrap();
        assert_eq!(docs[0].frontmatter.applies_to, vec!["src/auth/**"]);
        assert_eq!(docs[0].frontmatter.tags, vec!["security"]);
        assert!(docs[0].frontmatter.generated_by.is_some());
    }

    #[test]
    fn malformed_frontmatter_degrades_to_no_frontmatter_rather_than_erroring() {
        let repo = tempfile::tempdir().unwrap();
        fs::create_dir_all(repo.path().join("docs")).unwrap();
        fs::write(
            repo.path().join("docs/broken.md"),
            "---\napplies_to: [unterminated\n---\n# Broken\n",
        )
        .unwrap();
        let docs = list_docs(repo.path()).unwrap();
        assert!(docs[0].frontmatter.applies_to.is_empty());
    }

    #[test]
    fn missing_docs_directory_reads_as_empty() {
        let repo = tempfile::tempdir().unwrap();
        assert!(list_docs(repo.path()).unwrap().is_empty());
    }
}
