//! Section/chunk models, stable anchors, section refs and versioned chunk
//! limits for documentation extraction (Slice 5, Phase 2).
//!
//! This module owns the *identity* layer of extracted documentation sections:
//!
//! - the serializable [`SectionRecord`] / [`SectionRange`] / [`ChunkRef`]
//!   shapes that downstream phases (cache, lexical index, `get`, `tree`) consume;
//! - the versioned anchor algorithm [`anchor_for_heading`];
//! - deterministic duplicate-anchor suffixing [`dedupe_anchors`];
//! - stable section/chunk ref rendering [`section_ref`] / [`chunk_ref_string`];
//! - versioned index-config chunk limits.
//!
//! It is deliberately free of any Markdown parsing logic and of any IO: the
//! parser adapter lives in [`crate::docs::markdown`], and all values here are
//! pure functions over already-extracted text.
//!
//! All structs use `#[serde(deny_unknown_fields)]` and contain no floats.

use serde::{Deserialize, Serialize};

/// Anchor normalization algorithm version. Participates in the retrieval
/// fingerprint; bumping it invalidates all derived anchors and forces a rebuild.
pub const ANCHOR_VERSION: u32 = 1;

/// Chunk splitting algorithm version. Participates in the retrieval fingerprint.
pub const CHUNK_VERSION: u32 = 1;

/// Section extractor (preamble/boundary/fence) algorithm version. Participates
/// in the retrieval fingerprint.
pub const EXTRACTOR_VERSION: u32 = 1;

/// Soft per-chunk byte ceiling (UTF-8). Sections at or under both
/// [`CHUNK_SOFT_MAX_BYTES`] and [`CHUNK_SOFT_MAX_LINES`] emit a single record
/// with `chunk: None`.
pub const CHUNK_SOFT_MAX_BYTES: usize = 8_000;

/// Soft per-chunk line ceiling.
pub const CHUNK_SOFT_MAX_LINES: u32 = 160;

/// Hard per-chunk byte ceiling used by the bounded `get` budget. The indexer
/// cannot split a fenced code block, so a single oversized fence may exceed the
/// soft limits but is still emitted as one atomic chunk.
pub const CHUNK_HARD_MAX_BYTES: usize = 32_768;

/// Target line overlap between consecutive chunks of an oversized section.
/// Best-effort: the splitter snaps to safe boundaries and never overlaps inside
/// a fence, so the realized overlap may be smaller when a fence is near.
pub const CHUNK_OVERLAP_LINES: u32 = 8;

/// Fallback anchor for a heading whose normalized text is empty.
pub const EMPTY_HEADING_ANCHOR: &str = "section";

/// Reserved anchor for the document preamble (non-whitespace content before the
/// first heading). Only the preamble section may carry this anchor; a heading
/// whose text normalizes to `preamble` follows normal duplicate-suffix rules.
pub const PREAMBLE_ANCHOR: &str = "preamble";

/// Fallback display heading for an empty preamble or when no heading text
/// survives normalization.
pub const EMPTY_HEADING_TEXT: &str = "Section";

/// Inclusive 1-based source line range covering a section or chunk.
///
/// `start_line` and `end_line` are *logical* source lines: line numbers are
/// identical for LF and CRLF inputs (CRLF only changes byte hashes, not logical
/// ranges). The byte slice a range covers is defined by the parser adapter as
/// the exact stored bytes from the first byte of `start_line` through the line
/// terminator of `end_line` (or end-of-file for the final line).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SectionRange {
    pub start_line: u32,
    pub end_line: u32,
}

impl SectionRange {
    pub fn new(start_line: u32, end_line: u32) -> Self {
        Self {
            start_line,
            end_line,
        }
    }

    /// Number of source lines covered (inclusive).
    pub fn line_count(&self) -> u32 {
        debug_assert!(self.end_line >= self.start_line);
        self.end_line
            .saturating_sub(self.start_line)
            .saturating_add(1)
    }
}

/// Reference to one chunk of an oversized base section. Chunk records render as
/// `DOC-ID#anchor@N` where `N` is the 1-based [`Self::ordinal`] within the base
/// section and [`Self::total`] is the chunk count.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChunkRef {
    /// 1-based chunk ordinal within the base section.
    pub ordinal: u32,
    /// Total number of chunks the base section was split into.
    pub total: u32,
}

impl ChunkRef {
    pub fn new(ordinal: u32, total: u32) -> Self {
        Self { ordinal, total }
    }
}

/// Derived section (or chunk) record. Matches the proposal's "Base section
/// model" verbatim. Records are the unit consumed by the lexical index, cache
/// generation and `get`/`search`/`tree`.
///
/// A base section under the chunk limits emits exactly one record with
/// `chunk: None`. An oversized base section emits one record per chunk with
/// `chunk: Some(...)`; the base section ref still resolves outline metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SectionRecord {
    /// Section record schema version (currently `1`).
    pub schema_version: u32,
    /// Stable ref. `DOC-ID#anchor` for an unchunked section, `DOC-ID#anchor@N`
    /// for a chunk.
    pub section_ref: String,
    /// Same as [`Self::section_ref`] in this slice.
    pub section_id: String,
    /// Owning stable document ID.
    pub document_id: String,
    /// Receipt-bound document revision the extraction was performed against.
    pub document_revision: u64,
    /// Repository-relative canonical document path.
    pub path: String,
    /// Resolved document title (first H1, or normalized file stem fallback).
    pub document_title: String,
    /// Visible heading text (cleaned of inline Markdown formatting).
    pub heading: String,
    /// Ancestor heading texts plus this heading's text. For the reserved
    /// preamble this is `["Preamble"]`.
    pub heading_path: Vec<String>,
    /// Deduplicated normalized anchor.
    pub anchor: String,
    /// 0-based source-order ordinal within the document (preamble is `0`).
    pub ordinal: u32,
    /// Inclusive 1-based source line range.
    pub range: SectionRange,
    /// SHA-256 of the exact full file bytes (`sha256:<hex>`).
    pub document_content_hash: String,
    /// SHA-256 of the exact source-byte slice covering this record's range.
    pub section_content_hash: String,
    /// Authored document summary (registry metadata, not a title).
    pub summary: String,
    /// Document authority as a lower-snake string (serde of the enum).
    pub authority: String,
    /// Document lifecycle as a lower-snake string.
    pub lifecycle: String,
    /// Document owner.
    pub owner: String,
    /// Document kind as a lower-snake string.
    pub kind: String,
    /// Scope domains.
    pub domains: Vec<String>,
    /// Approved alternate terminology.
    pub aliases: Vec<String>,
    /// Whether the section body is part of the searchable corpus.
    pub body_indexed: bool,
    /// Chunk identity when the base section was split; `None` for a single
    /// unchunked record.
    pub chunk: Option<ChunkRef>,
}

impl SectionRecord {
    /// Record schema version for freshly extracted records.
    pub const SCHEMA_VERSION: u32 = 1;
}

/// Normalize a heading's visible text into a stable anchor (algorithm v1).
///
/// Steps (versioned in [`ANCHOR_VERSION`]):
/// 1. trim Unicode whitespace;
/// 2. Unicode lowercase;
/// 3. remove inline Markdown formatting while preserving visible text;
/// 4. replace runs of whitespace and separator punctuation (`-`, `_`, `.`, `/`,
///    `:` and Unicode whitespace) with a single `-`;
/// 5. preserve Unicode letters and numbers;
/// 6. trim leading/trailing `-`;
/// 7. if empty, use [`EMPTY_HEADING_ANCHOR`] (`section`).
///
/// This function operates on a single heading's text. The reserved preamble
/// anchor is assigned by the extractor, not here: a heading whose text
/// normalizes to `preamble` follows normal duplicate-suffix rules.
pub fn anchor_for_heading(heading_text: &str) -> String {
    let trimmed = heading_text.trim();
    let lowered = trimmed.to_lowercase();
    let stripped = strip_inline_markdown(&lowered);

    let mut out = String::with_capacity(stripped.len());
    let mut prev_is_dash = true; // suppresses a leading dash
    for ch in stripped.chars() {
        if ch.is_alphanumeric() {
            out.push(ch);
            prev_is_dash = false;
        } else if is_separator_char(ch) && !prev_is_dash {
            out.push('-');
            prev_is_dash = true;
        }
        // Other punctuation is dropped without affecting dash state.
    }

    while out.ends_with('-') {
        out.pop();
    }

    if out.is_empty() {
        EMPTY_HEADING_ANCHOR.to_string()
    } else {
        out
    }
}

/// Deduplicate base anchors in document source order, producing the
/// deterministic sequence `anchor`, `anchor-2`, `anchor-3`, ... for repeats.
///
/// This implements the simple ordinal-suffix scheme from the proposal. It does
/// not attempt to avoid collisions with anchors that already end in `-N`
/// (matching GitHub's basic behavior); the tradeoff is documented and stable.
pub fn dedupe_anchors(anchors: &[String]) -> Vec<String> {
    use std::collections::HashMap;
    let mut counts: HashMap<&str, u32> = HashMap::new();
    let mut out = Vec::with_capacity(anchors.len());
    for anchor in anchors {
        let count = counts.entry(anchor.as_str()).or_insert(0);
        *count += 1;
        if *count == 1 {
            out.push(anchor.clone());
        } else {
            out.push(format!("{}-{}", anchor, count));
        }
    }
    out
}

/// Render a base section ref: `{document_id}#{anchor}`.
pub fn section_ref(document_id: &str, anchor: &str) -> String {
    format!("{document_id}#{anchor}")
}

/// Render a chunk ref: `{document_id}#{anchor}@{ordinal}`.
pub fn chunk_ref_string(document_id: &str, anchor: &str, ordinal: u32) -> String {
    format!("{document_id}#{anchor}@{ordinal}")
}

/// A character that acts as a separator in anchor normalization: Unicode
/// whitespace or one of the separator punctuation `- _ . / :`.
fn is_separator_char(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '-' | '_' | '.' | '/' | ':')
}

/// Remove inline Markdown formatting while preserving visible text.
///
/// Handles images `![alt](url)`, links `[text](url)` (including reference-style
/// `[text][ref]` and bare `[text]`), emphasis/strong `* _`, strikethrough `~~`,
/// inline code backticks, autolinks/inline HTML `<...>`, and backslash escapes.
/// Operating on already-lowercased text is fine: it carries no case information.
pub(crate) fn strip_inline_markdown(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let n = chars.len();
    while i < n {
        let c = chars[i];
        match c {
            '\\' => {
                // Backslash escape: keep the next char literally.
                if i + 1 < n {
                    out.push(chars[i + 1]);
                    i += 2;
                } else {
                    i += 1;
                }
            }
            '!' if i + 1 < n && chars[i + 1] == '[' => {
                // Image: drop the leading '!' and let the link branch handle it.
                i += 1;
            }
            '[' => {
                if let Some((text, next_i)) = parse_link_text(&chars, i) {
                    out.push_str(&text);
                    i = next_i;
                } else {
                    // Unbalanced bracket: drop it.
                    i += 1;
                }
            }
            '<' => {
                // Autolink or inline HTML: keep inner text without the brackets.
                if let Some(gt) = find_char(&chars, i + 1, '>') {
                    out.extend(chars[i + 1..gt].iter().copied());
                    i = gt + 1;
                } else {
                    out.push('<');
                    i += 1;
                }
            }
            // Emphasis/strong/strikethrough/inline-code markers (`*`, `_`, `~`,
            // `` ` ``) are deliberately NOT stripped here: the normalizer drops
            // `*`, `~` and `` ` `` (they are neither letters nor separators),
            // while `_` is a documented separator punctuation (step 4) and is
            // converted to `-`. This keeps intraword underscores (`a_b`) visible
            // rather than treating them as emphasis, matching the proposal.
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// Parse a Markdown link/image starting at `open` (the `[`). Returns the
/// visible text and the index just past the consumed construct.
fn parse_link_text(chars: &[char], open: usize) -> Option<(String, usize)> {
    let n = chars.len();
    let mut j = open + 1;
    while j < n && chars[j] != ']' {
        j += 1;
    }
    if j >= n {
        return None;
    }
    let text_end = j;
    let text: String = chars[open + 1..text_end].iter().collect();
    if text_end + 1 < n && chars[text_end + 1] == '(' {
        // Inline link `[text](url)`: consume through the matching `)`.
        let mut k = text_end + 2;
        while k < n && chars[k] != ')' {
            k += 1;
        }
        if k < n {
            return Some((text, k + 1));
        }
        return None;
    }
    // Reference-style `[text][ref]` or bare `[text]`: keep text, resume after `]`.
    Some((text, text_end + 1))
}

fn find_char(chars: &[char], from: usize, target: char) -> Option<usize> {
    chars[from..]
        .iter()
        .position(|&c| c == target)
        .map(|p| from + p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_basic_lowercases_and_dashes() {
        assert_eq!(
            anchor_for_heading("Refresh token lifecycle"),
            "refresh-token-lifecycle"
        );
        assert_eq!(
            anchor_for_heading("  Token  Lifecycle  "),
            "token-lifecycle"
        );
    }

    #[test]
    fn anchor_preserves_unicode_letters_numbers() {
        assert_eq!(anchor_for_heading("Hết hạn token"), "hết-hạn-token");
        assert_eq!(anchor_for_heading("Section 2 — v3"), "section-2-v3");
        assert_eq!(anchor_for_heading("名詞 vocabulary"), "名詞-vocabulary");
    }

    #[test]
    fn anchor_replaces_separator_punctuation() {
        assert_eq!(
            anchor_for_heading("error.code/value: v2.1"),
            "error-code-value-v2-1"
        );
        assert_eq!(anchor_for_heading("a_b.c-d"), "a-b-c-d");
    }

    #[test]
    fn anchor_strips_inline_markdown() {
        assert_eq!(
            anchor_for_heading("**Bold** and _italic_"),
            "bold-and-italic"
        );
        assert_eq!(anchor_for_heading("`code` heading"), "code-heading");
        assert_eq!(
            anchor_for_heading("[Link text](https://example.com)"),
            "link-text"
        );
        assert_eq!(anchor_for_heading("![alt](x.png)"), "alt");
        assert_eq!(anchor_for_heading("~~struck~~"), "struck");
    }

    #[test]
    fn anchor_empty_falls_back_to_section() {
        assert_eq!(anchor_for_heading(""), "section");
        assert_eq!(anchor_for_heading("   "), "section");
        assert_eq!(anchor_for_heading("***"), "section");
        assert_eq!(anchor_for_heading("---"), "section");
    }

    #[test]
    fn dedupe_anchors_suffixes_repeats() {
        assert_eq!(
            dedupe_anchors(&[
                "errors".to_string(),
                "intro".to_string(),
                "errors".to_string(),
                "errors".to_string(),
            ]),
            vec!["errors", "intro", "errors-2", "errors-3"]
        );
    }

    #[test]
    fn refs_render_as_specified() {
        assert_eq!(
            section_ref("DOC-AUTH", "refresh-token"),
            "DOC-AUTH#refresh-token"
        );
        assert_eq!(
            chunk_ref_string("DOC-AUTH", "large-section", 2),
            "DOC-AUTH#large-section@2"
        );
    }

    #[test]
    fn section_range_line_count_is_inclusive() {
        assert_eq!(SectionRange::new(12, 44).line_count(), 33);
        assert_eq!(SectionRange::new(5, 5).line_count(), 1);
    }
}
