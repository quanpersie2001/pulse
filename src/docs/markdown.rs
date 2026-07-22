//! Markdown section extraction adapter (Slice 5, Phase 2).
//!
//! This module owns the *extraction semantics*: turning registered, current,
//! UTF-8 Markdown bytes into deterministic [`SectionRecord`]s with stable
//! anchors, exact 1-based source line ranges and exact source-byte content
//! hashes. It is pure (no IO) and treats all content as untrusted text — it
//! never renders or executes HTML/scripts.
//!
//! # Parser choice
//!
//! The contract is *extraction semantics*, not a crate name. The proposal
//! sanctions an equivalent pure-Rust parser, and every requirement here is
//! line-oriented (ATX headings, fenced code blocks, inclusive line ranges,
//! byte-exact slice hashing). A focused line scanner maps 1:1 to that contract,
//! keeps the crate's declared `rust-version` free of any third-party MSRV
//! pressure, and avoids pulling in a full Markdown renderer whose output we
//! explicitly must not use. [`comrak`] is therefore not a dependency.
//!
//! # Extraction rules
//!
//! - ATX headings `#`..`######` (up to three leading spaces); a `#` inside a
//!   fenced code block is never a heading.
//! - Fenced code blocks (``` and ~~~) are tracked so they are never split
//!   mid-chunk and never produce false headings.
//! - A heading section starts at its heading line and ends immediately before
//!   the next heading with level `<=` the current level.
//! - Non-whitespace content before the first heading becomes the reserved
//!   preamble section with anchor `preamble`.
//! - LF and CRLF produce identical logical line numbers/ranges; only the byte
//!   hashes differ.
//! - A UTF-8 BOM is accepted and excluded from heading text and from
//!   section-byte slices (so section hashes are BOM-stable).
//! - Oversized base sections (over the soft byte/line limits) are split into
//!   chunks with fence-atomic units and a best-effort line overlap.

use crate::canonical_json::hash_bytes;
use crate::docs::model::{DocumentRecord, RetrievalConfig};
use crate::docs::section::{
    anchor_for_heading, chunk_ref_string, dedupe_anchors, section_ref, ChunkRef, SectionRange,
    SectionRecord, CHUNK_OVERLAP_LINES, CHUNK_SOFT_MAX_BYTES, CHUNK_SOFT_MAX_LINES,
    PREAMBLE_ANCHOR,
};
use serde::{Deserialize, Serialize};

/// How the document title was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TitleSource {
    /// Resolved from the first level-1 heading.
    FirstH1,
    /// No level-1 heading found; title is empty and the caller is expected to
    /// substitute a normalized file stem.
    Fallback,
}

/// A non-fatal extraction diagnostic. Extraction is infallible for valid
/// UTF-8; warnings surface quality issues such as multiple document titles,
/// empty headings or a missing title fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum ExtractionWarning {
    /// More than one level-1 heading was found. `count` is the total H1 count;
    /// the first H1 still wins as the document title.
    MultipleDocumentTitles { count: u32 },
    /// A heading whose visible text is empty; it still gets an outline record
    /// with the `section` fallback anchor. Carries the affected section ref.
    EmptyHeading { section_ref: String },
    /// No level-1 heading was found and the document title fell back to empty.
    MissingTitleFallback,
    /// Defensive: the input bytes were not valid UTF-8. No sections extracted.
    /// (The indexer excludes non-UTF-8 documents before calling extraction.)
    InvalidUtf8,
}

/// Result of extracting sections from one document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionOutcome {
    /// Derived section/chunk records in source order.
    #[serde(default)]
    pub sections: Vec<SectionRecord>,
    /// Non-fatal diagnostics.
    #[serde(default)]
    pub warnings: Vec<ExtractionWarning>,
}

impl ExtractionOutcome {
    pub fn empty() -> Self {
        Self {
            sections: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Internal parser primitives
// ---------------------------------------------------------------------------

/// Byte span of one logical source line, relative to the BOM-stripped body.
/// Covers `[start, end)` where `end` is just past the line terminator (== next
/// line's start, or body end), so the bytes of lines `[s, e]` (inclusive) are
/// exactly `[spans[s-1].start, spans[e-1].end)`.
#[derive(Debug, Clone, Copy)]
struct LineSpan {
    /// First byte of the line's content.
    start: usize,
    /// Byte just past the line terminator (== next line's start, or body end).
    end: usize,
}

#[derive(Debug, Clone)]
struct HeadingInfo {
    /// 0-based line index.
    line_idx: usize,
    level: u8,
    /// Cleaned raw text (closing `#` sequence stripped, trimmed). Still
    /// contains inline Markdown formatting, which is stripped on demand.
    text: String,
}

#[derive(Debug)]
enum ParseResult {
    Ok(ParsedDoc),
    InvalidUtf8,
}

#[derive(Debug)]
struct ParsedDoc {
    /// One entry per logical line (no terminator, no BOM).
    lines: Vec<String>,
    /// Parallel byte spans relative to the BOM-stripped body.
    spans: Vec<LineSpan>,
    /// Per-line fence membership (true inside a fence block, including the
    /// opening and closing fence lines).
    in_fence: Vec<bool>,
    /// ATX headings outside fences, in source order.
    headings: Vec<HeadingInfo>,
    /// BOM-stripped body, for byte slicing.
    body: String,
}

impl ParsedDoc {
    fn total_lines(&self) -> u32 {
        self.lines.len() as u32
    }

    /// Exact source-byte slice covering inclusive 1-based lines [start, end].
    fn slice_bytes(&self, start_line: u32, end_line: u32) -> &[u8] {
        let s = self.spans[(start_line - 1) as usize].start;
        let e = self.spans[(end_line - 1) as usize].end;
        &self.body.as_bytes()[s..e]
    }

    fn slice_len(&self, start_line: u32, end_line: u32) -> usize {
        self.slice_bytes(start_line, end_line).len()
    }

    fn line_text(&self, line_no: u32) -> &str {
        &self.lines[(line_no - 1) as usize]
    }
}

fn utf8_bom_len(bytes: &[u8]) -> usize {
    if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        3
    } else {
        0
    }
}

fn parse_bytes(bytes: &[u8]) -> ParseResult {
    let bom_len = utf8_bom_len(bytes);
    let body_bytes = &bytes[bom_len..];
    let body = match std::str::from_utf8(body_bytes) {
        Ok(s) => s,
        Err(_) => return ParseResult::InvalidUtf8,
    };
    let (lines, spans) = split_lines(body);
    let in_fence = compute_fence(&lines);
    let headings = collect_headings(&lines, &in_fence);
    ParseResult::Ok(ParsedDoc {
        lines,
        spans,
        in_fence,
        headings,
        body: body.to_string(),
    })
}

/// Split a body string into logical lines and parallel byte spans. A single
/// trailing terminator does not create an extra empty line.
fn split_lines(body: &str) -> (Vec<String>, Vec<LineSpan>) {
    let bytes = body.as_bytes();
    let n = bytes.len();
    let mut lines = Vec::new();
    let mut spans = Vec::new();
    let mut line_start = 0usize;
    let mut i = 0usize;
    while i < n {
        if bytes[i] == b'\n' {
            let content_end = if i > line_start && bytes[i - 1] == b'\r' {
                i - 1
            } else {
                i
            };
            lines.push(body[line_start..content_end].to_string());
            spans.push(LineSpan {
                start: line_start,
                end: i + 1,
            });
            line_start = i + 1;
        }
        i += 1;
    }
    if line_start < n {
        lines.push(body[line_start..n].to_string());
        spans.push(LineSpan {
            start: line_start,
            end: n,
        });
    }
    (lines, spans)
}

/// Detect a fence-opening marker: up to three leading spaces then a run of at
/// least three `` ` `` or `~`. Returns (char, count, indent).
fn fence_marker(line: &str) -> Option<(char, usize, usize)> {
    let bytes = line.as_bytes();
    let mut indent = 0usize;
    while indent < bytes.len() && indent < 3 && bytes[indent] == b' ' {
        indent += 1;
    }
    if indent >= bytes.len() {
        return None;
    }
    let ch = bytes[indent];
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let mut count = 0usize;
    let mut i = indent;
    while i < bytes.len() && bytes[i] == ch {
        count += 1;
        i += 1;
    }
    if count >= 3 {
        Some((ch as char, count, indent))
    } else {
        None
    }
}

/// Whether `line` closes a fence opened with `open_char` and length `open_len`.
fn is_closing_fence(line: &str, open_char: char, open_len: usize) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && i < 3 && bytes[i] == b' ' {
        i += 1;
    }
    let mut count = 0usize;
    while i < bytes.len() && bytes[i] as char == open_char {
        count += 1;
        i += 1;
    }
    if count < open_len {
        return false;
    }
    while i < bytes.len() {
        if bytes[i] != b' ' {
            return false;
        }
        i += 1;
    }
    true
}

/// Compute per-line fence membership. The opening and closing fence lines are
/// themselves marked as in-fence so whole fence blocks are atomic for chunking.
fn compute_fence(lines: &[String]) -> Vec<bool> {
    let mut in_fence = vec![false; lines.len()];
    let mut fence_char: Option<char> = None;
    let mut fence_len = 0usize;
    for (idx, line) in lines.iter().enumerate() {
        match fence_char {
            Some(open_char) => {
                in_fence[idx] = true;
                if is_closing_fence(line, open_char, fence_len) {
                    fence_char = None;
                    fence_len = 0;
                }
            }
            None => {
                if let Some((ch, count, _)) = fence_marker(line) {
                    fence_char = Some(ch);
                    fence_len = count;
                    in_fence[idx] = true;
                }
            }
        }
    }
    in_fence
}

/// Parse an ATX heading from a line that is NOT inside a fence. Returns
/// (level, cleaned-text). Handles up to three leading spaces, the required
/// space/tab/EOL after the `#` run, and a trailing closing-`#` sequence.
fn parse_atx_heading(line: &str) -> Option<(u8, String)> {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && i < 3 && bytes[i] == b' ' {
        i += 1;
    }
    let mut level: u8 = 0;
    while i < bytes.len() && bytes[i] == b'#' && level < 6 {
        level += 1;
        i += 1;
    }
    if level == 0 {
        return None;
    }
    if i < bytes.len() && bytes[i] != b' ' && bytes[i] != b'\t' {
        return None;
    }
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    let rest = &line[i..];
    let trimmed = rest.trim_end_matches([' ', '\t']);
    let cleaned = strip_closing_hashes(trimmed);
    Some((level, cleaned))
}

/// Remove a CommonMark-style closing `#` sequence from the end of heading text.
fn strip_closing_hashes(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    let mut k = chars.len();
    while k > 0 && chars[k - 1] == '#' {
        k -= 1;
    }
    if k == chars.len() {
        return s.to_string();
    }
    let is_closing = k == 0 || chars[k - 1].is_whitespace();
    if !is_closing {
        return s.to_string();
    }
    let mut result: String = chars[..k].iter().collect();
    while result.ends_with(' ') || result.ends_with('\t') {
        result.pop();
    }
    result
}

fn collect_headings(lines: &[String], in_fence: &[bool]) -> Vec<HeadingInfo> {
    let mut out = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if in_fence[idx] {
            continue;
        }
        if let Some((level, text)) = parse_atx_heading(line) {
            out.push(HeadingInfo {
                line_idx: idx,
                level,
                text,
            });
        }
    }
    out
}

/// Visible text for a heading: inline Markdown formatting stripped and trimmed.
/// Reuses the anchor normalizer's stripper so heading display and anchor input
/// share one source of truth.
fn visible_heading_text(raw: &str) -> String {
    crate::docs::section::strip_inline_markdown(raw)
        .trim()
        .to_string()
}

/// Serialize a unit enum to its lower-snake string tag.
fn enum_tag<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => s,
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Section assembly
// ---------------------------------------------------------------------------

/// In-progress section before anchor dedupe and record emission.
struct SectionDesc {
    base_anchor: String,
    heading: String,
    heading_path: Vec<String>,
    start_line: u32,
    end_line: u32,
    heading_empty: bool,
}

/// Resolve the document title from decoded text: the first level-1 heading's
/// visible text, or empty with [`TitleSource::Fallback`].
pub fn extract_document_title(text: &str) -> (String, TitleSource) {
    let (lines, _spans) = split_lines(text);
    let in_fence = compute_fence(&lines);
    for (idx, line) in lines.iter().enumerate() {
        if in_fence[idx] {
            continue;
        }
        if let Some((level, raw)) = parse_atx_heading(line) {
            if level == 1 {
                return (visible_heading_text(&raw), TitleSource::FirstH1);
            }
        }
    }
    (String::new(), TitleSource::Fallback)
}

/// Extract deterministic section/chunk records from a document's bytes.
///
/// `content_hash` must equal `hash_bytes(bytes)`; this is asserted so a caller
/// bug (passing a stale hash) is caught immediately rather than silently
/// producing mismatched records. `config` is reserved for future per-config
/// chunk limits; the current limits are the versioned constants in
/// [`crate::docs::section`].
pub fn extract_sections(
    document: &DocumentRecord,
    document_revision: u64,
    content_hash: &str,
    bytes: &[u8],
    _config: &RetrievalConfig,
    body_indexed: bool,
) -> ExtractionOutcome {
    // Strong invariant: caller passes the hash of exactly these bytes.
    assert!(
        hash_bytes(bytes) == content_hash,
        "extract_sections: content_hash does not match hash_bytes(bytes); \
         caller must pass the hash of the exact same bytes"
    );

    let parsed = match parse_bytes(bytes) {
        ParseResult::Ok(doc) => doc,
        ParseResult::InvalidUtf8 => {
            let mut outcome = ExtractionOutcome::empty();
            outcome.warnings.push(ExtractionWarning::InvalidUtf8);
            return outcome;
        }
    };

    let mut warnings = Vec::new();

    // Document title (first H1) and H1 count diagnostics.
    let h1_headings: Vec<&HeadingInfo> = parsed.headings.iter().filter(|h| h.level == 1).collect();
    let (document_title, title_source) = match h1_headings.first() {
        Some(h) => (visible_heading_text(&h.text), TitleSource::FirstH1),
        None => (String::new(), TitleSource::Fallback),
    };
    let h1_count = h1_headings.len() as u32;
    if h1_count > 1 {
        warnings.push(ExtractionWarning::MultipleDocumentTitles { count: h1_count });
    }
    if title_source == TitleSource::Fallback {
        warnings.push(ExtractionWarning::MissingTitleFallback);
    }

    // Build section descriptors (preamble + heading sections).
    let descs = build_section_descs(&parsed);

    // Dedupe anchors across the whole document in source order.
    let base_anchors: Vec<String> = descs.iter().map(|d| d.base_anchor.clone()).collect();
    let deduped = dedupe_anchors(&base_anchors);

    // Shared document-level metadata for every record.
    let authority = enum_tag(&document.authority);
    let lifecycle = enum_tag(&document.lifecycle);
    let kind = enum_tag(&document.kind);
    let domains = document.scope.domains.clone();
    let aliases = document.aliases.clone();

    let mut sections = Vec::new();
    for (ordinal, desc) in descs.iter().enumerate() {
        let anchor = &deduped[ordinal];
        let base_ref = section_ref(&document.id, anchor);

        if desc.heading_empty {
            warnings.push(ExtractionWarning::EmptyHeading {
                section_ref: base_ref.clone(),
            });
        }

        let oversized = parsed.slice_len(desc.start_line, desc.end_line) > CHUNK_SOFT_MAX_BYTES
            || (desc.end_line - desc.start_line + 1) > CHUNK_SOFT_MAX_LINES;

        let chunk_ranges = if oversized {
            let units = build_atomic_units(desc.start_line, desc.end_line, &parsed.in_fence);
            chunk_section(&units, &parsed)
        } else {
            Vec::new()
        };

        if oversized {
            let total = chunk_ranges.len() as u32;
            for (idx, (cstart, cend)) in chunk_ranges.iter().enumerate() {
                let ordinal_n = (idx as u32) + 1;
                let cref = chunk_ref_string(&document.id, anchor, ordinal_n);
                sections.push(SectionRecord {
                    schema_version: SectionRecord::SCHEMA_VERSION,
                    section_ref: cref.clone(),
                    section_id: cref,
                    document_id: document.id.clone(),
                    document_revision,
                    path: document.path.clone(),
                    document_title: document_title.clone(),
                    heading: desc.heading.clone(),
                    heading_path: desc.heading_path.clone(),
                    anchor: anchor.clone(),
                    ordinal: ordinal as u32,
                    range: SectionRange::new(*cstart, *cend),
                    document_content_hash: content_hash.to_string(),
                    section_content_hash: hash_bytes(parsed.slice_bytes(*cstart, *cend)),
                    summary: document.summary.clone(),
                    authority: authority.clone(),
                    lifecycle: lifecycle.clone(),
                    owner: document.owner.clone(),
                    kind: kind.clone(),
                    domains: domains.clone(),
                    aliases: aliases.clone(),
                    body_indexed,
                    chunk: Some(ChunkRef::new(ordinal_n, total)),
                });
            }
        } else {
            sections.push(SectionRecord {
                schema_version: SectionRecord::SCHEMA_VERSION,
                section_ref: base_ref.clone(),
                section_id: base_ref,
                document_id: document.id.clone(),
                document_revision,
                path: document.path.clone(),
                document_title: document_title.clone(),
                heading: desc.heading.clone(),
                heading_path: desc.heading_path.clone(),
                anchor: anchor.clone(),
                ordinal: ordinal as u32,
                range: SectionRange::new(desc.start_line, desc.end_line),
                document_content_hash: content_hash.to_string(),
                section_content_hash: hash_bytes(
                    parsed.slice_bytes(desc.start_line, desc.end_line),
                ),
                summary: document.summary.clone(),
                authority: authority.clone(),
                lifecycle: lifecycle.clone(),
                owner: document.owner.clone(),
                kind: kind.clone(),
                domains: domains.clone(),
                aliases: aliases.clone(),
                body_indexed,
                chunk: None,
            });
        }
    }

    ExtractionOutcome { sections, warnings }
}

/// Build source-order section descriptors, including the reserved preamble.
fn build_section_descs(parsed: &ParsedDoc) -> Vec<SectionDesc> {
    let total = parsed.total_lines();
    let mut descs = Vec::new();

    // Preamble: meaningful non-whitespace content before the first heading.
    let first_heading_line_idx = parsed.headings.first().map(|h| h.line_idx);
    if let Some(first_idx) = first_heading_line_idx {
        if first_idx > 0 && lines_have_meaningful_content(parsed, 1, first_idx as u32) {
            descs.push(SectionDesc {
                base_anchor: PREAMBLE_ANCHOR.to_string(),
                heading: "Preamble".to_string(),
                heading_path: vec!["Preamble".to_string()],
                start_line: 1,
                end_line: first_idx as u32,
                heading_empty: false,
            });
        }
    } else if total > 0 && lines_have_meaningful_content(parsed, 1, total) {
        // No headings at all: the entire document is preamble.
        descs.push(SectionDesc {
            base_anchor: PREAMBLE_ANCHOR.to_string(),
            heading: "Preamble".to_string(),
            heading_path: vec!["Preamble".to_string()],
            start_line: 1,
            end_line: total,
            heading_empty: false,
        });
    }

    // Heading sections. End line = line before next heading with level <= this.
    for (i, heading) in parsed.headings.iter().enumerate() {
        let start_line = (heading.line_idx + 1) as u32;
        let mut end_line = total;
        for other in &parsed.headings[i + 1..] {
            if other.level <= heading.level {
                // `other.line_idx` is 0-based, so as a 1-based line it is the
                // line immediately before the next heading.
                end_line = other.line_idx as u32;
                break;
            }
        }
        let visible = visible_heading_text(&heading.text);
        let empty = visible.is_empty();
        let base_anchor = anchor_for_heading(&visible);

        let heading_path = build_heading_path(parsed, i);

        descs.push(SectionDesc {
            base_anchor,
            heading: visible,
            heading_path,
            start_line,
            end_line,
            heading_empty: empty,
        });
    }

    descs
}

/// Build the ancestor-heading path for the heading at `headings_index`.
fn build_heading_path(parsed: &ParsedDoc, headings_index: usize) -> Vec<String> {
    // Reconstruct the ancestor stack up to this heading.
    let mut stack: Vec<(u8, String)> = Vec::new();
    for heading in &parsed.headings[..headings_index] {
        let visible = visible_heading_text(&heading.text);
        while stack.last().is_some_and(|(lvl, _)| *lvl >= heading.level) {
            stack.pop();
        }
        stack.push((heading.level, visible));
    }
    let target = &parsed.headings[headings_index];
    while stack.last().is_some_and(|(lvl, _)| *lvl >= target.level) {
        stack.pop();
    }
    let target_visible = visible_heading_text(&target.text);
    stack.push((target.level, target_visible));
    stack.into_iter().map(|(_, text)| text).collect()
}

/// Whether lines [start, end] (1-based inclusive) contain any non-whitespace.
fn lines_have_meaningful_content(parsed: &ParsedDoc, start: u32, end: u32) -> bool {
    for line_no in start..=end {
        if parsed
            .line_text(line_no)
            .chars()
            .any(|c| !c.is_whitespace())
        {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Chunking
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Unit {
    start_line: u32,
    end_line: u32,
}

/// Group a section's lines into atomic units. A maximal run of fence lines is
/// one unit (never split); every other line is its own unit.
fn build_atomic_units(start_line: u32, end_line: u32, in_fence: &[bool]) -> Vec<Unit> {
    let mut units = Vec::new();
    let mut i = start_line;
    while i <= end_line {
        if in_fence[(i - 1) as usize] {
            let unit_start = i;
            while i <= end_line && in_fence[(i - 1) as usize] {
                i += 1;
            }
            units.push(Unit {
                start_line: unit_start,
                end_line: i - 1,
            });
        } else {
            units.push(Unit {
                start_line: i,
                end_line: i,
            });
            i += 1;
        }
    }
    units
}

/// Split an oversized section's units into chunk ranges. Fences stay atomic;
/// each chunk respects the soft byte/line limits unless a single fence unit
/// exceeds them; a best-effort [`CHUNK_OVERLAP_LINES`] overlap is applied when
/// it still guarantees forward progress.
fn chunk_section(units: &[Unit], parsed: &ParsedDoc) -> Vec<(u32, u32)> {
    if units.is_empty() {
        return Vec::new();
    }
    let byte_len = |s: u32, e: u32| parsed.slice_len(s, e);
    let mut ranges = Vec::new();
    let mut idx = 0usize;
    loop {
        let (start_line, end_line, k) = greedy_fill(units, idx, &byte_len);
        ranges.push((start_line, end_line));
        if k + 1 >= units.len() {
            break;
        }
        // Choose next start with overlap, falling back to no-overlap when the
        // overlap would not make the next chunk progress past `end_line`.
        let desired = end_line.saturating_sub(CHUNK_OVERLAP_LINES.saturating_sub(1));
        let mut next_idx = k + 1;
        for u in ((idx + 1)..=k).rev() {
            if units[u].start_line <= desired {
                let (_, greedy_end, _) = greedy_fill(units, u, &byte_len);
                if greedy_end > end_line {
                    next_idx = u;
                    break;
                }
            }
        }
        idx = next_idx;
    }
    ranges
}

/// Greedily fill from `from`, returning (start_line, end_line, last_unit_index).
fn greedy_fill(
    units: &[Unit],
    from: usize,
    byte_len: &dyn Fn(u32, u32) -> usize,
) -> (u32, u32, usize) {
    let start_line = units[from].start_line;
    let mut end_line = units[from].end_line;
    let mut k = from;
    while k + 1 < units.len() {
        let cand = units[k + 1].end_line;
        let nbytes = byte_len(start_line, cand);
        let nlines = cand - start_line + 1;
        if nbytes <= CHUNK_SOFT_MAX_BYTES && nlines <= CHUNK_SOFT_MAX_LINES {
            end_line = cand;
            k += 1;
        } else {
            break;
        }
    }
    (start_line, end_line, k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_first_h1_wins() {
        let text = "## Not the title\n\nbody\n\n# Real Title\n".to_string();
        let (title, src) = extract_document_title(&text);
        assert_eq!(title, "Real Title");
        assert_eq!(src, TitleSource::FirstH1);
    }

    #[test]
    fn title_fallback_without_h1() {
        let (title, src) = extract_document_title("## Only H2\n");
        assert_eq!(title, "");
        assert_eq!(src, TitleSource::Fallback);
    }

    #[test]
    fn heading_inside_fence_is_not_parsed() {
        let fenced = "```rust\n# not a heading\nlet x = 1;\n```\n# Real\n";
        let parsed = match parse_bytes(fenced.as_bytes()) {
            ParseResult::Ok(p) => p,
            _ => panic!("must parse"),
        };
        let levels: Vec<u8> = parsed.headings.iter().map(|h| h.level).collect();
        assert_eq!(levels, vec![1]);
        assert_eq!(parsed.headings[0].text, "Real");
    }

    #[test]
    fn crlf_and_lf_same_line_count() {
        let lf = "a\nb\nc";
        let crlf = "a\r\nb\r\nc";
        let (lf_lines, _) = split_lines(lf);
        let (crlf_lines, _) = split_lines(crlf);
        assert_eq!(lf_lines, crlf_lines);
    }
}
