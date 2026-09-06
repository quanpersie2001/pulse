//! Mechanical privacy boundary for the tracked evidence plane (Decision
//! 0012 §4).
//!
//! Receipt payloads, handoff/verification claims, note bodies and learning
//! text are committed and pushable, so they must never carry a developer
//! machine path or a secret-shaped string. This module owns the fixed
//! pattern list and the two rules:
//!
//! 1. A text field that *starts* with an absolute path marker (`/…` or
//!    `X:\…`) must canonicalize under the repository root; such a path is
//!    rewritten to repository-relative, anything outside is refused.
//! 2. A text field containing a secret-shaped string is refused outright.
//!
//! Violations are typed `receipt_privacy_violation` errors naming the field
//! and the matched pattern — never the matched value. Artifact contents,
//! `runtime/` and `cache/` are not scanned, and already-recorded content is
//! never rewritten: the boundary applies only at the write paths listed in
//! [`write_path_scan`].

use crate::{PulseError, Result};

/// Secret-shaped patterns, as `(pattern name, regex)`. Fixed list: extend by
/// PR with a test, never by configuration.
const SECRET_PATTERNS: &[(&str, &str)] = &[
    ("aws_access_key", r"AKIA[0-9A-Z]{16}"),
    ("github_token", r"ghp_[A-Za-z0-9]{36}"),
    ("api_key", r"sk-[A-Za-z0-9]{20,}"),
    ("private_key_block", r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
    ("bearer_token", r"Bearer [A-Za-z0-9._-]{20,}"),
];

/// The write paths that scan text fields before they reach the tracked
/// plane: `evidence receipt record`, `work handoff`, `work verify`, `note`
/// and `knowledge create|capture`.
pub const WRITE_PATH_SCAN: &str =
    "receipt record, work handoff, work verify, note, knowledge create|capture";

fn violation(field: &str, pattern: &str) -> PulseError {
    PulseError::validation(
        "receipt_privacy_violation",
        format!("text field {field:?} violates the tracked-plane boundary: matched {pattern}; move the value out of the tracked plane or reword it"),
    )
}

/// Check one text field and return the value as it may be recorded: a
/// leading absolute path under the repository root is rewritten to
/// repository-relative; everything else must be clean or the write is
/// refused.
///
/// # Errors
///
/// Returns `receipt_privacy_violation` when the value matches a secret
/// pattern or starts with an absolute path that does not resolve under the
/// repository root.
pub fn clean_text(repo_root: &std::path::Path, field: &str, value: &str) -> Result<String> {
    for (name, pattern) in SECRET_PATTERNS {
        let pattern = regex::Regex::new(pattern).expect("static secret pattern compiles");
        if pattern.is_match(value) {
            return Err(violation(field, name));
        }
    }
    let starts_with_absolute = value.starts_with('/')
        || value.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            && value.chars().nth(1) == Some(':')
            && value.chars().nth(2) == Some('\\');
    if starts_with_absolute {
        // The whole value must be the path: canonicalizing decides whether
        // it lives under the repository root. Path-plus-prose strings do
        // not resolve and are refused — reword them instead.
        let candidate = std::path::PathBuf::from(value);
        let canonical = candidate
            .canonicalize()
            .map_err(|_| violation(field, "absolute_path_outside_repo"))?;
        let repo_canonical = repo_root
            .canonicalize()
            .unwrap_or_else(|_| repo_root.to_path_buf());
        let relative = canonical
            .strip_prefix(&repo_canonical)
            .map_err(|_| violation(field, "absolute_path_outside_repo"))?;
        return Ok(relative.to_string_lossy().to_string());
    }
    Ok(value.to_string())
}

/// Check and rewrite every string value of a JSON tree (e.g. a receipt
/// payload): each leaf is [`clean_text`]ed with its field path as
/// `parent.child` for error messages.
///
/// # Errors
///
/// Returns `receipt_privacy_violation` for the first offending leaf.
pub fn clean_json_strings(
    repo_root: &std::path::Path,
    value: &mut serde_json::Value,
) -> Result<()> {
    match value {
        serde_json::Value::String(text) => {
            // Field attribution is best effort at the root; callers scanning
            // typed payloads pass a named root.
            *text = clean_text(repo_root, "payload", text)?;
        }
        serde_json::Value::Array(items) => {
            for item in items {
                clean_json_strings(repo_root, item)?;
            }
        }
        serde_json::Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                match item {
                    serde_json::Value::String(text) => {
                        *text = clean_text(repo_root, key, text)?;
                    }
                    _ => clean_json_strings(repo_root, item)?,
                }
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_patterns_are_refused_with_the_pattern_name() {
        let repo = tempfile::tempdir().unwrap();
        let cases = [
            ("AKIAIOSFODNN7EXAMPLE-key-field", "aws_access_key"),
            (
                "token ghp_0123456789abcdefghijklmnopqrstuvwxyz1234",
                "github_token",
            ),
            ("sk-0123456789abcdefghijklmnop", "api_key"),
            ("-----BEGIN RSA PRIVATE KEY-----", "private_key_block"),
            (
                "Authorization: Bearer abcdef.0123456789_-xyz",
                "bearer_token",
            ),
        ];
        for (value, pattern) in cases {
            let error = clean_text(repo.path(), "summary", value).unwrap_err();
            assert_eq!(error.code(), "receipt_privacy_violation");
            let message = error.to_string();
            assert!(message.contains(pattern), "{message} must name {pattern}");
            // The matched value itself never appears in the error.
            assert!(!message.contains("AKIAIOSFODNN7"));
        }
    }

    #[test]
    fn absolute_paths_under_the_repo_root_are_rewritten_relative() {
        let repo = tempfile::tempdir().unwrap();
        let file = repo.path().join("src/token.mjs");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"ok").unwrap();
        let cleaned = clean_text(repo.path(), "owner", &file.to_string_lossy()).unwrap();
        assert_eq!(cleaned, "src/token.mjs");
    }

    #[test]
    fn absolute_paths_outside_the_repo_root_are_refused() {
        let outside = tempfile::tempdir().unwrap();
        let error = clean_text(
            outside.path(),
            "summary",
            &outside.path().join("x").to_string_lossy(),
        )
        .unwrap_err();
        assert_eq!(error.code(), "receipt_privacy_violation");
        assert!(error.to_string().contains("absolute_path_outside_repo"));
    }

    #[test]
    fn relative_text_passes_through_untouched() {
        let repo = tempfile::tempdir().unwrap();
        assert_eq!(
            clean_text(repo.path(), "owner", "src/auth/RefreshTokenHandler.ts").unwrap(),
            "src/auth/RefreshTokenHandler.ts"
        );
        assert_eq!(
            clean_text(repo.path(), "summary", "expired branch missing").unwrap(),
            "expired branch missing"
        );
    }

    #[test]
    fn json_tree_scan_names_the_offending_field() {
        let repo = tempfile::tempdir().unwrap();
        let mut value = serde_json::json!({
            "cases": [{"id": "QA-1", "observation": "fine"}],
            "executor": {"name": "sk-0123456789abcdefghijklm"},
        });
        let error = clean_json_strings(repo.path(), &mut value).unwrap_err();
        assert_eq!(error.code(), "receipt_privacy_violation");
        assert!(error.to_string().contains("name"));
    }
}
