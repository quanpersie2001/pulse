pub mod applicability;
pub mod cache;
pub mod eval;
pub mod get;
pub mod index;
pub mod lexical;
pub mod manifest;
pub mod markdown;
pub mod model;
pub mod policy;
pub mod projection;
pub mod receipt_validation;
pub mod registry;
pub mod search;
pub mod section;
pub mod tree;
pub mod validate;

pub use applicability::{
    applicable_docs, ApplicabilityOptions, ContentResolver, FsContentResolver,
};
pub use cache::{
    cache_dir, classify, classify_against, cleanup_generations, current_pointer_path,
    generation_dir, generation_id_for_fingerprint, open_reader_generation, publish_current,
    read_current, validate_generation, CacheState, DocsSearchWriteLock, EngineState,
    ExtractorState, GenerationCounts, GenerationDocument, GenerationState, ValidatedGeneration,
};
pub use eval::{
    load_retrieval_eval_fixtures, run_retrieval_eval_fixtures, run_retrieval_evals,
    RetrievalEvalExpected, RetrievalEvalFilters, RetrievalEvalFixture, RetrievalEvalReport,
    RetrievalEvalResult, RetrievalEvalWorkContext,
};
pub use get::{
    get_docs, stale_anchor_report, GetDocument, GetOptions, GetOutlineItem, GetReport, GetSection,
    StaleAnchorReport,
};
pub use index::{
    build_index, cache_state_error_code, check_index, current_generation,
    ensure_auto_refresh_allowed, index_status, retrieval_fingerprint, within_auto_refresh_limits,
    IndexBuildReport, IndexDocumentsReport, IndexOptions, IndexRegistryReport, IndexStateReport,
    IndexStatusReport, ProjectionReport,
};
pub use lexical::{
    build_index as build_lexical_index, build_index_with_bodies,
    build_schema as build_lexical_schema, load_section_records, open_index as open_lexical_index,
    query as query_lexical_index, tokenize_query_text, write_sections_jsonl, LexicalHit,
    LexicalSchema, TANTIVY_COMPAT_VERSION,
};
pub use manifest::{
    bootstrap, load, load_existing, DocsBootstrapOutcome, DOCS_INDEX_STATE_SCHEMA,
    DOCS_SECTION_SCHEMA, DOCUMENT_SCHEMA, RETRIEVAL_EVAL_SCHEMA,
};
pub use markdown::{
    extract_document_title, extract_sections, ExtractionOutcome, ExtractionWarning, TitleSource,
};
pub use model::*;
pub use policy::{
    eligible_documents, is_generated_navigation_path, is_protected_path, is_runtime_or_cache_path,
    is_work_content_path, retrieval_exclusion, ResolvedRetrieval, RetrievalEligibilityOptions,
    RetrievalExclusion,
};
pub use projection::{
    check_projections, is_pulse_generated, projection_config, projection_state, projection_targets,
    render_area_index, render_root_index, ProjectionCheckReport, ProjectionConfig,
    ProjectionFileState, ProjectionState, ProjectionStatus, ProjectionTarget, PROJECTION_MARKER,
    PROJECTION_SCHEMA_VERSION,
};
pub use registry::{
    edit, is_retrieval_only_change, list, load_registry, load_registry_or_empty, register,
    registry_fingerprint, registry_path, retire, show, supersede, DocsRegistryStore,
    MutationOutcome, MutationStatus, OperationContext,
};
pub use search::{
    search_docs, SearchBudget, SearchIndexInfo, SearchOptions, SearchReport, SearchResult,
    SearchWorkInfo,
};
pub use section::{
    anchor_for_heading, chunk_ref_string, dedupe_anchors, section_ref, ChunkRef, SectionRange,
    SectionRecord, ANCHOR_VERSION, CHUNK_HARD_MAX_BYTES, CHUNK_OVERLAP_LINES, CHUNK_SOFT_MAX_BYTES,
    CHUNK_SOFT_MAX_LINES, CHUNK_VERSION, EXTRACTOR_VERSION,
};
pub use tree::{docs_tree, tree_from_registry, DocsTreeReport, TreeNode, TreeOptions};
pub use validate::{validate_registry, DocsFinding, DocsValidationReport};
