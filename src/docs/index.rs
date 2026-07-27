use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::ErrorKind;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::canonical_json::{hash_bytes, hash_value, to_canonical_bytes};
use crate::docs::cache::{
    builds_dir, classify_against, cleanup_generations, generation_dir,
    generation_id_for_fingerprint, publish_current, read_current, validate_generation, CacheState,
    DocsSearchWriteLock, EngineState, ExtractorState, GenerationCounts, GenerationDocument,
    GenerationState, ValidatedGeneration, CACHE_SCHEMA_VERSION,
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
use crate::docs::registry::{load_registry, registry_fingerprint, registry_path};
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
    #[serde(skip)]
    pub include_draft: bool,
    #[serde(skip)]
    pub include_stale: bool,
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

/// Build a disposable docs-search cache without writing tracked projections.
///
/// Per P2S1-D9, `work packet` must never write generated navigation
/// (`docs/**/_index.md`) because that would dirty a clean worktree and
/// invalidate the packet source snapshot. This function builds and publishes
/// only the cache generation under `.pulse/cache/docs-search/`, which is
/// already git-ignored.
///
/// The returned `IndexBuildReport` has `projections.state = "cache_only"`
/// to indicate that generated navigation was intentionally skipped.
pub fn build_search_cache(repo_root: &Path, opts: IndexOptions) -> PulseResult<IndexBuildReport> {
    if opts.check {
        return check_index(repo_root);
    }
    let _cache_lock = DocsSearchWriteLock::acquire(repo_root)?;
    cleanup_generations(repo_root, true)?;
    let capture =
        capture_inputs_with_options(repo_root, retrieval_options_from_index_options(opts))?;
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
    sync_file(&sections_path)?;
    let tantivy_path = build_dir.join("tantivy");
    let bodies = section_bodies(&capture, &sections);
    build_tantivy_index(&tantivy_path, &sections, &bodies)?;
    sync_directory_tree(&tantivy_path)?;
    // Cache-only: use empty projection hashes instead of computing actual ones.
    let projection_hashes = BTreeMap::new();
    let state = generation_state(
        &capture,
        &sections,
        &sections_bytes,
        projection_hashes,
        generation_id.clone(),
    );
    let state_bytes = to_canonical_bytes(&state)?;
    let state_path = build_dir.join("state.json");
    fs::write(&state_path, state_bytes).map_err(|e| PulseError::io(&state_path, e))?;
    sync_file(&state_path)?;
    sync_directory_best_effort(&build_dir)?;

    // Revalidate cheap inputs before publishing.
    let latest =
        capture_inputs_with_options(repo_root, retrieval_options_from_index_options(opts))?;
    if latest.fingerprint != capture.fingerprint {
        return Err(PulseError::validation(
            "docs_index_inputs_changed",
            "docs registry or document bytes changed while building index",
        ));
    }

    let final_dir = generation_dir(repo_root, &generation_id);
    let current = read_current(repo_root);
    let current_final_is_valid = final_dir.exists()
        && current.as_deref() == Some(&generation_id)
        && validate_generation(repo_root, &generation_id).is_ok();
    if current_final_is_valid {
        fs::remove_dir_all(&build_dir).map_err(|e| PulseError::io(&build_dir, e))?;
    } else {
        if final_dir.exists() {
            let replace_dir = final_dir.with_file_name(format!(
                ".{}-replace-{}",
                generation_id,
                std::process::id()
            ));
            if replace_dir.exists() {
                fs::remove_dir_all(&replace_dir).map_err(|e| PulseError::io(&replace_dir, e))?;
            }
            fs::rename(&final_dir, &replace_dir).map_err(|e| PulseError::io(&replace_dir, e))?;
            fs::rename(&build_dir, &final_dir).map_err(|e| PulseError::io(&final_dir, e))?;
            sync_generations_parent(repo_root)?;
            fs::remove_dir_all(&replace_dir).map_err(|e| PulseError::io(&replace_dir, e))?;
            sync_generations_parent(repo_root)?;
        } else {
            fs::create_dir_all(final_dir.parent().expect("generation parent"))
                .map_err(|e| PulseError::io(final_dir.parent().unwrap(), e))?;
            fs::rename(&build_dir, &final_dir).map_err(|e| PulseError::io(&final_dir, e))?;
            sync_generations_parent(repo_root)?;
        }
    }
    publish_current(repo_root, &generation_id)?;
    // IMPORTANT: Do NOT write tracked projections (docs/**/_index.md) here.
    // Per P2S1-D9, `work packet` must not dirty the worktree.
    cleanup_generations(repo_root, true)?;

    Ok(report_from_state(
        "indexed",
        CacheState::Current,
        Some(&state),
        &capture,
        changed_docs,
        ProjectionReport {
            state: "cache_only".to_string(),
            files: vec![],
        },
    ))
}

pub fn build_index(repo_root: &Path, opts: IndexOptions) -> PulseResult<IndexBuildReport> {
    if opts.check {
        return check_index(repo_root);
    }
    // Build the cache first (captures changed docs correctly).
    let cache_report = build_search_cache(repo_root, opts)?;

    // Re-read the current cache generation for projection computation.
    let _generation = current_generation(repo_root)?.ok_or_else(|| {
        PulseError::validation(
            "docs_index_missing",
            "docs-search generation missing after cache build",
        )
    })?;

    // Now write projections using the current registry.
    let registry: DocsRegistry = crate::storage::read_json(&registry_path(repo_root))?;
    write_projections(repo_root, &registry)?;

    let projection = projection_state_string(repo_root, &registry)?;
    Ok(IndexBuildReport {
        schema_version: cache_report.schema_version,
        code: cache_report.code,
        registry: cache_report.registry,
        index: cache_report.index,
        documents: cache_report.documents,
        sections: cache_report.sections,
        chunks: cache_report.chunks,
        projections: projection,
        warnings: cache_report.warnings,
    })
}

pub fn index_status(repo_root: &Path) -> PulseResult<IndexStatusReport> {
    let capture = capture_inputs_read_only(repo_root)?;
    let (state, valid) = classify_against(repo_root, &capture.fingerprint)?;
    let proj = projection_state_string(repo_root, &capture.registry)?;
    let generation = valid.as_ref().map(|v| &v.state);
    Ok(report_from_state(
        "ok", state, generation, &capture, 0, proj,
    ))
}

pub fn check_index(repo_root: &Path) -> PulseResult<IndexStatusReport> {
    let report = index_status(repo_root)?;
    if report.index.state != "current" {
        return Err(PulseError::validation(
            cache_state_error_code(&report.index.state),
            format!("docs-search cache is {}", report.index.state),
        ));
    }
    if report.projections.state != "current" {
        return Err(PulseError::validation(
            projection_state_error_code(&report.projections.state),
            format!("docs projections are {}", report.projections.state),
        ));
    }
    Ok(report)
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
            schema_version: crate::docs::model::DOCS_REGISTRY_SCHEMA_VERSION,
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

/// Current cache generation content fingerprint, or None if the cache is
/// absent or invalid. Uses the lightweight `classify` path instead of
/// building a full generation object.
pub fn current_cache_fingerprint(repo_root: &Path) -> PulseResult<Option<String>> {
    let (state, valid) = crate::docs::cache::classify(repo_root)?;
    if state != CacheState::Current {
        return Ok(None);
    }
    Ok(valid.map(|v| v.state.fingerprint.clone()))
}

pub fn index_status_with_options(
    repo_root: &Path,
    options: RetrievalEligibilityOptions,
) -> PulseResult<IndexStatusReport> {
    let registry: DocsRegistry = crate::storage::read_json(&registry_path(repo_root))?;
    let capture = capture_from_registry(repo_root, registry, options)?;
    let (state, valid) = classify_against(repo_root, &capture.fingerprint)?;
    let proj = projection_state_string(repo_root, &capture.registry)?;
    let generation = valid.as_ref().map(|v| &v.state);
    Ok(report_from_state(
        "ok", state, generation, &capture, 0, proj,
    ))
}

pub fn ensure_auto_refresh_allowed(repo_root: &Path) -> PulseResult<()> {
    let capture = capture_inputs_read_only(repo_root)?;
    if within_auto_refresh_limits(
        &capture.config,
        capture.docs.len(),
        capture.total_source_bytes,
    ) {
        Ok(())
    } else {
        Err(PulseError::validation(
            "docs_index_refresh_required",
            format!(
                "eligible docs/source bytes exceed auto-refresh limits: docs={}, bytes={}",
                capture.docs.len(),
                capture.total_source_bytes
            ),
        ))
    }
}

fn capture_inputs_read_only(repo_root: &Path) -> PulseResult<Capture> {
    let registry: DocsRegistry = crate::storage::read_json(&registry_path(repo_root))?;
    capture_from_registry(repo_root, registry, RetrievalEligibilityOptions::default())
}

fn capture_inputs_with_options(
    repo_root: &Path,
    options: RetrievalEligibilityOptions,
) -> PulseResult<Capture> {
    // `load_registry` already acquires the repository write guard and recovers
    // prepared canonical transactions. Do not acquire `WriteGuard` here again:
    // the lock is exclusive and non-reentrant, and doing so deadlocks tests and
    // real index builds in the same process.
    let registry = load_registry(repo_root)?;
    capture_from_registry(repo_root, registry, options)
}

fn retrieval_options_from_index_options(opts: IndexOptions) -> RetrievalEligibilityOptions {
    RetrievalEligibilityOptions {
        include_draft: opts.include_draft,
        include_stale: opts.include_stale,
    }
}

fn capture_from_registry(
    repo_root: &Path,
    registry: DocsRegistry,
    options: RetrievalEligibilityOptions,
) -> PulseResult<Capture> {
    let registry_fingerprint = registry_fingerprint(&registry)?;
    let config = registry.retrieval_config();
    let eligible = eligible_documents(&registry, options);
    let mut docs = Vec::new();
    let mut content_hashes = BTreeMap::new();
    let mut total_source_bytes = 0u64;
    for (doc, retrieval) in eligible {
        let full = resolve_repo_relative(repo_root, &doc.path)?;
        let bytes = fs::read(&full).map_err(|e| PulseError::io(&full, e))?;
        if std::str::from_utf8(&bytes).is_err() {
            return Err(PulseError::validation(
                "docs_document_not_utf8",
                format!("eligible indexed document is not UTF-8: {}", doc.path),
            ));
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
        let end_line = index_body_end_line(section, sections);
        let text = slice_lines_utf8(bytes, section.range.start_line, end_line);
        out.insert(section.section_ref.clone(), text);
    }
    out
}

fn index_body_end_line(section: &SectionRecord, sections: &[SectionRecord]) -> u32 {
    if section.chunk.is_some() {
        return section.range.end_line;
    }
    sections
        .iter()
        .filter(|candidate| candidate.document_id == section.document_id)
        .filter(|candidate| {
            candidate.range.start_line > section.range.start_line
                && candidate.range.start_line <= section.range.end_line
        })
        .filter(|candidate| is_nested_heading_path(&section.heading_path, &candidate.heading_path))
        .map(|candidate| candidate.range.start_line.saturating_sub(1))
        .min()
        .unwrap_or(section.range.end_line)
}

fn is_nested_heading_path(parent: &[String], candidate: &[String]) -> bool {
    candidate.len() > parent.len()
        && candidate
            .iter()
            .zip(parent.iter())
            .all(|(candidate, parent)| candidate == parent)
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

#[allow(dead_code)]
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
        // Projection writes use storage::atomic::atomic_replace, which syncs the
        // temp file before rename and best-effort syncs the parent directory
        // after rename. That is sufficient for these derived navigation files;
        // the docs-search generation/CURRENT path below carries the stricter
        // publication durability boundary.
        atomic_replace(&path, text.as_bytes()).map(|_| ())?;
    }
    Ok(())
}

fn sync_file(path: &Path) -> PulseResult<()> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|e| PulseError::io(path, e))
}

fn sync_directory_tree(path: &Path) -> PulseResult<()> {
    for entry in fs::read_dir(path).map_err(|e| PulseError::io(path, e))? {
        let entry = entry.map_err(|e| PulseError::io(path, e))?;
        let child = entry.path();
        let file_type = entry.file_type().map_err(|e| PulseError::io(&child, e))?;
        if file_type.is_dir() {
            sync_directory_tree(&child)?;
        } else if file_type.is_file() {
            sync_file(&child)?;
        }
    }
    sync_directory_best_effort(path)
}

fn sync_generations_parent(repo_root: &Path) -> PulseResult<()> {
    sync_directory_best_effort(&crate::docs::cache::cache_dir(repo_root).join("generations"))
}

fn sync_directory_best_effort(path: &Path) -> PulseResult<()> {
    if !path.exists() {
        return Ok(());
    }
    match File::open(path).and_then(|dir| dir.sync_all()) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::Unsupported
                    | ErrorKind::PermissionDenied
                    | ErrorKind::InvalidInput
                    | ErrorKind::Other
            ) =>
        {
            // Directory sync is unavailable on some platforms/filesystems
            // (notably Windows std without platform-specific flags). File data
            // has already been synced; name durability is best-effort there.
            Ok(())
        }
        Err(error) => Err(PulseError::io(path, error)),
    }
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

pub fn cache_state_error_code(state: &str) -> &'static str {
    match state {
        "missing" => "docs_index_missing",
        "stale" => "docs_index_stale",
        "corrupt" => "docs_index_corrupt",
        "incompatible" => "docs_index_incompatible",
        _ => "docs_index_not_current",
    }
}

fn projection_state_error_code(state: &str) -> &'static str {
    match state {
        "missing" => "docs_index_projection_missing",
        "stale" => "docs_index_projection_stale",
        "conflict" => "docs_index_projection_conflict",
        _ => "docs_index_projection_not_current",
    }
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
