use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::canonical_json::hash_bytes;
use crate::docs::markdown::extract_sections;
use crate::docs::model::{DocumentRecord, RetrievalConfig};
use crate::docs::registry::load_registry;
use crate::docs::section::{SectionRange, SectionRecord};
use crate::{PulseError, PulseResult};

#[derive(Debug, Clone, Default)]
pub struct GetOptions {
    pub max_lines: Option<u32>,
    pub max_bytes: Option<usize>,
    pub full: bool,
    pub full_section: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GetReport {
    pub schema_version: u32,
    pub ref_: String,
    pub document: GetDocument,
    pub section: Option<GetSection>,
    pub outline: Vec<GetOutlineItem>,
    pub body: Option<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GetDocument {
    pub id: String,
    pub revision: u64,
    pub path: String,
    pub content_hash: String,
    pub summary: String,
    pub authority: String,
    pub lifecycle: String,
    pub owner: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GetSection {
    pub section_ref: String,
    pub heading_path: Vec<String>,
    pub range: SectionRange,
    pub section_content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GetOutlineItem {
    pub section_ref: String,
    pub heading_path: Vec<String>,
    pub range: SectionRange,
    pub chunk: Option<crate::docs::ChunkRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StaleAnchorReport {
    pub schema_version: u32,
    pub code: String,
    pub message: String,
    pub requested_ref: String,
    pub current_document: GetDocument,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidate_section_refs: Vec<String>,
}

pub fn get_docs(repo_root: &Path, reference: &str, options: GetOptions) -> PulseResult<GetReport> {
    let registry = load_registry(repo_root)?;
    if let Some((path, range)) = parse_path_range(reference) {
        let doc = registry
            .documents
            .iter()
            .find(|doc| doc.path == path)
            .ok_or_else(|| {
                PulseError::validation(
                    "docs_anchor_stale",
                    "path range is not a registered document path",
                )
            })?;
        return get_path_range(
            repo_root,
            doc,
            reference,
            range,
            apply_registry_defaults(options, &registry.retrieval_config()),
        );
    }
    let (doc_id, section_ref) = match reference.split_once('#') {
        Some((doc_id, _)) => (doc_id, Some(reference.to_string())),
        None => (reference, None),
    };
    let doc = registry
        .documents
        .iter()
        .find(|doc| doc.id == doc_id)
        .ok_or_else(|| PulseError::NotFound {
            subject: format!("document {doc_id}"),
        })?;
    if doc.lifecycle != crate::docs::DocumentLifecycle::Current {
        return Err(PulseError::validation(
            "docs_anchor_stale",
            "document is not current",
        ));
    }
    let options = apply_registry_defaults(options, &registry.retrieval_config());
    let (bytes, content_hash, sections) = extract_current(repo_root, doc)?;

    let outline = sections.iter().map(outline_item).collect::<Vec<_>>();
    let doc_info = doc_info(doc, &content_hash);
    if let Some(section_ref) = section_ref {
        if let Some(section) = sections.iter().find(|section| section.section_ref == section_ref) {
            let (body, truncated) = bounded_body(&bytes, section.range, &options)?;
            return Ok(GetReport {
                schema_version: 1,
                ref_: reference.to_string(),
                document: doc_info,
                section: Some(GetSection {
                    section_ref: section.section_ref.clone(),
                    heading_path: section.heading_path.clone(),
                    range: section.range,
                    section_content_hash: section.section_content_hash.clone(),
                }),
                outline,
                body: Some(body),
                truncated,
            });
        }

        let chunks = chunk_records_for_base_ref(&sections, &section_ref);
        if !chunks.is_empty() {
            let first = chunks[0];
            let last = chunks[chunks.len() - 1];
            let base_range = SectionRange::new(first.range.start_line, last.range.end_line);
            let body_range = if options.full_section {
                base_range
            } else {
                first.range
            };
            let (body, body_truncated) = bounded_body(&bytes, body_range, &options)?;
            return Ok(GetReport {
                schema_version: 1,
                ref_: reference.to_string(),
                document: doc_info,
                section: Some(GetSection {
                    section_ref: section_ref.clone(),
                    heading_path: first.heading_path.clone(),
                    range: base_range,
                    section_content_hash: section_hash(&bytes, base_range)?,
                }),
                outline,
                body: Some(body),
                truncated: if options.full_section {
                    body_truncated
                } else {
                    true
                },
            });
        }

        return Err(stale_anchor_error(repo_root, reference, &doc_info, &sections));
    }
    if options.full {
        let max_bytes = options.max_bytes.unwrap_or(1_048_576);
        let (body, truncated) = utf8_prefix(&bytes, max_bytes)?;
        Ok(GetReport {
            schema_version: 1,
            ref_: reference.to_string(),
            document: doc_info,
            section: None,
            outline,
            body: Some(body),
            truncated,
        })
    } else {
        let preview = sections
            .first()
            .map(|s| bounded_body(&bytes, s.range, &options))
            .transpose()?;
        Ok(GetReport {
            schema_version: 1,
            ref_: reference.to_string(),
            document: doc_info,
            section: None,
            outline,
            body: preview.as_ref().map(|(body, _)| body.clone()),
            truncated: preview.map(|(_, truncated)| truncated).unwrap_or(false),
        })
    }
}

fn apply_registry_defaults(mut options: GetOptions, config: &RetrievalConfig) -> GetOptions {
    options.max_lines = options.max_lines.or(Some(config.default_get_max_lines));
    options.max_bytes = options
        .max_bytes
        .or(Some(config.default_get_max_bytes as usize));
    options
}

fn extract_current(
    repo_root: &Path,
    doc: &DocumentRecord,
) -> PulseResult<(Vec<u8>, String, Vec<SectionRecord>)> {
    let path = repo_root.join(crate::storage::safe_repo_relative(&doc.path)?);
    let bytes = std::fs::read(&path).map_err(|e| PulseError::io(&path, e))?;
    let content_hash = hash_bytes(&bytes);
    let outcome = extract_sections(
        doc,
        doc.revision,
        &content_hash,
        &bytes,
        &crate::docs::RetrievalConfig::defaults(),
        true,
    );
    Ok((bytes, content_hash, outcome.sections))
}

fn chunk_records_for_base_ref<'a>(
    sections: &'a [SectionRecord],
    base_ref: &str,
) -> Vec<&'a SectionRecord> {
    let mut chunks = sections
        .iter()
        .filter(|section| section.chunk.is_some())
        .filter(|section| format!("{}#{}", section.document_id, section.anchor) == base_ref)
        .collect::<Vec<_>>();
    chunks.sort_by_key(|section| {
        (
            section.chunk.map(|chunk| chunk.ordinal).unwrap_or(u32::MAX),
            section.range.start_line,
            section.range.end_line,
        )
    });
    chunks
}

fn section_hash(bytes: &[u8], range: SectionRange) -> PulseResult<String> {
    let line_spans = line_spans(bytes)?;
    validate_range(range, line_spans.len() as u32)?;
    let start = line_spans[(range.start_line - 1) as usize].0;
    let end = line_spans[(range.end_line - 1) as usize].1;
    Ok(hash_bytes(&bytes[start..end]))
}

fn bounded_body(
    bytes: &[u8],
    range: SectionRange,
    options: &GetOptions,
) -> PulseResult<(String, bool)> {
    let line_spans = line_spans(bytes)?;
    validate_range(range, line_spans.len() as u32)?;
    let max_lines = options.max_lines.unwrap_or(120);
    let max_bytes = options.max_bytes.unwrap_or(32_768);
    let effective_end = if options.full_section {
        range.end_line
    } else {
        range
            .end_line
            .min(range.start_line.saturating_add(max_lines).saturating_sub(1))
    };
    let start = line_spans[(range.start_line - 1) as usize].0;
    let end = line_spans[(effective_end - 1) as usize].1;
    let slice = &bytes[start..end];
    let (body, byte_truncated) = utf8_prefix(slice, max_bytes)?;
    let line_truncated = !options.full_section && range.line_count() > max_lines;
    Ok((body, line_truncated || byte_truncated))
}

fn doc_info(doc: &DocumentRecord, content_hash: &str) -> GetDocument {
    GetDocument {
        id: doc.id.clone(),
        revision: doc.revision,
        path: doc.path.clone(),
        content_hash: content_hash.to_string(),
        summary: doc.summary.clone(),
        authority: serde_variant(&doc.authority),
        lifecycle: serde_variant(&doc.lifecycle),
        owner: doc.owner.clone(),
        kind: serde_variant(&doc.kind),
    }
}

fn outline_item(section: &SectionRecord) -> GetOutlineItem {
    GetOutlineItem {
        section_ref: section.section_ref.clone(),
        heading_path: section.heading_path.clone(),
        range: section.range,
        chunk: section.chunk,
    }
}

fn parse_path_range(reference: &str) -> Option<(&str, SectionRange)> {
    let (path, range) = reference.rsplit_once(':')?;
    let (start, end) = range.split_once('-')?;
    Some((
        path,
        SectionRange {
            start_line: start.parse().ok()?,
            end_line: end.parse().ok()?,
        },
    ))
}

fn get_path_range(
    repo_root: &Path,
    doc: &DocumentRecord,
    reference: &str,
    range: SectionRange,
    options: GetOptions,
) -> PulseResult<GetReport> {
    if doc.lifecycle != crate::docs::DocumentLifecycle::Current {
        return Err(PulseError::validation(
            "docs_anchor_stale",
            "path range document is not current",
        ));
    }
    let (bytes, content_hash, sections) = extract_current(repo_root, doc)?;
    let line_spans = line_spans(&bytes)?;
    validate_range(range, line_spans.len() as u32)?;
    let start = line_spans[(range.start_line - 1) as usize].0;
    let end = line_spans[(range.end_line - 1) as usize].1;
    let section_content_hash = hash_bytes(&bytes[start..end]);
    let (body, truncated) = bounded_body(&bytes, range, &options)?;
    Ok(GetReport {
        schema_version: 1,
        ref_: reference.to_string(),
        document: doc_info(doc, &content_hash),
        section: Some(GetSection {
            section_ref: reference.to_string(),
            heading_path: Vec::new(),
            range,
            section_content_hash,
        }),
        outline: sections.iter().map(outline_item).collect(),
        body: Some(body),
        truncated,
    })
}

fn validate_range(range: SectionRange, total_lines: u32) -> PulseResult<()> {
    if range.start_line == 0 || range.end_line == 0 || range.start_line > range.end_line {
        return Err(PulseError::validation(
            "docs_get_range_invalid",
            "path range must satisfy start<=end and use 1-based lines",
        ));
    }
    if range.end_line > total_lines {
        return Err(PulseError::validation(
            "docs_get_range_invalid",
            format!(
                "path range end {} exceeds document line count {total_lines}",
                range.end_line
            ),
        ));
    }
    Ok(())
}

fn line_spans(bytes: &[u8]) -> PulseResult<Vec<(usize, usize)>> {
    std::str::from_utf8(bytes).map_err(|e| PulseError::validation("utf8_error", e.to_string()))?;
    let mut spans = Vec::new();
    let mut start = 0usize;
    for (idx, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            spans.push((start, idx + 1));
            start = idx + 1;
        }
    }
    if start < bytes.len() {
        spans.push((start, bytes.len()));
    }
    Ok(spans)
}

fn utf8_prefix(bytes: &[u8], max_bytes: usize) -> PulseResult<(String, bool)> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| PulseError::validation("utf8_error", e.to_string()))?;
    if text.len() <= max_bytes {
        return Ok((text.to_string(), false));
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok((text[..end].to_string(), true))
}

pub fn stale_anchor_report(
    requested_ref: &str,
    current_document: &GetDocument,
    sections: &[SectionRecord],
) -> StaleAnchorReport {
    stale_anchor_report_with_hint(requested_ref, current_document, sections, None)
}

fn stale_anchor_report_with_hint(
    requested_ref: &str,
    current_document: &GetDocument,
    sections: &[SectionRecord],
    source_line_hint: Option<u32>,
) -> StaleAnchorReport {
    StaleAnchorReport {
        schema_version: 1,
        code: "docs_anchor_stale".to_string(),
        message: format!("section ref not found: {requested_ref}"),
        requested_ref: requested_ref.to_string(),
        current_document: current_document.clone(),
        candidate_section_refs: ranked_stale_candidates(requested_ref, sections, source_line_hint),
    }
}

fn stale_anchor_error(
    repo_root: &Path,
    requested_ref: &str,
    current_document: &GetDocument,
    sections: &[SectionRecord],
) -> PulseError {
    let source_line_hint = cached_source_line_hint(repo_root, requested_ref);
    let report =
        stale_anchor_report_with_hint(requested_ref, current_document, sections, source_line_hint);
    let message = serde_json::to_string(&report).unwrap_or(report.message);
    PulseError::validation("docs_anchor_stale", message)
}

fn cached_source_line_hint(repo_root: &Path, requested_ref: &str) -> Option<u32> {
    let generation = crate::docs::cache::open_reader_generation(repo_root)
        .ok()
        .flatten()?;
    let sections = crate::docs::lexical::load_section_records(&generation.sections_path).ok()?;
    sections
        .iter()
        .find(|section| section.section_ref == requested_ref)
        .map(|section| section.range.start_line)
}

fn ranked_stale_candidates(
    requested_ref: &str,
    sections: &[SectionRecord],
    source_line_hint: Option<u32>,
) -> Vec<String> {
    let requested_anchor = anchor_from_section_ref(requested_ref).unwrap_or_default();
    let requested_base = duplicate_base_anchor(&requested_anchor);
    let requested_tokens = stale_anchor_tokens(&requested_anchor);
    let mut ranked = sections
        .iter()
        .map(|section| {
            let candidate_base = duplicate_base_anchor(&section.anchor);
            let prefix_score = prefix_anchor_score(
                &requested_anchor,
                &requested_base,
                &section.anchor,
                &candidate_base,
            );
            let candidate_tokens = stale_anchor_tokens(&format!(
                "{} {} {}",
                section.anchor,
                section.heading,
                section.heading_path.join(" ")
            ));
            let token_overlap = requested_tokens.intersection(&candidate_tokens).count() as i64;
            let proximity = source_line_hint
                .map(|line| section.range.start_line.abs_diff(line))
                .unwrap_or(u32::MAX);
            let proximity_score = source_line_hint
                .map(|_| 30_i64.saturating_sub(proximity.min(30) as i64))
                .unwrap_or(0);
            let score = prefix_score + (token_overlap * 12) + proximity_score;
            (score, token_overlap, proximity, section.section_ref.clone())
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| left.2.cmp(&right.2))
            .then_with(|| left.3.cmp(&right.3))
    });
    ranked
        .into_iter()
        .take(5)
        .map(|(_, _, _, section_ref)| section_ref)
        .collect()
}

fn anchor_from_section_ref(section_ref: &str) -> Option<String> {
    let (_, anchor) = section_ref.split_once('#')?;
    Some(anchor.split('@').next().unwrap_or(anchor).to_string())
}

fn duplicate_base_anchor(anchor: &str) -> String {
    let Some((base, suffix)) = anchor.rsplit_once('-') else {
        return anchor.to_string();
    };
    if suffix
        .parse::<u32>()
        .is_ok_and(|value| value >= 2 && !base.is_empty())
    {
        base.to_string()
    } else {
        anchor.to_string()
    }
}

fn prefix_anchor_score(
    requested_anchor: &str,
    requested_base: &str,
    candidate_anchor: &str,
    candidate_base: &str,
) -> i64 {
    if requested_anchor.is_empty() {
        return 0;
    }
    if candidate_anchor == requested_anchor {
        return 100;
    }
    if candidate_base == requested_base {
        return 90;
    }
    if candidate_anchor.starts_with(requested_base)
        || requested_base.starts_with(candidate_anchor)
        || candidate_base.starts_with(requested_base)
        || requested_base.starts_with(candidate_base)
    {
        return 60;
    }
    0
}

fn stale_anchor_tokens(text: &str) -> BTreeSet<String> {
    text.chars()
        .map(|ch| {
            if ch.is_alphanumeric() {
                ch.to_lowercase().collect::<String>()
            } else {
                " ".to_string()
            }
        })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn serde_variant<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}
