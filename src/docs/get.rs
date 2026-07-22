use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::canonical_json::hash_bytes;
use crate::docs::markdown::extract_sections;
use crate::docs::model::DocumentRecord;
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
        return get_path_range(repo_root, doc, reference, range, options);
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
    let (bytes, content_hash, sections) = extract_current(repo_root, doc)?;
    let outline = sections.iter().map(outline_item).collect::<Vec<_>>();
    let doc_info = doc_info(doc, &content_hash);
    if let Some(section_ref) = section_ref {
        let section = sections
            .iter()
            .find(|section| section.section_ref == section_ref)
            .ok_or_else(|| stale_anchor_error(reference, &doc_info, &sections))?;
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
    StaleAnchorReport {
        schema_version: 1,
        code: "docs_anchor_stale".to_string(),
        message: format!("section ref not found: {requested_ref}"),
        requested_ref: requested_ref.to_string(),
        current_document: current_document.clone(),
        candidate_section_refs: sections
            .iter()
            .take(5)
            .map(|section| section.section_ref.clone())
            .collect(),
    }
}

fn stale_anchor_error(
    requested_ref: &str,
    current_document: &GetDocument,
    sections: &[SectionRecord],
) -> PulseError {
    let report = stale_anchor_report(requested_ref, current_document, sections);
    let message = serde_json::to_string(&report).unwrap_or(report.message);
    PulseError::validation("docs_anchor_stale", message)
}

fn serde_variant<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}
