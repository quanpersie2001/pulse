use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::canonical_json::{hash_bytes, hash_value, to_canonical_bytes};
use crate::docs::cache::{
    builds_dir, classify_against, cleanup_generations, generation_dir,
    generation_id_for_fingerprint, publish_current, CacheState, DocsSearchWriteLock, EngineState,
    ExtractorState, GenerationCounts, GenerationDocument, GenerationState, ValidatedGeneration,
    CACHE_SCHEMA_VERSION,
};
use crate::docs::lexical::{
    build_index_with_bodies as build_tantivy_index, load_section_records, write_sections_jsonl,
    TANTIVY_COMPAT_VERSION,
};
use crate::docs::markdown::extract_sections;
use crate::docs::model::{DocsRegistry, DocumentRecord, RetrievalConfig};
use crate::docs::policy::{eligible_documents, ResolvedRetrieval, RetrievalEligibilityOptions};
use crate::docs::projection::{
    check_projections, projection_targets, render_area_index, render_root_index,
};
use crate::docs::registry::{load_registry, registry_fingerprint};
use crate::docs::section::{SectionRecord, ANCHOR_VERSION, CHUNK_VERSION, EXTRACTOR_VERSION};
use crate::storage::atomic::atomic_replace;
use crate::storage::paths::resolve_repo_relative;
use crate::{PulseError, PulseResult};

pub const INDEX_CONFIG_VERSION: u32 = 1;
pub const TOKENIZER_CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IndexOptions {
    pub changed: bool,
    pub rebuild: bool,
    pub check: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IndexRegistryReport {
    pub revision: u64,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IndexStateReport {
    pub state: String,
    pub fingerprint: Option<String>,
    pub generation_id: Option<String>,
    pub engine: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct IndexDocumentsReport {
    pub registered: u32,
    pub eligible: u32,
    pub indexed: u32,
    pub excluded: u32,
    pub changed: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectionReport {
    pub state: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IndexBuildReport {
    pub schema_version: u32,
    pub code: String,
    pub registry: IndexRegistryReport,
    pub index: IndexStateReport,
    pub documents: IndexDocumentsReport,
    pub sections: u32,
    pub chunks: u32,
    pub projections: ProjectionReport,
    pub warnings: Vec<String>,
}

pub type IndexStatusReport = IndexBuildReport;

#[derive(Debug, Clone)]
struct CapturedDoc {
    record: DocumentRecord,
    retrieval: ResolvedRetrieval,
    content_hash: String,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
struct Capture {
    registry: DocsRegistry,
    registry_fingerprint: String,
    config: RetrievalConfig,
    docs: Vec<CapturedDoc>,
    fingerprint: String,
    config_hash: String,
    registry_retrieval_hash: String,
    total_source_bytes: u64,
}

pub fn build_index(repo_root: &Path, opts: IndexOptions) -> PulseResult<IndexBuildReport> {
    if opts.check {
        return index_status(repo_root);
    }
    let _cache_lock = DocsSearchWriteLock::acquire(repo_root)?;
    cleanup_generations(repo_root, true)?;
    let capture = capture_inputs(repo_root)?;
    if !within_auto_refresh_limits(
        &capture.config,
        capture.docs.len(),
        capture.total_source_bytes,
    ) {
        return Err(PulseError::validation(
            "docs_index_refresh_required",
            format!(
                "eligible docs/source bytes exceed auto-refresh limits: docs={}, bytes={}",
                capture.docs.len(),
                capture.total_source_bytes
            ),
        ));
    }
    let previous = crate::docs::cache::open_reader_generation(repo_root)
        .ok()
        .flatten();
    let reuse = if opts.rebuild {
        None
    } else {
        previous.as_ref()
    };
    let sections = extract_or_reuse_sections(&capture, reuse)?;
    let changed_docs = changed_document_count(&capture, reuse);
    let generation_id = generation_id_for_fingerprint(&capture.fingerprint)?;
    let build_id = format!("build_{}", generation_id.trim_start_matches("gen_"));
    let build_dir = builds_dir(repo_root).join(&build_id);
    if build_dir.exists() {
        fs::remove_dir_all(&build_dir).map_err(|e| PulseError::io(&build_dir, e))?;
    }
    fs::create_dir_all(&build_dir).map_err(|e| PulseError::io(&build_dir, e))?;
    let sections_path = build_dir.join("sections.jsonl");
    let sections_bytes = write_sections_jsonl(&sections_path, &sections)?;
    let tantivy_path = build_dir.join("tantivy");
    let bodies = section_bodies(&capture, &sections);
    build_tantivy_index(&tantivy_path, &sections, &bodies)?;
    let projection_hashes = expected_projection_hashes(repo_root, &capture.registry, true)?;
    let state = generation_state(
        &capture,
        &sections,
        &sections_bytes,
        projection_hashes,
        generation_id.clone(),
    );
    let state_bytes = to_canonical_bytes(&state)?;
    fs::write(build_dir.join("state.json"), state_bytes)
        .map_err(|e| PulseError::io(build_dir.join("state.json"), e))?;

    // Revalidate cheap inputs before publishing.
    let latest = capture_inputs(repo_root)?;
    if latest.fingerprint != capture.fingerprint {
        return Err(PulseError::validation(
            "docs_index_inputs_changed",
            "docs registry or document bytes changed while building index",
        ));
    }

    let final_dir = generation_dir(repo_root, &generation_id);
    if final_dir.exists() {
        fs::remove_dir_all(&final_dir).map_err(|e| PulseError::io(&final_dir, e))?;
    }
    fs::create_dir_all(final_dir.parent().expect("generation parent"))
        .map_err(|e| PulseError::io(final_dir.parent().unwrap(), e))?;
    fs::rename(&build_dir, &final_dir).map_err(|e| PulseError::io(&final_dir, e))?;
    publish_current(repo_root, &generation_id)?;
    write_projections(repo_root, &capture.registry)?;
    cleanup_generations(repo_root, true)?;

    Ok(report_from_state(
        "indexed",
        CacheState::Current,
        Some(&state),
        &capture,
        changed_docs,
        projection_state_string(repo_root, &capture.registry)?,
    ))
}

pub fn index_status(repo_root: &Path) -> PulseResult<IndexStatusReport> {
    let capture = capture_inputs(repo_root)?;
    let (state, valid) = classify_against(repo_root, &capture.fingerprint)?;
    let proj = projection_state_string(repo_root, &capture.registry)?;
    let generation = valid.as_ref().map(|v| &v.state);
    Ok(report_from_state(
        "ok", state, generation, &capture, 0, proj,
    ))
}

pub fn retrieval_fingerprint(
    config: &RetrievalConfig,
    eligible: &[(&DocumentRecord, ResolvedRetrieval)],
    content_hashes: &BTreeMap<String, String>,
) -> PulseResult<String> {
    let docs = eligible
        .iter()
        .map(|(doc, resolved)| {
            json!({
                "id": doc.id,
                "revision": doc.revision,
                "path": doc.path,
                "kind": doc.kind,
                "authority": doc.authority,
                "lifecycle": doc.lifecycle,
                "summary": doc.summary,
                "aliases": doc.aliases,
                "scope": doc.scope,
                "retrieval": doc.retrieval,
                "resolved_retrieval": {
                    "index": resolved.index,
                    "include_body": resolved.include_body,
                    "materialize_index": resolved.materialize_index,
                },
                "content_hash": content_hashes.get(&doc.id),
            })
        })
        .collect::<Vec<_>>();
    let payload = json!({
        "schema_version": CACHE_SCHEMA_VERSION,
        "extractor_version": EXTRACTOR_VERSION,
        "anchor_version": ANCHOR_VERSION,
        "chunk_version": CHUNK_VERSION,
        "engine": TANTIVY_COMPAT_VERSION,
        "index_config_version": INDEX_CONFIG_VERSION,
        "tokenizer_config_version": TOKENIZER_CONFIG_VERSION,
        "retrieval_config": config,
        "projection_config": crate::docs::projection::projection_config(&DocsRegistry {
            schema_version: crate::docs::model::DOCS_REGISTRY_SCHEMA_VERSION_V2,
            revision: 1,
            repository_id: "repo_fingerprint_placeholder".to_string(),
            documents: eligible.iter().map(|(d, _)| (*d).clone()).collect(),
            retrieval: Some(config.clone()),
        }),
        "documents": docs,
    });
    hash_value(&payload)
}

pub fn within_auto_refresh_limits(config: &RetrievalConfig, docs: usize, bytes: u64) -> bool {
    docs <= config.auto_refresh_max_documents as usize
        && bytes <= config.auto_refresh_max_source_bytes
}

pub fn current_generation(repo_root: &Path) -> PulseResult<Option<ValidatedGeneration>> {
    crate::docs::cache::open_reader_generation(repo_root)
}

fn capture_inputs(repo_root: &Path) -> PulseResult<Capture> {
    // `load_registry` already acquires the repository write guard and recovers
    // prepared canonical transactions. Do not acquire `WriteGuard` here again:
    // the lock is exclusive and non-reentrant, and doing so deadlocks tests and
    // real index builds in the same process.
    let registry = load_registry(repo_root)?;
    let registry_fingerprint = registry_fingerprint(&registry)?;
    let config = registry.retrieval_config();
    let eligible = eligible_documents(&registry, RetrievalEligibilityOptions::default());
    let mut docs = Vec::new();
    let mut content_hashes = BTreeMap::new();
    let mut total_source_bytes = 0u64;
    for (doc, retrieval) in eligible {
        let full = resolve_repo_relative(repo_root, &doc.path)?;
        let bytes = fs::read(&full).map_err(|e| PulseError::io(&full, e))?;
        if std::str::from_utf8(&bytes).is_err() {
            continue;
        }
        let content_hash = hash_bytes(&bytes);
        total_source_bytes += bytes.len() as u64;
        content_hashes.insert(doc.id.clone(), content_hash.clone());
        docs.push(CapturedDoc {
            record: doc.clone(),
            retrieval,
            content_hash,
            bytes,
        });
    }
    docs.sort_by(|a, b| a.record.id.cmp(&b.record.id));
    let eligible_refs = docs
        .iter()
        .map(|d| (&d.record, d.retrieval))
        .collect::<Vec<_>>();
    let fingerprint = retrieval_fingerprint(&config, &eligible_refs, &content_hashes)?;
    let config_hash = hash_value(&config)?;
    let registry_retrieval_hash = hash_value(&json!({
        "retrieval": registry.retrieval,
        "documents": registry.documents.iter().map(|doc| json!({
            "id": doc.id,
            "kind": doc.kind,
            "authority": doc.authority,
            "lifecycle": doc.lifecycle,
            "summary": doc.summary,
            "aliases": doc.aliases,
            "scope": doc.scope,
            "retrieval": doc.retrieval,
        })).collect::<Vec<_>>()
    }))?;
    Ok(Capture {
        registry,
        registry_fingerprint,
        config,
        docs,
        fingerprint,
        config_hash,
        registry_retrieval_hash,
        total_source_bytes,
    })
}

fn extract_or_reuse_sections(
    capture: &Capture,
    previous: Option<&ValidatedGeneration>,
) -> PulseResult<Vec<SectionRecord>> {
    let mut reused_by_doc: BTreeMap<String, Vec<SectionRecord>> = BTreeMap::new();
    if let Some(previous) = previous {
        let old_sections = load_section_records(&previous.sections_path)?;
        for section in old_sections {
            reused_by_doc
                .entry(section.document_id.clone())
                .or_default()
                .push(section);
        }
    }
    let mut sections = Vec::new();
    for doc in &capture.docs {
        let reusable = previous
            .and_then(|prev| prev.state.documents.get(&doc.record.id))
            .is_some_and(|old| {
                old.document_revision == doc.record.revision
                    && old.path == doc.record.path
                    && old.content_hash == doc.content_hash
                    && old.body_indexed == doc.retrieval.include_body
            });
        if reusable {
            if let Some(mut old) = reused_by_doc.remove(&doc.record.id) {
                sections.append(&mut old);
                continue;
            }
        }
        let outcome = extract_sections(
            &doc.record,
            doc.record.revision,
            &doc.content_hash,
            &doc.bytes,
            &capture.config,
            doc.retrieval.include_body,
        );
        sections.extend(outcome.sections);
    }
    sections.sort_by(|a, b| a.section_ref.cmp(&b.section_ref));
    Ok(sections)
}

fn changed_document_count(capture: &Capture, previous: Option<&ValidatedGeneration>) -> u32 {
    let Some(previous) = previous else {
        return capture.docs.len() as u32;
    };
    capture
        .docs
        .iter()
        .filter(|doc| {
            !previous
                .state
                .documents
                .get(&doc.record.id)
                .is_some_and(|old| {
                    old.document_revision == doc.record.revision
                        && old.path == doc.record.path
                        && old.content_hash == doc.content_hash
                        && old.body_indexed == doc.retrieval.include_body
                })
        })
        .count() as u32
}

fn section_bodies(capture: &Capture, sections: &[SectionRecord]) -> BTreeMap<String, String> {
    let by_doc = capture
        .docs
        .iter()
        .map(|doc| (doc.record.id.as_str(), doc.bytes.as_slice()))
        .collect::<BTreeMap<_, _>>();
    let mut out = BTreeMap::new();
    for section in sections {
        let Some(bytes) = by_doc.get(section.document_id.as_str()) else {
            continue;
        };
        let text = slice_lines_utf8(bytes, section.range.start_line, section.range.end_line);
        out.insert(section.section_ref.clone(), text);
    }
    out
}

fn slice_lines_utf8(bytes: &[u8], start_line: u32, end_line: u32) -> String {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return String::new();
    };
    let mut out = String::new();
    for (idx, line) in text.lines().enumerate() {
        let line_no = (idx as u32) + 1;
        if line_no >= start_line && line_no <= end_line {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn generation_state(
    capture: &Capture,
    sections: &[SectionRecord],
    sections_bytes: &[u8],
    projection_hashes: BTreeMap<String, String>,
    generation_id: String,
) -> GenerationState {
    let mut documents = BTreeMap::new();
    for doc in &capture.docs {
        let doc_sections = sections
            .iter()
            .filter(|section| section.document_id == doc.record.id)
            .collect::<Vec<_>>();
        let chunk_count = doc_sections.iter().filter(|s| s.chunk.is_some()).count() as u32;
        documents.insert(
            doc.record.id.clone(),
            GenerationDocument {
                document_revision: doc.record.revision,
                path: doc.record.path.clone(),
                content_hash: doc.content_hash.clone(),
                section_count: doc_sections.len() as u32,
                chunk_count,
                body_indexed: doc.retrieval.include_body,
            },
        );
    }
    let chunks = sections.iter().filter(|s| s.chunk.is_some()).count() as u32;
    GenerationState {
        schema_version: CACHE_SCHEMA_VERSION,
        generation_id,
        fingerprint: capture.fingerprint.clone(),
        engine: EngineState::current(),
        extractor: ExtractorState::current(),
        config_hash: capture.config_hash.clone(),
        registry_retrieval_hash: capture.registry_retrieval_hash.clone(),
        documents,
        sections_file_hash: hash_bytes(sections_bytes),
        projection_hashes,
        counts: GenerationCounts {
            registered: capture.registry.documents.len() as u32,
            eligible: capture.docs.len() as u32,
            indexed: capture.docs.len() as u32,
            sections: sections.len() as u32,
            chunks,
            excluded: capture
                .registry
                .documents
                .len()
                .saturating_sub(capture.docs.len()) as u32,
        },
    }
}

fn expected_projection_hashes(
    repo_root: &Path,
    registry: &DocsRegistry,
    write: bool,
) -> PulseResult<BTreeMap<String, String>> {
    let mut hashes = BTreeMap::new();
    for target in projection_targets(registry) {
        let text = match &target.area {
            None => render_root_index(registry)?,
            Some(area) => render_area_index(registry, area)?,
        };
        hashes.insert(target.path.clone(), hash_bytes(text.as_bytes()));
        if write {
            let path = repo_root.join(crate::storage::safe_repo_relative(&target.path)?);
            if let Ok(existing) = fs::read(&path) {
                if !crate::docs::projection::is_pulse_generated(&existing) {
                    return Err(PulseError::validation(
                        "docs_index_projection_conflict",
                        format!("projection conflict at {}", target.path),
                    ));
                }
            }
        }
    }
    Ok(hashes)
}

fn write_projections(repo_root: &Path, registry: &DocsRegistry) -> PulseResult<()> {
    for target in projection_targets(registry) {
        let text = match &target.area {
            None => render_root_index(registry)?,
            Some(area) => render_area_index(registry, area)?,
        };
        let path = repo_root.join(crate::storage::safe_repo_relative(&target.path)?);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| PulseError::io(parent, e))?;
        }
        if let Ok(existing) = fs::read(&path) {
            if !crate::docs::projection::is_pulse_generated(&existing) {
                return Err(PulseError::validation(
                    "docs_index_projection_conflict",
                    format!("projection conflict at {}", target.path),
                ));
            }
        }
        atomic_replace(&path, text.as_bytes()).map(|_| ())?;
    }
    Ok(())
}

fn projection_state_string(
    repo_root: &Path,
    registry: &DocsRegistry,
) -> PulseResult<ProjectionReport> {
    let report = check_projections(repo_root, registry)?;
    Ok(ProjectionReport {
        state: report.state.as_str().to_string(),
        files: projection_targets(registry)
            .into_iter()
            .map(|target| target.path)
            .collect(),
    })
}

fn report_from_state(
    code: &str,
    cache_state: CacheState,
    state: Option<&GenerationState>,
    capture: &Capture,
    changed_docs: u32,
    projections: ProjectionReport,
) -> IndexBuildReport {
    IndexBuildReport {
        schema_version: 1,
        code: code.to_string(),
        registry: IndexRegistryReport {
            revision: capture.registry.revision,
            fingerprint: capture.registry_fingerprint.clone(),
        },
        index: IndexStateReport {
            state: cache_state.as_str().to_string(),
            fingerprint: state.map(|s| s.fingerprint.clone()),
            generation_id: state.map(|s| s.generation_id.clone()),
            engine: crate::docs::cache::ENGINE_NAME.to_string(),
            mode: crate::docs::cache::ENGINE_MODE.to_string(),
        },
        documents: IndexDocumentsReport {
            registered: capture.registry.documents.len() as u32,
            eligible: capture.docs.len() as u32,
            indexed: state.map(|s| s.counts.indexed).unwrap_or(0),
            excluded: capture
                .registry
                .documents
                .len()
                .saturating_sub(capture.docs.len()) as u32,
            changed: changed_docs,
        },
        sections: state.map(|s| s.counts.sections).unwrap_or(0),
        chunks: state.map(|s| s.counts.chunks).unwrap_or(0),
        projections,
        warnings: Vec::new(),
    }
}
