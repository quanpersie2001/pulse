pub mod applicability;
pub mod manifest;
pub mod markdown;
pub mod model;
pub mod policy;
pub mod registry;
pub mod section;
pub mod validate;

pub use applicability::{
    applicable_docs, ApplicabilityOptions, ContentResolver, FsContentResolver,
};
pub use manifest::{
    bootstrap, load, migrate_registry, predecessor_schema, DocsBootstrapOutcome,
    DocsMigrationOutcome, MigrationStatus, SchemaVersion, DOCUMENT_SCHEMA,
};
pub use markdown::{
    extract_document_title, extract_sections, ExtractionOutcome, ExtractionWarning, TitleSource,
};
pub use model::*;
pub use policy::{
    is_generated_navigation_path, is_protected_path, is_runtime_or_cache_path,
    is_work_content_path, retrieval_exclusion, ResolvedRetrieval, RetrievalEligibilityOptions,
    RetrievalExclusion,
};
pub use registry::{
    edit, is_retrieval_only_change, list, load_registry, load_registry_or_empty, register,
    registry_fingerprint, registry_path, retire, show, supersede, DocsRegistryStore,
    MutationOutcome, MutationStatus, OperationContext,
};
pub use section::{
    anchor_for_heading, chunk_ref_string, dedupe_anchors, section_ref, ChunkRef, SectionRange,
    SectionRecord, ANCHOR_VERSION, CHUNK_HARD_MAX_BYTES, CHUNK_OVERLAP_LINES, CHUNK_SOFT_MAX_BYTES,
    CHUNK_SOFT_MAX_LINES, CHUNK_VERSION, EXTRACTOR_VERSION,
};
pub use validate::{validate_registry, DocsFinding, DocsValidationReport};
