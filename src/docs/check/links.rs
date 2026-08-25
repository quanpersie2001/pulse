//! Deterministic repository-local Markdown link validation.
//!
//! The checker resolves current registered Markdown content without network
//! access. Fenced and inline-code examples are excluded from link discovery.

use std::fs;
use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;
use regex::Regex;

use crate::docs::model::{DocsRegistry, DocumentLifecycle, DocumentRecord};
use crate::docs::validate::DocsFinding;
use crate::storage;
use crate::{PulseError, PulseResult};

static MARKDOWN_LINK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"!?\[[^\]\n]*\]\(([^)\n]+)\)"#).expect("markdown link regex is valid")
});
static URI_SCHEME: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9+.-]*:").expect("URI scheme regex is valid"));

pub(super) fn validate_internal_links(
    repo_root: &Path,
    registry: &DocsRegistry,
    errors: &mut Vec<DocsFinding>,
) -> PulseResult<usize> {
    let mut checked = 0;
    for document in registry
        .documents
        .iter()
        .filter(|document| document.lifecycle == DocumentLifecycle::Current)
        .filter(|document| is_markdown(&document.path))
    {
        let Ok(path) = storage::paths::resolve_repo_relative(repo_root, &document.path) else {
            continue;
        };
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(PulseError::io(&path, error)),
        };
        let Ok(markdown) = std::str::from_utf8(&bytes) else {
            errors.push(finding(
                "docs_document_not_utf8",
                "registered Markdown content must be UTF-8",
                document,
                document.path.clone(),
            ));
            continue;
        };
        for target in markdown_link_targets(markdown) {
            let Some(relative_target) = internal_link_path(document, &target) else {
                continue;
            };
            checked += 1;
            match storage::paths::resolve_repo_relative(repo_root, &relative_target) {
                Ok(target_path) if target_path.exists() => {}
                Ok(_) => errors.push(finding(
                    "docs_internal_link_broken",
                    format!("internal link target does not exist: {target}"),
                    document,
                    relative_target.to_string_lossy().replace('\\', "/"),
                )),
                Err(_) => errors.push(finding(
                    "docs_internal_link_unsafe",
                    format!("internal link escapes repository: {target}"),
                    document,
                    document.path.clone(),
                )),
            }
        }
    }
    Ok(checked)
}

fn markdown_link_targets(markdown: &str) -> Vec<String> {
    let mut visible = String::new();
    let mut fence: Option<char> = None;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        let marker = if trimmed.starts_with("```") {
            Some('`')
        } else if trimmed.starts_with("~~~") {
            Some('~')
        } else {
            None
        };
        if let Some(marker) = marker {
            if fence == Some(marker) {
                fence = None;
            } else if fence.is_none() {
                fence = Some(marker);
            }
            continue;
        }
        if fence.is_none() {
            visible.push_str(&without_inline_code(line));
            visible.push('\n');
        }
    }
    MARKDOWN_LINK
        .captures_iter(&visible)
        .filter_map(|capture| destination(&capture[1]))
        .collect()
}

fn without_inline_code(line: &str) -> String {
    let characters: Vec<char> = line.chars().collect();
    let mut visible = String::with_capacity(line.len());
    let mut code_delimiter: Option<usize> = None;
    let mut index = 0;
    while index < characters.len() {
        if characters[index] == '`' {
            let start = index;
            while index < characters.len() && characters[index] == '`' {
                index += 1;
            }
            let width = index - start;
            if code_delimiter == Some(width) {
                code_delimiter = None;
            } else if code_delimiter.is_none() {
                code_delimiter = Some(width);
            }
            for _ in 0..width {
                visible.push(' ');
            }
        } else {
            visible.push(if code_delimiter.is_some() {
                ' '
            } else {
                characters[index]
            });
            index += 1;
        }
    }
    visible
}

fn destination(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix('<') {
        return rest.find('>').map(|end| rest[..end].to_string());
    }
    let value = trimmed.split_whitespace().next()?;
    (!value.is_empty()).then(|| value.to_string())
}

fn internal_link_path(document: &DocumentRecord, target: &str) -> Option<PathBuf> {
    if target.is_empty()
        || target.starts_with('#')
        || target.starts_with("//")
        || URI_SCHEME.is_match(target)
    {
        return None;
    }
    let without_fragment = target.split(['#', '?']).next().unwrap_or_default();
    if without_fragment.is_empty() {
        return None;
    }
    let decoded = percent_decode(without_fragment).unwrap_or_else(|| without_fragment.to_string());
    if decoded.starts_with('/') {
        return Some(PathBuf::from(decoded.trim_start_matches('/')));
    }
    let parent = Path::new(&document.path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    Some(normalize_relative_link(parent, Path::new(&decoded)))
}

fn normalize_relative_link(parent: &Path, target: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in parent.components().chain(target.components()) {
        match component {
            std::path::Component::Normal(part) => normalized.push(part),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir if normalized.pop() => {}
            std::path::Component::ParentDir => return PathBuf::from("../outside-repository"),
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return PathBuf::from("../outside-repository");
            }
        }
    }
    normalized
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex_digit(*bytes.get(index + 1)?)?;
            let low = hex_digit(*bytes.get(index + 2)?)?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn finding(
    code: impl Into<String>,
    message: impl Into<String>,
    document: &DocumentRecord,
    path: String,
) -> DocsFinding {
    DocsFinding {
        code: code.into(),
        message: message.into(),
        document_id: Some(document.id.clone()),
        path: Some(path),
    }
}

fn is_markdown(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(extension.to_ascii_lowercase().as_str(), "md" | "markdown")
        })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{destination, markdown_link_targets, normalize_relative_link, percent_decode};

    #[test]
    fn markdown_targets_ignore_code_examples_and_parse_titles() {
        let markdown = "[current](../current.md \"title\") and `[inline](ignored.md)`\n```md\n[example](missing.md)\n```\n";
        assert_eq!(markdown_link_targets(markdown), vec!["../current.md"]);
        assert_eq!(
            destination("<path with spaces.md> 'title'"),
            Some("path with spaces.md".into())
        );
        assert_eq!(percent_decode("docs/a%20b.md"), Some("docs/a b.md".into()));
        assert_eq!(
            normalize_relative_link(Path::new("docs/domain"), Path::new("../current.md")),
            PathBuf::from("docs/current.md")
        );
    }
}
