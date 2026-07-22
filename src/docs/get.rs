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
            .ok_or_else(|| {
                PulseError::validation(
                    "docs_anchor_stale",
                    format!("section ref not found: {section_ref}"),
                )
            })?;
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
        let body = String::from_utf8(bytes)
            .map_err(|e| PulseError::validation("utf8_error", e.to_string()))?;
        let max_bytes = options.max_bytes.unwrap_or(1_048_576);
        let truncated = body.len() > max_bytes;
        let mut body = body;
        if truncated {
            body.truncate(max_bytes);
        }
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
    let text = std::str::from_utf8(bytes)
        .map_err(|e| PulseError::validation("utf8_error", e.to_string()))?;
    let max_lines = options.max_lines.unwrap_or(120);
    let max_bytes = options.max_bytes.unwrap_or(32_768);
    let mut lines = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let line_no = idx as u32 + 1;
        if line_no >= range.start_line && line_no <= range.end_line {
            lines.push(line);
            if lines.len() as u32 >= max_lines && !options.full_section {
                break;
            }
        }
    }
    let mut body = lines.join("\n");
    if !body.is_empty() {
        body.push('\n');
    }
    let truncated =
        (range.line_count() > max_lines && !options.full_section) || body.len() > max_bytes;
    if body.len() > max_bytes {
        body.truncate(max_bytes);
    }
    Ok((body, truncated))
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
    let (bytes, content_hash, sections) = extract_current(repo_root, doc)?;
    let (body, truncated) = bounded_body(&bytes, range, &options)?;
    Ok(GetReport {
        schema_version: 1,
        ref_: reference.to_string(),
        document: doc_info(doc, &content_hash),
        section: Some(GetSection {
            section_ref: reference.to_string(),
            heading_path: Vec::new(),
            range,
            section_content_hash: hash_bytes(body.as_bytes()),
        }),
        outline: sections.iter().map(outline_item).collect(),
        body: Some(body),
        truncated,
    })
}

fn serde_variant<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}
